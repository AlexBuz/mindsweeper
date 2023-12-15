use super::storage_keys;
use gloo::{
    storage::{LocalStorage, Storage},
    timers::callback::Interval,
};
use itertools::Itertools;
use js_sys::Date;
use mindsweeper::server::GameConfig;
use std::{collections::BTreeMap, fmt};
use yew::prelude::*;

#[derive(Debug, PartialEq)]
pub enum TimerMode {
    Reset,
    Running,
    Stopped { won_game: bool },
}

#[derive(Debug, PartialEq, Properties)]
pub struct TimerProps {
    pub game_config: GameConfig,
    pub timer_mode: TimerMode,
}

pub struct Timer {
    start_date: Option<Date>,
    stop_date: Option<Date>,
    interval: Option<Interval>,
    best_times: BTreeMap<GameConfig, f64>,
}

pub enum TimerMsg {
    Tick,
}

impl Timer {
    fn elapsed_secs(&self) -> f64 {
        let elapsed_ms = match (&self.start_date, &self.stop_date) {
            (Some(start_date), None) => Date::new_0().get_time() - start_date.get_time(),
            (Some(start_date), Some(stop_date)) => stop_date.get_time() - start_date.get_time(),
            _ => 0.0,
        };
        elapsed_ms / 1000.0
    }
}

struct TimerElapsed(f64);

impl fmt::Display for TimerElapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cs = (self.0 % 1.0 * 100.0) as u64;
        let s = (self.0 % 60.0) as u64;
        let mut m = (self.0 / 60.0) as u64;

        let h = m / 60;
        m %= 60;

        if h > 0 {
            write!(f, "{h:02}:")?;
        }

        write!(f, "{m:02}:{s:02}.{cs:02}")
    }
}

impl Component for Timer {
    type Message = TimerMsg;
    type Properties = TimerProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            start_date: None,
            stop_date: None,
            interval: None,
            best_times: LocalStorage::get::<Vec<_>>(storage_keys::BEST_TIMES)
                .unwrap_or_default()
                .into_iter()
                .collect(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let new_props = ctx.props();
        if old_props.timer_mode == new_props.timer_mode {
            return new_props.timer_mode == TimerMode::Running
                || old_props.game_config != new_props.game_config;
        }
        match new_props.timer_mode {
            TimerMode::Reset => {
                self.start_date = None;
                self.stop_date = None;
                self.interval.take().map(Interval::cancel);
            }
            TimerMode::Running => {
                self.start_date = Some(Date::new_0());
                self.interval = Some(Interval::new(0, {
                    let scope = ctx.link().clone();
                    move || scope.send_message(TimerMsg::Tick)
                }));
            }
            TimerMode::Stopped { won_game } => {
                self.stop_date = Some(Date::new_0());
                self.interval.take().map(Interval::cancel);
                if won_game {
                    let time = self.elapsed_secs();
                    if self
                        .best_times
                        .get(&new_props.game_config)
                        .map_or(true, |&best| time < best)
                    {
                        self.best_times.insert(new_props.game_config, time);
                        LocalStorage::set(
                            storage_keys::BEST_TIMES,
                            self.best_times.iter().collect_vec(),
                        )
                        .unwrap_or_default();
                    }
                }
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let best = self.best_times.get(&props.game_config).copied();
        let mut timer_classes = classes!("timer");
        let content = if let TimerMode::Reset = props.timer_mode {
            match best {
                Some(best) => format!("Best: {}", TimerElapsed(best)),
                None => String::from("Best: N/A"),
            }
        } else {
            let time = self.elapsed_secs();
            if let TimerMode::Stopped { won_game: true } = props.timer_mode {
                if best == Some(time) {
                    timer_classes.push("bg-green");
                }
            }
            format!("Time: {}", TimerElapsed(time))
        };
        html! {
            <span class={timer_classes}>
                { content }
            </span>
        }
    }
}
