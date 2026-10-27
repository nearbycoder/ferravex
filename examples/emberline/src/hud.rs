//! Wide-layout HUD, title/pause/results cards and scene transitions in the
//! authored 1280×720 canvas, drawn with the shared modern UI kit.
//!
//! The play HUD keeps one job per glass chip: where you are and what you can
//! do (left), the clock, medal pace and course track (center), and embers,
//! points and a live chain (right). One toast slot and one coaching prompt keep
//! messages from stacking. Copy uses the Modern face; the clock, counters,
//! logo, rank and pops keep the Classic face's arcade punch. Every steady-state
//! label formats into a stack [`Line`], so drawing a frame allocates nothing.
use super::juice::{GOLD, Juice, MOTH, Toast};
use super::presentation::ControlHint;
use super::quips::{self, RunSummary};
use super::style::{self, EMBER, INK, LEAF, Line, Scene, fade, line};
use super::*;
use ferravex_play::gameplay::trial::Medal;
use ferravex_play::tween::{
    Appearance, Entrance, Motion, RollCounter, ScreenTransition, Spring, TransitionKind,
    TransitionPhase,
};
use ferravex_render::shapes::{self, Fill};
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
    if state == SessionState::Won {
        Rect::new(250., 104., 780., 476.)
    } else {
        Rect::new(290., 150., 700., 380.)
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
    let color = if lit { GOLD } else { MUTED };
    if lit {
        shapes::glow_disc(vec2(x, y - 1.), 14., Color::new(1., 0.62, 0.25, 0.55));
    }
    draw_triangle(
        vec2(x - 8., y - 9.),
        vec2(x + 8., y - 9.),
        vec2(x, y - 15.),
        color,
    );
    draw_rectangle(x - 7., y - 9., 14., 16., Color::new(0.10, 0.14, 0.12, 1.));
    draw_rectangle_lines(x - 7., y - 9., 14., 16., 2., color);
    if lit {
        diamond(vec2(x, y - 1.), 5., ORANGE);
    }
}

/// The one coaching hint a first-time player needs right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Coach {
    Jump,
    Lantern,
    AirJump,
}

impl Coach {
    pub fn text(self) -> &'static str {
        match self {
            Self::Jump => "Jump · hold it for a bigger leap",
            Self::Lantern => "Land by a lantern to save your spot",
            Self::AirJump => "One more ember unlocks an air jump",
        }
    }
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

/// The in-run HUD. `veteran` players (a stored clear) skip onboarding prompts.
pub(super) fn play(
    game: &Game,
    juice: &Juice,
    overlay: &Overlay,
    hints: &Hints,
    veteran: bool,
    cluster: f32,
) {
    let theme = style::theme();
    // Left chip: section, lanterns and the earned air jump.
    let left = Rect::new(24., CHIP_TOP, 286., CHIP_H);
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
    for index in 1..=2 {
        lantern_icon(
            left.x + 26. + (index - 1) as f32 * 24.,
            left.y + 52.,
            game.checkpoints.index() >= index,
        );
    }
    let earned = game.ember_inventory.count(1).min(3);
    let ready = earned >= 3 && !game.air_jump_used;
    for pip in 0..3 {
        let at = vec2(left.x + 88. + pip as f32 * 14., left.y + 48.);
        if pip < earned as usize {
            let color = if game.air_jump_used { MUTED } else { LEAF };
            if ready {
                shapes::glow_disc(at, 10., fade(LEAF, 0.6));
            }
            shapes::fill_circle(at, 4.5, color);
        } else {
            shapes::stroke_circle(at, 4., 1.5, fade(MUTED, 0.8));
        }
    }
    let (air, air_color) = if ready {
        ("Air jump ready", LEAF)
    } else if earned >= 3 {
        ("Air jump used", MUTED)
    } else {
        ("Air jump", MUTED)
    };
    style::draw(
        air,
        left.x + 132.,
        left.y + 54.,
        style::body(16., air_color),
    );

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
    let clock_w = style::center(
        &clock,
        WIDTH * 0.5 - 12.,
        CHIP_TOP + 24.,
        style::arcade(32., medal_color(medal)),
    );
    if medal != Medal::Unranked {
        medal_badge(
            vec2(WIDTH * 0.5 - 12. + clock_w * 0.5 + 20., CHIP_TOP + 12.),
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

    // Right chip: embers, then points and any live chain.
    let right = Rect::new(cluster - RIGHT_CHIP_W, CHIP_TOP, RIGHT_CHIP_W, CHIP_H);
    style::chip(right, 1.);
    let collected = game.embers.iter().filter(|(_, found)| *found).count();
    let embers = line!("{collected}/{}", game.embers.len());
    let ember_w = style::right(
        &embers,
        right.right() - 16.,
        right.y + 30.,
        style::arcade(24., INK),
    );
    let bump = overlay.ember_bump() * juice.motion();
    let gem = vec2(right.right() - 34. - ember_w, right.y + 21.);
    shapes::glow_disc(
        gem,
        16. + bump * 3.,
        Color::new(1., 0.55, 0.2, 0.55 + bump * 0.1),
    );
    diamond(gem, 10. + bump * 1.5, ORANGE);
    let points = line!("{}", overlay.points());
    let pts_w = style::right(
        "pts",
        right.right() - 16.,
        right.y + 56.,
        style::body(16., MUTED),
    );
    let points_w = style::right(
        &points,
        right.right() - 22. - pts_w,
        right.y + 56.,
        style::arcade(16., EMBER),
    );
    if let Some(life) = game
        .score
        .remaining_life()
        .filter(|_| game.score.multiplier() > 1)
    {
        let chain = line!("×{}", game.score.multiplier());
        let x = right.right() - 34. - pts_w - points_w;
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
    if !veteran
        && juice.toast.is_none()
        && let Some(lesson) = coach(game)
    {
        coach_prompt(lesson, hints, 652., 1.);
    }
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

/// The toast pill: an icon, a Modern headline and a muted detail line. It
/// drops in from above and fades out; reduced motion keeps only the fade.
pub(super) fn toast(toast: &Toast, center: f32, top: f32, motion: Motion) {
    let enter = Entrance::fade_up(-14.)
        .with_duration(0.24)
        .sample(toast.age, 0, motion);
    let alpha = toast.alpha().min(enter.alpha);
    let head = style::body(24., fade(INK, alpha));
    let sub = style::body(16., fade(MUTED, alpha));
    let icon_w = 26.;
    let w = (style::width(&toast.head, head) + icon_w)
        .max(style::width(&toast.sub, sub))
        .min(760.)
        + 48.;
    let h = if toast.sub.is_empty() { 44. } else { 68. };
    let rect = Rect::new(center - w * 0.5, top + enter.offset[1], w, h);
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
    shapes::stroke_rounded_rect(rect, 16., 1.5, fade(toast.color, 0.45 * alpha));
    let head_w = style::width(&toast.head, head) + icon_w;
    let x = center - head_w.min(w - 48.) * 0.5;
    let mut buffer = [0_u8; 4];
    style::draw(
        toast.icon.encode_utf8(&mut buffer),
        x,
        top + enter.offset[1] + 32.,
        head.with_color(fade(toast.color, alpha)),
    );
    let fit = if head_w > w - 48. {
        head.with_size(24. * (w - 48. - icon_w) / (head_w - icon_w))
    } else {
        head
    };
    style::draw(&toast.head, x + icon_w, top + enter.offset[1] + 32., fit);
    if !toast.sub.is_empty() {
        style::center_fit(
            &toast.sub,
            center,
            top + enter.offset[1] + 56.,
            w - 32.,
            sub,
        );
    }
}

/// The onboarding prompt as a pill at the bottom of the course.
fn coach_prompt(lesson: Coach, hints: &Hints, top: f32, alpha: f32) {
    let theme = style::theme().faded(alpha);
    let text = lesson.text();
    let keys = if lesson == Coach::Jump && !hints.touch {
        Some(hints.jump.text())
    } else {
        None
    };
    let w = match keys {
        Some(keys) => style::key_prompt_width(keys, text),
        None => style::width(text, style::body(16., INK)) + 22.,
    } + 40.;
    let rect = Rect::new(WIDTH * 0.5 - w * 0.5, top, w, 44.);
    style::chip(rect, alpha);
    match keys {
        Some(keys) => {
            style::key_prompt(rect.x + 20., rect.y + 9., keys, text, &theme);
        }
        None => {
            style::draw(
                "→",
                rect.x + 18.,
                rect.y + 28.,
                style::body(16., fade(EMBER, alpha)),
            );
            style::draw(
                text,
                rect.x + 40.,
                rect.y + 28.,
                style::body(16., fade(INK, alpha)),
            );
        }
    }
}

fn stats(items: &[(&str, Line, Color)], area: Rect, overlay: &Overlay, delay: f32, alpha: f32) {
    let gap = 12.;
    let w = (area.w - gap * (items.len() - 1) as f32) / items.len() as f32;
    for (i, (name, value, color)) in items.iter().enumerate() {
        let look = overlay.look(Entrance::fade_up(10.).with_stagger(0.05), i, delay);
        let rect = kit::appear(
            Rect::new(area.x + i as f32 * (w + gap), area.y, w, area.h),
            look,
        );
        let a = look.alpha * alpha;
        style::tile(rect, a);
        let c = rect.center().x;
        style::center_fit(
            name,
            c,
            rect.y + 24.,
            w - 12.,
            style::body(16., fade(MUTED, a)),
        );
        style::center_fit(
            value,
            c,
            rect.y + 54.,
            w - 12.,
            style::arcade(24., fade(*color, a)),
        );
    }
}

fn cta(label: &str, rect: Rect, overlay: &Overlay, look: Appearance) {
    kit::button(
        kit::appear(rect, look),
        label,
        ButtonKind::Primary,
        &overlay.cta,
        &style::theme().faded(look.alpha),
    );
}

/// The title card: an animated logo, three goals, key prompts and a clear call to action.
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
            rect.y + 78. + bob + look.offset[1],
            logo.with_size(size)
                .with_color(fade(GOLD, look.alpha))
                .with_outline(fade(Color::from_hex(0x2a1306), look.alpha))
                .with_pixel_snap(look.scale == 1.),
        );
        x += advance;
    }
    let tag = overlay.look(Entrance::fade_up(8.), 0, 0.45);
    style::center(
        "A lantern, a long night, and suspiciously many spikes.",
        c,
        rect.y + 108. + tag.offset[1],
        style::body(16., fade(MUTED, tag.alpha)),
    );

    let goals = [
        ("Light 2 lanterns", "to save your spot"),
        ("Grab embers", "3 unlock an air jump"),
        ("Ride the lift", "under 12s for gold"),
    ];
    let tile_w = (rect.w - 48. - 24.) / 3.;
    for (i, (head, sub)) in goals.iter().enumerate() {
        let look = overlay.look(Entrance::fade_up(12.).with_stagger(0.06), i, 0.5);
        let tile = kit::appear(
            Rect::new(
                rect.x + 24. + i as f32 * (tile_w + 12.),
                rect.y + 126.,
                tile_w,
                92.,
            ),
            look,
        );
        style::tile(tile, look.alpha);
        let x = tile.center().x;
        let icon_y = tile.y + 26.;
        match i {
            0 => {
                lantern_icon(x - 12., icon_y + 6., false);
                lantern_icon(x + 12., icon_y + 6., true);
            }
            1 => {
                shapes::glow_disc(
                    vec2(x, icon_y),
                    18.,
                    Color::new(1., 0.55, 0.2, 0.5 * look.alpha),
                );
                diamond(vec2(x, icon_y), 11., fade(ORANGE, look.alpha));
            }
            _ => {
                shapes::stroke_rounded_rect(
                    Rect::new(x - 11., icon_y - 13., 22., 26.),
                    3.,
                    2.,
                    fade(LEAF, look.alpha),
                );
                shapes::glow_disc(vec2(x, icon_y - 18.), 10., fade(LEAF, 0.6 * look.alpha));
            }
        }
        style::center(
            head,
            x,
            tile.y + 62.,
            style::body(16., fade(INK, look.alpha)),
        );
        style::center(
            sub,
            x,
            tile.y + 80.,
            style::body(16., fade(MUTED, look.alpha)),
        );
    }

    // Key prompts for the current device; touch shows the on-screen buttons.
    let prompts = overlay.look(Entrance::fade_up(8.), 0, 0.72);
    let theme = style::theme().faded(prompts.alpha);
    if hints.touch {
        style::center(
            "Use the on-screen ◀ ▶ and jump buttons",
            c,
            rect.y + 254.,
            style::body(16., fade(INK, prompts.alpha)),
        );
    } else {
        let row = [
            (hints.movement.as_str(), "Move"),
            (hints.jump.text(), "Jump"),
            (hints.pause.text(), "Pause"),
        ];
        let gap = 28.;
        let total: f32 = row
            .iter()
            .map(|(keys, action)| style::key_prompt_width(keys, action))
            .sum::<f32>()
            + gap * 2.;
        let mut x = c - total.min(rect.w - 32.) * 0.5;
        for (keys, action) in row {
            x +=
                style::key_prompt(x, rect.y + 236. + prompts.offset[1], keys, action, &theme) + gap;
        }
    }
    let best_look = overlay.look(Entrance::fade_up(8.), 0, 0.8);
    if let Some((time, medal)) = best {
        let text = line!("★ Best {time:.2}s · {}", quips::medal_label(medal));
        style::center(
            &text,
            c,
            rect.y + 296.,
            style::body(16., fade(medal_color(medal), best_look.alpha)),
        );
    } else {
        style::center(
            "Gold under 12s · silver 18s · bronze 30s",
            c,
            rect.y + 296.,
            style::body(16., fade(MUTED, best_look.alpha)),
        );
    }
    let button = overlay.look(Entrance::pop(0.9), 0, 0.9);
    let label = if hints.tap_confirm {
        line!("Tap to start")
    } else {
        line!("Press {} to start", hints.confirm.text())
    };
    cta(
        &label,
        Rect::new(c - 160., rect.y + 314., 320., 44.),
        overlay,
        button,
    );
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
    stats(
        &[
            ("Time", line!("{elapsed:.2}"), medal_color(pace(elapsed))),
            ("Embers", line!("{collected}/{}", game.embers.len()), ORANGE),
            ("Lanterns", line!("{}/2", game.checkpoints.index()), GOLD),
            ("Falls", line!("{}", game.deaths), INK),
        ],
        Rect::new(inner.x, inner.y + 36., inner.w, 76.),
        overlay,
        0.06,
        look.alpha,
    );
    let buttons = overlay.look(Entrance::fade_up(8.), 0, 0.18);
    let label = if hints.tap_confirm {
        line!("Tap to resume")
    } else {
        line!("{} / {}  Resume", hints.confirm.text(), hints.pause.text())
    };
    let y = inner.y + 136.;
    cta(
        &label,
        Rect::new(inner.x, y, inner.w * 0.6 - 6., 44.),
        overlay,
        buttons,
    );
    let restart = line!("{}  Restart", hints.restart.text());
    kit::button(
        kit::appear(
            Rect::new(inner.x + inner.w * 0.6 + 6., y, inner.w * 0.4 - 6., 44.),
            buttons,
        ),
        &restart,
        ButtonKind::Secondary,
        &ButtonMotion::default(),
        &style::theme().faded(buttons.alpha),
    );
    if hints.keyboard {
        style::key_prompt(
            inner.x,
            inner.y + inner.h - 26.,
            "F2",
            "Controls & settings",
            &style::theme().faded(buttons.alpha),
        );
    }
}

/// The hook line under the result: next medal, PB chase, or a secret nudge.
pub(super) fn hook(game: &Game) -> (Line, Color) {
    let run = summary(game);
    let time = game.trial.elapsed() as f32;
    if let Some((name, target)) = quips::next_medal(run.medal, MEDALS) {
        (line!("{:.2}s faster for {name}!", time - target), LEAF)
    } else if run.embers < run.total_embers {
        (
            line!("Gold! Now try all {} embers.", run.total_embers),
            LEAF,
        )
    } else if !run.secret {
        (
            Line::from_str("Psst… something flutters above the Furnace."),
            MOTH,
        )
    } else if run.falls > 0 {
        (
            Line::from_str("Now do it without falling. No pressure."),
            LEAF,
        )
    } else {
        (Line::from_str("Nothing left to prove. Except speed."), LEAF)
    }
}

pub(super) fn record_line(game: &Game, results: &Results) -> (Line, Color) {
    let time = game.trial.elapsed();
    if results.demo {
        (Line::from_str("Demo run · not recorded"), MUTED)
    } else if let Some(best) = results.previous_best {
        if results.new_best(time) {
            (line!("★ New personal best! −{:.2}s", best - time), GOLD)
        } else {
            (
                line!("Personal best {best:.2}s  (+{:.2}s)", time - best),
                INK,
            )
        }
    } else {
        (
            Line::from_str("★ First clear! That's your time to beat."),
            GOLD,
        )
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
    style::center(
        "The light carries on",
        c,
        rect.y + 40. + r0.offset[1],
        style::body(16., fade(LEAF, r0.alpha)),
    );
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
        vec2(c, rect.y + 80.),
        190.,
        Color::new(1., 0.7, 0.3, 0.16 * stamp_alpha),
    );
    style::center_fit(
        rank.title,
        c,
        rect.y + 92.,
        rect.w - 48.,
        style::display(40. * stamp, fade(GOLD, stamp_alpha)).with_pixel_snap(stamp == 1.),
    );
    let r1 = row(1);
    style::center_fit(
        rank.quip,
        c,
        rect.y + 122. + r1.offset[1],
        rect.w - 48.,
        style::body(16., fade(INK, r1.alpha)),
    );

    let r2 = row(2);
    let time = line!("{:.2}s", overlay.clock());
    let time_style = style::display(40., fade(INK, r2.alpha));
    let time_w = style::width(&line!("{:.2}s", game.trial.elapsed()), time_style);
    let group = 56. + time_w;
    let badge = vec2(c - group * 0.5 + 22., rect.y + 170. + r2.offset[1]);
    medal_badge(badge, 22., run.medal, r2.alpha);
    style::draw(
        &time,
        badge.x + 34.,
        rect.y + 190. + r2.offset[1],
        time_style,
    );
    let medal_text = line!("{} medal", quips::medal_label(run.medal));
    let medal_text = if run.medal == Medal::Unranked {
        Line::from_str("No medal yet")
    } else {
        medal_text
    };
    style::center(
        &medal_text,
        c,
        rect.y + 222. + r2.offset[1],
        style::body(16., fade(medal_color(run.medal), r2.alpha)),
    );

    let r3 = row(3);
    let (record, record_color) = record_line(game, results);
    let record_style = style::body(16., fade(record_color, r3.alpha));
    let pill_w = style::width(&record, record_style).min(rect.w - 80.) + 32.;
    let pill = Rect::new(c - pill_w * 0.5, rect.y + 236. + r3.offset[1], pill_w, 28.);
    shapes::fill_rounded_rect(pill, 14., fade(record_color, 0.12 * r3.alpha));
    shapes::stroke_rounded_rect(pill, 14., 1., fade(record_color, 0.35 * r3.alpha));
    style::center_fit(&record, c, pill.y + 19., pill_w - 24., record_style);
    let (hook_text, hook_color) = hook(game);
    style::center_fit(
        &hook_text,
        c,
        rect.y + 290. + r3.offset[1],
        rect.w - 48.,
        style::body(16., fade(hook_color, r3.alpha)),
    );

    stats(
        &[
            (
                "Embers",
                line!("{}/{}", run.embers, run.total_embers),
                ORANGE,
            ),
            ("Points", line!("{}", game.score.total()), ORANGE),
            ("Falls", line!("{}", run.falls), INK),
            ("Close calls", line!("{}", run.close_calls), INK),
            (
                "Moth",
                Line::from_str(if run.secret { "★" } else { "?" }),
                if run.secret { MOTH } else { MUTED },
            ),
        ],
        Rect::new(rect.x + 24., rect.y + 306., rect.w - 48., 76.),
        overlay,
        delay + 0.3,
        alpha,
    );
    let button = overlay.look(Entrance::pop(0.9), 0, delay + 0.55);
    let label = if hints.tap_confirm {
        line!("Tap to run again")
    } else {
        line!(
            "{} / {}  Run again",
            hints.confirm.text(),
            hints.restart.text()
        )
    };
    cta(
        &label,
        Rect::new(c - 170., rect.y + 404., 340., 46.),
        overlay,
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
        assert_eq!(hook(&game).0, "Gold! Now try all 12 embers.");
        let results = Results {
            previous_best: Some(game.trial.elapsed() - 0.5),
            demo: false,
        };
        assert_eq!(
            record_line(&game, &results).0,
            "Personal best 8.60s  (+0.50s)"
        );
        let improved = Results {
            previous_best: Some(game.trial.elapsed() + 0.25),
            demo: false,
        };
        assert_eq!(
            record_line(&game, &improved).0,
            "★ New personal best! −0.25s"
        );
        for embers in &mut game.embers {
            embers.1 = true;
        }
        assert!(hook(&game).0.contains("flutters"));
        game.secret = true;
        assert!(hook(&game).0.contains("speed"));
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
