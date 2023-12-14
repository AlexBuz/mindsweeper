use std::collections::BTreeMap;

use crate::analyzer::Partition;

use super::*;
use itertools::{chain, izip, repeat_n};
use num::{BigUint, One};
use rand::{distributions::WeightedError, seq::SliceRandom};
use tinyvec::ArrayVec;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Tile {
    Hidden { is_mine: bool },
    Revealed { adjacent_mine_count: u8 },
}

impl Tile {
    fn is_revealed(&self) -> bool {
        matches!(self, Tile::Revealed { .. })
    }

    fn adjacent_mine_count(&self) -> Option<u8> {
        match self {
            Tile::Hidden { .. } => None,
            Tile::Revealed {
                adjacent_mine_count,
            } => Some(*adjacent_mine_count),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LocalGame {
    config: GameConfig,
    tiles: Vec<Tile>,
    hidden_safe_count: usize,
    status: GameStatus,
    analyzer: Option<Analyzer>,
}

struct SolutionGroup {
    mine_count_by_component: Vec<usize>,
    weight: BigUint,
}

impl LocalGame {
    // precondition: tile must be hidden and not a mine
    fn reveal_tile_unchecked(&mut self, tile_id: usize) {
        let mut adjacent_mine_count = 0;
        let adjacent_safe_tile_ids: ArrayVec<[usize; 8]> = self
            .config
            .grid_config
            .iter_adjacent(tile_id)
            .filter(|&adjacent_tile_id| match self.tiles[adjacent_tile_id] {
                Tile::Hidden { is_mine } => {
                    if is_mine {
                        adjacent_mine_count += 1;
                        false
                    } else {
                        true
                    }
                }
                _ => false,
            })
            .collect();
        self.tiles[tile_id] = Tile::Revealed {
            adjacent_mine_count,
        };
        self.hidden_safe_count -= 1;
        if self.hidden_safe_count == 0 {
            self.status = GameStatus::Won
        } else if adjacent_mine_count == 0 {
            self.chord_unchecked(&adjacent_safe_tile_ids);
        }
    }

    // precondition: all the adjacent hidden tile ids should be safe
    fn chord_unchecked(&mut self, adjacent_all_safe_hidden_tile_ids: &[usize]) {
        for &tile_id in adjacent_all_safe_hidden_tile_ids {
            if self.tiles[tile_id].is_revealed() {
                continue;
            }
            self.reveal_tile_unchecked(tile_id);
            if self.status.is_won() {
                break;
            }
        }
    }

    fn compute_weights(
        mut solution_groups: Vec<SolutionGroup>,
        mine_count_by_component_so_far: &mut Vec<usize>,
        unconstrained_unknown_tile_ids: &[usize],
        mine_arrangements_by_mine_count_by_component: &[BTreeMap<usize, Vec<Vec<usize>>>],
        remaining_mine_count: usize,
        factor: BigUint,
    ) -> Vec<SolutionGroup> {
        match mine_arrangements_by_mine_count_by_component.split_first() {
            None => {
                solution_groups.push(SolutionGroup {
                    mine_count_by_component: mine_count_by_component_so_far.clone(),
                    weight: factor
                        * big_binomial(unconstrained_unknown_tile_ids.len(), remaining_mine_count),
                });
            }
            Some((
                mine_arrangements_by_mine_count,
                mine_arrangements_by_mine_count_by_component,
            )) => {
                for (&mine_count, arrangements) in mine_arrangements_by_mine_count {
                    if mine_count > remaining_mine_count {
                        break;
                    }
                    mine_count_by_component_so_far.push(mine_count);
                    solution_groups = Self::compute_weights(
                        solution_groups,
                        mine_count_by_component_so_far,
                        unconstrained_unknown_tile_ids,
                        mine_arrangements_by_mine_count_by_component,
                        remaining_mine_count - mine_count,
                        &factor * arrangements.len(),
                    );
                    mine_count_by_component_so_far.pop();
                }
            }
        }
        solution_groups
    }

    fn rearrange_mines(
        &mut self,
        partition: &Partition,
        mine_arrangements_by_mine_count_by_component: &[BTreeMap<usize, Vec<Vec<usize>>>],
    ) -> bool {
        let solution_groups = Self::compute_weights(
            vec![],
            &mut vec![],
            &partition.unconstrained_unknown_tile_ids,
            mine_arrangements_by_mine_count_by_component,
            self.config.grid_config.mine_count - partition.known_mine_count,
            BigUint::one(),
        );

        let mut rng = rand::thread_rng();
        let random_solution_group: &SolutionGroup = {
            match solution_groups.choose_weighted(&mut rng, |group| group.weight.clone()) {
                Ok(group) => group,
                Err(error) => match error {
                    WeightedError::NoItem | WeightedError::AllWeightsZero => return false,
                    _ => panic!("error while choosing solution group: {error}"),
                },
            }
        };

        let remaining_mine_count = self.config.grid_config.mine_count
            - partition.known_mine_count
            - random_solution_group
                .mine_count_by_component
                .iter()
                .sum::<usize>();

        izip!(
            &partition.components,
            &random_solution_group.mine_count_by_component,
            mine_arrangements_by_mine_count_by_component
        )
        .for_each(|(component, mine_count, mine_arrangements_by_mine_count)| {
            for &unknown_tile_id in &component.unknown_tile_ids {
                self.tiles[unknown_tile_id] = Tile::Hidden { is_mine: false };
            }
            let component_mine_ids = mine_arrangements_by_mine_count[mine_count]
                .choose(&mut rng)
                .unwrap();
            for &mine_tile_id in component_mine_ids {
                self.tiles[mine_tile_id] = Tile::Hidden { is_mine: true };
            }
        });

        for &unknown_tile_id in &partition.unconstrained_unknown_tile_ids {
            self.tiles[unknown_tile_id] = Tile::Hidden { is_mine: false };
        }

        let unconstrained_mine_ids = partition
            .unconstrained_unknown_tile_ids
            .choose_multiple(&mut rng, remaining_mine_count);

        for &mine_tile_id in unconstrained_mine_ids {
            self.tiles[mine_tile_id] = Tile::Hidden { is_mine: true };
        }

        true
    }

    // precondition: the tile must not actually be a mine
    fn punish(&mut self, tile_id: usize, analyzer: &mut Analyzer) -> bool {
        analyzer.update_from(self);

        if !analyzer.get_tile(tile_id).may_be_mine() {
            return false;
        }

        let mut partition = analyzer.partition();

        let find_arrangements =
            |component| analyzer.find_possible_mine_arrangements_by_mine_count(component);

        let mine_arrangements_by_mine_count_by_component = match partition
            .components
            .iter()
            .position(|component| component.unknown_tile_ids.contains(&tile_id))
        {
            None => {
                partition.unconstrained_unknown_tile_ids.swap_remove(
                    partition
                        .unconstrained_unknown_tile_ids
                        .binary_search(&tile_id)
                        .unwrap(),
                );
                if partition.known_mine_count == self.config.grid_config.mine_count {
                    return false;
                }
                // pretend the clicked tile is a mine, and try to rearrange the other mines to make it work
                partition.known_mine_count += 1;
                partition
                    .components
                    .iter()
                    .map(find_arrangements)
                    .collect_vec()
            }
            Some(i) => {
                let mut component_mine_arrangements_by_mine_count = analyzer
                    .find_possible_mine_arrangements_by_mine_count(&partition.components[i]);
                component_mine_arrangements_by_mine_count.retain(|_mine_count, arrangements| {
                    arrangements.retain(|arrangement| arrangement.binary_search(&tile_id).is_ok());
                    !arrangements.is_empty()
                });
                if component_mine_arrangements_by_mine_count.is_empty() {
                    return false;
                }
                chain!(
                    partition.components[..i].iter().map(find_arrangements),
                    [component_mine_arrangements_by_mine_count],
                    partition.components[i + 1..].iter().map(find_arrangements)
                )
                .collect_vec()
            }
        };

        if self.rearrange_mines(&partition, &mine_arrangements_by_mine_count_by_component) {
            // make sure it's a mine (in case it's unconstrained and we only pretended it was one)
            self.tiles[tile_id] = Tile::Hidden { is_mine: true };
            true
        } else {
            false
        }
    }

    // precondition: every adjacent hidden tile must not actually be a mine
    fn punish_chord(
        &mut self,
        number_tile_id: usize,
        adjacent_hidden_tile_ids: &[usize],
        analyzer: &mut Analyzer,
    ) -> bool {
        analyzer.update_from(self);

        let mine_candidates: ArrayVec<[usize; 8]> = adjacent_hidden_tile_ids
            .iter()
            .copied()
            .filter(|&id| analyzer.get_tile(id).may_be_mine())
            .collect();

        if mine_candidates.is_empty() {
            return false;
        }

        let partition = analyzer.partition();
        let i = partition
            .components
            .iter()
            .position(|component| component.number_tile_ids.contains(&number_tile_id))
            .expect("number tile should be in one of the components");

        let find_arrangements =
            |component| analyzer.find_possible_mine_arrangements_by_mine_count(component);

        let mine_arrangements_by_mine_count_by_component = {
            let mut component_mine_arrangements_by_mine_count =
                find_arrangements(&partition.components[i]);
            component_mine_arrangements_by_mine_count.retain(|_mine_count, arrangements| {
                arrangements.retain(|arrangement| {
                    mine_candidates
                        .iter()
                        .any(|tile_id| arrangement.binary_search(tile_id).is_ok())
                });
                !arrangements.is_empty()
            });
            if component_mine_arrangements_by_mine_count.is_empty() {
                return false;
            }
            chain!(
                partition.components[..i].iter().map(find_arrangements),
                [component_mine_arrangements_by_mine_count],
                partition.components[i + 1..].iter().map(find_arrangements)
            )
            .collect_vec()
        };

        self.rearrange_mines(&partition, &mine_arrangements_by_mine_count_by_component)
    }

    fn run_autopilot_if_enabled(&mut self, analyzer: &mut Analyzer) {
        if self.config.mode != GameMode::Autopilot {
            return;
        }
        let mut prev_hidden_safe_count = 0;
        while self.hidden_safe_count != prev_hidden_safe_count {
            prev_hidden_safe_count = self.hidden_safe_count;
            analyzer.update_from(self);
            for tile_id in 0..self.config.grid_config.tile_count() {
                if self.tiles[tile_id].is_revealed() || analyzer.get_tile(tile_id).may_be_mine() {
                    continue;
                }
                self.reveal_tile_unchecked(tile_id);
                if self.status.is_won() {
                    return;
                }
            }
        }
    }
}

impl Oracle for LocalGame {
    fn new(config: GameConfig, first_click_id: usize) -> Self {
        // NOTE: rayon::iter::ParallelIterator::find_map_first doesn't seem to speed this up at all
        loop {
            // this assumes the field config is not degenerate
            let protected_tile_ids = config
                .grid_config
                .iter_adjacent(first_click_id)
                .chain([first_click_id])
                .sorted();
            let mut tiles: Vec<Tile> = chain!(
                repeat_n(
                    Tile::Hidden { is_mine: true },
                    config.grid_config.mine_count,
                ),
                repeat_n(
                    Tile::Hidden { is_mine: false },
                    config.grid_config.safe_count() - protected_tile_ids.len(),
                )
            )
            .collect();
            tiles.shuffle(&mut rand::thread_rng());
            for tile_id in protected_tile_ids {
                tiles.insert(tile_id, Tile::Hidden { is_mine: false });
            }
            let mut game = Self {
                config,
                tiles: tiles.clone(),
                status: GameStatus::Ongoing,
                hidden_safe_count: config.grid_config.safe_count(),
                analyzer: None,
            };
            let mut analyzer = Analyzer::new(config);
            game.reveal_tile_unchecked(first_click_id);
            game.run_autopilot_if_enabled(&mut analyzer);
            if game.status.is_won() {
                continue;
            }
            if game.config.mode != GameMode::Autopilot {
                // this has already been done if autopilot is on
                analyzer.update_from(&game);
            }
            let game_before_first_click = Self {
                tiles,
                hidden_safe_count: config.grid_config.safe_count(),
                analyzer: Some(analyzer.clone()),
                ..game
            };
            loop {
                let safe_moves = analyzer.find_safe_moves(false);
                if safe_moves.is_empty() {
                    break;
                }
                for tile_id in safe_moves {
                    if game.tiles[tile_id].is_revealed() {
                        continue;
                    }
                    game.reveal_tile_unchecked(tile_id);
                    match game.status {
                        GameStatus::Ongoing => continue,
                        GameStatus::Won => return game_before_first_click,
                        GameStatus::Lost => {
                            unreachable!("clicking safe tile should not lead to loss")
                        }
                    }
                }
                analyzer.update_from(&game);
            }
        }
    }

    fn config(&self) -> GameConfig {
        self.config
    }

    fn adjacent_mine_count(&self, tile_id: usize) -> Option<u8> {
        self.tiles[tile_id].adjacent_mine_count()
    }

    fn iter_adjacent_mine_counts(&self) -> impl Iterator<Item = Option<u8>> + '_ {
        self.tiles.iter().map(Tile::adjacent_mine_count)
    }

    fn hidden_safe_count(&self) -> usize {
        self.hidden_safe_count
    }

    fn status(&self) -> GameStatus {
        self.status
    }

    fn is_mine(&self, tile_id: usize) -> bool {
        if self.status.is_ongoing() {
            panic!("cannot check mine: game is ongoing");
        }
        matches!(self.tiles[tile_id], Tile::Hidden { is_mine: true })
    }

    fn reveal_tile(&mut self, tile_id: usize) {
        assert!(
            self.status.is_ongoing(),
            "cannot reveal tile: game is already over"
        );
        match self.tiles[tile_id] {
            Tile::Revealed { .. } => {}
            Tile::Hidden { is_mine } => {
                if is_mine {
                    self.status = GameStatus::Lost;
                    return;
                }
                let Some(mut analyzer) = self.analyzer.take() else {
                    self.reveal_tile_unchecked(tile_id);
                    return;
                };
                if self.config.punish_guessing && self.punish(tile_id, &mut analyzer) {
                    self.status = GameStatus::Lost;
                } else {
                    self.reveal_tile_unchecked(tile_id);
                    self.run_autopilot_if_enabled(&mut analyzer);
                }
                self.analyzer = Some(analyzer);
            }
        }
    }

    fn chord(&mut self, number_tile_id: usize, adjacent_hidden_tile_ids: &[usize]) {
        for &tile_id in adjacent_hidden_tile_ids {
            match self.tiles[tile_id] {
                Tile::Revealed { .. } => panic!("cannot chord to revealed tile"),
                Tile::Hidden { is_mine } => {
                    if is_mine {
                        self.status = GameStatus::Lost;
                        return;
                    }
                }
            }
        }
        let Some(mut analyzer) = self.analyzer.take() else {
            self.chord_unchecked(adjacent_hidden_tile_ids);
            return;
        };
        if self.config.punish_guessing
            && self.punish_chord(number_tile_id, adjacent_hidden_tile_ids, &mut analyzer)
        {
            self.status = GameStatus::Lost;
        } else {
            self.chord_unchecked(adjacent_hidden_tile_ids);
            self.run_autopilot_if_enabled(&mut analyzer);
        }
        self.analyzer = Some(analyzer);
    }

    fn visualize(&self) {
        println!(
            "{}\n",
            self.tiles
                .iter()
                .chunks(self.config.grid_config.width)
                .into_iter()
                .map(|row| {
                    row.map(|&tile| match tile {
                        Tile::Hidden { is_mine } => {
                            if self.status.is_game_over() && is_mine {
                                '•'
                            } else {
                                '-'
                            }
                        }
                        Tile::Revealed {
                            adjacent_mine_count,
                        } => adjacent_mine_count_to_char(adjacent_mine_count),
                    })
                    .collect::<String>()
                })
                .join("\n")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win_all_games(config: GameConfig) {
        let trial_count = 100;
        let win_count = simulate_games::<LocalGame>(config, trial_count, true, false);
        assert_eq!(win_count, trial_count);
    }

    #[test]
    fn win_all_games_with_punishment() {
        win_all_games(GameConfig {
            grid_config: GridConfig::expert(),
            mode: GameMode::Normal,
            punish_guessing: true,
        })
    }

    #[test]
    fn win_all_games_without_punishment() {
        win_all_games(GameConfig {
            grid_config: GridConfig::expert(),
            mode: GameMode::Normal,
            punish_guessing: false,
        })
    }
}
