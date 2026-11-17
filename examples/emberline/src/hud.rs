//! Wide-layout HUD, title/pause/results cards and scene transitions in the
//! authored 1280×720 canvas, drawn with the shared modern UI kit.
//!
//! The play HUD keeps one job per glass chip: where you are and what you can
//! do (left), the clock, medal pace and course track (center), and embers,
//! points and a live chain (right). One toast slot and one coaching prompt keep
//! messages from stacking. Copy uses the Modern face; the clock, counters,
//! logo, rank and pops keep the Classic face's arcade punch. Every steady-state
//! label formats into a stack [`Line`], so drawing a frame allocates nothing.
use super::juice::{GOLD, Glyph, Juice, MOTH, Toast};
use super::presentation::ControlHint;
use super::quips::{self, RunSummary};
use super::style::{self, EMBER, INK, LEAF, Line, Scene, fade, line};
use super::*;
use ferravex_play::gameplay::trial::Medal;
use ferravex_play::tween::{
    Appearance, Entrance, Motion, RollCounter, ScreenTransition, Spring, TransitionKind,
    TransitionPhase,
};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::shapes::{self, Cap, Fill, Join, Stroke};
use ferravex_render::text::TextStyle;
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, ButtonState};

pub(super) const MUTED: Color = style::MUTED;
const SILVER: Color = Color::new(0.82, 0.87, 0.90, 1.0);
const BRONZE: Color = Color::new(0.86, 0.58, 0.34, 1.0);
const TRACK: (f32, f32) = (488., 792.);

/// The browser fullscreen game menu (a 44×44 button inset 8) owns this
/// top-right screen region; HUD text and controls must stay out of it.
pub(super) const MENU_CORNER: Vec2 = Vec2::new(72., 60.);
/// Canvas-space top of the HUD chips.
const CHIP_TOP: f32 = 24.;
const CHIP_H: f32 = 68.;
const LEFT_CHIP_W: f32 = 214.;
const RIGHT_CHIP_W: f32 = 236.;

/// Wide-layout placements that respect [`MENU_CORNER`] for the current screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WideLayout {
    /// Right edge of the ember/points chip, in canvas units.
    pub right: f32,
    /// The default touch pause button, in canvas units.
    pub pause: Rect,
}

impl WideLayout {
    /// Letterboxed screens keep the authored symmetric layout; when the canvas
    /// reaches the top edge, the right chip is inset and the pause button
    /// lowered so neither sits beneath the menu button.
    pub fn new(canvas: ferravex_render::canvas::Canvas2D, screen: Vec2) -> Self {
        let corner_x = canvas.unproject(vec2(screen.x - MENU_CORNER.x - 4., 0.)).x;
        let below = |y: f32| canvas.project(vec2(0., y)).y >= MENU_CORNER.y + 4.;
        let right = if below(CHIP_TOP) {
            WIDTH - 32.
        } else {
            (WIDTH - 32.).min(corner_x)
        };
        let pause_y = if below(104.) {
            104.
        } else {
            canvas.unproject(vec2(0., MENU_CORNER.y + 4.)).y
        };
        let pause_x = if below(pause_y) {
            WIDTH - 112.
        } else {
            (WIDTH - 112.).min(corner_x - 96.)
        };
        Self {
            right,
            pause: Rect::new(pause_x, pause_y, 96., 56.),
        }
    }
}

/// The visible card, which is also the default touch confirm region.
pub(super) fn card_rect(state: SessionState) -> Rect {
    match state {
        SessionState::Won => Rect::new(250., 104., 780., 476.),
        SessionState::Paused => Rect::new(290., 208., 700., 264.),
        _ => Rect::new(290., 150., 700., 380.),
    }
}

/// Result of the finished run against stored records, captured once at the finish.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Results {
    /// The fastest stored clear before this run, in seconds.
    pub previous_best: Option<f64>,
    /// Autoplay demonstrations are shown but never recorded.
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, time: f64) -> bool {
        !self.demo && self.previous_best.is_none_or(|best| time < best - 1e-6)
    }
}

pub(super) fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Gold => GOLD,
        Medal::Silver => SILVER,
        Medal::Bronze => BRONZE,
        Medal::Unranked => MUTED,
    }
}

/// The medal the running clock would still earn.
pub(super) fn pace(elapsed: f64) -> Medal {
    let t = elapsed as f32;
    if t <= MEDALS[0] {
        Medal::Gold
    } else if t <= MEDALS[1] {
        Medal::Silver
    } else if t <= MEDALS[2] {
        Medal::Bronze
    } else {
        Medal::Unranked
    }
}

pub(super) fn summary(game: &Game) -> RunSummary {
    RunSummary {
        medal: game.trial.medal().unwrap_or(Medal::Unranked),
        falls: game.deaths,
        embers: game.embers.iter().filter(|(_, found)| *found).count(),
        total_embers: game.embers.len(),
        close_calls: game.close_calls,
        secret: game.secret,
    }
}

pub(super) fn zone(game: &Game) -> usize {
    match game.position().x {
        x if x < 900. => 0,
        x if x < 1800. => 1,
        _ => 2,
    }
}

/// Small lantern icon used in the HUD and on the title card.
pub(super) fn lantern_icon(x: f32, y: f32, lit: bool) {
    lantern_glyph(vec2(x, y - 4.), 22., lit, 1.);
}

/// Draws `glyph` centered at `at`, fitted to a `size` box. `color` tints
/// engine icons and the air-jump chevrons; Emberline's own pictograms keep
/// their palette and take only `color`'s alpha.
pub(super) fn glyph(glyph: Glyph, at: Vec2, size: f32, color: Color) {
    match glyph {
        Glyph::Icon(icon) => icons::draw_styled(
            icon,
            at,
            size,
            IconStyle::new(color).shadow(fade(style::SHADOW, color.a * 0.6)),
        ),
        Glyph::Lantern(lit) => lantern_glyph(at, size, lit, color.a),
        Glyph::Ember => {
            shapes::glow_disc(at, size * 0.75, Color::new(1., 0.55, 0.2, 0.45 * color.a));
            gem(at, size * 0.48, fade(ORANGE, color.a));
        }
        Glyph::AirJump => air_jump_glyph(at, size, color),
        Glyph::Moth => moth_glyph(at, size, color.a),
        Glyph::Medal(medal) => medal_badge(at, size * 0.5, medal, color.a),
        Glyph::Move => {
            for (icon, dx) in [(Icon::ArrowLeft, -0.3), (Icon::ArrowRight, 0.3)] {
                icons::draw(icon, at + vec2(dx * size, 0.), size * 0.62, color);
            }
        }
    }
}

/// The caged lantern: a lit one holds a flame and glows (shape, not only color).
fn lantern_glyph(at: Vec2, size: f32, lit: bool, alpha: f32) {
    let u = size / 22.;
    let (x, y) = (at.x, at.y + 4. * u);
    let color = fade(if lit { GOLD } else { MUTED }, alpha);
    if lit {
        shapes::glow_disc(
            vec2(x, y - u),
            14. * u,
            Color::new(1., 0.62, 0.25, 0.55 * alpha),
        );
    }
    shapes::fill_triangle(
        vec2(x - 8. * u, y - 9. * u),
        vec2(x + 8. * u, y - 9. * u),
        vec2(x, y - 15. * u),
        color,
    );
    let cage = Rect::new(x - 7. * u, y - 9. * u, 14. * u, 16. * u);
    shapes::fill_rounded_rect(cage, 2. * u, fade(Color::new(0.10, 0.14, 0.12, 1.), alpha));
    shapes::stroke_rounded_rect(cage, 2. * u, 2. * u, color);
    if lit {
        gem(vec2(x, y - u), 5. * u, fade(ORANGE, alpha));
    }
}

/// An anti-aliased ember diamond with a darker left facet.
pub(super) fn gem(at: Vec2, radius: f32, color: Color) {
    let (top, bottom) = (at - vec2(0., radius), at + vec2(0., radius));
    let (left, right) = (at - vec2(radius * 0.65, 0.), at + vec2(radius * 0.65, 0.));
    shapes::fill_triangle(top, right, bottom, color);
    shapes::fill_triangle(
        top,
        bottom,
        left,
        Color::new(color.r * 0.7, color.g * 0.7, color.b * 0.7, color.a),
    );
}

fn gem_outline(at: Vec2, radius: f32, color: Color) {
    shapes::polyline(
        &[
            at - vec2(0., radius),
            at + vec2(radius * 0.65, 0.),
            at + vec2(0., radius),
            at - vec2(radius * 0.65, 0.),
        ],
        Stroke::new(1.5, color).closed(true).join(Join::Round),
    );
}

/// Two rising chevrons: the earned mid-air jump.
fn air_jump_glyph(at: Vec2, size: f32, color: Color) {
    let u = size * 0.5;
    for (dy, alpha) in [(-0.36, 1.), (0.26, 0.55)] {
        let y = at.y + dy * u;
        shapes::polyline(
            &[
                vec2(at.x - 0.66 * u, y + 0.34 * u),
                vec2(at.x, y - 0.3 * u),
                vec2(at.x + 0.66 * u, y + 0.34 * u),
            ],
            Stroke::new(size * 0.17, fade(color, alpha))
                .cap(Cap::Round)
                .join(Join::Round),
        );
    }
}

fn moth_glyph(at: Vec2, size: f32, alpha: f32) {
    let u = size * 0.5;
    shapes::glow_disc(at, size * 0.7, fade(MOTH, 0.22 * alpha));
    let hind = shapes::mix(MOTH, Color::new(0.35, 0.3, 0.5, 1.), 0.35);
    for side in [-1., 1.] {
        shapes::fill_triangle(
            at + vec2(0., -0.12 * u),
            at + vec2(side * 0.98 * u, -0.78 * u),
            at + vec2(side * 0.86 * u, 0.12 * u),
            fade(MOTH, alpha),
        );
        shapes::fill_triangle(
            at + vec2(0., 0.04 * u),
            at + vec2(side * 0.72 * u, 0.22 * u),
            at + vec2(side * 0.38 * u, 0.74 * u),
            fade(hind, alpha),
        );
    }
    shapes::fill_capsule(
        at + vec2(0., -0.5 * u),
        at + vec2(0., 0.56 * u),
        0.15 * u,
        fade(Color::from_hex(0x5a4f6e), alpha),
    );
}

/// The one coaching moment a first-time player needs right now. Lessons are
/// taught by the scene and the HUD, never by sentences: the jump lesson is a
/// key-cap prompt over the hero, the lantern lesson brightens the save marker,
/// and the air-jump lesson pulses the last empty ember pip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Coach {
    Jump,
    Lantern,
    AirJump,
}

/// The coaching hint for a first-time player, or `None` once learned.
pub(super) fn coach(game: &Game) -> Option<Coach> {
    let x = game.position().x;
    if game.jumps == 0 && x < 600. {
        Some(Coach::Jump)
    } else if game.checkpoints.index() == 0 && (1000. ..1500.).contains(&x) {
        Some(Coach::Lantern)
    } else if game.ember_inventory.count(1) == 2 {
        Some(Coach::AirJump)
    } else {
        None
    }
}

/// A key cap and pictogram shown over the hero exactly when the move is
/// useful, until the player has used it once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prompt {
    /// Before the first jump.
    Jump,
    /// In mid-air with the earned air jump ready, until it is first used.
    AirJump,
}

/// The prompt a first-time player needs this frame. `veteran` players (a
/// stored clear) never see prompts.
pub(super) fn prompt(game: &Game, juice: &Juice, veteran: bool) -> Option<Prompt> {
    if veteran || !game.session.is_playing() {
        return None;
    }
    if coach(game) == Some(Coach::Jump) {
        return Some(Prompt::Jump);
    }
    let ready = game.ember_inventory.count(1) >= 3 && !game.air_jump_used;
    (ready && !game.motor.grounded && !juice.air_jump_taught).then_some(Prompt::AirJump)
}

impl Prompt {
    pub fn glyph(self) -> Glyph {
        match self {
            Self::Jump => Glyph::Icon(Icon::ArrowUp),
            Self::AirJump => Glyph::AirJump,
        }
    }
}

/// Text pops and expanding rings, in world space.
pub(super) fn world_effects(juice: &Juice, camera: f32) {
    for ring in juice.rings.iter().flatten() {
        let t = ring.age / ring.life;
        shapes::glow_ring(
            vec2(ring.at.x - camera, ring.at.y),
            ring.radius * (0.3 + 0.7 * t.sqrt()),
            10. * (1. - t) + 3.,
            fade(ring.color, (1. - t) * 0.9),
        );
    }
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let rise = 46. * (1. - (1. - t).powi(2)) * juice.motion();
        let grow = 1. + 0.35 * (1. - (pop.age / 0.1).min(1.)) * juice.motion();
        let alpha = ((1. - t) / 0.35).min(1.);
        let x = (pop.at.x - camera).clamp(70., WIDTH - 70.);
        style::center(
            &pop.text,
            x,
            pop.at.y - rise,
            style::display(pop.size * grow, fade(pop.color, alpha)).with_pixel_snap(grow == 1.),
        );
    }
}

pub(super) struct Hints {
    pub confirm: ControlHint,
    pub pause: ControlHint,
    pub restart: ControlHint,
    pub jump: ControlHint,
    /// First move-left key, a separator and the first move-right key.
    pub movement: Line,
    pub tap_confirm: bool,
    pub keyboard: bool,
    pub touch: bool,
}

fn first_key(hint: &str) -> &str {
    hint.split(" / ").next().unwrap_or(hint)
}

impl Hints {
    pub fn new(input: &PlayerInput, touch_mode: bool) -> Self {
        let left = ControlHint::new(input, "move_left");
        let right = ControlHint::new(input, "move_right");
        Self {
            confirm: ControlHint::new(input, "confirm"),
            pause: ControlHint::new(input, "pause"),
            restart: ControlHint::new(input, "restart"),
            jump: ControlHint::new(input, "jump"),
            movement: line!("{} / {}", first_key(left.text()), first_key(right.text())),
            tap_confirm: touch_mode
                && !input.touch_customized()
                && presentation::touch_button_matches(input, "confirm", 10),
            keyboard: input.current_device() == HintDevice::KeyboardMouse,
            touch: touch_mode,
        }
    }
}

/// Render-time overlay state: scene looks, transitions, roll counters and
/// menu entrances. None of it is simulation state, and all motion honors
/// `Motion::Reduced` (snaps, fades and no attract-mode drift).
pub(super) struct Overlay {
    scene: Option<Scene>,
    state: SessionState,
    /// Render seconds since the session state last changed.
    pub since: f32,
    /// Render seconds on the title screen, for the attract-mode pan.
    pub attract: f32,
    transition: ScreenTransition,
    points: RollCounter,
    clock: RollCounter,
    embers: Spring,
    last_embers: usize,
    /// HUD lantern pop when a lantern is lit.
    lanterns: Spring,
    last_lanterns: usize,
    /// HUD air-jump pop on unlock and on every recharge.
    air: Spring,
    last_ready: bool,
    /// The hero prompt being shown or faded out, and its opacity.
    prompt: Option<Prompt>,
    prompt_alpha: f32,
    cta: ButtonMotion,
    motion: Motion,
}

/// A transition that starts fully covered and reveals over `duration`.
fn reveal(kind: TransitionKind, duration: f32) -> ScreenTransition {
    let mut t = ScreenTransition::new(kind, duration);
    t.cover();
    t.advance(duration * 4. + 1., Motion::Full);
    t.reveal();
    t
}

impl Overlay {
    pub fn new(state: SessionState) -> Self {
        Self {
            scene: None,
            state,
            since: 0.,
            attract: 0.,
            transition: ScreenTransition::new(TransitionKind::Fade, 0.3),
            points: RollCounter::new(0),
            clock: RollCounter::new(0),
            embers: Spring::underdamped(0., 9., 0.3),
            last_embers: 0,
            lanterns: Spring::underdamped(0., 9., 0.3),
            last_lanterns: 0,
            air: Spring::underdamped(0., 9., 0.3),
            last_ready: false,
            prompt: None,
            prompt_alpha: 0.,
            cta: ButtonMotion::default(),
            motion: Motion::Full,
        }
    }

    pub fn motion(&self) -> Motion {
        self.motion
    }

    /// Ember count bump (0 at rest) for a springy HUD reaction.
    pub fn ember_bump(&self) -> f32 {
        self.embers.value
    }

    /// Lantern pip bump (0 at rest) when a lantern is lit.
    pub fn lantern_bump(&self) -> f32 {
        self.lanterns.value
    }

    /// Air-jump bump (0 at rest) when the jump unlocks or recharges.
    pub fn air_bump(&self) -> f32 {
        self.air.value
    }

    /// Fades the hero prompt toward `target` on render time. Changing prompt
    /// kind restarts the fade so two prompts never cross-blend.
    pub fn prompt_frame(&mut self, target: Option<Prompt>, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        match target {
            Some(kind) => {
                if self.prompt != Some(kind) {
                    self.prompt_alpha = 0.;
                }
                self.prompt = Some(kind);
                self.prompt_alpha = (self.prompt_alpha + dt * 6.).min(1.);
            }
            None => self.prompt_alpha = (self.prompt_alpha - dt * 3.).max(0.),
        }
    }

    /// The visible prompt and its opacity.
    pub fn prompt(&self) -> Option<(Prompt, f32)> {
        self.prompt
            .filter(|_| self.prompt_alpha > 0.01)
            .map(|kind| (kind, self.prompt_alpha))
    }

    pub fn points(&self) -> i64 {
        self.points.shown()
    }

    /// The results clock rolls up from zero, in centiseconds.
    pub fn clock(&self) -> f64 {
        self.clock.shown() as f64 / 100.
    }

    /// Advance once per render frame, after simulation and menus. `restarted`
    /// and `respawned` are this frame's accepted edges.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        game: &Game,
        dt: f32,
        policy: FeedbackPolicy,
        restarted: bool,
        respawned: bool,
        confirm_held: bool,
    ) {
        let dt = dt.clamp(0., 0.1);
        self.motion = Motion::from(policy);
        let state = game.session.state();
        if restarted || state != self.state {
            if restarted || (self.state == SessionState::Ready && state == SessionState::Playing) {
                // Title → play and every retry open with an iris on the hero.
                self.transition = reveal(TransitionKind::Iris, 0.45);
                self.points.snap(0);
                self.last_embers = 0;
                self.last_lanterns = 0;
                self.last_ready = false;
                self.prompt_alpha = 0.;
            }
            if state == SessionState::Won {
                self.clock.snap(0);
                self.clock
                    .set_target((game.trial.elapsed() * 100.).round() as i64);
            }
            self.state = state;
            self.since = 0.;
        } else if respawned {
            // A quick Celeste-style wipe sells the instant respawn.
            self.transition = reveal(TransitionKind::Wipe { direction: 0. }, 0.22);
        }
        self.since += dt;
        if state == SessionState::Ready {
            self.attract += dt;
        }
        let scene = match state {
            SessionState::Ready => Scene::Title,
            SessionState::Paused => Scene::Paused,
            SessionState::Won | SessionState::Lost => Scene::Results,
            SessionState::Playing => Scene::Play,
        };
        if self.scene != Some(scene) {
            ferravex_render::post::set_post_fx(scene.post());
            self.scene = Some(scene);
        }
        self.transition.advance(dt, self.motion);
        self.points
            .set_target(i64::try_from(game.score.total()).unwrap_or(i64::MAX));
        self.points.advance(dt, self.motion);
        if state == SessionState::Won && self.since > 0.6 {
            self.clock.advance(dt, self.motion);
        }
        let embers = game.embers.iter().filter(|(_, found)| *found).count();
        if embers > self.last_embers {
            self.embers.impulse(9.);
        }
        self.last_embers = embers;
        self.embers.advance(dt, self.motion);
        let lanterns = game.checkpoints.index();
        if lanterns > self.last_lanterns {
            self.lanterns.impulse(9.);
        }
        self.last_lanterns = lanterns;
        self.lanterns.advance(dt, self.motion);
        let ready = game.ember_inventory.count(1) >= 3 && !game.air_jump_used;
        if ready && !self.last_ready {
            self.air.impulse(9.);
        }
        self.last_ready = ready;
        self.air.advance(dt, self.motion);
        self.cta.update(
            ButtonState {
                focused: true,
                pressed: confirm_held,
                ..ButtonState::default()
            },
            dt,
            self.motion,
        );
    }

    /// Draws the active transition over everything, in screen coordinates.
    pub fn draw_transition(&self, focus: Vec2) {
        if self.transition.phase() != TransitionPhase::Idle {
            kit::draw_screen_transition(
                self.transition.kind(self.motion),
                self.transition.coverage(),
                style::NIGHT,
                focus,
            );
        }
    }

    fn look(&self, entrance: Entrance, index: usize, delay: f32) -> Appearance {
        entrance.sample(self.since - delay, index, self.motion)
    }
}

/// The in-run HUD: icons, numbers, pips and one toast slot. No sentences.
pub(super) fn play(game: &Game, juice: &Juice, overlay: &Overlay, cluster: f32) {
    let theme = style::theme();
    // Left chip: section, lanterns and the earned air jump.
    let left = Rect::new(24., CHIP_TOP, LEFT_CHIP_W, CHIP_H);
    style::chip(left, 1.);
    let section = line!("{:02}", zone(game) + 1);
    let w = style::draw(
        &section,
        left.x + 16.,
        left.y + 26.,
        style::arcade(16., EMBER),
    );
    style::draw(
        ZONES[zone(game)],
        left.x + 24. + w,
        left.y + 26.,
        style::body(16., INK),
    );
    let lit = game.checkpoints.index();
    let motion = juice.motion();
    let bump = overlay.lantern_bump() * motion;
    for index in 1..=2 {
        let pop = if index == lit { 1. + bump * 0.1 } else { 1. };
        glyph(
            Glyph::Lantern(lit >= index),
            vec2(left.x + 26. + (index - 1) as f32 * 24., left.y + 47.),
            22. * pop,
            INK,
        );
    }
    air_jump_meter(game, overlay, vec2(left.x + 92., left.y + 47.), motion);

    // Center chip: the clock in its medal-pace color and a course track.
    let center = Rect::new(
        TRACK.0 - 24.,
        CHIP_TOP - 6.,
        TRACK.1 - TRACK.0 + 48.,
        CHIP_H + 6.,
    );
    style::chip(center, 1.);
    let elapsed = game.trial.elapsed();
    let medal = pace(elapsed);
    let clock = line!("{elapsed:.2}");
    let clock_x = WIDTH * 0.5 - 12.;
    let clock_w = style::center(
        &clock,
        clock_x,
        CHIP_TOP + 24.,
        style::arcade(32., medal_color(medal)),
    );
    icons::draw(
        Icon::Clock,
        vec2(clock_x - clock_w * 0.5 - 20., CHIP_TOP + 10.),
        18.,
        fade(MUTED, 0.9),
    );
    if medal != Medal::Unranked {
        medal_badge(
            vec2(clock_x + clock_w * 0.5 + 20., CHIP_TOP + 12.),
            11.,
            medal,
            1.,
        );
    }
    let (a, b) = TRACK;
    let y = CHIP_TOP + 46.;
    let at = |x: f32| a + (b - a) * (x / LIFT_X).clamp(0., 1.);
    shapes::fill_capsule(vec2(a, y), vec2(b, y), 2., Color::new(1., 1., 1., 0.14));
    let me = at(game.position().x + PLAYER.x * 0.5);
    shapes::fill_capsule(vec2(a, y), vec2(me.max(a + 0.5), y), 2., fade(EMBER, 0.9));
    for index in 1..=2 {
        let x = at(lantern_position(index).x);
        let lit = game.checkpoints.index() >= index;
        shapes::fill_rounded_rect(
            Rect::new(x - 4., y - 6., 8., 12.),
            2.,
            if lit { GOLD } else { MUTED },
        );
    }
    shapes::fill_rounded_rect(Rect::new(b - 2., y - 9., 4., 18.), 1., LEAF);
    draw_triangle(
        vec2(b + 2., y - 9.),
        vec2(b + 14., y - 5.),
        vec2(b + 2., y - 1.),
        LEAF,
    );
    shapes::glow_disc(vec2(me, y), 12., Color::new(1., 0.6, 0.25, 0.7));
    shapes::fill_circle(vec2(me, y), 5., EMBER);
    shapes::stroke_circle(vec2(me, y), 5., 1.5, INK);

    // Right chip: embers (count over total), then a star with the points and
    // any live chain multiplier with its draining timer.
    let right = Rect::new(cluster - RIGHT_CHIP_W, CHIP_TOP, RIGHT_CHIP_W, CHIP_H);
    style::chip(right, 1.);
    let collected = game.embers.iter().filter(|(_, found)| *found).count();
    let total = line!("/{}", game.embers.len());
    let total_w = style::right(
        &total,
        right.right() - 16.,
        right.y + 30.,
        style::arcade(16., MUTED),
    );
    let count = line!("{collected}");
    let count_w = style::right(
        &count,
        right.right() - 18. - total_w,
        right.y + 30.,
        style::arcade(24., INK),
    );
    let bump = overlay.ember_bump() * motion;
    let gem_at = vec2(right.right() - 36. - total_w - count_w, right.y + 21.);
    shapes::glow_disc(
        gem_at,
        16. + bump * 3.,
        Color::new(1., 0.55, 0.2, 0.55 + bump * 0.1),
    );
    gem(gem_at, 10. + bump * 1.5, ORANGE);
    let points = line!("{}", overlay.points());
    let points_w = style::right(
        &points,
        right.right() - 16.,
        right.y + 56.,
        style::arcade(16., EMBER),
    );
    let star = vec2(right.right() - 30. - points_w, right.y + 50.);
    icons::draw_styled(
        Icon::Star,
        star,
        17.,
        IconStyle::new(GOLD).shadow(fade(style::SHADOW, 0.6)),
    );
    if let Some(life) = game
        .score
        .remaining_life()
        .filter(|_| game.score.multiplier() > 1)
    {
        let chain = line!("×{}", game.score.multiplier());
        let x = star.x - 14.;
        let pill = Rect::new(right.x + 12., right.y + 41., x - right.x - 12., 20.);
        shapes::fill_rounded_rect(pill, 10., fade(LEAF, 0.18));
        let life = (life as f32 / 5.).clamp(0., 1.);
        kit::meter(
            Rect::new(pill.x + 34., pill.y + 7., pill.w - 42., 6.),
            life,
            None,
            LEAF,
            &theme,
        );
        style::draw(&chain, pill.x + 8., pill.y + 15., style::arcade(16., LEAF));
    }

    if let Some(state) = &juice.toast {
        toast(state, WIDTH * 0.5, 112., overlay.motion());
    }
}

/// The air jump at a glance: chevrons that glow while the jump is ready and
/// dim once spent (until the next landing), then three ember pips that fill
/// toward the unlock. With one ember to go, the last pip breathes.
pub(super) fn air_jump_meter(game: &Game, overlay: &Overlay, at: Vec2, motion: f32) {
    let earned = (game.ember_inventory.count(1) as usize).min(3);
    let unlocked = earned >= 3;
    let ready = unlocked && !game.air_jump_used;
    let bump = overlay.air_bump() * motion;
    if ready {
        shapes::glow_disc(at, 17. + bump * 5., fade(LEAF, 0.4));
    }
    let color = if ready {
        LEAF
    } else {
        fade(MUTED, if unlocked { 0.95 } else { 0.6 })
    };
    glyph(Glyph::AirJump, at, 22. * (1. + bump * 0.12), color);
    let breathe = if motion > 0. {
        ferravex_render::juice::pulse(overlay.since, 1.2)
    } else {
        1.
    };
    for pip in 0..3 {
        let p = vec2(at.x + 26. + pip as f32 * 17., at.y);
        if pip < earned {
            let color = if !unlocked {
                ORANGE
            } else if ready {
                LEAF
            } else {
                fade(MUTED, 0.8)
            };
            gem(p, 6.5, color);
        } else {
            if earned == 2 {
                shapes::glow_disc(p, 11., fade(ORANGE, 0.25 + 0.35 * breathe));
            }
            gem_outline(p, 6., fade(MUTED, 0.85));
        }
    }
}

/// Draws the hero prompt, a key cap and the move's pictogram, over the
/// hero's head. `camera` is the world camera's left edge in canvas units.
pub(super) fn hero_prompt(game: &Game, hints: &Hints, overlay: &Overlay, camera: f32) {
    let Some((kind, alpha)) = overlay.prompt() else {
        return;
    };
    let bob = style::wave(overlay.since, 4., overlay.motion()) * 3.;
    let head = game.position() - vec2(camera, 0.) + vec2(PLAYER.x * 0.5, -16. + bob);
    let keys = (!hints.touch).then(|| hints.jump.text());
    let keys_w = keys.map_or(0., |keys| style::key_prompt_width(keys, "") + 2.);
    let w = keys_w + 46.;
    let x = head.x.clamp(w * 0.5 + 12., WIDTH - w * 0.5 - 12.);
    let rect = Rect::new(x - w * 0.5, head.y - 40., w, 36.);
    style::soft_shadow(rect, 12., alpha * 0.6);
    style::chip(rect, alpha);
    shapes::fill_triangle(
        vec2(head.x - 6., rect.bottom()),
        vec2(head.x + 6., rect.bottom()),
        vec2(head.x, rect.bottom() + 6.),
        fade(style::GLASS, alpha),
    );
    let mut cursor = rect.x + 10.;
    if let Some(keys) = keys {
        cursor += style::key_prompt(cursor, rect.y + 5., keys, "", &style::theme().faded(alpha));
    }
    let color = if kind == Prompt::AirJump { LEAF } else { INK };
    glyph(
        kind.glyph(),
        vec2(cursor + 14., rect.y + 17.),
        22.,
        fade(color, alpha),
    );
}

fn medal_badge(at: Vec2, radius: f32, medal: Medal, alpha: f32) {
    let color = medal_color(medal);
    shapes::glow_disc(at, radius * 2.2, fade(color, 0.35 * alpha));
    shapes::fill_circle(
        at,
        radius,
        Fill::radial(
            at - vec2(radius * 0.3, radius * 0.4),
            radius * 1.4,
            fade(shapes::mix(color, Color::new(1., 1., 1., 1.), 0.45), alpha),
            fade(shapes::mix(color, Color::new(0., 0., 0., 1.), 0.25), alpha),
        ),
    );
    shapes::stroke_circle(
        at,
        radius,
        1.5,
        fade(Color::new(0.1, 0.07, 0.03, 0.6), alpha),
    );
    let letter = &quips::medal_name(medal)[..1];
    let style = TextStyle::classic(
        if radius > 16. { 24. } else { 16. },
        fade(Color::from_hex(0x2a1a08), alpha),
    )
    .with_align(ferravex_render::text::TextAlign::Center);
    style::draw(letter, at.x + 1., style.centered_baseline(at.y), style);
}

/// The toast pill: one line with a pictogram, a short callout and an
/// optional number. It drops in from above and fades out; reduced motion
/// keeps only the fade.
pub(super) fn toast(toast: &Toast, center: f32, top: f32, motion: Motion) {
    let enter = Entrance::fade_up(-14.)
        .with_duration(0.24)
        .sample(toast.age, 0, motion);
    let alpha = toast.alpha().min(enter.alpha);
    let head = style::arcade(24., fade(INK, alpha));
    let value = style::arcade(24., fade(toast.value_color, alpha));
    let icon = 28.;
    let head_w = style::width(&toast.head, head);
    let value_w = if toast.value.is_empty() {
        0.
    } else {
        18. + style::width(&toast.value, value)
    };
    let w = (icon + 12. + head_w + value_w).min(760.) + 44.;
    let rect = Rect::new(center - w * 0.5, top + enter.offset[1], w, 50.);
    style::soft_shadow(rect, 16., alpha);
    shapes::fill_rounded_rect(
        rect,
        16.,
        Fill::vertical(
            rect,
            fade(Color::new(0.10, 0.14, 0.13, 0.96), alpha),
            fade(Color::new(0.06, 0.09, 0.09, 0.96), alpha),
        ),
    );
    shapes::stroke_rounded_rect(rect, 16., 1.5, fade(toast.color, 0.55 * alpha));
    let cy = rect.y + rect.h * 0.5;
    let x = rect.x + 22.;
    glyph(
        toast.icon,
        vec2(x + icon * 0.5, cy),
        icon,
        fade(toast.color, alpha),
    );
    let x = x + icon + 12.;
    style::draw(&toast.head, x, head.centered_baseline(cy), head);
    if !toast.value.is_empty() {
        style::draw(
            &toast.value,
            x + head_w + 18.,
            value.centered_baseline(cy),
            value,
        );
    }
}

/// What a card stat shows under its pictogram.
#[derive(Clone, Copy)]
enum Value {
    Text(Line),
    Icon(Icon),
}

fn stats(items: &[(Glyph, Value, Color)], area: Rect, overlay: &Overlay, delay: f32, alpha: f32) {
    let gap = 12.;
    let w = (area.w - gap * (items.len() - 1) as f32) / items.len() as f32;
    for (i, (icon, value, color)) in items.iter().enumerate() {
        let look = overlay.look(Entrance::fade_up(10.).with_stagger(0.05), i, delay);
        let rect = kit::appear(
            Rect::new(area.x + i as f32 * (w + gap), area.y, w, area.h),
            look,
        );
        let a = look.alpha * alpha;
        style::tile(rect, a);
        let c = rect.center().x;
        glyph(*icon, vec2(c, rect.y + 22.), 22., fade(MUTED, a));
        match value {
            Value::Text(text) => style::center_fit(
                text,
                c,
                rect.y + 60.,
                w - 12.,
                style::arcade(24., fade(*color, a)),
            ),
            Value::Icon(icon) => icons::draw(*icon, vec2(c, rect.y + 51.), 22., fade(*color, a)),
        }
    }
}

fn glyph_width(icon: Glyph) -> f32 {
    if icon == Glyph::Move { 34. } else { 22. }
}

/// Width of [`key_group`] without drawing.
fn key_group_width(keys: Option<&str>, icon: Glyph, label: &str) -> f32 {
    let keys = keys.map_or(0., |keys| style::key_prompt_width(keys, "") + 4.);
    let label = if label.is_empty() {
        0.
    } else {
        8. + style::width(label, style::body(16., INK))
    };
    keys + glyph_width(icon) + label
}

/// Key caps, a pictogram and an optional one-word label, vertically centered
/// on `cy`. Returns the drawn width.
fn key_group(
    x: f32,
    cy: f32,
    keys: Option<&str>,
    icon: Glyph,
    label: &str,
    ink: Color,
    alpha: f32,
) -> f32 {
    let mut cursor = x;
    if let Some(keys) = keys {
        cursor += style::key_prompt(cursor, cy - 12., keys, "", &style::theme().faded(alpha)) + 4.;
    }
    let w = glyph_width(icon);
    glyph(icon, vec2(cursor + w * 0.5, cy), 20., fade(ink, alpha));
    cursor += w;
    if !label.is_empty() {
        // Dark ink on a bright button reads crisper without a drop shadow.
        let text = if ink.r + ink.g + ink.b > 1.5 {
            style::body(16., fade(ink, alpha))
        } else {
            TextStyle::modern(16., fade(ink, alpha))
        };
        cursor += 8. + style::draw(label, cursor + 8., text.centered_baseline(cy), text);
    }
    cursor - x
}

/// A button whose face is a key group: `[Enter] ▶ Start`.
fn key_button(
    rect: Rect,
    keys: Option<&str>,
    icon: Glyph,
    label: &str,
    kind: ButtonKind,
    motion: &ButtonMotion,
    look: Appearance,
) {
    let theme = style::theme().faded(look.alpha);
    let rect = kit::appear(rect, look);
    kit::button(rect, "", kind, motion, &theme);
    let ink = if kind == ButtonKind::Primary {
        theme.accent_ink
    } else {
        INK
    };
    let w = key_group_width(keys, icon, label);
    let sink = 2. * motion.press();
    key_group(
        rect.center().x - w * 0.5,
        rect.center().y + sink,
        keys,
        icon,
        label,
        ink,
        look.alpha,
    );
}

/// A pill with a pictogram, short text and an optional trailing number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Chip {
    pub glyph: Glyph,
    pub text: Line,
    pub color: Color,
    pub value: Line,
    pub value_color: Color,
}

impl Chip {
    fn new(glyph: Glyph, text: Line, color: Color) -> Self {
        Self {
            glyph,
            text,
            color,
            value: Line::new(),
            value_color: color,
        }
    }

    fn with_value(mut self, value: Line, color: Color) -> Self {
        self.value = value;
        self.value_color = color;
        self
    }

    fn width(&self, lead: bool) -> f32 {
        let text = style::body(16., INK);
        let value = if self.value.is_empty() {
            0.
        } else {
            10. + style::width(&self.value, text)
        };
        (if lead { 26. } else { 0. }) + 22. + 8. + style::width(&self.text, text) + value + 28.
    }

    /// Draws the pill centered on `center`, optionally led by `lead`.
    fn draw(&self, center: Vec2, lead: Option<Icon>, alpha: f32) {
        let w = self.width(lead.is_some());
        let pill = Rect::new(center.x - w * 0.5, center.y - 15., w, 30.);
        shapes::fill_rounded_rect(pill, 15., fade(self.color, 0.12 * alpha));
        shapes::stroke_rounded_rect(pill, 15., 1., fade(self.color, 0.35 * alpha));
        let mut x = pill.x + 14.;
        if let Some(icon) = lead {
            icons::draw(icon, vec2(x + 9., center.y), 18., fade(MUTED, alpha));
            x += 26.;
        }
        glyph(
            self.glyph,
            vec2(x + 11., center.y),
            20.,
            fade(self.color, alpha),
        );
        x += 30.;
        let text = style::body(16., fade(self.color, alpha));
        x += style::draw(&self.text, x, text.centered_baseline(center.y), text);
        if !self.value.is_empty() {
            let value = text.with_color(fade(self.value_color, alpha));
            style::draw(
                &self.value,
                x + 10.,
                value.centered_baseline(center.y),
                value,
            );
        }
    }
}

/// The three medal targets as badges with their times.
fn medal_targets(center: Vec2, alpha: f32) {
    let text = style::body(16., INK);
    let item = |secs: f32| 28. + style::width(&line!("{secs:.0}s"), text);
    let gap = 22.;
    let total: f32 = MEDALS.iter().map(|&s| item(s)).sum::<f32>() + gap * 2.;
    let mut x = center.x - total * 0.5;
    for (medal, secs) in [Medal::Gold, Medal::Silver, Medal::Bronze]
        .into_iter()
        .zip(MEDALS)
    {
        medal_badge(vec2(x + 10., center.y), 10., medal, alpha);
        let label = line!("{secs:.0}s");
        style::draw(
            &label,
            x + 28.,
            text.centered_baseline(center.y),
            text.with_color(fade(medal_color(medal), alpha)),
        );
        x += item(secs) + gap;
    }
}

/// The title card: an animated logo, three pictogram goals, key caps and a
/// clear call to action.
pub(super) fn ready(hints: &Hints, best: Option<(f64, Medal)>, overlay: &Overlay) {
    let rect = card_rect(SessionState::Ready);
    let motion = overlay.motion();
    let card = overlay.look(Entrance::pop(0.94).with_duration(0.36), 0, 0.);
    style::card(kit::appear(rect, card), card.alpha);
    let c = rect.center().x;

    // Logo: letters pop in one after another, then breathe on a slow wave.
    let logo = style::display(48., GOLD).with_letter_spacing(1.);
    let word = "EMBERLINE";
    let total = style::width(word, logo);
    let mut x = c - total * 0.5;
    let glow = style::wave(overlay.attract, 1.6, motion) * 0.08;
    shapes::glow_disc(
        vec2(c, rect.y + 58.),
        150.,
        Color::new(1., 0.5, 0.18, (0.22 + glow) * card.alpha),
    );
    for (i, ch) in word.char_indices() {
        let letter = &word[i..i + ch.len_utf8()];
        let look = overlay.look(Entrance::pop(0.4).with_stagger(0.045), i, 0.12);
        let bob = style::wave(overlay.attract + i as f32 * 0.35, 2.2, motion) * 2.;
        let advance = style::width(letter, logo) + logo.pixel() * (1. + logo.letter_spacing);
        let size = 48. * look.scale;
        style::center(
            letter,
            x + advance * 0.5,
            rect.y + 80. + bob + look.offset[1],
            logo.with_size(size)
                .with_color(fade(GOLD, look.alpha))
                .with_outline(fade(Color::from_hex(0x2a1306), look.alpha))
                .with_pixel_snap(look.scale == 1.),
        );
        x += advance;
    }

    // Goals as pictogram equations with a caption of at most three words.
    let goals = ["LIGHT LANTERNS", "GRAB EMBERS", "RIDE THE LIFT"];
    let tile_w = (rect.w - 48. - 24.) / 3.;
    for (i, caption) in goals.iter().enumerate() {
        let look = overlay.look(Entrance::fade_up(12.).with_stagger(0.06), i, 0.5);
        let a = look.alpha;
        let tile = kit::appear(
            Rect::new(
                rect.x + 24. + i as f32 * (tile_w + 12.),
                rect.y + 108.,
                tile_w,
                104.,
            ),
            look,
        );
        style::tile(tile, a);
        let x = tile.center().x;
        let y = tile.y + 40.;
        let arrow = |x: f32| icons::draw(Icon::ArrowRight, vec2(x, y), 14., fade(MUTED, a));
        match i {
            0 => {
                lantern_glyph(vec2(x - 26., y), 28., false, a);
                arrow(x);
                lantern_glyph(vec2(x + 26., y), 28., true, a);
            }
            1 => {
                for k in 0..3 {
                    gem(vec2(x - 50. + k as f32 * 16., y), 8., fade(ORANGE, a));
                }
                arrow(x + 4.);
                shapes::glow_disc(vec2(x + 32., y), 18., fade(LEAF, 0.35 * a));
                glyph(Glyph::AirJump, vec2(x + 32., y), 26., fade(LEAF, a));
            }
            _ => {
                lift_glyph(vec2(x - 18., y), 30., a);
                glyph(
                    Glyph::Icon(Icon::Flag),
                    vec2(x + 20., y - 2.),
                    24.,
                    fade(LEAF, a),
                );
            }
        }
        style::center(caption, x, tile.y + 86., style::body(16., fade(INK, a)));
    }

    // Controls as key caps beside pictograms; touch shows the on-screen buttons.
    let prompts = overlay.look(Entrance::fade_up(8.), 0, 0.72);
    let pa = prompts.alpha;
    let row_y = rect.y + 244. + prompts.offset[1];
    if hints.touch {
        let buttons = [Icon::ArrowLeft, Icon::ArrowRight, Icon::ArrowUp];
        let mut x = c - 96.;
        icons::draw(Icon::Hand, vec2(x - 20., row_y), 22., fade(MUTED, pa));
        for icon in buttons {
            x += 40.;
            shapes::fill_circle(
                vec2(x, row_y),
                17.,
                fade(Color::new(0.09, 0.14, 0.12, 0.9), pa),
            );
            shapes::stroke_circle(vec2(x, row_y), 17., 1., fade(style::GLASS_EDGE, pa * 4.));
            icons::draw(icon, vec2(x, row_y), 18., fade(INK, pa));
        }
    } else {
        let row = [
            (hints.movement.as_str(), Glyph::Move),
            (hints.jump.text(), Glyph::Icon(Icon::ArrowUp)),
            (hints.pause.text(), Glyph::Icon(Icon::Pause)),
        ];
        let gap = 34.;
        let total: f32 = row
            .iter()
            .map(|(keys, icon)| key_group_width(Some(keys), *icon, ""))
            .sum::<f32>()
            + gap * 2.;
        let mut x = c - total.min(rect.w - 32.) * 0.5;
        for (keys, icon) in row {
            x += key_group(x, row_y, Some(keys), icon, "", MUTED, pa) + gap;
        }
    }

    // Your best, or the three medal targets to chase.
    let best_look = overlay.look(Entrance::fade_up(8.), 0, 0.8);
    let ba = best_look.alpha;
    let best_y = rect.y + 290.;
    if let Some((time, medal)) = best {
        Chip::new(
            Glyph::Icon(Icon::Trophy),
            line!("{time:.2}s"),
            medal_color(medal),
        )
        .draw(vec2(c, best_y), None, ba);
    } else {
        medal_targets(vec2(c, best_y), ba);
    }
    let button = overlay.look(Entrance::pop(0.9), 0, 0.9);
    let keys = (!hints.tap_confirm).then(|| hints.confirm.text());
    let icon = if hints.tap_confirm {
        Glyph::Icon(Icon::Hand)
    } else {
        Glyph::Icon(Icon::Play)
    };
    key_button(
        Rect::new(c - 160., rect.y + 316., 320., 46.),
        keys,
        icon,
        "Start",
        ButtonKind::Primary,
        &overlay.cta,
        button,
    );
}

/// A miniature exit lift: a green frame with a car and its arrow.
fn lift_glyph(at: Vec2, size: f32, alpha: f32) {
    let u = size / 30.;
    let frame = Rect::new(at.x - 10. * u, at.y - 13. * u, 20. * u, 28. * u);
    shapes::fill_rounded_rect(frame, 3. * u, fade(Color::from_hex(0x1a2925), alpha));
    shapes::stroke_rounded_rect(frame, 3. * u, 2. * u, fade(LEAF, alpha));
    shapes::fill_triangle(
        at + vec2(-3. * u, -5. * u),
        at + vec2(5. * u, 0.),
        at + vec2(-3. * u, 5. * u),
        fade(LEAF, alpha),
    );
    shapes::glow_disc(at - vec2(0., 17. * u), 6. * u, fade(LEAF, 0.6 * alpha));
}

pub(super) fn paused(game: &Game, hints: &Hints, overlay: &Overlay) {
    let rect = card_rect(SessionState::Paused);
    let look = overlay.look(Entrance::fade_up(16.).with_duration(0.24), 0, 0.);
    let theme = style::theme().faded(look.alpha);
    let card = kit::appear(rect, look);
    let inner = kit::card(card, Some("Paused"), &theme);
    style::draw(
        quips::pause(game.deaths + game.jumps),
        inner.x,
        inner.y + 18.,
        style::body(16., fade(MUTED, look.alpha)),
    );
    let collected = game.embers.iter().filter(|(_, found)| *found).count();
    let elapsed = game.trial.elapsed();
    let lanterns = game.checkpoints.index();
    stats(
        &[
            (
                Glyph::Icon(Icon::Clock),
                Value::Text(line!("{elapsed:.2}")),
                medal_color(pace(elapsed)),
            ),
            (
                Glyph::Ember,
                Value::Text(line!("{collected}/{}", game.embers.len())),
                ORANGE,
            ),
            (
                Glyph::Lantern(lanterns > 0),
                Value::Text(line!("{lanterns}/2")),
                GOLD,
            ),
            (
                Glyph::Icon(Icon::Skull),
                Value::Text(line!("{}", game.deaths)),
                INK,
            ),
        ],
        Rect::new(inner.x, inner.y + 36., inner.w, 80.),
        overlay,
        0.06,
        look.alpha,
    );
    let buttons = overlay.look(Entrance::fade_up(8.), 0, 0.18);
    let resume_keys = line!(
        "{} / {}",
        first_key(hints.confirm.text()),
        first_key(hints.pause.text())
    );
    let y = inner.y + 138.;
    key_button(
        Rect::new(inner.x, y, inner.w * 0.6 - 6., 46.),
        (!hints.tap_confirm).then_some(resume_keys.as_str()),
        Glyph::Icon(if hints.tap_confirm {
            Icon::Hand
        } else {
            Icon::Play
        }),
        "Resume",
        ButtonKind::Primary,
        &overlay.cta,
        buttons,
    );
    key_button(
        Rect::new(inner.x + inner.w * 0.6 + 6., y, inner.w * 0.4 - 6., 46.),
        Some(hints.restart.text()),
        Glyph::Icon(Icon::Restart),
        "",
        ButtonKind::Secondary,
        &ButtonMotion::default(),
        buttons,
    );
    if hints.keyboard {
        // Controls and settings live in the header: [F2] and a gear.
        let icon = Glyph::Icon(Icon::Gear);
        key_group(
            card.right() - 16. - key_group_width(Some("F2"), icon, ""),
            card.y + 23.,
            Some("F2"),
            icon,
            "",
            MUTED,
            look.alpha,
        );
    }
}

/// The next goal after a finish: the next medal's time to cut, all embers,
/// a nudge toward the moth, a fall-free run, or simply going faster.
pub(super) fn hook(game: &Game) -> Chip {
    let run = summary(game);
    let time = game.trial.elapsed() as f32;
    if let Some((medal, target)) = quips::next_medal(run.medal, MEDALS) {
        Chip::new(Glyph::Medal(medal), line!("−{:.2}s", time - target), LEAF)
    } else if run.embers < run.total_embers {
        Chip::new(Glyph::Ember, line!("ALL {}", run.total_embers), LEAF)
    } else if !run.secret {
        Chip::new(Glyph::Moth, Line::from_str("?"), MOTH)
    } else if run.falls > 0 {
        Chip::new(Glyph::Icon(Icon::Skull), Line::from_str("0"), LEAF)
    } else {
        Chip::new(Glyph::Icon(Icon::Clock), Line::from_str("FASTER?"), LEAF)
    }
}

/// This run against the stored best, as a trophy chip.
pub(super) fn record_line(game: &Game, results: &Results) -> Chip {
    let time = game.trial.elapsed();
    let trophy = Glyph::Icon(Icon::Trophy);
    if results.demo {
        Chip::new(Glyph::Icon(Icon::EyeOff), Line::from_str("DEMO RUN"), MUTED)
    } else if let Some(best) = results.previous_best {
        if results.new_best(time) {
            Chip::new(trophy, Line::from_str("NEW BEST"), GOLD)
                .with_value(line!("−{:.2}s", best - time), LEAF)
        } else {
            Chip::new(trophy, line!("{best:.2}s"), INK)
                .with_value(line!("+{:.2}s", time - best), style::DANGER)
        }
    } else {
        Chip::new(trophy, Line::from_str("FIRST CLEAR"), GOLD)
    }
}

pub(super) fn won(game: &Game, hints: &Hints, juice: &Juice, overlay: &Overlay, results: &Results) {
    // Let the lift rise and the confetti fly before the card settles in.
    let intro = if juice.motion() > 0. {
        juice.celebrate
    } else {
        10.
    };
    let gate = ((intro - 0.45) / 0.25).clamp(0., 1.);
    if gate <= 0. {
        return;
    }
    let rect = card_rect(SessionState::Won);
    let delay = if juice.motion() > 0. { 0.45 } else { 0. };
    let card = overlay.look(Entrance::pop(0.92).with_duration(0.34), 0, delay);
    let alpha = card.alpha.min(gate);
    style::card(kit::appear(rect, card), alpha);
    let c = rect.center().x;
    let run = summary(game);
    let rank = quips::rank(run);
    let row = |i: usize| overlay.look(Entrance::fade_up(10.).with_stagger(0.06), i, delay + 0.1);

    let r0 = row(0);
    let stamp_t = ((overlay.since - delay - 0.18) / 0.22).clamp(0., 1.);
    let stamp = if overlay.motion().is_reduced() {
        1.
    } else {
        1. + 0.7 * (1. - ferravex_play::tween::Ease::BackOut.apply(stamp_t))
    };
    let stamp_alpha = if overlay.motion().is_reduced() {
        r0.alpha
    } else {
        stamp_t.min(1.) * alpha
    };
    shapes::glow_disc(
        vec2(c, rect.y + 62.),
        190.,
        Color::new(1., 0.7, 0.3, 0.16 * stamp_alpha),
    );
    style::center_fit(
        rank.title,
        c,
        rect.y + 78.,
        rect.w - 48.,
        style::display(40. * stamp, fade(GOLD, stamp_alpha)).with_pixel_snap(stamp == 1.),
    );
    let r1 = row(1);
    style::center_fit(
        rank.quip,
        c,
        rect.y + 110. + r1.offset[1],
        rect.w - 48.,
        style::body(16., fade(INK, r1.alpha)),
    );

    let r2 = row(2);
    let time = line!("{:.2}s", overlay.clock());
    let time_style = style::display(40., fade(INK, r2.alpha));
    let time_w = style::width(&line!("{:.2}s", game.trial.elapsed()), time_style);
    let group = 56. + time_w;
    let badge = vec2(c - group * 0.5 + 22., rect.y + 162. + r2.offset[1]);
    medal_badge(badge, 22., run.medal, r2.alpha);
    style::draw(
        &time,
        badge.x + 34.,
        rect.y + 182. + r2.offset[1],
        time_style,
    );

    // Record and next goal side by side as pictogram chips.
    let r3 = row(3);
    let record = record_line(game, results);
    let next = hook(game);
    let (record_w, next_w) = (record.width(false), next.width(true));
    let gap = 16.;
    let start = c - (record_w + gap + next_w) * 0.5;
    let y = rect.y + 228. + r3.offset[1];
    record.draw(vec2(start + record_w * 0.5, y), None, r3.alpha);
    next.draw(
        vec2(start + record_w + gap + next_w * 0.5, y),
        Some(Icon::Target),
        r3.alpha,
    );

    stats(
        &[
            (
                Glyph::Ember,
                Value::Text(line!("{}/{}", run.embers, run.total_embers)),
                ORANGE,
            ),
            (
                Glyph::Icon(Icon::Star),
                Value::Text(line!("{}", game.score.total())),
                GOLD,
            ),
            (
                Glyph::Icon(Icon::Skull),
                Value::Text(line!("{}", run.falls)),
                INK,
            ),
            (
                Glyph::Icon(Icon::Alert),
                Value::Text(line!("{}", run.close_calls)),
                INK,
            ),
            (
                Glyph::Moth,
                if run.secret {
                    Value::Icon(Icon::Check)
                } else {
                    Value::Text(Line::from_str("?"))
                },
                if run.secret { MOTH } else { MUTED },
            ),
        ],
        Rect::new(rect.x + 24., rect.y + 274., rect.w - 48., 84.),
        overlay,
        delay + 0.3,
        alpha,
    );
    let button = overlay.look(Entrance::pop(0.9), 0, delay + 0.55);
    let keys = line!(
        "{} / {}",
        first_key(hints.confirm.text()),
        first_key(hints.restart.text())
    );
    key_button(
        Rect::new(c - 170., rect.y + 392., 340., 48.),
        (!hints.tap_confirm).then_some(keys.as_str()),
        Glyph::Icon(if hints.tap_confirm {
            Icon::Hand
        } else {
            Icon::Restart
        }),
        "Again",
        ButtonKind::Primary,
        &overlay.cta,
        button,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Screens from the fullscreen audit plus common letterbox-free 16:9 sizes.
    const SCREENS: [Vec2; 8] = [
        Vec2::new(1280., 800.),
        Vec2::new(1600., 1000.),
        Vec2::new(390., 844.),
        Vec2::new(844., 390.),
        Vec2::new(320., 480.),
        Vec2::new(1280., 720.),
        Vec2::new(1920., 1080.),
        Vec2::new(700., 394.),
    ];

    fn clear_of_menu(screen: Vec2, rect: Rect) -> bool {
        rect.right() <= screen.x - MENU_CORNER.x || rect.y >= MENU_CORNER.y
    }

    #[test]
    fn fullscreen_menu_corner_stays_clear_of_hud_text_and_controls() {
        for screen in SCREENS {
            if let Some(hud) = compact::CompactHud::for_screen(screen) {
                // Compact HUD draws in unscaled screen units from the origin.
                let right = compact::bar_right(screen.x);
                assert!(right <= screen.x - MENU_CORNER.x, "{screen:?}");
                for button in hud.touch_layout() {
                    let ferravex_render::input::touch::Region::Rect(rect) = button.region else {
                        unreachable!()
                    };
                    assert!(clear_of_menu(screen, rect), "{screen:?} {rect:?}");
                }
                continue;
            }
            let canvas = ferravex_render::canvas::Canvas2D::fit(vec2(WIDTH, HEIGHT), screen);
            let layout = WideLayout::new(canvas, screen);
            let to_screen = |r: Rect| {
                let a = canvas.project(vec2(r.x, r.y));
                let b = canvas.project(vec2(r.right(), r.bottom()));
                Rect::new(a.x, a.y, b.x - a.x, b.y - a.y)
            };
            // The whole right chip, including its text.
            let chip = Rect::new(layout.right - RIGHT_CHIP_W, CHIP_TOP, RIGHT_CHIP_W, CHIP_H);
            assert!(clear_of_menu(screen, to_screen(chip)), "{screen:?}");
            assert!(clear_of_menu(screen, to_screen(layout.pause)), "{screen:?}");
            assert!(layout.right >= WIDTH * 0.75 && layout.pause.x >= WIDTH * 0.7);
            for state in [SessionState::Ready, SessionState::Paused, SessionState::Won] {
                assert!(
                    clear_of_menu(screen, to_screen(card_rect(state))),
                    "{screen:?}"
                );
            }
        }
        // Letterboxed desktop windows keep the authored symmetric 32-unit margin.
        for screen in [vec2(1280., 800.), vec2(1600., 1000.)] {
            let canvas = ferravex_render::canvas::Canvas2D::fit(vec2(WIDTH, HEIGHT), screen);
            assert_eq!(WideLayout::new(canvas, screen).right, WIDTH - 32.);
        }
    }

    #[test]
    fn pace_and_records_follow_authored_targets_and_never_claim_a_demo_best() {
        assert_eq!(pace(0.), Medal::Gold);
        assert_eq!(pace(12.), Medal::Gold);
        assert_eq!(pace(12.01), Medal::Silver);
        assert_eq!(pace(29.9), Medal::Bronze);
        assert_eq!(pace(31.), Medal::Unranked);
        let first = Results::default();
        assert!(first.new_best(10.));
        let chase = Results {
            previous_best: Some(9.5),
            demo: false,
        };
        assert!(!chase.new_best(9.5) && !chase.new_best(10.) && chase.new_best(9.4));
        let demo = Results {
            previous_best: None,
            demo: true,
        };
        assert!(!demo.new_best(1.));
    }

    #[test]
    fn coaching_fades_as_each_lesson_is_learned() {
        let mut game = Game::new(true);
        assert_eq!(coach(&game), Some(Coach::Jump));
        game.jumps = 1;
        assert_eq!(coach(&game), None);
        game.motor = Character::new(feet(vec2(1100., 520.)));
        assert_eq!(coach(&game), Some(Coach::Lantern));
        game.motor = Character::new(feet(vec2(1410., 520.)));
        game.update(Input::default(), 1. / 120.);
        assert_eq!(game.checkpoints.index(), 1);
        assert_eq!(coach(&game), None, "a lit lantern ends that lesson");
    }

    #[test]
    fn finished_run_hooks_always_offer_a_next_goal() {
        let mut game = Game::new(true);
        for _ in 0..2400 {
            game.update(game.autoplay(), 1. / 120.);
            if game.won() {
                break;
            }
        }
        assert_eq!(game.trial.medal(), Some(Medal::Gold));
        let next = hook(&game);
        assert_eq!((next.glyph, next.text.as_str()), (Glyph::Ember, "ALL 12"));
        let results = Results {
            previous_best: Some(game.trial.elapsed() - 0.5),
            demo: false,
        };
        let slower = record_line(&game, &results);
        assert_eq!(
            (slower.text, slower.value),
            (Line::from_str("8.60s"), Line::from_str("+0.50s"))
        );
        assert_eq!(slower.value_color, style::DANGER);
        let improved = Results {
            previous_best: Some(game.trial.elapsed() + 0.25),
            demo: false,
        };
        let best = record_line(&game, &improved);
        assert_eq!(
            (best.text, best.value),
            (Line::from_str("NEW BEST"), Line::from_str("−0.25s"))
        );
        let demo = record_line(
            &game,
            &Results {
                previous_best: None,
                demo: true,
            },
        );
        assert_eq!(demo.text, "DEMO RUN");
        assert_eq!(record_line(&game, &Results::default()).text, "FIRST CLEAR");
        for embers in &mut game.embers {
            embers.1 = true;
        }
        assert_eq!(hook(&game).glyph, Glyph::Moth, "nudge toward the secret");
        game.secret = true;
        assert_eq!(hook(&game).glyph, Glyph::Icon(Icon::Clock));
        game.deaths = 2;
        assert_eq!(hook(&game).text, "0", "a fall-free run is next");
        // Every chip is a pictogram with at most three words.
        for chip in [next, slower, best, demo, hook(&game)] {
            assert!(chip.text.split_whitespace().count() <= 3);
        }
    }

    #[test]
    fn prompts_appear_only_when_useful_and_retire_after_first_use() {
        let mut game = Game::new(true);
        let mut juice = Juice::new();
        assert_eq!(prompt(&game, &juice, false), Some(Prompt::Jump));
        assert_eq!(prompt(&game, &juice, true), None, "veterans skip prompts");
        game.jumps = 1;
        assert_eq!(prompt(&game, &juice, false), None);
        // An unlocked, ready air jump is prompted only in mid-air.
        game.embers = vec![(vec2(102., 540.), false); 3];
        game.update(Input::default(), 1. / 120.);
        assert!(game.feedback.air_jump_unlocked);
        juice.on_tick(&game);
        assert_eq!(prompt(&game, &juice, false), None, "grounded");
        game.motor.grounded = false;
        assert_eq!(prompt(&game, &juice, false), Some(Prompt::AirJump));
        juice.air_jump_taught = true;
        assert_eq!(prompt(&game, &juice, false), None, "taught once");

        // The overlay fades the prompt in and out on render time.
        let mut overlay = Overlay::new(game.session.state());
        overlay.prompt_frame(Some(Prompt::Jump), 0.05);
        let (kind, alpha) = overlay.prompt().unwrap();
        assert_eq!(kind, Prompt::Jump);
        assert!(alpha > 0. && alpha < 1.);
        for _ in 0..30 {
            overlay.prompt_frame(None, 0.05);
        }
        assert_eq!(overlay.prompt(), None);
    }

    #[test]
    fn overlay_opens_play_with_an_iris_and_respawns_with_a_quick_wipe() {
        let mut game = Game::new(false);
        let mut overlay = Overlay::new(game.session.state());
        let policy = FeedbackPolicy::default();
        overlay.frame(&game, 1. / 60., policy, false, false, false);
        assert_eq!(overlay.transition.phase(), TransitionPhase::Idle);
        assert_eq!(overlay.scene, Some(Scene::Title));
        game.menu(false, true, false);
        overlay.frame(&game, 1. / 60., policy, false, false, false);
        assert_eq!(overlay.transition.phase(), TransitionPhase::Revealing);
        assert_eq!(overlay.transition.kind(Motion::Full), TransitionKind::Iris);
        assert_eq!(overlay.scene, Some(Scene::Play));
        for _ in 0..60 {
            overlay.frame(&game, 1. / 60., policy, false, false, false);
        }
        assert_eq!(overlay.transition.phase(), TransitionPhase::Idle);
        overlay.frame(&game, 1. / 60., policy, false, true, false);
        assert!(matches!(
            overlay.transition.kind(Motion::Full),
            TransitionKind::Wipe { .. }
        ));
        // Reduced motion draws transitions as fades and freezes attract drift.
        let calm = FeedbackPolicy::new(true, false, 1.).unwrap();
        overlay.frame(&game, 1. / 60., calm, false, false, false);
        assert_eq!(
            overlay.transition.kind(overlay.motion()),
            TransitionKind::Fade
        );
        assert_eq!(style::wave(3., 2., overlay.motion()), 0.);
    }
}
