//! Copper Greens: twelve-hole miniature golf in 2D.
use ferravex_core::session::GameSession;
use ferravex_render::{demo::Demo, prelude::*};
use mini_golf::{App, View};
fn conf() -> Conf {
    let mut conf = ferravex_render::window_conf("Copper Greens / 2D");
    // Debug-native screenshot sizing only; inert in release and Wasm.
    mini_golf::capture::window(&mut conf);
    conf
}
async fn game() {
    let mut demo = Demo::new();
    let mut app = App::new(View::TwoD, demo.autoplay);
    let mut session = GameSession::new(120., 12, true);
    loop {
        let dt = demo.dt() * mini_golf::capture::turbo();
        app.prepare(&mut session, dt);
        let batch = session.advance(f64::from(dt));
        for _ in 0..batch.steps {
            app.tick();
        }
        app.draw();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
ferravex_render::game!(conf, game);

/// Read-only UTF-8 golf diagnostic snapshot. Not a game-control API.
#[unsafe(no_mangle)]
pub extern "C" fn golf_status_v1() -> *const u8 {
    mini_golf::presentation::status_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn golf_status_v1_len() -> u32 {
    mini_golf::presentation::status_len()
}
