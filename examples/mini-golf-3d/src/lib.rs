//! Copper Greens: twelve-hole miniature golf in 3D.
use ferravex_core::session::GameSession;
use ferravex_render::{demo::Demo, prelude::*};
use mini_golf::{App, View};
fn conf() -> Conf {
    ferravex_render::window_conf("Copper Greens / 3D")
}
async fn game() {
    let mut demo = Demo::new();
    let mut app = App::new(View::ThreeD, demo.autoplay);
    let mut session = GameSession::new(120., 12, true);
    loop {
        let dt = demo.dt();
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
