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
pub use chrome::{grouped, surface};
/// The player's motion and flash preferences, for effects a game draws
/// itself (pulsing danger glows, idle motion). `Juice` already honors them.
pub use ferravex_render::preferences::feedback_policy;
/// The engine's pictograms and presentation effects. Prefer an icon, a meter
/// or an effect over a sentence: see `docs/VISUAL_LANGUAGE.md`.
pub use ferravex_render::{
    icons::{self, Icon, IconStyle},
    juice::{self, Juice, bob, pulse},
};
/// Styled text and vector shapes for games that need more than the helpers.
pub use ferravex_render::{shapes, text as typography};
pub use host::{clock, pointer, run};
pub use paint::{
    card, chip, chip_width, darken, divider, fit, glow, icon, icon_badge, icon_chip,
    icon_chip_width, icon_value, keycap, keycap_faded, lighten, measure, meter, outline, panel,
    pips, ring_meter, rounded, rounded_gradient, style, text, text_centered, text_right,
    with_alpha, wrapped,
};

/// Logical canvas width; pointer coordinates use this space.
pub const WIDTH: f32 = 960.;
/// Logical canvas height. Games draw between y=112 and y=568.
pub const HEIGHT: f32 = 640.;
/// Duration of one executed simulation update.
pub const TICK_DT: f32 = 1. / 60.;
/// The header strip handed to [`ArcadeGame::draw_objective`]; effects aimed
/// at a game's objective tokens can target it.
pub const OBJECTIVE_AREA: Rect = Rect {
    x: 22.,
    y: 64.,
    w: 916.,
    h: 34.,
};
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
    /// Campaign position as `(current, total)` for the header's progress
    /// pips, where `current` is the one-based stage being played (a finished
    /// campaign may report `total`). `None` shows only the stage chip.
    ///
    /// ```
    /// # use ferravex_arcade::*;
    /// # struct G(u32);
    /// # impl ArcadeGame for G {
    /// #     fn new() -> Self { G(2) }
    /// #     fn update(&mut self, _: &Input) -> Cue { Cue::None }
    /// #     fn draw(&self) {}
    /// #     fn hud(&self) -> Hud { Hud { stage: String::new(), objective: String::new(), status: String::new(), score: 0 } }
    /// #     fn outcome(&self) -> Outcome { Outcome::Playing }
    /// #     fn summary(&self) -> String { String::new() }
    /// #     fn bot_input(&self, _: u64) -> Input { Input::default() }
    /// fn progress(&self) -> Option<(u32, u32)> {
    ///     Some((self.0, 6)) // floor 2 of 6
    /// }
    /// # }
    /// assert_eq!(G::new().progress(), Some((2, 6)));
    /// ```
    fn progress(&self) -> Option<(u32, u32)> {
        None
    }
    /// Request presentation effects for the tick that just executed, in the
    /// game's logical canvas units: bursts where a crop was harvested, a coin
    /// popup over the shop, shake when the hull is hit. The host calls this
    /// exactly once after every executed update (after [`Self::music_notes`])
    /// with that update's cue. Effects never feed back into the rules; derive
    /// them only from events the update accepted.
    fn effects(&self, cue: Cue, fx: &mut Juice) {
        let _ = (cue, fx);
    }
    /// Draw the current objective visually (icons, counts, mini meters) inside
    /// `area`, the header row under the title. Return `true` when drawn; the
    /// host then omits the objective text (it still publishes it for status
    /// readers). The default draws nothing and returns `false`.
    fn draw_objective(&self, area: Rect) -> bool {
        let _ = area;
        false
    }
    /// Draw the ending's key results (icons and numbers rather than prose)
    /// inside `area` of the results card. Return `true` when drawn; otherwise
    /// the host shows the HUD status line there.
    fn draw_result(&self, area: Rect) -> bool {
        let _ = area;
        false
    }
    /// Whether the host may shake the screen and flash its edges red on
    /// `Cue::Bad`. Precision and rhythm games, where a miss must never hide
    /// the next input, return `false`; the failure sound still plays and the
    /// game's own `effects` still run.
    fn shake_on_bad(&self) -> bool {
        true
    }
}
/// Default resizable native window. The browser supplies its own canvas size.
pub fn window(title: &str) -> Conf {
    let mut conf = ferravex_render::window_conf(title);
    conf.window_width = 1200;
    conf.window_height = 800;
    conf
}
