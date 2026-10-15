//! Shared first-party audio core; platform output lives in ferravex-render.
pub use ferravex_play::audio::*;
pub use ferravex_render::audio::suspend;
pub use ferravex_render::audio::{
    GameAudio, OutputStats, listener, muted, output_stats, play, play_sound, playing, preload_cues,
    set_bus_gain, set_master_gain, set_muted, set_params, stats, stop, stop_all,
};
