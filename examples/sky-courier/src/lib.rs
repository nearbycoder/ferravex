//! Fixed-step coastal mail delivery, using the shared game session and player UI.
//!
//! The interactive host owns rendering; flight rules and session transitions are
//! exercised headlessly by the crate's tests. Autoplay uses the same flight inputs.
//!
//! ```no_run
//! sky_courier::run();
//! ```
use ferravex_core::session::{GameSession, SessionState};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Pose3, Rig3, Settings3, Target3};
use ferravex_play::feedback::{FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackFrame};
use ferravex_play::particles::{
    Emitter, EmitterTransform, ParticlePool, RibbonFacing, Trail, TrailConfig, TrailSample, presets,
};
use ferravex_play::vehicle::{FlightHandling, FlightState};
use ferravex_render::audio::{Cue, GameAudio};
use ferravex_render::effects::{
    BillboardView, EffectStyle, ParticleRenderer, ParticleStyle, draw_ribbon,
};
use ferravex_render::input::{
    HintDevice, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::preferences;
use ferravex_render::ui::{Align, ButtonOptions, ButtonStyle, TextStyle, Ui, UiRect, Viewport};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
};

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod courier;
mod flight_ui;
mod look;
mod overlay;
mod presentation;
mod quips;
use courier::{BONUS_RADIUS, BONUS_RINGS, Impact, Ledger, line_miss};
use flight_ui::{Coach, HudView, Progress, Results, hud};
use presentation::FlightPresentation;

const INK: Color = Color::new(0.10, 0.20, 0.22, 1.0);
const CREAM: Color = Color::new(0.95, 0.91, 0.78, 1.0);
const CORAL: Color = Color::new(0.82, 0.36, 0.23, 1.0);
const AMBER: Color = Color::new(0.93, 0.70, 0.34, 1.0);
const GATES: usize = 5;
const DELIVERY_SPEED: f32 = 18.;
const ROUTE: [Vec3; 6] = [
    vec3(0., 24., 100.),
    vec3(44., 40., 205.),
    vec3(-20., 30., 320.),
    vec3(-62., 47., 430.),
    vec3(10., 34., 550.),
    vec3(0., 16., 680.),
];
const ISLANDS: [(Vec3, f32); 9] = [
    (vec3(-42., 1., 73.), 30.),
    (vec3(67., 13., 207.), 30.),
    (vec3(-48., 7., 315.), 31.),
    (vec3(-86., 22., 430.), 30.),
    (vec3(38., 10., 553.), 30.),
    (vec3(0., 8., 680.), 37.),
    (vec3(112., 4., 380.), 40.),
    (vec3(-130., 14., 170.), 36.),
    (vec3(105., 7., 680.), 29.),
];

#[derive(Default, Clone, Copy)]
struct Input {
    turn: f32,
    climb: f32,
    boost: bool,
    slow: bool,
}
impl Input {
    fn neutral(self) -> bool {
        self.turn.abs() < 0.05 && self.climb.abs() < 0.05 && !self.boost && !self.slow
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Flying,
    Delivered,
    Lost,
}
struct Flight {
    pos: Vec3,
    motor: FlightState,
    next: usize,
    hull: u8,
    remaining: f32,
    outcome: Outcome,
    recovery: f32,
    boost_energy: f32,
    boost_exhausted: bool,
    gate_notice: f32,
    boosting: bool,
    slowing: bool,
    ledger: Ledger,
    /// Seconds left on the hidden-ring callout.
    bonus_notice: f32,
}
impl Flight {
    fn new() -> Self {
        Self {
            pos: vec3(0., 20., 0.),
            motor: FlightState {
                yaw: 0.,
                pitch: 0.,
                bank: 0.,
                turn_rate: 0.,
                speed: 24.,
            },
            next: 0,
            hull: 3,
            remaining: 90.,
            outcome: Outcome::Flying,
            recovery: 0.,
            boost_energy: 1.,
            boost_exhausted: false,
            gate_notice: 0.,
            boosting: false,
            slowing: false,
            ledger: Ledger::default(),
            bonus_notice: 0.,
        }
    }
    fn forward(&self) -> Vec3 {
        self.motor.forward()
    }
    #[cfg(test)]
    fn damage(&mut self) {
        self.crash(Impact::Water);
    }
    fn crash(&mut self, cause: Impact) {
        if self.recovery > 0. || self.outcome != Outcome::Flying {
            return;
        }
        self.ledger.impact(cause);
        self.hull = self.hull.saturating_sub(1);
        self.remaining = (self.remaining - 5.).max(0.);
        if self.hull == 0 || self.remaining == 0. {
            self.outcome = Outcome::Lost;
            return;
        }
        self.pos = if self.next == 0 {
            vec3(0., 20., 0.)
        } else {
            ROUTE[self.next - 1]
        };
        let delta = ROUTE[self.next] - self.pos;
        self.motor
            .reset(delta.x.atan2(delta.z), 0., 24.)
            .expect("authored checkpoint heading and cruise speed");
        self.boosting = false;
        self.slowing = false;
        self.recovery = 2.5;
    }
    fn tick(&mut self, input: Input, dt: f32) {
        if self.outcome != Outcome::Flying {
            return;
        }
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining <= 0. {
            self.outcome = Outcome::Lost;
            return;
        }
        self.recovery = (self.recovery - dt).max(0.);
        self.gate_notice = (self.gate_notice - dt).max(0.);
        self.bonus_notice = (self.bonus_notice - dt).max(0.);
        if self.boost_energy < 0.03 {
            self.boost_exhausted = true;
        }
        if self.boost_energy > 0.3 {
            self.boost_exhausted = false;
        }
        let boosting = input.boost && !input.slow && !self.boost_exhausted;
        self.boosting = boosting;
        self.slowing = input.slow;
        self.boost_energy =
            (self.boost_energy + dt * if boosting { -0.32 } else { 0.16 }).clamp(0., 1.);
        let target_speed = if input.slow {
            13.
        } else if boosting {
            35.
        } else {
            24.
        };
        self.motor
            .step(
                FlightHandling::default(),
                input.turn,
                input.climb,
                target_speed,
                dt,
            )
            .unwrap();
        let previous = self.pos;
        self.pos += self.forward() * self.motor.speed * dt;
        let impact = if self.pos.y < -17. {
            Some(Impact::Water)
        } else if self.pos.y > 110.
            || self.pos.x.abs() > 260.
            || self.pos.z < -90.
            || self.pos.z > 850.
        {
            Some(Impact::Boundary)
        } else if island_hit(self.pos) {
            Some(Impact::Island)
        } else {
            None
        };
        if let Some(cause) = impact {
            self.crash(cause);
            return;
        }
        // Hidden rings are optional and never affect route order.
        for (index, ring) in BONUS_RINGS.iter().enumerate() {
            if segment_hits(previous, self.pos, *ring, BONUS_RADIUS)
                && self.ledger.bonus(index).is_some()
            {
                self.boost_energy = 1.;
                self.boost_exhausted = false;
                self.bonus_notice = 2.5;
            }
        }
        // Swept sphere capture prevents missing a beacon between fixed steps.
        let radius = if self.next == GATES { 8. } else { 8.5 };
        if segment_hits(previous, self.pos, ROUTE[self.next], radius)
            && (self.next < GATES || self.motor.speed <= DELIVERY_SPEED)
        {
            // Grade the line being flown, not the entry point of the sphere.
            let miss = line_miss(self.pos, self.forward(), ROUTE[self.next]);
            if self.next < GATES {
                self.ledger.gate(self.next, miss, self.boosting);
            } else {
                self.ledger.deliver(miss, self.motor.speed, self.remaining);
            }
            self.boost_energy = (self.boost_energy + 0.18).min(1.);
            self.next += 1;
            self.gate_notice = 2.;
            if self.next == ROUTE.len() {
                self.outcome = Outcome::Delivered;
            }
        }
    }
    /// Ordinary turn/climb inputs toward a point; shared by autopilot and tests.
    fn steer_toward(&self, target: Vec3) -> Input {
        let delta = target - self.pos;
        let bearing = (delta.x.atan2(delta.z) - self.motor.yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let pitch = delta.y.atan2(vec2(delta.x, delta.z).length());
        Input {
            turn: (bearing * 2.7).clamp(-1., 1.),
            climb: (pitch / 0.58).clamp(-1., 1.),
            ..Input::default()
        }
    }
    fn autopilot(&self) -> Input {
        if self.outcome != Outcome::Flying {
            return Input::default();
        }
        let delta = ROUTE[self.next] - self.pos;
        Input {
            slow: self.next == GATES && delta.length() < 90.,
            ..self.steer_toward(ROUTE[self.next])
        }
    }
    /// Autopilot with detours through each hidden ring, using ordinary inputs.
    #[cfg(any(test, all(debug_assertions, not(target_arch = "wasm32"))))]
    fn scenic_tour(&self) -> Input {
        if self.outcome == Outcome::Flying {
            for (index, after) in [1, 3, 5].into_iter().enumerate() {
                if self.next == after && self.ledger.bonus & (1 << index) == 0 {
                    return self.steer_toward(BONUS_RINGS[index]);
                }
            }
        }
        self.autopilot()
    }
    fn navigation(&self) -> Option<Navigation> {
        let target = *ROUTE.get(self.next)?;
        let delta = target - self.pos;
        Some(Navigation {
            bearing: (delta.x.atan2(delta.z) - self.motor.yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI,
            altitude: delta.y,
            distance: delta.length(),
        })
    }
    fn flight_mode(&self) -> &'static str {
        if self.outcome != Outcome::Flying {
            "FLIGHT ENDED"
        } else if self.slowing {
            "SLOW FLIGHT"
        } else if self.boosting {
            "BOOSTING"
        } else if self.boost_exhausted {
            "RECHARGING"
        } else {
            "CRUISE"
        }
    }
    fn countdown(&self) -> u32 {
        // A still-open delivery window must never read 00:00.
        self.remaining.ceil() as u32
    }
}

struct Navigation {
    bearing: f32,
    altitude: f32,
    distance: f32,
}
impl Navigation {
    fn turn(&self) -> &'static str {
        if self.bearing.abs() < 0.12 {
            "ON COURSE"
        } else if self.bearing < 0. {
            "TURN LEFT"
        } else {
            "TURN RIGHT"
        }
    }
    fn climb(&self) -> &'static str {
        if self.altitude.abs() < 3. {
            "HOLD ALTITUDE"
        } else if self.altitude > 0. {
            "CLIMB"
        } else {
            "DESCEND"
        }
    }
}

#[derive(Default)]
struct FlightFrame {
    seconds: f32,
    gates: usize,
    damaged: bool,
    outcome: Option<Outcome>,
    /// Hidden rings first collected during this frame's accepted ticks.
    bonus: u8,
}

// The same coordinator drives the host and headless regression tests.
struct FlightRun {
    session: GameSession,
    flight: Flight,
    release_controls: bool,
    elapsed: f64,
}
impl FlightRun {
    fn new(autoplay: bool) -> Self {
        Self {
            session: GameSession::new(120., 12, autoplay),
            flight: Flight::new(),
            release_controls: !autoplay,
            elapsed: 0.,
        }
    }
    fn restart(&mut self) {
        self.flight = Flight::new();
        self.session.restart();
        self.release_controls = true;
        self.elapsed = 0.;
    }
    fn advance(&mut self, elapsed: f64, mut input: Input, autoplay: bool) -> FlightFrame {
        let mut frame = FlightFrame::default();
        if !self.session.is_playing() {
            self.session.advance(elapsed);
            return frame;
        }
        if self.release_controls && !autoplay {
            self.release_controls = !input.neutral();
            input = Input::default();
        }
        let steps = self.session.advance(elapsed);
        for _ in 0..steps.steps {
            let next = self.flight.next;
            let hull = self.flight.hull;
            let bonus = self.flight.ledger.bonus;
            self.flight.tick(
                if autoplay {
                    self.flight.autopilot()
                } else {
                    input
                },
                steps.dt,
            );
            frame.seconds += steps.dt;
            self.elapsed += f64::from(steps.dt);
            frame.gates += self.flight.next - next;
            frame.damaged |= self.flight.hull < hull;
            frame.bonus |= self.flight.ledger.bonus & !bonus;
            if self.flight.outcome != Outcome::Flying {
                self.session
                    .finish(self.flight.outcome == Outcome::Delivered);
                frame.outcome = Some(self.flight.outcome);
                break;
            }
        }
        frame
    }
}
fn segment_hits(a: Vec3, b: Vec3, center: Vec3, radius: f32) -> bool {
    let delta = b - a;
    let t = if delta.length_squared() > 0. {
        ((center - a).dot(delta) / delta.length_squared()).clamp(0., 1.)
    } else {
        0.
    };
    (a + delta * t).distance_squared(center) <= radius * radius
}
fn island_hit(pos: Vec3) -> bool {
    ISLANDS.iter().any(|(center, radius)| {
        let horizontal = vec2(pos.x - center.x, pos.z - center.z).length();
        horizontal < radius * 0.85 + 1. && pos.y < center.y + 2. && pos.y > center.y - 27.
    })
}

struct Scene {
    ocean: Mesh3d,
    islands: Vec<(Mesh3d, Mesh3d)>,
    house: Mesh3d,
    roof: Mesh3d,
    trunk: Mesh3d,
    leaves: Mesh3d,
    body: Mesh3d,
    nose: Mesh3d,
    wing: Mesh3d,
    tail: Mesh3d,
    fin: Mesh3d,
    glass: Mesh3d,
    float: Mesh3d,
    strut: Mesh3d,
    propeller: Mesh3d,
    ring: Mesh3d,
    hot_ring: Mesh3d,
    dim_ring: Mesh3d,
    bonus_ring: Mesh3d,
    pad: Mesh3d,
    stripe: Mesh3d,
    cloud: Mesh3d,
    puff: Mesh3d,
    parcel: Mesh3d,
    gull_body: Mesh3d,
    gull_wing: Mesh3d,
}
/// Presentation clocks for one scene draw; simulation never reads them.
#[derive(Clone, Copy, Default)]
struct SceneTime {
    /// Propeller angle source.
    propeller: f32,
    /// Ring pulse/spin clock; zero under reduced motion.
    clock: f32,
    /// Cosmetic victory-roll angle.
    roll: f32,
    /// False during a recovery blink phase.
    aircraft: bool,
    /// Gulls and idle motion; frozen in pause and under reduced motion.
    ambient: f32,
    /// Cosmetic vertical bob on the briefing screen.
    bob: f32,
}
impl Scene {
    fn new() -> Self {
        Self {
            ocean: Mesh3d::plane(vec2(1200., 1400.), Color::from_rgba(50, 108, 128, 255), 28),
            islands: ISLANDS
                .iter()
                .map(|(_, r)| {
                    (
                        Mesh3d::frustum(r * 0.35, *r, 24., Color::from_rgba(122, 106, 98, 255), 7),
                        Mesh3d::frustum(*r, r * 0.85, 3., Color::from_rgba(160, 168, 102, 255), 7),
                    )
                })
                .collect(),
            house: Mesh3d::cuboid(vec3(6., 5., 5.), Color::from_rgba(242, 228, 200, 255)),
            roof: Mesh3d::cone(5., 3.4, CORAL, 4),
            trunk: Mesh3d::cylinder(0.55, 4., Color::from_rgba(104, 89, 65, 255), 5),
            leaves: Mesh3d::cone(3.6, 8., Color::from_rgba(50, 92, 76, 255), 6),
            body: Mesh3d::cuboid(vec3(1.9, 1.6, 5.5), CREAM),
            nose: Mesh3d::cylinder(1., 1.8, CORAL, 7),
            wing: Mesh3d::cuboid(vec3(11.5, 0.23, 1.9), CORAL),
            tail: Mesh3d::cuboid(vec3(4.5, 0.2, 1.3), CREAM),
            fin: Mesh3d::cuboid(vec3(0.2, 1.8, 1.5), CORAL),
            glass: Mesh3d::cuboid(vec3(1.6, 0.85, 1.8), Color::from_rgba(61, 89, 94, 255)),
            float: Mesh3d::cuboid(vec3(0.65, 0.6, 4.2), Color::from_rgba(76, 85, 81, 255)),
            strut: Mesh3d::cuboid(vec3(0.12, 1.6, 0.12), INK),
            propeller: Mesh3d::cuboid(vec3(0.15, 3.3, 0.12), INK),
            ring: Mesh3d::cuboid(vec3(3.6, 0.45, 0.5), Color::from_rgba(236, 178, 86, 255)),
            hot_ring: Mesh3d::cuboid(vec3(3.7, 0.6, 0.65), Color::from_rgba(255, 236, 150, 255)),
            dim_ring: Mesh3d::cuboid(vec3(3.6, 0.4, 0.4), Color::from_rgba(121, 163, 157, 255)),
            bonus_ring: Mesh3d::cuboid(vec3(2.5, 0.5, 0.5), Color::from_rgba(255, 226, 120, 255)),
            pad: Mesh3d::cylinder(11., 0.25, Color::from_rgba(228, 206, 146, 255), 16),
            stripe: Mesh3d::cuboid(vec3(1.2, 0.12, 9.), CORAL),
            cloud: Mesh3d::sphere(10., Color::from_rgba(255, 238, 224, 255), 12, 7),
            puff: Mesh3d::sphere(6.5, Color::from_rgba(255, 244, 232, 255), 10, 6),
            parcel: Mesh3d::cuboid(vec3(1.4, 1.2, 1.1), Color::from_rgba(165, 121, 71, 255)),
            gull_body: Mesh3d::cuboid(vec3(0.35, 0.3, 1.1), Color::from_rgba(246, 242, 234, 255)),
            gull_wing: Mesh3d::cuboid(vec3(1.5, 0.08, 0.5), Color::from_rgba(236, 232, 224, 255)),
        }
    }
    fn draw(&self, game: &Flight, t: SceneTime) {
        let SceneTime {
            propeller: time,
            clock,
            roll,
            aircraft,
            ambient,
            bob,
        } = t;
        self.ocean.draw(vec3(0., -20., 350.), Quat::IDENTITY);
        for (i, (center, radius)) in ISLANDS.iter().enumerate() {
            let (rock, grass) = &self.islands[i];
            rock.draw(*center - Vec3::Y * 13.5, Quat::IDENTITY);
            grass.draw(*center, Quat::IDENTITY);
            for j in 0..5 {
                let a = j as f32 * 1.23 + i as f32;
                let p = *center + vec3(a.cos() * radius * 0.57, 3.4, a.sin() * radius * 0.57);
                self.trunk.draw(p, Quat::IDENTITY);
                self.leaves.draw(p + Vec3::Y * 4.5, Quat::IDENTITY);
            }
            let p = *center + vec3(radius * 0.3, 4., -radius * 0.15);
            self.house.draw(p, Quat::IDENTITY);
            self.roof.draw(
                p + Vec3::Y * 4.1,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            );
        }
        // The live ring breathes, and turns gold once a streak pays ×3 or more.
        let hot = game.ledger.multiplier() >= 3;
        for (i, gate) in ROUTE.iter().enumerate().take(5) {
            if i < game.next {
                continue;
            }
            let current = i == game.next;
            let mesh = match (current, hot) {
                (true, true) => &self.hot_ring,
                (true, false) => &self.ring,
                _ => &self.dim_ring,
            };
            let radius = if current {
                9. * (1. + 0.05 * (clock * 4.).sin())
            } else {
                9.
            };
            for segment in 0..16 {
                let angle = segment as f32 * std::f32::consts::TAU / 16.;
                let position = *gate + vec3(angle.cos() * radius, angle.sin() * radius, 0.);
                mesh.draw(
                    position,
                    Quat::from_rotation_z(angle + std::f32::consts::FRAC_PI_2),
                );
            }
        }
        // Hidden rings spin slowly so their glint catches the eye.
        for (i, ring) in BONUS_RINGS.iter().enumerate() {
            if game.ledger.bonus & (1 << i) != 0 {
                continue;
            }
            let spin = Quat::from_rotation_y(clock * 0.9 + i as f32);
            for segment in 0..12 {
                let angle = segment as f32 * std::f32::consts::TAU / 12.;
                let offset = spin * vec3(angle.cos() * 5.5, angle.sin() * 5.5, 0.);
                self.bonus_ring.draw(
                    *ring + offset,
                    spin * Quat::from_rotation_z(angle + std::f32::consts::FRAC_PI_2),
                );
            }
        }
        self.pad.draw(vec3(0., 9.7, 680.), Quat::IDENTITY);
        self.stripe.draw(vec3(0., 9.9, 680.), Quat::IDENTITY);
        self.stripe.draw(
            vec3(0., 9.9, 680.),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );
        // Soft cumulus: one body and two puffs per cluster, drifting gently.
        for i in 0..13 {
            let drift = (ambient * 0.06 + i as f32).sin() * 3.;
            let x = ((i * 79) % 330) as f32 - 165. + drift;
            let z = i as f32 * 67. - 50.;
            let y = 79. + (i % 3) as f32 * 10.;
            self.cloud.draw(vec3(x, y, z), Quat::IDENTITY);
            self.puff
                .draw(vec3(x + 11., y - 3., z + 3.), Quat::IDENTITY);
            self.puff.draw(vec3(x - 9., y - 4., z - 2.), Quat::IDENTITY);
        }
        for index in 0..6 {
            let (pos, heading, flap) = look::gull(index, ambient);
            let body = Quat::from_rotation_y(heading);
            self.gull_body.draw(pos, body);
            for side in [-1., 1.] {
                let wing = body * Quat::from_rotation_z(side * flap);
                self.gull_wing
                    .draw(pos + wing * vec3(side * 0.8, 0.05, 0.), wing);
            }
        }
        if !aircraft {
            return;
        }
        let game_pos = game.pos + Vec3::Y * bob;
        let rotation = Quat::from_rotation_y(game.motor.yaw)
            * Quat::from_rotation_x(-game.motor.pitch)
            * Quat::from_rotation_z(game.motor.bank + roll);
        let draw = |mesh: &Mesh3d, offset: Vec3| mesh.draw(game_pos + rotation * offset, rotation);
        draw(&self.body, Vec3::ZERO);
        self.nose.draw(
            game_pos + rotation * vec3(0., 0., 3.2),
            rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
        draw(&self.wing, vec3(0., 0.2, 0.6));
        draw(&self.tail, vec3(0., 0.3, -3.));
        draw(&self.fin, vec3(0., 1., -2.8));
        draw(&self.glass, vec3(0., 1., 0.5));
        draw(&self.parcel, vec3(0., -1.2, -0.8));
        for x in [-1.5, 1.5] {
            draw(&self.float, vec3(x, -2., 0.));
            draw(&self.strut, vec3(x, -1.1, 0.7));
            draw(&self.strut, vec3(x, -1.1, -1.3));
        }
        self.propeller.draw(
            game_pos + rotation * vec3(0., 0., 4.2),
            rotation * Quat::from_rotation_z(time * 42.),
        );
    }
}

fn ui_rect(x: f32, y: f32, w: f32, h: f32) -> UiRect {
    UiRect {
        min: [x, y],
        size: [w, h],
    }
}
fn ui_text(size: f32, color: Color, align: Align) -> TextStyle {
    TextStyle {
        size,
        color,
        align,
        wrap: true,
        shadow: false,
    }
}
fn control_hint<'a>(player: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    if player.current_device() == HintDevice::Touch && !player.touch_customized() {
        // Only these authored touch regions actually exist in this game.
        return match action {
            "move_x" | "move_y" => "Left stick",
            "brake" => "Right side",
            "confirm" => "Tap button",
            _ => "Open Controls",
        };
    }
    match player.format_hint(action, buffer) {
        Ok(label) if label.len() <= 32 => label,
        // Long/custom bindings remain available in the shared editor. Never
        // replace an unavailable hint with a misleading default key.
        _ => "Open Controls",
    }
}
#[cfg(test)]
fn movement_hint<'a>(
    player: &PlayerInput,
    actions: [&str; 3],
    buffers: &'a mut [[u8; 96]; 3],
) -> (&'a str, &'a str) {
    let [axis, negative, positive] = buffers;
    let axis = control_hint(player, actions[0], axis);
    if axis != "Unbound" {
        return (axis, "");
    }
    let negative = control_hint(player, actions[1], negative);
    let positive = control_hint(player, actions[2], positive);
    if negative.len() + positive.len() > 32 {
        ("Open Controls", "")
    } else {
        (negative, positive)
    }
}
fn conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Sky Courier — Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

/// Records context for this route, tip table and delivery rules.
const RECORD_RULES: u64 = 0x534b_5943_0001;
const RECORD_ID: &str = "north-coast-mail";

/// Best delivered tips. Failed shifts are recorded without a time, so only a
/// record with a successful time has a meaningful tip best.
fn best_tips(records: Option<&ferravex_render::storage::Records>) -> Option<u32> {
    records?
        .get(RECORD_ID, RECORD_RULES)
        .filter(|record| record.fastest_us.is_some())
        .map(|record| record.score.min(u64::from(u32::MAX)) as u32)
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut presentation = FlightPresentation::new();
    let mut exhaust_renderer = ParticleRenderer::new(128).unwrap();
    let mut spark_renderer = ParticleRenderer::new(256).unwrap();
    let mut run = FlightRun::new(demo.autoplay);
    let mut ui = Ui::new();
    let mut activate = false;
    let mut menu_ready = true;
    let scene = Scene::new();
    let mut grade = look::Grade::default();
    let material = LitMaterial::new().expect("portable flight material");
    let mut player_input = PlayerInput::preset("sky-courier", Preset::Flight);
    let mut reported = false;
    // Personal bests are optional: a storage failure still leaves a playable game.
    let mut records = ferravex_render::storage::Records::new("sky-courier").ok();
    let mut results = Results::default();
    let mut coach = Coach::default();
    let mut pauses = 0_u32;
    let mut shifts = 0_u32;
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    loop {
        let w = screen_width();
        let h = screen_height();
        // Keep the authored stick origin while bounding its hit region to the left half.
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: vec2(90., h - 100.),
                        radius: 85.,
                    },
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 60.,
                        floating: false,
                    },
                },
                Control {
                    region: Region::Rect(Rect::new(w * 0.55, 0., w * 0.45, h)),
                    output: Output::Button(6),
                },
            ])
            .unwrap();
        player_input.sample_play();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if capture.pause_now(&run) {
            player_input.set_paused(true);
        }
        let activated = std::mem::take(&mut activate);
        let restart = player_input.pressed("restart")
            || (activated && matches!(run.session.state(), SessionState::Won | SessionState::Lost));
        let mut transitioned = false;
        if restart && !player_input.controls_open() {
            run.restart();
            player_input.set_paused(false);
            player_input.sample(true);
            presentation.reset(run.flight.pos);
            sound.reset();
            reported = false;
            ui.cancel();
            transitioned = true;
        } else if activated && !player_input.controls_open() {
            player_input.set_paused(false);
            player_input.sample(true);
            transitioned = run.session.start() || run.session.set_paused(false);
        }
        // Explicit pause always applies; smoke autoplay remains independent of focus.
        let was_paused = run.session.is_paused();
        transitioned |= run.session.set_paused(
            player_input.paused() || (!demo.autoplay && !player_input.gameplay_active()),
        );
        if run.session.is_paused() && !was_paused {
            pauses = pauses.wrapping_add(1);
        }
        let movement = player_input.movement();
        #[allow(unused_mut)]
        let mut human = Input {
            turn: movement.x,
            climb: movement.y,
            boost: player_input.held("boost"),
            slow: player_input.held("brake"),
        };
        #[allow(unused_mut)]
        let mut autoplay = demo.autoplay;
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if demo.autoplay
            && let Some(input) = capture.input(&run.flight)
        {
            human = input;
            autoplay = false;
        }
        let frame = run.advance(
            if transitioned { 0. } else { demo.dt() as f64 },
            human,
            autoplay,
        );
        coach.update(&run.flight, frame.seconds);
        if let Some(outcome) = frame.outcome {
            // Compare against the stored best before recording this shift.
            shifts = shifts.wrapping_add(1);
            let delivered = outcome == Outcome::Delivered;
            results = Results {
                previous_best: best_tips(records.as_ref()),
                shift: shifts,
                demo: demo.autoplay,
            };
            #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
            if let Some(best) = capture.fake_best() {
                results.previous_best = Some(best);
                results.demo = false;
            }
            if !demo.autoplay
                && let Some(records) = records.as_mut()
            {
                let tips = if delivered { run.flight.ledger.tips } else { 0 };
                let time = if delivered {
                    ferravex_render::storage::run_micros(run.elapsed as f32)
                } else {
                    None
                };
                let _ = records.attempt(RECORD_ID, RECORD_RULES, u64::from(tips), time);
                if let Some(record) = records.get(RECORD_ID, RECORD_RULES) {
                    results.shift = record.runs.min(u64::from(u32::MAX)) as u32;
                }
            }
        }
        let flight = &run.flight;
        let (view, feedback) = presentation.update(&run, &frame, demo.dt(), |cue, gain| {
            sound.cue(cue, gain);
        });
        sound.motor(flight.motor.speed, 38., run.session.is_playing());
        let camera_pos = view.position;
        grade.apply(run.session.state());
        clear_background(look::ZENITH);
        let camera = Camera3D {
            position: camera_pos,
            target: view.target,
            up: view.up,
            fovy: view.fov_y,
            z_near: 0.2,
            z_far: 650.,
            ..Default::default()
        };
        let projector = overlay::Projector::new(&camera, w, h);
        look::sky(&projector, camera_pos, view.target, w, h);
        set_camera(&camera);
        material.begin(LitSettings {
            camera_position: camera_pos,
            fog_color: look::HORIZON,
            fog_start: 160.,
            fog_end: 500.,
            ambient: 0.65,
            ..Default::default()
        });
        scene.draw(
            flight,
            SceneTime {
                propeller: presentation.propeller_time,
                clock: presentation.clock,
                roll: presentation.roll_angle(),
                aircraft: presentation.aircraft_visible(flight),
                ambient: presentation.ambient,
                bob: presentation.bob(run.session.state()),
            },
        );
        material.end();
        for trail in &presentation.wing_trails {
            draw_ribbon(
                trail,
                RibbonFacing::Camera {
                    eye: camera_pos.to_array(),
                    up: view.up.to_array(),
                },
                2.,
                EffectStyle::default(),
            )
            .unwrap();
        }
        let billboard = BillboardView::new(
            camera_pos,
            (view.target - camera_pos).cross(view.up),
            view.up,
        )
        .unwrap();
        exhaust_renderer
            .draw_3d(&presentation.exhaust, billboard, ParticleStyle::default())
            .unwrap();
        spark_renderer
            .draw_3d(&presentation.sparks, billboard, ParticleStyle::default())
            .unwrap();
        set_default_camera();
        look::glows(
            &projector,
            flight,
            camera_pos,
            view.target,
            presentation.clock,
            !preferences::feedback_policy().reduced_flash(),
        );
        post_fx_hud_begin();
        if run.session.is_playing() {
            overlay::speed_lines(&presentation, w, h);
        }
        overlay::gate_glow(&presentation, w, h);
        if feedback.flash > 0. && run.session.is_playing() {
            // A small edge cue, independently governed by reduced flash.
            draw_rectangle_lines(
                2.,
                2.,
                (w - 4.).max(0.),
                (h - 4.).max(0.),
                4.,
                Color::new(CORAL.r, CORAL.g, CORAL.b, feedback.flash * 0.55),
            );
        }
        if run.session.is_playing() {
            overlay::waypoint(&projector, flight, w, h);
        }
        overlay::pops(&projector, &presentation);
        if player_input.touch_seen() && !player_input.touch_customized() && run.session.is_playing()
        {
            draw_circle_lines(90., h - 100., 60., 2., CREAM);
            draw_circle(90., h - 100., 4., CREAM);
        }
        if demo.autoplay && flight.outcome != Outcome::Flying && !reported {
            println!(
                "Sky Courier autoplay: {:?}; beacons={}; hull={}; remaining={:.1}s; tips=${}; streak={}; rings={}/3",
                flight.outcome,
                flight.next,
                flight.hull,
                flight.remaining,
                flight.ledger.tips,
                flight.ledger.best_streak,
                flight.ledger.bonus_count()
            );
            reported = true;
        }
        // UI activation is handled once next frame, before any simulation work.
        if transitioned || frame.outcome.is_some() {
            menu_ready = false;
        } else if !player_input.held("confirm") && !player_input.held("restart") {
            menu_ready = true;
        }
        let hud_view = HudView {
            progress: Progress {
                best_tips: best_tips(records.as_ref()),
            },
            results: &results,
            coach: &coach,
            pauses,
        };
        activate = hud(&run, &mut ui, &mut player_input, menu_ready, &hud_view)
            .expect("bounded courier HUD");
        player_input.draw_controls();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod journey200_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_hints_follow_remaps_unbinding_and_bounded_fallbacks() {
        use ferravex_render::input::{Binding, InputCode};
        let mut player = PlayerInput::preset("sky-courier-hint-test", Preset::Flight);
        let mut buffer = [0; 96];
        assert_eq!(control_hint(&player, "boost", &mut buffer), "Space");
        player
            .rebind(
                "boost",
                vec![Binding::digital(InputCode::Key(KeyCode::B as u16))],
            )
            .unwrap();
        assert_eq!(control_hint(&player, "boost", &mut buffer), "B");
        player.rebind("brake", vec![]).unwrap();
        assert_eq!(control_hint(&player, "brake", &mut buffer), "Unbound");
        player
            .rebind(
                "pause",
                vec![Binding::digital(InputCode::Key(KeyCode::P as u16))],
            )
            .unwrap();
        assert_eq!(control_hint(&player, "pause", &mut buffer), "P");
        player
            .rebind(
                "confirm",
                vec![Binding::digital(InputCode::Key(KeyCode::C as u16))],
            )
            .unwrap();
        assert_eq!(control_hint(&player, "confirm", &mut buffer), "C");
        player
            .rebind(
                "boost",
                vec![Binding::digital(InputCode::Key(KeyCode::B as u16)); 32],
            )
            .unwrap();
        assert_eq!(control_hint(&player, "boost", &mut buffer), "Open Controls");
        assert_eq!(
            control_hint(&player, "missing", &mut buffer),
            "Open Controls"
        );
        let mut buffers = [[0; 96]; 3];
        assert_eq!(
            movement_hint(&player, ["move_x", "move_left", "move_right"], &mut buffers),
            ("A / Left", "D / Right")
        );
        player
            .rebind(
                "move_x",
                vec![Binding::digital(InputCode::Key(KeyCode::J as u16))],
            )
            .unwrap();
        assert_eq!(
            movement_hint(&player, ["move_x", "move_left", "move_right"], &mut buffers),
            ("J", "")
        );
        player.rebind("move_x", vec![]).unwrap();
        assert_eq!(
            movement_hint(&player, ["move_x", "move_left", "move_right"], &mut buffers),
            ("A / Left", "D / Right")
        );
    }

    #[test]
    fn flight_status_reports_accepted_boost_slow_and_exhaustion_without_advancing_on_render() {
        let mut run = FlightRun::new(true);
        let boost = Input {
            boost: true,
            ..Default::default()
        };
        assert_eq!(run.flight.flight_mode(), "CRUISE");
        run.advance(0., boost, false);
        assert_eq!(run.flight.flight_mode(), "CRUISE");
        run.advance(1. / 120., boost, false);
        assert_eq!(run.flight.flight_mode(), "BOOSTING");
        run.advance(
            1. / 120.,
            Input {
                slow: true,
                ..boost
            },
            false,
        );
        assert_eq!(run.flight.flight_mode(), "SLOW FLIGHT");
        run.flight.boost_energy = 0.;
        run.advance(1. / 120., boost, false);
        assert_eq!(run.flight.flight_mode(), "RECHARGING");
        let elapsed = run.elapsed;
        run.session.set_paused(true);
        run.advance(15., Input::default(), false);
        assert_eq!(run.elapsed, elapsed);
        run.restart();
        assert_eq!(run.elapsed, 0.);
        assert_eq!(run.flight.flight_mode(), "CRUISE");
    }

    #[test]
    fn checkpoint_motor_reset_preserves_route_boost_and_penalty() {
        let mut game = Flight::new();
        game.pos = ROUTE[0];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 1);
        game.motor.pitch = 0.4;
        game.motor.bank = 0.5;
        game.motor.turn_rate = 1.;
        game.motor.speed = 35.;
        game.boost_energy = 0.4;
        let before = game.remaining;
        game.damage();
        assert_eq!(game.pos, ROUTE[0]);
        assert_eq!(game.next, 1);
        assert_eq!(game.remaining, before - 5.);
        assert_eq!(game.boost_energy, 0.4);
        assert_eq!(
            (
                game.motor.pitch,
                game.motor.bank,
                game.motor.turn_rate,
                game.motor.speed
            ),
            (0., 0., 0., 24.)
        );
        assert!(game.navigation().unwrap().bearing.abs() < 0.001);
        let position = game.pos;
        game.damage();
        assert_eq!(game.pos, position);
        assert_eq!(game.hull, 2);
    }

    #[test]
    fn ready_and_pause_freeze_flight_countdown_and_feedback_without_catch_up() {
        let mut run = FlightRun::new(false);
        let position = run.flight.pos;
        assert_eq!(run.session.state(), SessionState::Ready);
        assert_eq!(run.advance(30., Input::default(), false).seconds, 0.);
        assert_eq!(run.flight.pos, position);
        assert_eq!(run.flight.remaining, 90.);
        assert!(run.session.start());
        run.flight.recovery = 2.;
        run.flight.gate_notice = 1.;
        run.advance(1. / 120., Input::default(), false);
        let before = (
            run.flight.pos,
            run.flight.remaining,
            run.flight.recovery,
            run.flight.gate_notice,
            run.session.tick(),
        );
        assert!(run.session.set_paused(true));
        for _ in 0..10 {
            let frame = run.advance(
                10.,
                Input {
                    boost: true,
                    ..Default::default()
                },
                false,
            );
            assert_eq!(frame.seconds, 0.);
            assert_eq!(frame.gates, 0);
            assert!(!frame.damaged);
            assert_eq!(frame.outcome, None);
        }
        assert_eq!(
            (
                run.flight.pos,
                run.flight.remaining,
                run.flight.recovery,
                run.flight.gate_notice,
                run.session.tick()
            ),
            before
        );
        assert!(run.session.set_paused(false));
        let frame = run.advance(1. / 120., Input::default(), false);
        assert!((frame.seconds - 1. / 120.).abs() < 0.00001);
        assert_eq!(run.session.tick(), before.4 + 1);
        assert!(before.1 - run.flight.remaining < 0.01);
    }

    #[test]
    fn retry_clears_clock_progress_and_holds_controls_until_released() {
        let mut run = FlightRun::new(true);
        run.advance(1. / 240., Input::default(), false);
        run.flight.next = 3;
        run.flight.hull = 1;
        run.flight.remaining = 7.;
        run.flight.recovery = 2.;
        run.flight.gate_notice = 1.;
        run.session.set_paused(true);
        run.restart();
        assert_eq!(run.session.state(), SessionState::Playing);
        assert_eq!(run.session.tick(), 0);
        assert_eq!(run.flight.next, 0);
        assert_eq!(run.flight.hull, 3);
        assert_eq!(run.flight.remaining, 90.);
        assert_eq!(run.flight.recovery, 0.);
        assert_eq!(run.flight.gate_notice, 0.);
        let stale = Input {
            turn: 1.,
            climb: 1.,
            boost: true,
            slow: false,
        };
        assert_eq!(run.advance(1. / 240., stale, false).seconds, 0.);
        for _ in 0..10 {
            run.advance(1. / 60., stale, false);
        }
        assert_eq!(run.flight.motor.yaw, 0.);
        assert_eq!(run.flight.motor.pitch, 0.);
        assert_eq!(run.flight.motor.speed, 24.);
        assert_eq!(run.flight.boost_energy, 1.);
        run.advance(1. / 60., Input::default(), false);
        run.advance(1. / 60., stale, false);
        assert!(run.flight.motor.yaw > 0. && run.flight.motor.pitch > 0.);
        assert!(run.flight.motor.speed > 24. && run.flight.boost_energy < 1.);
        run.restart();
        run.advance(
            0.1,
            Input {
                slow: true,
                ..Default::default()
            },
            false,
        );
        assert_eq!(run.flight.motor.speed, 24.);
    }

    #[test]
    fn crash_penalty_can_close_delivery_window_on_the_same_tick() {
        let mut run = FlightRun::new(true);
        run.flight.remaining = 4.;
        run.flight.pos.y = -21.;
        let frame = run.advance(0.1, Input::default(), false);
        assert!(frame.damaged);
        assert_eq!(frame.outcome, Some(Outcome::Lost));
        assert_eq!(run.flight.hull, 2);
        assert_eq!(run.flight.remaining, 0.);
        assert_eq!(run.session.state(), SessionState::Lost);
        assert!((frame.seconds - 1. / 120.).abs() < 0.00001);
        let position = run.flight.pos;
        let frame = run.advance(30., Input::default(), false);
        assert_eq!(frame.seconds, 0.);
        assert_eq!(frame.outcome, None);
        assert_eq!(run.flight.pos, position);
    }

    #[test]
    fn guidance_wraps_bearings_and_countdown_never_expires_early() {
        let mut game = Flight::new();
        assert_eq!(game.navigation().unwrap().turn(), "ON COURSE");
        assert_eq!(game.navigation().unwrap().climb(), "CLIMB");
        game.pos = ROUTE[0] + vec3(10., 10., -30.);
        assert_eq!(game.navigation().unwrap().turn(), "TURN LEFT");
        assert_eq!(game.navigation().unwrap().climb(), "DESCEND");
        game.motor.yaw = std::f32::consts::TAU;
        assert_eq!(game.navigation().unwrap().turn(), "TURN LEFT");
        game.pos = ROUTE[0] - Vec3::X * 20.;
        assert_eq!(game.navigation().unwrap().turn(), "TURN RIGHT");
        assert_eq!(game.navigation().unwrap().climb(), "HOLD ALTITUDE");
        assert!((game.navigation().unwrap().distance - 20.).abs() < 0.001);
        game.remaining = 0.001;
        assert_eq!(game.countdown(), 1);
        game.remaining = 60.01;
        assert_eq!(game.countdown(), 61);
        game.remaining = 0.;
        assert_eq!(game.countdown(), 0);
        game.next = ROUTE.len();
        assert!(game.navigation().is_none());
    }

    #[test]
    fn session_autoplay_delivers_identically_at_regular_and_irregular_frame_rates() {
        fn deliver(pattern: &[f64]) -> FlightRun {
            let mut run = FlightRun::new(true);
            let mut gates = 0;
            let mut successes = 0;
            for frame in 0..30_000 {
                let events = run.advance(pattern[frame % pattern.len()], Input::default(), true);
                gates += events.gates;
                successes += usize::from(events.outcome == Some(Outcome::Delivered));
                if !run.session.is_playing() {
                    break;
                }
            }
            assert_eq!(run.session.state(), SessionState::Won);
            assert_eq!(run.flight.hull, 3);
            assert_eq!(gates, 6);
            assert_eq!(successes, 1);
            assert_eq!(run.advance(5., Input::default(), true).outcome, None);
            run
        }
        let baseline = deliver(&[1. / 60.]);
        for pattern in [
            &[1. / 30.][..],
            &[1. / 144.][..],
            &[0., 1. / 240., 0.08, 1. / 60.][..],
        ] {
            let run = deliver(pattern);
            assert_eq!(run.flight.pos, baseline.flight.pos);
            assert_eq!(run.flight.remaining, baseline.flight.remaining);
            assert_eq!(run.flight.motor.speed, baseline.flight.motor.speed);
        }
    }

    #[test]
    fn boost_is_a_recoverable_resource_and_slow_overrides_it() {
        let mut game = Flight::new();
        for _ in 0..600 {
            game.tick(
                Input {
                    boost: true,
                    ..Default::default()
                },
                1. / 120.,
            );
        }
        assert_eq!(game.outcome, Outcome::Flying);
        assert!(game.boost_energy < 0.3 && game.motor.speed < 34.);
        let energy = game.boost_energy;
        for _ in 0..360 {
            game.tick(
                Input {
                    boost: true,
                    slow: true,
                    ..Default::default()
                },
                1. / 120.,
            );
        }
        assert!(game.boost_energy > energy + 0.3);
        assert!(game.motor.speed < 14.);
    }

    #[test]
    fn ordinary_flight_inputs_complete_the_route_and_delivery() {
        let mut game = Flight::new();
        for _ in 0..120 * 85 {
            game.tick(game.autopilot(), 1. / 120.);
        }
        assert_eq!(game.outcome, Outcome::Delivered);
        assert_eq!(game.next, 6);
        assert_eq!(game.hull, 3);
        assert!(game.remaining > 0.);
    }
    #[test]
    fn beacons_must_be_visited_in_order() {
        let mut game = Flight::new();
        game.pos = ROUTE[3];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 0);
        game.pos = ROUTE[0];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 1);
    }
    #[test]
    fn damage_recovers_and_three_crashes_end_the_flight() {
        let mut game = Flight::new();
        for hull in [2, 1, 0] {
            game.recovery = 0.;
            game.pos.y = -21.;
            game.tick(Input::default(), 1. / 120.);
            assert_eq!(game.hull, hull);
            if hull > 0 {
                assert_eq!(game.pos, vec3(0., 20., 0.));
            }
        }
        assert_eq!(game.outcome, Outcome::Lost);
        assert!(island_hit(ISLANDS[0].0));
        assert!(!island_hit(ISLANDS[0].0 + Vec3::Y * 30.));
    }
    #[test]
    fn timeout_freezes_and_restart_restores_game() {
        let mut game = Flight::new();
        game.remaining = 0.001;
        game.tick(Input::default(), 0.02);
        assert_eq!(game.outcome, Outcome::Lost);
        let pos = game.pos;
        game.tick(Input::default(), 1.);
        assert_eq!(game.pos, pos);
        game = Flight::new();
        assert_eq!(game.next, 0);
        assert_eq!(game.hull, 3);
        assert_eq!(game.outcome, Outcome::Flying);
    }
    #[test]
    fn cargo_requires_a_slow_pass_and_swept_capture_handles_fast_motion() {
        let mut game = Flight::new();
        game.next = 5;
        game.pos = ROUTE[5];
        game.motor.speed = 30.;
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 5);
        game.motor.speed = 13.;
        game.tick(
            Input {
                slow: true,
                ..Input::default()
            },
            1. / 120.,
        );
        assert_eq!(game.outcome, Outcome::Delivered);
        assert!(segment_hits(Vec3::ZERO, Vec3::Z * 20., Vec3::Z * 10., 1.));
    }
}

#[cfg(test)]
mod courier_tests {
    use super::*;

    fn fly(pattern: &[f64], tour: bool) -> (FlightRun, u8, usize) {
        let mut run = FlightRun::new(true);
        let mut bonus = 0;
        let mut bonus_events = 0;
        for frame in 0..30_000 {
            let input = if tour {
                run.flight.scenic_tour()
            } else {
                run.flight.autopilot()
            };
            // Re-steer from the current state only at whole-frame boundaries,
            // like a human; the coordinator applies it to each executed tick.
            let events = run.advance(pattern[frame % pattern.len()], input, false);
            assert_eq!(bonus & events.bonus, 0, "a hidden ring is reported once");
            bonus |= events.bonus;
            bonus_events += events.bonus.count_ones() as usize;
            if !run.session.is_playing() {
                break;
            }
        }
        (run, bonus, bonus_events)
    }

    #[test]
    fn hidden_rings_are_off_route_safe_and_reachable_with_ordinary_inputs() {
        for ring in BONUS_RINGS {
            assert!(!island_hit(ring), "{ring:?} is clear of terrain");
            assert!(ring.y - BONUS_RADIUS > -17. && ring.y + BONUS_RADIUS < 110.);
            for gate in ROUTE {
                assert!(ring.distance(gate) > 40., "hidden rings stay off-route");
            }
        }
        let (route, bonus, _) = fly(&[1. / 120.], false);
        assert_eq!(route.session.state(), SessionState::Won);
        assert_eq!(
            (bonus, route.flight.ledger.bonus),
            (0, 0),
            "autopilot never stumbles on them"
        );
        let (tour, bonus, events) = fly(&[1. / 120.], true);
        assert_eq!(tour.session.state(), SessionState::Won);
        assert_eq!(tour.flight.hull, 3);
        assert_eq!((bonus, events), (0b111, 3));
        assert!(tour.flight.ledger.scenic());
        assert!(tour.flight.ledger.tips > route.flight.ledger.tips + 150);
        assert_eq!(
            quips::rank(&tour.flight.ledger, true, 3, GATES).0,
            "SCENIC ROUTE SPECIALIST"
        );
    }

    #[test]
    fn autopilot_ledger_is_a_clean_deterministic_streak() {
        let (run, _, _) = fly(&[1. / 120.], false);
        let ledger = &run.flight.ledger;
        assert_eq!(ledger.best_streak, GATES as u32);
        assert_eq!(ledger.impacts, 0);
        assert!(ledger.gates.iter().all(Option::is_some));
        let drop = ledger.drop.expect("graded delivery");
        assert_eq!(drop.multiplier, courier::MAX_MULTIPLIER);
        let gates: u32 = ledger.gates.iter().flatten().map(|g| g.tip).sum();
        assert_eq!(ledger.tips, gates + drop.tip + drop.early);
        // Every render cadence produces the same accepted ledger.
        for pattern in [
            &[1. / 30.][..],
            &[1. / 144.],
            &[0., 1. / 240., 0.08, 1. / 60.],
        ] {
            let (other, _, _) = fly(pattern, false);
            assert_eq!(other.flight.ledger, run.flight.ledger);
        }
    }

    #[test]
    fn impacts_break_the_streak_and_record_their_cause() {
        let mut game = Flight::new();
        game.pos = ROUTE[0];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!((game.ledger.streak, game.ledger.multiplier()), (1, 2));
        game.pos = ISLANDS[1].0;
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.hull, 2);
        assert_eq!(game.ledger.last_impact, Impact::Island);
        assert_eq!((game.ledger.streak, game.ledger.impacts), (0, 1));
        game.recovery = 0.;
        game.pos = vec3(0., 115., 300.);
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.ledger.last_impact, Impact::Boundary);
        assert_eq!(game.ledger.best_streak, 1);
        // Recovery protection ignores further contacts entirely.
        game.pos = vec3(0., -30., 300.);
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.ledger.impacts, 2);
    }
}
