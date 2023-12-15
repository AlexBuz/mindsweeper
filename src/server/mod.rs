use crate::{analyzer::Analyzer, utils::*};
use itertools::Itertools;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub mod local;

#[derive(Deserialize)]
struct GridConfigValidator {
    width: usize,
    height: usize,
    mine_count: usize,
}

#[derive(Debug, Error)]
pub enum GridConfigValidationError {
    #[error("degenerate grid")]
    DegenerateGrid,
}

impl TryFrom<GridConfigValidator> for GridConfig {
    type Error = GridConfigValidationError;
    fn try_from(shadow: GridConfigValidator) -> Result<Self, Self::Error> {
        let GridConfigValidator {
            width,
            height,
            mine_count,
        } = shadow;
        if width < 4 || height < 3 || mine_count > width * height - 9 {
            return Err(GridConfigValidationError::DegenerateGrid);
        }
        Ok(GridConfig {
            width,
            height,
            mine_count,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "GridConfigValidator")]
pub struct GridConfig {
    width: usize,
    height: usize,
    mine_count: usize,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self::beginner()
    }
}

impl fmt::Display for GridConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[allow(clippy::match_single_binding)] // false positive
        match format_args!(
            "{}×{} with {} mines",
            self.height, self.width, self.mine_count
        ) {
            description => {
                let name = match (self.width, self.height, self.mine_count) {
                    (9, 9, 10) => "Beginner",
                    (16, 16, 40) => "Intermediate",
                    (30, 16, 99) => "Expert",
                    _ => return f.write_fmt(description),
                };
                f.write_fmt(format_args!("{name} ({description})"))
            }
        }
    }
}

impl GridConfig {
    pub fn new(
        width: usize,
        height: usize,
        mine_count: usize,
    ) -> Result<Self, GridConfigValidationError> {
        // a field config is defined to be valid iff its dimensions are at least 4x4 and for every tile in the field, there exists a mine arrangement where no mines are adjacent to that tile and where that tile is a suitable first click (either winning the game immediately or leading to a game that is solvable without guessing)
        GridConfig::try_from(GridConfigValidator {
            width,
            height,
            mine_count,
        })
    }

    pub const fn width(self) -> usize {
        self.width
    }

    pub const fn height(self) -> usize {
        self.height
    }

    pub const fn mine_count(self) -> usize {
        self.mine_count
    }

    pub const fn beginner() -> Self {
        Self {
            width: 9,
            height: 9,
            mine_count: 10,
        }
    }

    pub const fn intermediate() -> Self {
        Self {
            width: 16,
            height: 16,
            mine_count: 40,
        }
    }

    pub const fn expert() -> Self {
        Self {
            width: 30,
            height: 16,
            mine_count: 99,
        }
    }

    pub const fn standard_configs() -> impl IntoIterator<Item = Self> {
        [Self::beginner(), Self::intermediate(), Self::expert()]
    }

    pub const fn tile_count(self) -> usize {
        self.width * self.height
    }

    pub const fn safe_count(self) -> usize {
        self.tile_count() - self.mine_count
    }

    pub fn mine_density(self) -> f64 {
        self.mine_count as f64 / self.tile_count() as f64
    }

    pub fn iter_adjacent(self, id: usize) -> impl Iterator<Item = usize> {
        let row = id / self.width;
        let col = id % self.width;

        let can_go_left = col > 0;
        let can_go_right = col < self.width - 1;
        let can_go_up = row > 0;
        let can_go_down = row < self.height - 1;

        [
            (can_go_up && can_go_left, id.wrapping_sub(self.width + 1)),
            (can_go_up, id.wrapping_sub(self.width)),
            (can_go_up && can_go_right, id.wrapping_sub(self.width - 1)),
            (can_go_left, id.wrapping_sub(1)),
            (can_go_right, id + 1),
            (can_go_down && can_go_left, id + self.width - 1),
            (can_go_down, id + self.width),
            (can_go_down && can_go_right, id + self.width + 1),
        ]
        .into_iter()
        .filter_map(|(valid, id)| valid.then_some(id))
    }

    pub fn random_tile_id(self) -> usize {
        rand::thread_rng().gen_range(0..self.tile_count())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GameStatus {
    Ongoing,
    Won,
    Lost,
}

impl GameStatus {
    pub fn is_ongoing(self) -> bool {
        matches!(self, GameStatus::Ongoing)
    }

    pub fn is_won(self) -> bool {
        matches!(self, GameStatus::Won)
    }

    pub fn is_lost(self) -> bool {
        matches!(self, GameStatus::Lost)
    }

    pub fn is_game_over(self) -> bool {
        self.is_won() || self.is_lost()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GameMode {
    #[default]
    Normal,
    Mindless,
    Autopilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GameConfig {
    pub grid_config: GridConfig,
    pub mode: GameMode,
    pub punish_guessing: bool,
    pub count_flags: bool,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            grid_config: Default::default(),
            mode: Default::default(),
            punish_guessing: true,
            count_flags: false,
        }
    }
}

pub trait Oracle: Serialize + for<'a> Deserialize<'a> + 'static {
    fn new(config: GameConfig, first_click_id: usize) -> Self;

    fn config(&self) -> GameConfig;

    fn adjacent_mine_count(&self, tile_id: usize) -> Option<u8>;

    fn iter_adjacent_mine_counts(&self) -> impl Iterator<Item = Option<u8>> + '_;

    fn hidden_safe_count(&self) -> usize;

    fn status(&self) -> GameStatus;

    /// Note: this function panics if the game is ongoing
    fn is_mine(&self, tile_id: usize) -> bool;

    fn reveal_tile(&mut self, tile_id: usize);

    fn chord(&mut self, number_tile_id: usize, adjacent_hidden_tile_ids: &[usize]);

    fn visualize(&self) {
        println!(
            "{}\n",
            self.iter_adjacent_mine_counts()
                .chunks(self.config().grid_config.width)
                .into_iter()
                .map(|row| {
                    row.map(|tile| tile.map_or('-', adjacent_mine_count_to_char))
                        .collect::<String>()
                })
                .join("\n")
        );
    }
}

pub fn simulate_games<Game: Oracle>(
    config: GameConfig,
    trial_count: usize,
    should_visualize: bool,
    just_generate: bool,
) -> usize {
    // let win_count = rayon::iter::repeatn((), trial_count)
    let win_count = itertools::repeat_n((), trial_count)
        .filter(|_| {
            let first_click_id = config.grid_config.random_tile_id();
            let mut game = Game::new(config, first_click_id);
            if just_generate {
                std::hint::black_box(&mut game);
                return true;
            }
            game.reveal_tile(first_click_id);
            let mut analyzer = Analyzer::new(config);
            loop {
                match game.status() {
                    GameStatus::Ongoing => {
                        analyzer.update_from(&game);
                        let safe_moves = analyzer.find_safe_moves(false);
                        debug_assert!(!safe_moves.is_empty());
                        for tile_id in safe_moves {
                            game.reveal_tile(tile_id);
                            if game.status().is_game_over() {
                                break;
                            }
                        }
                    }
                    GameStatus::Won => {
                        // assert!(game.hidden_safe_count() == 0);
                        if should_visualize {
                            game.visualize();
                        }
                        return true;
                    }
                    GameStatus::Lost => {
                        if should_visualize {
                            game.visualize();
                        }
                        return false;
                    }
                }
            }
        })
        .count();
    println!("won {win_count}/{trial_count}");
    win_count
}
