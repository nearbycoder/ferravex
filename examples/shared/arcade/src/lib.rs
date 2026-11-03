//! Fixed-tick lifecycle and presentation shared by twenty independent games.
//!
//! Game models consume plain values; the host owns input, audio, storage and time.
//! ```
//! use ferravex_arcade::{Input, Outcome, TICK_DT};
//! let command = Input { primary: true, ..Input::default() };
//! assert!(command.primary);
//! assert_eq!(Outcome::default(), Outcome::Playing);
//! assert_eq!(TICK_DT, 1.0 / 60.0);
//! ```
pub use ferravex_render::prelude::*;
mod chrome;
mod driver;
mod host;
mod music;
mod paint;
mod touch;
pub use chrome::surface;
/// Styled text and vector shapes for games that need more than the helpers.
pub use ferravex_render::{shapes, text as typography};
pub use host::run;
pub use paint::{
    card, chip, darken, divider, fit, glow, keycap, lighten, measure, meter, outline, panel,
    rounded, rounded_gradient, style, text, text_centered, text_right, with_alpha, wrapped,
};

/// Logical canvas width; pointer coordinates use this space.
pub const WIDTH: f32 = 960.;
/// Logical canvas height. Games draw between y=112 and y=568.
pub const HEIGHT: f32 = 640.;
/// Duration of one executed simulation update.
pub const TICK_DT: f32 = 1. / 60.;
pub const INK: Color = Color::new(0.035, 0.05, 0.08, 1.);
pub const PAPER: Color = Color::new(0.93, 0.95, 0.96, 1.);
pub const MUTED: Color = Color::new(0.6, 0.67, 0.74, 1.);

/// A single executed tick's controls. Press edges are never repeated on catch-up.
/// `slot` is one-based (1..=5); pointer is in the logical canvas space.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub up_pressed: bool,
    pub down_pressed: bool,
    pub primary: bool,
    pub secondary: bool,
    pub tertiary: bool,
    pub click: bool,
    pub primary_down: bool,
    pub pointer: Vec2,
    pub slot: Option<usize>,
}
/// Feedback for an accepted model event; the host plays it after the update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cue {
    #[default]
    None,
    Action,
    Good,
    Bad,
    Win,
}
/// Terminal outcomes freeze updates until an explicit new run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    #[default]
    Playing,
    Won,
    Lost,
}
/// Read-only game-authored HUD copy, regenerated for presentation frames.
pub struct Hud {
    pub stage: String,
    pub objective: String,
    pub status: String,
    pub score: u32,
}
/// Static identity, default-control hints and paginated tutorial copy.
#[derive(Clone, Copy)]
pub struct Meta {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub controls: &'static str,
    pub help: &'static [&'static str],
    pub accent: Color,
}
/// Each game owns a complete campaign; the host never alters its rules.
/// See the individual game crates for executable model examples.
pub trait ArcadeGame: Sized {
    fn new() -> Self;
    fn update(&mut self, input: &Input) -> Cue;
    fn draw(&self);
    fn hud(&self) -> Hud;
    fn outcome(&self) -> Outcome;
    fn summary(&self) -> String;
    /// Read-only, legal commands for reproducible smoke sessions.
    fn bot_input(&self, tick: u64) -> Input;
    /// MIDI note-on events accepted during the most recent update. The host
    /// queries this exactly once per executed tick, never during drawing.
    /// Up to four notes may sound together; values above 127 are ignored.
    fn music_notes(&self) -> [Option<u8>; 4] {
        [None; 4]
    }
}
/// Default resizable native window. The browser supplies its own canvas size.
pub fn window(title: &str) -> Conf {
    let mut conf = ferravex_render::window_conf(title);
    conf.window_width = 1200;
    conf.window_height = 800;
    conf
}
