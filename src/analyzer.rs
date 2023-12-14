use crate::{
    bitset::BitSet,
    server::{GameConfig, GameMode, Oracle},
    utils::*,
};
use itertools::{izip, Itertools};
use serde::{Serialize, Deserialize};
use std::collections::{BTreeMap, BTreeSet};
use tinyvec::array_vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyzerTile {
    /// Hidden tile that may or may not be a mine
    Unknown,
    /// Hidden tile that is definitely safe
    KnownSafe,
    /// Hidden tile that is definitely a mine
    KnownMine,
    /// Revealed tile
    Revealed { adjacent_mine_count: u8 },
}

impl AnalyzerTile {
    pub fn is_unknown(&self) -> bool {
        matches!(self, AnalyzerTile::Unknown)
    }

    pub fn is_known_safe(&self) -> bool {
        matches!(self, AnalyzerTile::KnownSafe)
    }

    pub fn is_known_mine(&self) -> bool {
        matches!(self, AnalyzerTile::KnownMine)
    }

    pub fn is_revealed(&self) -> bool {
        matches!(self, AnalyzerTile::Revealed { .. })
    }

    pub fn may_be_mine(&self) -> bool {
        matches!(self, AnalyzerTile::Unknown | AnalyzerTile::KnownMine)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Analyzer {
    config: GameConfig,
    known_mine_count: usize,
    tiles: Vec<AnalyzerTile>,
}

#[derive(Default)]
pub struct Component {
    pub number_tile_ids: BTreeSet<usize>,
    pub unknown_tile_ids: BTreeSet<usize>,
}

#[derive(Default)]
pub struct ComponentPossibilityAnalysis {
    pub possible_safe_by_mine_count: BTreeMap<usize, BTreeSet<usize>>,
    pub possible_mines_by_mine_count: BTreeMap<usize, BTreeSet<usize>>,
}

#[derive(Default)]
pub struct Partition {
    pub components: Vec<Component>,
    pub unconstrained_unknown_tile_ids: Vec<usize>,
    pub known_mine_count: usize,
}

struct PartitionMineDistributionAnalysis {
    possible_mine_counts_by_component: Vec<BTreeSet<usize>>,
    unconstrained_implies_safe: bool,
    unconstrained_implies_mine: bool,
}

impl Analyzer {
    pub fn new(config: GameConfig) -> Self {
        Self {
            config,
            known_mine_count: 0,
            tiles: vec![AnalyzerTile::Unknown; config.grid_config.tile_count()],
        }
    }

    /// Updates the analyzer's internal state and performs some basic (mindless) analysis
    pub fn update_from(&mut self, game: &impl Oracle) {
        debug_assert!(self.config == game.config());

        for (analyzer_tile, tile) in self.tiles.iter_mut().zip(game.iter_adjacent_mine_counts()) {
            match *analyzer_tile {
                AnalyzerTile::Unknown | AnalyzerTile::KnownSafe => {
                    if let Some(adjacent_mine_count) = tile {
                        *analyzer_tile = AnalyzerTile::Revealed {
                            adjacent_mine_count,
                        }
                    }
                }
                AnalyzerTile::KnownMine => {
                    debug_assert!(
                        tile.is_none(),
                        "updated tile should remain hidden since it's known to be a mine"
                    );
                }
                AnalyzerTile::Revealed {
                    adjacent_mine_count,
                } => {
                    debug_assert!(
                        [Some(adjacent_mine_count), None].contains(&tile),
                        "updated tile should not show a conflicting number"
                    );
                }
            }
        }

        let mut whitelist = BitSet::with_capacity(self.config.grid_config.tile_count());
        let mut number_tile_queue = self
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(id, tile)| match tile {
                AnalyzerTile::Revealed {
                    adjacent_mine_count: 1..,
                } => Some(id),
                _ => None,
            })
            .collect_vec();

        while let Some(id) = number_tile_queue.pop() {
            let AnalyzerTile::Revealed {
                adjacent_mine_count: mut adjacent_remaining_mine_count,
            } = self.tiles[id]
            else {
                unreachable!("tile should be revealed since it's in the number tile queue");
            };
            let mut adjacent_unknown_tile_ids = array_vec!([usize; 8]);
            for adjacent_tile_id in self.config.grid_config.iter_adjacent(id) {
                match self.tiles[adjacent_tile_id] {
                    AnalyzerTile::KnownMine => adjacent_remaining_mine_count -= 1,
                    AnalyzerTile::Unknown => adjacent_unknown_tile_ids.push(adjacent_tile_id),
                    _ => {}
                }
            }
            if adjacent_unknown_tile_ids.is_empty() {
                continue;
            }
            let adjacent_now_known = if adjacent_remaining_mine_count == 0 {
                AnalyzerTile::KnownSafe
            } else if adjacent_remaining_mine_count == adjacent_unknown_tile_ids.len() as u8 {
                self.known_mine_count += adjacent_unknown_tile_ids.len();
                AnalyzerTile::KnownMine
            } else {
                whitelist.insert(id);
                continue;
            };
            for adjacent_unknown_tile_id in adjacent_unknown_tile_ids {
                self.tiles[adjacent_unknown_tile_id] = adjacent_now_known;
                self.filter_adjacent_tile_ids(adjacent_unknown_tile_id, AnalyzerTile::is_revealed)
                    .for_each(|number_tile_id| {
                        debug_assert_ne!(
                            whitelist.contains(number_tile_id),
                            id == number_tile_id || number_tile_queue.contains(&number_tile_id)
                        );
                        if whitelist.remove(number_tile_id) {
                            number_tile_queue.push(number_tile_id);
                        }
                    });
            }
        }

        if self.known_mine_count == self.config.grid_config.mine_count() {
            for tile in &mut self.tiles {
                if tile.is_unknown() {
                    *tile = AnalyzerTile::KnownSafe;
                }
            }
        }
    }

    pub fn get_tile(&self, tile_id: usize) -> AnalyzerTile {
        self.tiles[tile_id]
    }

    pub fn visualize(&self) {
        println!(
            "{}\n",
            self.tiles
                .iter()
                .chunks(self.config.grid_config.width())
                .into_iter()
                .map(|row| {
                    row.map(|tile| match tile {
                        AnalyzerTile::KnownSafe => ' ',
                        AnalyzerTile::KnownMine => '•',
                        AnalyzerTile::Unknown => '-',
                        AnalyzerTile::Revealed {
                            adjacent_mine_count,
                        } => adjacent_mine_count_to_char(*adjacent_mine_count),
                    })
                    .collect::<String>()
                })
                .join("\n")
        );
    }

    fn mines_valid_so_far(&self, unknown_tile_id: usize, mines_so_far: &[usize]) -> bool {
        self.config
            .grid_config
            .iter_adjacent(unknown_tile_id)
            .all(|adjacent_tile_id| {
                /*
                TODO: Maybe instead of looking at every adjacent unknown tile of every adjacent number tile, just keep track of how many mines and safe tiles are next to each number tile, and increase/decrease those numbers for the number tiles adjacent to the newly filled-in tile. This will remove the need for converting the unknown tile ids to a Vec because you'll no longer need to find the solution index for each unknown tile.
                */
                let AnalyzerTile::Revealed {
                    adjacent_mine_count,
                } = self.tiles[adjacent_tile_id]
                else {
                    return true;
                };
                let mut adjacent_hidden_count = 0;
                let mut safe_count_so_far = 0;
                let mut mine_count_so_far = 0;
                self.config
                    .grid_config
                    .iter_adjacent(adjacent_tile_id)
                    .for_each(|adjacent_tile_id| match self.tiles[adjacent_tile_id] {
                        AnalyzerTile::KnownSafe => {
                            adjacent_hidden_count += 1;
                            safe_count_so_far += 1;
                        }
                        AnalyzerTile::KnownMine => {
                            adjacent_hidden_count += 1;
                            mine_count_so_far += 1;
                        }
                        AnalyzerTile::Unknown => {
                            adjacent_hidden_count += 1;
                            if adjacent_tile_id <= unknown_tile_id {
                                if mines_so_far.binary_search(&adjacent_tile_id).is_ok() {
                                    mine_count_so_far += 1;
                                } else {
                                    safe_count_so_far += 1;
                                }
                            }
                        }
                        AnalyzerTile::Revealed { .. } => {}
                    });
                mine_count_so_far <= adjacent_mine_count
                    && safe_count_so_far + adjacent_mine_count <= adjacent_hidden_count
            })
    }

    fn analyze_component_tile_possibilities_helper(
        &self,
        mut unknown_tile_ids: impl Iterator<Item = usize> + Clone,
        possible_safe_by_mine_count: &mut BTreeMap<usize, BTreeSet<usize>>,
        possible_mines_by_mine_count: &mut BTreeMap<usize, BTreeSet<usize>>,
        safe_so_far: &mut Vec<usize>,
        mines_so_far: &mut Vec<usize>,
    ) {
        let Some(unknown_tile_id) = unknown_tile_ids.next() else {
            possible_safe_by_mine_count
                .entry(mines_so_far.len())
                .or_default()
                .extend(safe_so_far.iter().copied());
            possible_mines_by_mine_count
                .entry(mines_so_far.len())
                .or_default()
                .extend(mines_so_far.iter().copied());
            return;
        };
        safe_so_far.push(unknown_tile_id);
        if self.mines_valid_so_far(unknown_tile_id, mines_so_far) {
            self.analyze_component_tile_possibilities_helper(
                unknown_tile_ids.clone(),
                possible_safe_by_mine_count,
                possible_mines_by_mine_count,
                safe_so_far,
                mines_so_far,
            );
        }
        safe_so_far.pop();
        mines_so_far.push(unknown_tile_id);
        if self.mines_valid_so_far(unknown_tile_id, mines_so_far) {
            self.analyze_component_tile_possibilities_helper(
                unknown_tile_ids.clone(),
                possible_safe_by_mine_count,
                possible_mines_by_mine_count,
                safe_so_far,
                mines_so_far,
            );
        }
        mines_so_far.pop();
    }

    fn analyze_component_tile_possibilities(
        &self,
        component: &Component,
    ) -> ComponentPossibilityAnalysis {
        let mut analysis = ComponentPossibilityAnalysis::default();
        self.analyze_component_tile_possibilities_helper(
            component.unknown_tile_ids.iter().copied(),
            &mut analysis.possible_safe_by_mine_count,
            &mut analysis.possible_mines_by_mine_count,
            &mut Vec::new(),
            &mut Vec::new(),
        );
        analysis
    }

    fn analyze_possible_mine_distribution_helper(
        analysis: &mut PartitionMineDistributionAnalysis,
        mine_count_by_component_so_far: &mut Vec<usize>,
        possibility_analysis_by_component: &[ComponentPossibilityAnalysis],
        unconstrained_count: usize,
        remaining_mine_count: usize,
    ) {
        match possibility_analysis_by_component.split_first() {
            None => {
                for (i, &mine_count) in mine_count_by_component_so_far.iter().enumerate() {
                    analysis.possible_mine_counts_by_component[i].insert(mine_count);
                }
                if remaining_mine_count != 0 {
                    analysis.unconstrained_implies_safe = false;
                }
                if remaining_mine_count != unconstrained_count {
                    analysis.unconstrained_implies_mine = false;
                }
            }
            Some((possibility_analysis, possiblity_analysis_by_component)) => {
                for &mine_count in possibility_analysis.possible_mines_by_mine_count.keys() {
                    if mine_count > remaining_mine_count {
                        break;
                    }
                    mine_count_by_component_so_far.push(mine_count);
                    Self::analyze_possible_mine_distribution_helper(
                        analysis,
                        mine_count_by_component_so_far,
                        possiblity_analysis_by_component,
                        unconstrained_count,
                        remaining_mine_count - mine_count,
                    );
                    mine_count_by_component_so_far.pop();
                }
            }
        }
    }

    fn analyze_possible_mine_distribution(
        &self,
        partition: &Partition,
        possibility_analysis_by_component: &[ComponentPossibilityAnalysis],
    ) -> PartitionMineDistributionAnalysis {
        let mut analysis = PartitionMineDistributionAnalysis {
            possible_mine_counts_by_component: vec![BTreeSet::new(); partition.components.len()],
            unconstrained_implies_safe: true,
            unconstrained_implies_mine: true,
        };
        Self::analyze_possible_mine_distribution_helper(
            &mut analysis,
            &mut Vec::new(),
            possibility_analysis_by_component,
            partition.unconstrained_unknown_tile_ids.len(),
            self.config.grid_config.mine_count() - partition.known_mine_count,
        );
        analysis
    }

    /// If there are any safe moves, then a `Vec` containing at least one of them will be returned. If there are no safe moves (or if mindless mode is enabled and there are no trivially safe moves), then an empty `Vec` will be returned.
    ///
    /// Exception: if `exhaustive` is `true` then every safe move will be found, regardless of the game mode.
    pub fn find_safe_moves(&mut self, exhaustive: bool) -> Vec<usize> {
        /*
        Find some tiles that are safe to click, if there are any. Specifically:
        - If there are any KnownSafe tiles, then return those and do not compute anything more.
        - Else, consider each component on its own, and if you can find any safe tiles, then return them.
        - Else, consider the partition as a whole, and if you can find any safe tiles, then return them.
        - Else, return an empty Vec.
         */

        if !exhaustive || self.config.mode == GameMode::Mindless {
            // all safe moves already found (including all mindlessly safe moves)
            let known_safe_tile_ids = self
                .tiles
                .iter()
                .positions(AnalyzerTile::is_known_safe)
                .collect_vec();
            if !known_safe_tile_ids.is_empty() || self.config.mode == GameMode::Mindless {
                return known_safe_tile_ids;
            }
        }

        let partition = self.partition();

        let possibility_analysis_by_component = partition
            .components
            .iter()
            .map(|component| self.analyze_component_tile_possibilities(component))
            .collect_vec();

        let mine_distribution_analysis =
            self.analyze_possible_mine_distribution(&partition, &possibility_analysis_by_component);

        let mut safe_tile_ids = Vec::new();

        izip!(
            partition.components,
            mine_distribution_analysis.possible_mine_counts_by_component,
            possibility_analysis_by_component,
        )
        .flat_map(|(component, possible_mine_counts, possibility_analysis)| {
            let mut component_safe_tile_ids = component.unknown_tile_ids.clone();
            let mut component_mine_tile_ids = component.unknown_tile_ids;
            for mine_count in possible_mine_counts {
                for tile_id in &possibility_analysis.possible_mines_by_mine_count[&mine_count] {
                    component_safe_tile_ids.remove(tile_id);
                }
                for tile_id in &possibility_analysis.possible_safe_by_mine_count[&mine_count] {
                    component_mine_tile_ids.remove(tile_id);
                }
            }
            safe_tile_ids.extend(component_safe_tile_ids);
            component_mine_tile_ids
        })
        .chain(
            mine_distribution_analysis
                .unconstrained_implies_mine
                .then_some(&partition.unconstrained_unknown_tile_ids)
                .into_iter()
                .flatten()
                .copied(),
        )
        .for_each(|mine_tile_id| {
            self.tiles[mine_tile_id] = AnalyzerTile::KnownMine;
        });

        if mine_distribution_analysis.unconstrained_implies_safe {
            safe_tile_ids.extend(partition.unconstrained_unknown_tile_ids);
        }

        for &safe_tile_id in &safe_tile_ids {
            self.tiles[safe_tile_id] = AnalyzerTile::KnownSafe;
        }

        safe_tile_ids
    }

    fn find_possible_mine_arrangements_by_mine_count_helper(
        &self,
        mut unknown_tile_ids: impl Iterator<Item = usize> + Clone,
        mine_arrangements_by_mine_count: &mut BTreeMap<usize, Vec<Vec<usize>>>,
        mines_so_far: &mut Vec<usize>,
    ) {
        match unknown_tile_ids.next() {
            None => {
                mine_arrangements_by_mine_count
                    .entry(mines_so_far.len())
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push(mines_so_far.clone());
            }
            Some(unknown_tile_id) => {
                for is_mine in [false, true] {
                    if is_mine {
                        mines_so_far.push(unknown_tile_id);
                    }
                    if self.config.grid_config.iter_adjacent(unknown_tile_id).all(
                        |adjacent_tile_id| {
                            /*
                            TODO: Maybe instead of looking at every adjacent unknown tile of every adjacent number tile, just keep track of how many mines and safe tiles are next to each number tile, and increase/decrease those numbers for the number tiles adjacent to the newly filled-in tile. This will remove the need for converting the unknown tile ids to a Vec because you'll no longer need to find the solution index for each unknown tile.
                            */
                            let AnalyzerTile::Revealed {
                                adjacent_mine_count,
                            } = self.tiles[adjacent_tile_id]
                            else {
                                return true;
                            };
                            let mut adjacent_hidden_count = 0;
                            let mut safe_count_so_far = 0;
                            let mut mine_count_so_far = 0;
                            self.config
                                .grid_config
                                .iter_adjacent(adjacent_tile_id)
                                .for_each(|adjacent_tile_id| match self.tiles[adjacent_tile_id] {
                                    AnalyzerTile::KnownSafe => {
                                        adjacent_hidden_count += 1;
                                        safe_count_so_far += 1;
                                    }
                                    AnalyzerTile::KnownMine => {
                                        adjacent_hidden_count += 1;
                                        mine_count_so_far += 1;
                                    }
                                    AnalyzerTile::Unknown => {
                                        adjacent_hidden_count += 1;
                                        if adjacent_tile_id <= unknown_tile_id {
                                            if mines_so_far.binary_search(&adjacent_tile_id).is_ok()
                                            {
                                                mine_count_so_far += 1;
                                            } else {
                                                safe_count_so_far += 1;
                                            }
                                        }
                                    }
                                    AnalyzerTile::Revealed { .. } => {}
                                });
                            mine_count_so_far <= adjacent_mine_count
                                && safe_count_so_far + adjacent_mine_count <= adjacent_hidden_count
                        },
                    ) {
                        self.find_possible_mine_arrangements_by_mine_count_helper(
                            unknown_tile_ids.clone(),
                            mine_arrangements_by_mine_count,
                            mines_so_far,
                        );
                    }
                    if is_mine {
                        mines_so_far.pop();
                    }
                }
            }
        };
    }

    pub fn find_possible_mine_arrangements_by_mine_count(
        &self,
        component: &Component,
    ) -> BTreeMap<usize, Vec<Vec<usize>>> {
        let mut mine_arrangements_by_mine_count = BTreeMap::new();
        self.find_possible_mine_arrangements_by_mine_count_helper(
            component.unknown_tile_ids.iter().copied(),
            &mut mine_arrangements_by_mine_count,
            &mut Vec::new(),
        );
        mine_arrangements_by_mine_count
    }

    fn filter_adjacent_tile_ids<'a>(
        &'a self,
        id: usize,
        predicate: impl Fn(&AnalyzerTile) -> bool + 'a,
    ) -> impl Iterator<Item = usize> + '_ {
        self.config
            .grid_config
            .iter_adjacent(id)
            .filter(move |&adjacent_tile_id| predicate(&self.tiles[adjacent_tile_id]))
    }

    pub fn partition(&self) -> Partition {
        let mut visited_tiles = BitSet::with_capacity(self.tiles.len());

        let mut pending_number_tile_ids = Vec::new();
        let mut pending_unknown_tile_ids = Vec::new();

        let mut partition = Partition::default();

        for (id, tile) in self.tiles.iter().enumerate() {
            match tile {
                AnalyzerTile::KnownMine => partition.known_mine_count += 1,
                AnalyzerTile::Unknown if visited_tiles.insert(id) => {
                    let mut component = Component::default();
                    component.unknown_tile_ids.insert(id);
                    pending_unknown_tile_ids.push(id);
                    loop {
                        for unknown_tile_id in pending_unknown_tile_ids.drain(..) {
                            for number_tile_id in self.filter_adjacent_tile_ids(
                                unknown_tile_id,
                                AnalyzerTile::is_revealed,
                            ) {
                                if visited_tiles.insert(number_tile_id) {
                                    component.number_tile_ids.insert(number_tile_id);
                                    pending_number_tile_ids.push(number_tile_id);
                                }
                            }
                        }
                        for number_tile_id in pending_number_tile_ids.drain(..) {
                            for unknown_tile_id in self
                                .filter_adjacent_tile_ids(number_tile_id, AnalyzerTile::is_unknown)
                            {
                                if visited_tiles.insert(unknown_tile_id) {
                                    component.unknown_tile_ids.insert(unknown_tile_id);
                                    pending_unknown_tile_ids.push(unknown_tile_id);
                                }
                            }
                        }
                        if pending_unknown_tile_ids.is_empty() {
                            break;
                        }
                    }
                    if component.number_tile_ids.is_empty() {
                        partition.unconstrained_unknown_tile_ids.push(id);
                    } else {
                        partition.components.push(component);
                    }
                }
                _ => {}
            }
        }

        // for component in &mut partition.components {
        //     self.find_component_possible_mines(component);
        // }

        // partition.components.par_iter_mut().for_each(|component| {
        //     self.find_component_mine_arrangements(component);
        // });

        partition
    }
}
