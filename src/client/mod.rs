use float_ord::FloatOrd;
use gloo::storage::{LocalStorage, Storage};
use itertools::Itertools;
use mindsweeper::{analyzer::Analyzer, server::*, utils::*};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use strum::{Display, EnumIter, IntoEnumIterator};
use tinyvec::array_vec;
use web_sys::{Event, HtmlDialogElement, HtmlInputElement, HtmlSelectElement, MouseEvent};
use yew::{html::Scope, prelude::*};

mod flag;
use flag::*;

mod timer;
use timer::*;

#[derive(Debug)]
pub enum Msg {
    TileMouseEvent {
        tile_id: usize,
        button: i16,
        buttons: u16,
    },
    ShowDialog,
    CloseDialog,
    NewGame,
    SetNumbersStyle(NumbersStyle),
    SetGridConfig(GridConfig),
    SetGameMode(GameMode),
    SetPunishGuessing(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default, EnumIter, Display)]
pub enum NumbersStyle {
    #[default]
    Digits,
    Dots,
}

impl NumbersStyle {
    fn render(&self, adjacent_mine_count: u8) -> char {
        match self {
            NumbersStyle::Digits => adjacent_mine_count_to_char(adjacent_mine_count),
            NumbersStyle::Dots => '•',
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Theme {
    #[serde(default)]
    numbers_style: NumbersStyle,
}

static DEFAULT_GAME_CONFIG: GameConfig = GameConfig {
    grid_config: GridConfig::beginner(),
    mode: GameMode::Normal,
    punish_guessing: true,
};

static DEFAULT_THEME: Theme = Theme {
    numbers_style: NumbersStyle::Digits,
};

pub struct Client<Game: Oracle> {
    dialog_ref: NodeRef,
    should_show_dialog: bool,
    game_config: GameConfig,
    theme: Theme,
    prepared_game: Option<PreparedGame<Game>>,
    game: Option<Game>,
    flags: FlagStore,
    last_revealed: Vec<usize>,
}

mod storage_keys {
    pub static GAME_CONFIG: &str = "game_config";
    pub static THEME: &str = "theme";
    pub static CLOSED_DIALOG: &str = "closed_dialog";
    pub static BEST_TIMES: &str = "best_times";
}

struct PreparedGame<Game: Oracle> {
    game: Game,
    first_click_id: usize,
}

impl<Game: Oracle> PreparedGame<Game> {
    fn matches(&self, game_config: GameConfig, first_click_id: usize) -> bool {
        let self_game_config = self.game.config();
        self_game_config.mode == game_config.mode
            && self_game_config.punish_guessing == game_config.punish_guessing
            && self.first_click_id == first_click_id
    }
}

impl<Game: Oracle> Client<Game> {
    fn get_dialog(&self) -> HtmlDialogElement {
        self.dialog_ref.cast::<HtmlDialogElement>().unwrap()
    }

    fn show_dialog(&self) {
        self.get_dialog().show_modal().ok();
    }

    fn save_game_config(&self) {
        LocalStorage::set(storage_keys::GAME_CONFIG, self.game_config).ok();
    }

    fn save_theme(&self) {
        LocalStorage::set(storage_keys::THEME, self.theme).ok();
    }

    fn close_dialog(&self) {
        self.save_game_config();
        self.save_theme();
        LocalStorage::set(storage_keys::CLOSED_DIALOG, true).ok();
        self.get_dialog().close();
    }

    /// Reduces first-click latency by pre-generating a game on mousedown
    fn prepare_for_click(&mut self, tile_id: usize) {
        if !self
            .game
            .as_ref()
            .is_some_and(|game| game.status().is_ongoing())
            && !self
                .prepared_game
                .as_ref()
                .is_some_and(|prepared| prepared.matches(self.game_config, tile_id))
        {
            // TODO: perhaps use Yew agents to do this concurrently and not freeze the game if it takes long
            self.prepared_game = Some(PreparedGame {
                game: Game::new(self.game_config, tile_id),
                first_click_id: tile_id,
            });
        }
    }

    fn click(&mut self, tile_id: usize) {
        if self.flags.contains(tile_id) {
            return;
        }
        let game = self
            .game
            .get_or_insert_with(|| match self.prepared_game.take() {
                Some(prepared) if prepared.matches(self.game_config, tile_id) => prepared.game,
                _ => Game::new(self.game_config, tile_id),
            });
        if game.status().is_game_over() {
            return;
        }
        self.last_revealed.clear();
        match game.adjacent_mine_count(tile_id) {
            Some(adjacent_mine_count) => {
                let mut adjacent_flag_count = 0;
                let mut adjacent_unknown_tile_ids = array_vec!([usize; 8]);
                for adjacent_tile_id in self.game_config.grid_config.iter_adjacent(tile_id) {
                    if self.flags.contains(adjacent_tile_id) {
                        adjacent_flag_count += 1;
                    } else if game.adjacent_mine_count(adjacent_tile_id).is_none() {
                        adjacent_unknown_tile_ids.push(adjacent_tile_id)
                    }
                }
                if adjacent_mine_count != adjacent_flag_count {
                    return;
                }
                game.chord(tile_id, &adjacent_unknown_tile_ids);
                for hidden_tile_id in adjacent_unknown_tile_ids {
                    self.last_revealed.push(hidden_tile_id);
                }
            }
            None => {
                game.reveal_tile(tile_id);
                self.last_revealed.push(tile_id);
            }
        }
        for (id, adjacent_mine_count) in game.iter_adjacent_mine_counts().enumerate() {
            if adjacent_mine_count.is_some() {
                // revealed, so flag was wrong
                self.flags.remove(id);
            }
        }
        if game.status().is_ongoing() && self.game_config.mode == GameMode::Autopilot {
            for (id, tile) in game.iter_adjacent_mine_counts().enumerate() {
                let Some(number) = tile else { continue };
                let adjacent_hidden_tile_ids = game
                    .config()
                    .grid_config
                    .iter_adjacent(id)
                    .filter(|&adjacent_tile_id| {
                        game.adjacent_mine_count(adjacent_tile_id).is_none()
                    })
                    .collect_vec();
                if adjacent_hidden_tile_ids.len() == number as usize {
                    for adjacent_tile_id in adjacent_hidden_tile_ids {
                        self.flags.insert_permanent(adjacent_tile_id);
                    }
                }
            }
        }
    }

    fn right_click(&mut self, tile_id: usize) {
        if let Some(game) = &self.game {
            if game.adjacent_mine_count(tile_id).is_none() {
                self.flags.toggle(tile_id);
            }
        }
    }

    fn new_game(&mut self) {
        self.game = None;
        self.flags.clear();
        self.last_revealed.clear();
    }

    fn view_tile(&self, tile_id: usize, analyzer: Option<&Analyzer>, scope: &Scope<Self>) -> Html {
        const FLAG_SYMBOL: char = '⚑';
        const MINE_SYMBOL: char = '💣';
        let mut tile_classes = classes!("tile");
        let mut tile_contents = String::new();
        let mut tooltip = None;
        if let Some(game) = self.game.as_ref() {
            if let Some(adjacent_mine_count) = game.adjacent_mine_count(tile_id) {
                tile_classes.push("revealed");
                if adjacent_mine_count > 0 {
                    tile_classes.push(format!("number-{adjacent_mine_count}"));
                    tile_contents.push(self.theme.numbers_style.render(adjacent_mine_count));
                }
            } else if game.status().is_won() {
                tile_contents.push(FLAG_SYMBOL);
                tile_classes.push("bg-green");
            } else if game.status().is_lost() {
                let Some(analyzer) = analyzer else {
                    panic!("expected analyzer");
                };
                let analyzer_tile = analyzer.get_tile(tile_id);
                if let Some(flag) = self.flags.get(tile_id) {
                    tile_contents.push(FLAG_SYMBOL);
                    if game.config().mode == GameMode::Autopilot && flag.is_tentative() {
                        tile_classes.push("text-faded");
                    }
                    if analyzer_tile.is_known_mine() {
                        tooltip = Some("This was definitely a mine, so you were correct to flag it.");
                        tile_classes.push("bg-green");
                    } else if analyzer_tile.is_known_safe() {
                        tooltip = Some("This was definitely safe, so you were wrong to flag it.");
                        tile_classes.push("bg-red");
                    } else if game.is_mine(tile_id) {
                        tooltip = Some("This happened to be a mine, but it could've been safe. You were wrong to flag it, and you would've been wrong to reveal it too.");
                        tile_classes.push("bg-yellow");
                    } else {
                        tooltip = Some("This happened to be safe, but it could've been a mine. You were wrong to flag it, and you would've been wrong to reveal it too.");
                        tile_classes.push("bg-orange");
                    }
                } else if game.is_mine(tile_id) {
                    tile_contents.push(MINE_SYMBOL);
                    if analyzer_tile.is_unknown() {
                        tile_classes.push("text-faded");
                        if self.last_revealed.contains(&tile_id) {
                            tooltip = Some("This may or may not have been a mine, so you were wrong to reveal it. In this case, it was in fact a mine, so you lost.");
                            tile_classes.push("bg-orange");
                        } else {
                            tooltip = Some("This may or may not have been a mine, and in this case it was.");
                        }
                    } else if self.last_revealed.contains(&tile_id) {
                        tooltip = Some("This was definitely a mine, and you revealed it, so you lost.");
                        tile_classes.push("bg-red");
                    } else {
                        tooltip = Some("This was definitely a mine, so you could've safely flagged it.");
                    }
                } else if analyzer_tile.is_known_safe() {
                    tooltip = Some("This was definitely safe, so you could've safely revealed it.");
                    tile_classes.push("bg-blue");
                } else {
                    tooltip = Some("This may or may not have been a mine, and in this case it was not.");
                }
            } else if let Some(flag) = self.flags.get(tile_id) {
                tile_contents.push(FLAG_SYMBOL);
                if game.config().mode == GameMode::Autopilot && flag.is_tentative() {
                    tile_classes.push("text-faded");
                }
            }
        }

        html! {
            <td key={tile_id}
                title={tooltip}
                class={tile_classes}
                onmousedown={scope.callback(move |e: MouseEvent| Msg::TileMouseEvent { tile_id, button: e.button(), buttons: e.buttons() })}
                onmouseup={scope.callback(move |e: MouseEvent| Msg::TileMouseEvent { tile_id, button: e.button(), buttons: e.buttons() })}>
                <div>
                    { tile_contents }
                </div>
            </td>
        }
    }

    fn remaining_flag_count(&self) -> isize {
        match &self.game {
            Some(game) if game.status().is_won() => 0,
            _ => self.game_config.grid_config.mine_count() as isize - self.flags.len() as isize,
        }
    }
}

impl<Game: Oracle> Component for Client<Game> {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let stored_game_config = LocalStorage::get(storage_keys::GAME_CONFIG);
        Self {
            dialog_ref: NodeRef::default(),
            should_show_dialog: stored_game_config.is_err()
                || !LocalStorage::get::<bool>(storage_keys::CLOSED_DIALOG).unwrap_or_default(),
            game_config: stored_game_config.unwrap_or(DEFAULT_GAME_CONFIG),
            theme: LocalStorage::get(storage_keys::THEME).unwrap_or(DEFAULT_THEME),
            prepared_game: None,
            game: None,
            flags: FlagStore::new(),
            last_revealed: vec![],
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::TileMouseEvent {
                tile_id,
                button,
                buttons,
            } => {
                // https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent/buttons
                // https://developer.mozilla.org/en-US/docs/Web/API/MouseEvent/button
                let changed_button = match button {
                    1 => 4,
                    2 => 2,
                    _ => 1 << button,
                };
                if buttons & changed_button != 0 {
                    // mouse down
                    match &self.game {
                        Some(game) if game.status().is_game_over() => {
                            if buttons == 3 {
                                self.new_game();
                            }
                        }
                        _ => {
                            if changed_button == 1 {
                                self.prepare_for_click(tile_id);
                            } else if changed_button == 2 {
                                self.right_click(tile_id);
                            }
                        }
                    }
                } else if changed_button == 1 {
                    // mouse up
                    self.click(tile_id);
                }
            }
            Msg::ShowDialog => self.show_dialog(),
            Msg::CloseDialog => self.close_dialog(),
            Msg::NewGame => self.new_game(),
            Msg::SetNumbersStyle(style) => {
                self.theme.numbers_style = style;
                self.save_theme();
            }
            Msg::SetGridConfig(config) => {
                self.game_config.grid_config = config;
                self.save_game_config();
                self.new_game();
            }
            Msg::SetGameMode(mode) => {
                self.game_config.mode = mode;
                self.save_game_config();
                self.new_game();
            }
            Msg::SetPunishGuessing(value) => {
                self.game_config.punish_guessing = value;
                self.save_game_config();
                self.new_game();
            }
        }
        true
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render && self.should_show_dialog {
            self.show_dialog();
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let scope = ctx.link();
        let analyzer = self.game.as_ref().and_then(|game| {
            game.status().is_game_over().then(|| {
                let mut analyzer = Analyzer::new(self.game_config);
                analyzer.update_from(game);
                analyzer.find_safe_moves(true);
                analyzer
            })
        });
        let stop_propagation = |e: MouseEvent| e.stop_propagation();
        html! {<>
            <dialog ref={self.dialog_ref.clone()} onclick={scope.callback(|_| Msg::CloseDialog)}>
                <div onclick={stop_propagation} oncontextmenu={stop_propagation}>
                    <h2>
                        { "Mindsweeper — a "}
                        <a href="https://github.com/alexbuz/mindsweeper/" target="_blank">
                            { "principled" }
                        </a>
                        { " take on minesweeper" }
                    </h2>
                    <p>
                        { "Begin by clicking any tile, and a patch of safe tiles will be revealed. When a revealed tile displays a number, that indicates how many of its adjacent tiles (including diagonals) contain mines, which you must avoid revealing. The total number of safe tiles that remain to be revealed is shown at the top right of the minefield. Each time you reveal a safe tile, you'll gain information that helps you find more safe tiles. To win, you must reveal every safe tile without revealing a single mine." }
                    </p>
                    <p>
                        { "You may find it helpful to place flags (by right-clicking) to mark the tiles that you know contain mines. The number of unplaced flags (which is initially equal to the total number of mines) is shown at the top left of the minefield. Flagging is entirely optional, but it enables you to chord, where if you click a number tile whose adjacent mines are all flagged, you'll instantly reveal the rest of its adjacent tiles. Note, though, that if you mistakenly flag a safe tile, then chording may cause a mine to be revealed." }
                    </p>
                    <p>
                        { "After a win or loss, easily start a new game by clicking a tile with both mouse buttons simultaneously, and then release to immediately reveal that tile in the new game." }
                    </p>
                    <h2>
                        { "Gameplay" }
                    </h2>
                    <p class={if self.game.as_ref().map(Game::status).is_some_and(GameStatus::is_ongoing) { "text-red" } else { "hidden" }}>
                        { "Warning: changing gameplay options will start a new game." }
                    </p>
                    <ul>
                        <li>
                            <label>
                                { "Grid: " }
                                <select name="grid" onchange={scope.callback(|e: Event| {
                                    Msg::SetGridConfig(
                                        serde_json::from_str(
                                            &e.target_unchecked_into::<HtmlSelectElement>().value()
                                        )
                                        .unwrap(),
                                    )
                                })}> {
                                    for GridConfig::standard_configs()
                                        .into_iter()
                                        .map(|config| (FloatOrd(config.mine_density()), config))
                                        .chain([
                                            (
                                                FloatOrd(DEFAULT_GAME_CONFIG.grid_config.mine_density()),
                                                DEFAULT_GAME_CONFIG.grid_config,
                                            ),
                                            (
                                                FloatOrd(self.game_config.grid_config.mine_density()),
                                                self.game_config.grid_config,
                                            ),
                                        ])
                                        .collect::<BTreeMap<FloatOrd<f64>, GridConfig>>()
                                        .into_values()
                                        .map(|config| html! {
                                            <option value={serde_json::to_string(&config).unwrap()}
                                                    selected={config == self.game_config.grid_config}>
                                                { config.to_string() }
                                            </option>
                                        })
                                    } </select>
                            </label>
                        </li>
                        <li>
                            { "Mode: "}
                            <label>
                                <input
                                    type="radio"
                                    name="mode"
                                    onclick={scope.callback(|_| Msg::SetGameMode(GameMode::Normal))}
                                    checked={self.game_config.mode == GameMode::Normal} />
                                <span> { "Normal " } </span>
                            </label>
                            <label>
                                <input
                                    type="radio"
                                    name="mode"
                                    onclick={scope.callback(|_| Msg::SetGameMode(GameMode::Autopilot))}
                                    checked={self.game_config.mode == GameMode::Autopilot} />
                                { "Autopilot " }
                            </label>
                            <label>
                                <input
                                    type="radio"
                                    name="mode"
                                    onclick={scope.callback(|_| Msg::SetGameMode(GameMode::Mindless))}
                                    checked={self.game_config.mode == GameMode::Mindless} />
                                { "Mindless " }
                            </label>
                            <ul>
                                <li> { "Autopilot mode auto-flags tiles that are clearly mines and auto-reveals tiles that are clearly safe, effectively distilling the game down to its most challenging aspects." } </li>
                                <li> { "Mindless mode does the opposite, ensuring that the game is easy from start to finish." } </li>
                            </ul>
                        </li>
                        <li>
                            <label>
                                { "Punish guessing: " }
                                <input
                                    type="checkbox"
                                    name="punish_guessing"
                                    checked={self.game_config.punish_guessing}
                                    onchange={scope.callback(|e: Event| {
                                        Msg::SetPunishGuessing(
                                            e.target_unchecked_into::<HtmlInputElement>().checked()
                                        )
                                    })} />
                            </label>
                            <ul>
                                <li>
                                    { "If you reveal a tile (after your first click) that " }
                                    <em> { "can" } </em>
                                    { " contain a mine, then this ensures that it " }
                                    <em> { "does" } </em>
                                    { " contain a mine, effectively removing all luck from the game. Highly recommended." }
                                </li>
                            </ul>
                        </li>
                    </ul>
                    <h2>
                        { "Theme" }
                    </h2>
                    <ul>
                        <li>
                            <label>
                                { "Numbers Style: " }
                                <select name="numbers_style" onchange={scope.callback(|e: Event| {
                                    Msg::SetNumbersStyle(
                                        serde_json::from_str(
                                            &e.target_unchecked_into::<HtmlSelectElement>().value()
                                        )
                                        .unwrap(),
                                    )
                                })}> {
                                    for NumbersStyle::iter()
                                        .map(|style| html! {
                                            <option value={serde_json::to_string(&style).unwrap()}
                                                    selected={style == self.theme.numbers_style}>
                                                { style.to_string() }
                                            </option>
                                        })
                                    } </select>
                            </label>
                        </li>
                    </ul>
                    <form method="dialog">
                        <button id="close-dialog" onclick={scope.callback(|_| Msg::CloseDialog)}> { "✕" }</button>
                    </form>
                </div>
            </dialog>
            <table>
                <thead>
                    <tr>
                        <td colspan={self.game_config.grid_config.width().to_string()}>
                            <div>
                                <button onclick={scope.callback(|_| Msg::ShowDialog)}>
                                    { "Options & Info" }
                                </button>
                                <button onclick={scope.callback(|_| Msg::NewGame)}
                                        disabled={self.game.is_none()}>
                                    { "New Game" }
                                </button>
                            </div>
                        </td>
                    </tr>
                    <tr>
                        <td colspan={self.game_config.grid_config.width().to_string()}>
                            <div>
                                <span class={self.remaining_flag_count().is_negative().then_some("text-red")}>
                                    { "⚑: " } { self.remaining_flag_count() }
                                </span>
                                <Timer game_config={self.game_config} timer_mode={
                                    match self.game.as_ref().map(Game::status) {
                                        None => TimerMode::Reset,
                                        Some(GameStatus::Ongoing) => TimerMode::Running,
                                        Some(GameStatus::Won) => TimerMode::Stopped { won_game: true },
                                        Some(GameStatus::Lost) => TimerMode::Stopped { won_game: false },
                                    }
                                }/>
                                <span>
                                    { "Safe: " }
                                    { self.game
                                        .as_ref()
                                        .map_or_else(
                                            || self.game_config.grid_config.safe_count(),
                                            Game::hidden_safe_count
                                        )
                                    }
                                </span>
                            </div>
                        </td>
                    </tr>
                </thead>
                <tbody class={classes!(
                    self.game_config.punish_guessing.then_some("punish-guessing"),
                    match self.game_config.mode {
                        GameMode::Normal => None,
                        GameMode::Autopilot => Some("autopilot"),
                        GameMode::Mindless => Some("mindless"),
                    }
                )}> {
                    for (0..self.game_config.grid_config.tile_count())
                        .chunks(self.game_config.grid_config.width())
                        .into_iter()
                        .map(|row| html! {
                            <tr>
                                { for row.map(|tile_id| self.view_tile(tile_id, analyzer.as_ref(), scope)) }
                            </tr>
                        })
                } </tbody>
            </table>
        </>}
    }
}
