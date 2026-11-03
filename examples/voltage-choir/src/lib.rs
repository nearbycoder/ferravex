//! Voltage Choir: an original six-track, four-lane rhythm concert.
//! The pure [`Game`] model uses the same note judgments for humans and autoplay.
mod content;
pub mod model;
mod view;
use ferravex_arcade::{Color, Conf, Meta, window};
pub use model::Game;

/// The shared host's identity, control hints and field-guide pages.
pub const META: Meta = Meta {
    id: "voltage-choir",
    title: "Voltage Choir",
    subtitle: "Rhythm game",
    controls: "Arrows Hit Notes | Act Continue | X Encore | C Retry",
    help: &[
        "Hit the notes on beat and reach the accuracy target in six tracks.",
        "The four lanes, from left to right, are LEFT, DOWN, UP, RIGHT. Use arrow keys or A, S, W, D. Tap when a note's CENTER meets the bright horizontal line. Holding a key does not play later notes.",
        "A horizontal link joins a chord. Press both direction keys together. Number keys 1-4 and pointer taps can play single notes, but directions are recommended for chords.",
        "Perfect: within 2 ticks (33 ms). Good: within 4 (67 ms). Okay: within 7 (117 ms). Later notes miss. Empty taps break your combo and cost crowd energy, so button mashing is dangerous.",
        "Perfect and Good notes rebuild the crowd and fill Encore. At 100 charge, press X for eight beats of bonus hit points and protection from miss damage. Shielded misses still reduce accuracy and break the combo.",
        "There are three retry tickets for the whole concert. C retries the current song while it is playing; after a failed take, Space retries and X ends the concert. Completed-song points stay banked. Failed attempt points do not.",
        "Every song begins with four count-in tones. The first note arrives immediately after the count-in. Sound follows fixed musical ticks; use the bright line as your timing reference if your output device has audible latency.",
        "A strong combo raises the score multiplier, up to x5. Reach the six-track ending and return for cleaner grades. H opens this guide without advancing the song; Esc/P pauses; R starts the whole concert again.",
    ],
    accent: Color::new(0.42, 0.84, 0.98, 1.),
};
fn conf() -> Conf {
    window(META.title)
}
async fn game() {
    ferravex_arcade::run::<Game>(META).await;
}
ferravex_render::game!(conf, game);
