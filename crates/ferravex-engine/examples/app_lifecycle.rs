//! Run with `cargo run -p ferravex-engine --example app_lifecycle --offline --locked`.
//! A host injects elapsed time, pauses the session, interpolates a moving point,
//! and explicitly stops. This example needs no window or graphics context.

use ferravex_core::session::GameSession;
use ferravex_engine::app::{App, AppControl, Game};
use std::convert::Infallible;

#[derive(Default)]
struct MovingPoint {
    previous: f32,
    current: f32,
    presented: f32,
    updates: u32,
    shutdowns: u32,
}

impl Game for MovingPoint {
    type Error = Infallible;

    fn startup(&mut self) -> Result<AppControl, Self::Error> {
        println!("startup");
        Ok(AppControl::Continue)
    }

    fn fixed_update(&mut self, dt: f32) -> Result<AppControl, Self::Error> {
        self.previous = self.current;
        self.current += 8.0 * dt;
        self.updates += 1;
        Ok(AppControl::Continue)
    }

    fn render(&mut self, alpha: f32) -> Result<AppControl, Self::Error> {
        self.presented = self.previous + (self.current - self.previous) * alpha;
        println!("render: x={:.4}, alpha={alpha:.2}", self.presented);
        Ok(AppControl::Continue)
    }

    fn shutdown(&mut self) -> Result<(), Self::Error> {
        self.shutdowns += 1;
        println!("shutdown after {} fixed updates", self.updates);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new(MovingPoint::default(), GameSession::new(64.0, 4, true));
    app.startup()?;
    for elapsed in [0.5 / 64.0, 2.0 / 64.0, 1.0 / 64.0] {
        if app.frame(elapsed)? == AppControl::Stop {
            break;
        }
    }
    assert_eq!(app.game().updates, 3);
    assert_eq!(app.game().presented, 0.3125);

    // Snap both interpolation poses when pausing to keep the latest state visible.
    app.game_mut().previous = app.game().current;
    app.session_mut().set_paused(true);
    app.frame(30.0)?;
    assert_eq!(app.game().updates, 3);
    assert_eq!(app.game().presented, 0.375);
    app.session_mut().set_paused(false);
    app.frame(1.0 / 64.0)?;
    assert_eq!(app.game().updates, 4);

    app.stop()?;
    app.stop()?;
    assert_eq!(app.game().shutdowns, 1);
    assert_eq!(app.frame(1.0)?, AppControl::Stop);
    Ok(())
}
