//! Orbit Breaker: original gameplay, original CPU raster, and only first-party Ferravex crates.
pub mod game;
#[cfg(target_arch = "wasm32")]
mod interactive;
mod raster;
use ferravex_core::FixedClock;
use ferravex_engine::input::{ActionMap, Binding, InputCode};
use game::{Game, HEIGHT, Input, WIDTH};
use raster::Canvas;
use std::sync::Mutex;

pub struct Orbit {
    pub game: Game,
    pub canvas: Canvas,
    clock: FixedClock,
    actions: ActionMap,
    axis: usize,
    launch: usize,
    restart: usize,
}
impl Default for Orbit {
    fn default() -> Self {
        Self::new()
    }
}
impl Orbit {
    pub fn new() -> Self {
        let mut actions = ActionMap::default();
        let axis = actions.action(vec![Binding {
            code: InputCode::PadAxis(0),
            scale: 1.0,
            deadzone: 0.02,
        }]);
        let launch = actions.action(vec![Binding {
            code: InputCode::Key(32),
            scale: 1.0,
            deadzone: 0.0,
        }]);
        let restart = actions.action(vec![Binding {
            code: InputCode::Key(82),
            scale: 1.0,
            deadzone: 0.0,
        }]);
        let mut app = Self {
            game: Game::new(),
            canvas: Canvas::new(),
            clock: FixedClock::new(120.0, 12),
            actions,
            axis,
            launch,
            restart,
        };
        app.render();
        app
    }
    pub fn advance(&mut self, dt: f64, axis: f32, buttons: u32) {
        self.actions.feed(InputCode::PadAxis(0), axis);
        self.actions
            .feed(InputCode::Key(32), f32::from(buttons & 1 != 0));
        self.actions
            .feed(InputCode::Key(82), f32::from(buttons & 2 != 0));
        let step = self.clock.advance(dt);
        for _ in 0..step.steps {
            self.game.tick(
                Input {
                    axis: self.actions.consume_tick(self.axis).value,
                    launch: self.actions.consume_tick(self.launch).pressed,
                    restart: self.actions.consume_tick(self.restart).pressed,
                },
                step.dt,
            );
        }
    }
    pub fn render(&mut self) {
        self.canvas.render(&self.game);
    }
}
static APP: Mutex<Option<Orbit>> = Mutex::new(None);
#[unsafe(no_mangle)]
pub extern "C" fn init() {
    *APP.lock().expect("app lock") = Some(Orbit::new());
}
/// buttons: bit0 launch/retry, bit1 reset. Input edges survive frames with no simulation tick.
#[unsafe(no_mangle)]
pub extern "C" fn step(dt: f64, axis: f32, buttons: u32) {
    if let Some(app) = APP.lock().expect("app lock").as_mut() {
        app.advance(dt, axis, buttons);
        app.render();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn pixels() -> *const u8 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(std::ptr::null(), |app| {
            app.canvas.frame.pixels().as_ptr().cast()
        })
}
#[unsafe(no_mangle)]
pub extern "C" fn width() -> u32 {
    WIDTH
}
#[unsafe(no_mangle)]
pub extern "C" fn height() -> u32 {
    HEIGHT
}
#[unsafe(no_mangle)]
pub extern "C" fn status() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(0, |app| app.game.phase as u32)
}
#[unsafe(no_mangle)]
pub extern "C" fn score() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(0, |app| app.game.score)
}
#[unsafe(no_mangle)]
pub extern "C" fn lives() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(3, |app| app.game.lives)
}
#[unsafe(no_mangle)]
pub extern "C" fn sector() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(1, |app| app.game.sector)
}
#[unsafe(no_mangle)]
pub extern "C" fn paddle() -> f32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(320.0, |app| app.game.paddle_x)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_input_tap_survives_zero_tick_and_frame_is_rgba() {
        let mut app = Orbit::new();
        app.advance(0.0, 0.0, 1);
        app.advance(0.0, 0.0, 0);
        app.advance(1.0 / 120.0, 0.0, 0);
        assert_eq!(app.game.phase, game::Phase::Playing);
        app.render();
        assert_eq!(app.canvas.frame.pixels().len(), (WIDTH * HEIGHT) as usize);
        assert!(app.canvas.frame.pixels().iter().all(|p| p[3] == 255));
        assert!(
            app.canvas
                .frame
                .pixels()
                .iter()
                .filter(|p| p[0] > 100)
                .count()
                > 5000
        );
    }
}
