//! Read-only procedural railway art, the departure board and the switchboard.
//!
//! Drawing reads the model plus the host's presentation clock and pointer and
//! never feeds back into the rules. State is shown rather than told: lit
//! lamps, point blades, lettered destination badges with draining due rings,
//! platform pips and a dashed preview of where each train is about to run.
use crate::content::{SHIFTS, Service, Side};
use crate::model::{
    CONTROL_POINTS, CONTROL_SIGNAL, EDGES, Event, Game, Incident, Phase, SIGNAL_CONTROL, TrainState,
};
use ferravex_arcade::shapes::{self, Fill, Shadow};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;

const BRASS: Color = Color::new(0.92, 0.73, 0.40, 1.);
const CREAM: Color = Color::new(0.94, 0.90, 0.77, 1.);
const WOOD: Color = Color::new(0.19, 0.15, 0.13, 1.);
const GREEN_LAMP: Color = Color::new(0.40, 0.91, 0.66, 1.);
const RED_LAMP: Color = Color::new(0.95, 0.40, 0.34, 1.);
const SOFT: Color = Color::new(0.70, 0.74, 0.70, 1.);
const DIM: Color = Color::new(0.33, 0.36, 0.37, 1.);
const BOARD: Color = Color::new(0.075, 0.10, 0.11, 1.);
const GOLD: Color = Color::new(0.98, 0.80, 0.36, 1.);
const DESTINATIONS: [Color; 4] = [
    Color::new(0.95, 0.73, 0.42, 1.),
    Color::new(0.55, 0.82, 0.55, 1.),
    Color::new(0.65, 0.75, 0.98, 1.),
    Color::new(0.90, 0.57, 0.67, 1.),
];
/// Platform names for the station plates; the letter disc carries the code.
const STATIONS: [&str; 4] = ["Abbey", "Orchard", "Quarry", "Harbour"];
/// Medal tone and name for one, two and three stars.
const MEDALS: [(Color, &str); 3] = [
    (Color::new(0.84, 0.58, 0.38, 1.), "Bronze"),
    (Color::new(0.80, 0.84, 0.88, 1.), "Silver"),
    (GOLD, "Gold"),
];
/// Each shift's new challenge as an icon and a short headline.
const CHALLENGES: [(Icon, &str); 8] = [
    (Icon::Map, "Learn the routes"),
    (Icon::Alert, "Opposing trains"),
    (Icon::Crate, "Slow freight"),
    (Icon::Lock, "Bridge inspection"),
    (Icon::Clock, "Tight timetable"),
    (Icon::Rain, "Rain"),
    (Icon::Snowflake, "Snow"),
    (Icon::Crown, "Final shift"),
];
/// Switchboard captions; the pictograms carry each control's state.
const CAPTIONS: [&str; 6] = ["Gate", "Points", "Bridge", "Bridge", "Points", "Gate"];

const MAP: Rect = Rect {
    x: 20.,
    y: 120.,
    w: 704.,
    h: 389.,
};
/// The departure board card.
const PANEL: Rect = Rect {
    x: 734.,
    y: 120.,
    w: 206.,
    h: 385.,
};
const ROWS_TOP: f32 = 128.;
const ROWS_BOTTOM: f32 = 438.;
/// Center of the run/plan clock dial (its hit area is `Game::clock_button`).
const DIAL: Vec2 = vec2(763., 540.5);
/// The closing-bell readout beside the dial.
const BELL: Vec2 = vec2(832., 540.5);

/// Flat text for labels on solid surfaces.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
/// Left-aligned label vertically centered on `cy`; returns its width.
fn label_mid(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Right-aligned label vertically centered on `cy`; returns its width.
fn label_end(value: &str, right: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color).with_align(TextAlign::Right);
    draw_text_styled(value, right, s.centered_baseline(cy), s).width
}
/// Centered label vertically centered on `cy`.
fn label_center(value: &str, cx: f32, cy: f32, size: f32, color: Color) {
    let s = ink(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s);
}
fn letter(destination: usize) -> char {
    (b'A' + destination.min(25) as u8) as char
}
/// `4560` as `4,560`.
fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
/// Whole seconds as `m:ss`.
fn clock_text(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn service_icon(service: Service) -> Icon {
    match service {
        Service::Local => Icon::People,
        Service::Mail => Icon::Bolt,
        Service::Freight => Icon::Crate,
    }
}
/// Icon for a shift whose trains run at other than normal speed.
fn speed_icon(shift: usize) -> Icon {
    match shift {
        5 => Icon::Rain,
        6 => Icon::Snowflake,
        7 => Icon::Crown,
        _ => Icon::Sun,
    }
}
fn medal(stars: u8) -> (Color, &'static str) {
    MEDALS[usize::from(stars.clamp(1, 3)) - 1]
}

fn line(a: Vec2, b: Vec2, width: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, width, color);
}

/// Soft lamp halo: a cheap radial fill rather than an additive glow.
fn halo(at: Vec2, radius: f32, color: Color, strength: f32) {
    shapes::fill_circle(
        at,
        radius,
        Fill::radial(
            at,
            radius,
            with_alpha(color, strength),
            with_alpha(color, 0.),
        ),
    );
}

/// Moving dashes from `a` to `b`; `along` is the route distance at `a` and
/// `phase` slides the pattern forward so the line reads as travel.
fn dashes(a: Vec2, b: Vec2, along: f32, phase: f32, width: f32, color: Color) {
    const PERIOD: f32 = 15.;
    const DASH: f32 = 8.;
    let length = a.distance(b);
    if length < 0.5 {
        return;
    }
    let direction = (b - a) / length;
    let mut s = (phase - along).rem_euclid(PERIOD) - PERIOD;
    while s < length {
        let (from, to) = (s.max(0.), (s + DASH).min(length));
        if to > from + 0.5 {
            shapes::fill_capsule(a + direction * from, a + direction * to, width * 0.5, color);
        }
        s += PERIOD;
    }
}

/// A destination disc with its letter, the identity every train carries.
fn destination_disc(at: Vec2, destination: usize, radius: f32, alpha: f32) {
    let color = DESTINATIONS[destination.min(3)];
    shapes::fill_circle(
        at,
        radius,
        Fill::radial(
            at - vec2(radius * 0.35, radius * 0.35),
            radius * 1.5,
            with_alpha(lighten(color, 0.22), alpha),
            with_alpha(color, alpha),
        ),
    );
    let s = ink(radius * 1.3, with_alpha(INK, alpha)).with_align(TextAlign::Center);
    let mut buffer = [0u8; 4];
    let text = letter(destination).encode_utf8(&mut buffer);
    draw_text_styled(text, at.x + 0.5, s.centered_baseline(at.y), s);
}

/// Two-aspect signal head: red over green, the lit lamp glowing.
fn signal_head(at: Vec2, green: bool, scale: f32, t: f32) {
    let (w, h) = (14. * scale, 28. * scale);
    let body = Rect::new(at.x - w * 0.5, at.y - h * 0.5, w, h);
    shapes::fill_rounded_rect(
        Rect::new(body.x + 1., body.y + 2., w, h),
        w * 0.5,
        Color::new(0., 0., 0., 0.35),
    );
    shapes::fill_rounded_rect(body, w * 0.5, Color::new(0.05, 0.06, 0.07, 1.));
    shapes::stroke_rounded_rect(body, w * 0.5, 1., Color::new(1., 1., 1., 0.12));
    let radius = 4.4 * scale;
    let red = at - vec2(0., h * 0.24);
    let go = at + vec2(0., h * 0.24);
    shapes::fill_circle(red, radius, Color::new(0.32, 0.11, 0.10, 1.));
    shapes::fill_circle(go, radius, Color::new(0.08, 0.24, 0.17, 1.));
    let (lit, color) = if green {
        (go, GREEN_LAMP)
    } else {
        (red, RED_LAMP)
    };
    let breathe = 0.85 + 0.15 * pulse(t, 0.6);
    halo(
        lit,
        13. * scale * breathe,
        color,
        if green { 0.55 } else { 0.4 },
    );
    shapes::fill_circle(
        lit,
        radius,
        Fill::radial(
            lit - vec2(1., 1.) * scale,
            radius * 1.4,
            lighten(color, 0.5),
            color,
        ),
    );
}

/// A small dark disc with an icon, for locks and markers on the map.
fn marker(icon_kind: Icon, at: Vec2, radius: f32, color: Color) {
    shapes::fill_circle(at + vec2(0., 1.5), radius, Color::new(0., 0., 0., 0.35));
    shapes::fill_circle(at, radius, Color::new(0.05, 0.06, 0.07, 0.95));
    shapes::stroke_circle(at, radius, 1.2, with_alpha(color, 0.8));
    icon(icon_kind, at.x, at.y, radius * 1.15, color);
}

/// A wide call-to-action matching the host's buttons, lifted on hover.
fn action_button(key: &str, action: &str, base: Color, hovered: bool) {
    let lift = if hovered { 2. } else { 0. };
    let r = Rect::new(99., 444. - lift, 578., 40.);
    shapes::drop_shadow(
        r,
        9.,
        Shadow::soft(Color::new(0., 0., 0., if hovered { 0.5 } else { 0.35 }))
            .offset(vec2(0., 3. + lift))
            .blur(10.),
    );
    let base = if hovered { lighten(base, 0.1) } else { base };
    shapes::fill_rounded_rect(r, 9., Fill::vertical(r, lighten(base, 0.12), base));
    shapes::stroke_rounded_rect(
        r,
        9.,
        1.,
        Color::new(1., 1., 1., if hovered { 0.3 } else { 0.12 }),
    );
    let s = ink(17., CREAM);
    let key_w = (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.);
    let total = key_w + 10. + text_width(action, s);
    let left = r.x + (r.w - total) * 0.5;
    keycap(key, "", left, r.y + r.h * 0.5 - 11., CREAM);
    draw_text_styled(
        action,
        left + key_w + 10.,
        s.centered_baseline(r.y + r.h * 0.5),
        s,
    );
}

fn track(a: Vec2, b: Vec2, active: bool, glint: f32) {
    let direction = (b - a).normalize_or_zero();
    let normal = direction.perp();
    // Ballast bed with a darker shoulder.
    line(a, b, 19., Color::new(0.14, 0.17, 0.16, 0.55));
    line(a, b, 15., Color::new(0.27, 0.29, 0.27, 1.));
    let ties = (a.distance(b) / 12.).ceil() as usize;
    for step in 0..=ties {
        let at = a.lerp(b, step as f32 / ties.max(1) as f32);
        line(
            at - normal * 8.,
            at + normal * 8.,
            3.4,
            Color::new(0.42, 0.33, 0.25, 1.),
        );
    }
    let rail = if active {
        lighten(BRASS, 0.08 + glint * 0.12)
    } else {
        Color::new(0.64, 0.68, 0.65, 1.)
    };
    for side in [-3.7, 3.7] {
        line(
            a + normal * (side + 0.9),
            b + normal * (side + 0.9),
            1.4,
            Color::new(0., 0., 0., 0.35),
        );
        line(a + normal * side, b + normal * side, 1.9, rail);
    }
}

/// What the pointer is over this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hover {
    None,
    /// A switchboard control, or its lamp or points on the map.
    Control(usize),
    /// A train on the map.
    Train(usize),
    /// A departure board row.
    Row(usize),
    /// The run/plan clock.
    Clock,
    /// The briefing, shift-cleared or incident card's button.
    Main,
}

/// Per-frame presentation context.
struct Ui {
    t: f32,
    hover: Hover,
    /// The service whose route is previewed and highlighted.
    focus: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Home,
    Wrong,
    Exit,
}

/// Where a train will run with some point settings: a polyline from its
/// current position, the vertex where a red signal will hold it, how the run
/// ends and the first points that send it the wrong way.
struct Route {
    pts: [Vec2; 12],
    len: usize,
    hold: Option<usize>,
    end: End,
    fix: Option<usize>,
}

fn row_height(count: usize) -> f32 {
    ((ROWS_BOTTOM - ROWS_TOP) / count.max(1) as f32).min(40.)
}

impl Game {
    fn ui(&self) -> Ui {
        let hover = self.hover_at(pointer());
        Ui {
            t: clock(),
            hover,
            focus: self.focus(hover),
        }
    }

    fn hover_at(&self, p: Vec2) -> Hover {
        match self.phase {
            Phase::Briefing | Phase::Cleared | Phase::Incident => {
                return if Game::main_button(p) {
                    Hover::Main
                } else {
                    Hover::None
                };
            }
            Phase::Working => {}
            Phase::Won | Phase::Lost => return Hover::None,
        }
        if let Some(control) = Game::control_at(p).or_else(|| self.map_control_at(p)) {
            return Hover::Control(control);
        }
        if Game::clock_button(p) {
            return Hover::Clock;
        }
        let count = self.schedule().trains.len();
        if let Some(train) = (0..count).find(|&i| {
            self.position(i).is_some_and(|at| {
                at.distance_squared(p) < 16. * 16.
                    || (at + vec2(0., -24.)).distance_squared(p) < 12. * 12.
            })
        }) {
            return Hover::Train(train);
        }
        let h = row_height(count);
        if (PANEL.x + 6. ..PANEL.x + PANEL.w - 6.).contains(&p.x) && p.y >= ROWS_TOP {
            let row = ((p.y - ROWS_TOP) / h) as usize;
            if row < count {
                return Hover::Row(row);
            }
        }
        Hover::None
    }

    /// The hovered service; else the first on the railway that is heading
    /// the wrong way; else the first on the railway; else the next to enter.
    fn focus(&self, hover: Hover) -> Option<usize> {
        let count = self.schedule().trains.len();
        let open = |i: usize| self.trains[i].state != TrainState::Delivered;
        if let Hover::Train(i) | Hover::Row(i) = hover
            && open(i)
        {
            return Some(i);
        }
        (0..count)
            .find(|&i| {
                self.live(i)
                    && self
                        .route(i, self.points)
                        .is_some_and(|route| route.end != End::Home)
            })
            .or_else(|| (0..count).find(|&i| self.live(i)))
            .or_else(|| (0..count).find(|&i| open(i)))
    }

    fn live(&self, train: usize) -> bool {
        matches!(
            self.trains[train].state,
            TrainState::At(_) | TrainState::Moving { .. }
        )
    }

    fn is_late(&self, train: usize) -> bool {
        self.trains[train].state != TrainState::Delivered
            && self.time > self.schedule().trains[train].due * 60
    }

    /// Fraction of a service's release-to-due window still remaining.
    fn due_left(&self, train: usize) -> f32 {
        let plan = self.schedule().trains[train];
        let (release, due) = (plan.release * 60, plan.due * 60);
        if self.time >= due {
            return 0.;
        }
        (due - self.time.max(release)) as f32 / due.saturating_sub(release).max(1) as f32
    }

    /// The red signal holding a train, if it is waiting at one.
    fn held_by(&self, train: usize) -> Option<usize> {
        let TrainState::At(node) = self.trains[train].state else {
            return None;
        };
        let east = self.schedule().trains[train].side == Side::West;
        Game::signal_at(node, east).filter(|s| !self.signals[*s])
    }

    /// Delay already certain this shift, in ticks: banked lateness plus the
    /// lateness of every service still out. It only grows, so the stars it
    /// implies are a promise, not a guess.
    fn projected_delay(&self) -> u32 {
        let trains = self.schedule().trains;
        self.delays
            + (0..trains.len())
                .filter(|&i| self.trains[i].state != TrainState::Delivered)
                .map(|i| self.time.saturating_sub(trains[i].due * 60))
                .sum::<u32>()
    }

    fn route(&self, train: usize, points: [usize; 2]) -> Option<Route> {
        let plan = self.schedule().trains[train];
        let east = plan.side == Side::West;
        let nodes = self.nodes();
        let source = if east { 0 } else { 7 };
        let (start, mut node) = match self.trains[train].state {
            TrainState::Delivered => return None,
            TrainState::Waiting => (nodes[source], source),
            TrainState::At(n) => (nodes[n], n),
            TrainState::Moving { to, .. } => (self.position(train)?, to),
        };
        let mut route = Route {
            pts: [start; 12],
            len: 1,
            hold: None,
            end: End::Exit,
            fix: None,
        };
        if nodes[node] != start {
            route.pts[1] = nodes[node];
            route.len = 2;
        }
        let want = Game::desired_points(plan.destination);
        for _ in 0..10 {
            if node >= 8 {
                route.end = if node - 8 == plan.destination {
                    End::Home
                } else {
                    End::Wrong
                };
                break;
            }
            if (node == 0 && !east) || (node == 7 && east) {
                route.end = End::Exit;
                break;
            }
            if route.hold.is_none() && Game::signal_at(node, east).is_some_and(|s| !self.signals[s])
            {
                route.hold = Some(route.len - 1);
            }
            let next = Game::next_node(points, node, east);
            if route.fix.is_none()
                && matches!(node, 2 | 5)
                && next != Game::next_node(want, node, east)
            {
                route.fix = Some(usize::from(node == 5));
            }
            node = next;
            if route.len < route.pts.len() {
                route.pts[route.len] = nodes[node];
                route.len += 1;
            }
        }
        Some(route)
    }

    /// Points that would rescue the focus service from a wrong route.
    fn hint(&self, ui: &Ui) -> Option<usize> {
        if self.phase != Phase::Working {
            return None;
        }
        let train = ui.focus?;
        let route = self.route(train, self.points)?;
        let destination = self.schedule().trains[train].destination;
        let savable = self
            .route(train, Game::desired_points(destination))
            .is_some_and(|r| r.end == End::Home);
        (route.end != End::Home && savable)
            .then_some(route.fix)
            .flatten()
    }

    fn scene(&self) {
        let ui = self.ui();
        self.landscape();
        self.railway(&ui);
        self.routes(&ui);
        self.furniture(&ui);
        self.locomotives(&ui);
        self.planning_veil(&ui);
        self.board(&ui);
        self.switchboard(&ui);
        self.clock_face(&ui);
        match self.phase {
            Phase::Briefing => self.briefing(&ui),
            Phase::Cleared => self.cleared(&ui),
            Phase::Incident => self.incident_card(&ui),
            _ => {}
        }
    }

    fn landscape(&self) {
        let snow = self.shift == 6;
        let rain = self.shift == 5;
        let (top, bottom) = if snow {
            (
                Color::new(0.64, 0.71, 0.72, 1.),
                Color::new(0.53, 0.61, 0.63, 1.),
            )
        } else if rain {
            (
                Color::new(0.19, 0.27, 0.29, 1.),
                Color::new(0.14, 0.21, 0.23, 1.),
            )
        } else {
            (
                Color::new(0.27, 0.37, 0.31, 1.),
                Color::new(0.21, 0.31, 0.26, 1.),
            )
        };
        rounded(
            MAP.x - 1.,
            MAP.y + 2.,
            MAP.w + 2.,
            MAP.h + 3.,
            13.,
            Color::new(0., 0., 0., 0.35),
        );
        shapes::fill_rounded_rect(MAP, 12., Fill::vertical(MAP, top, bottom));
        // Field boundaries and furrows keep the landscape authored and quiet.
        for (x, y, w, h) in [
            (39., 190., 127., 61.),
            (41., 410., 119., 63.),
            (590., 201., 109., 47.),
        ] {
            let field = if snow {
                Color::new(0.72, 0.78, 0.76, 1.)
            } else {
                Color::new(0.35, 0.40, 0.29, 1.)
            };
            rounded(x, y, w, h, 6., field);
            for n in 0..7 {
                let fy = y + 7. + n as f32 * 7.;
                draw_line(
                    x + 6.,
                    fy,
                    x + w - 6.,
                    fy,
                    1.,
                    Color::new(0.47, 0.49, 0.34, 0.4),
                );
            }
        }
        // River with a darker channel and ripples that drift with railway time.
        let river = Rect::new(377., 121., 47., 387.);
        shapes::fill_rounded_rect(
            river,
            0.,
            Fill::horizontal(
                river,
                Color::new(0.13, 0.30, 0.36, 1.),
                Color::new(0.17, 0.37, 0.43, 1.),
            ),
        );
        panel(377., 121., 2., 387., Color::new(0.30, 0.42, 0.36, 0.7));
        panel(422., 121., 2., 387., Color::new(0.30, 0.42, 0.36, 0.7));
        let flow = (self.time / 4 % 15) as f32;
        for i in 0..25 {
            let y = 128. + (i as f32 * 15. + flow) % 375.;
            let x = 384. + (i % 3) as f32 * 8.;
            draw_line(
                x,
                y,
                x + 13.,
                y - 2.5,
                1.3,
                Color::new(0.58, 0.78, 0.80, 0.35),
            );
        }
        let nodes = self.nodes();
        for i in 0..26 {
            let x = 44. + ((i * 83 + self.shift * 19) % 646) as f32;
            let y = 152. + ((i * 53) % 327) as f32;
            // Keep vegetation away from rails, stations and the yard queues.
            let p = vec2(x, y);
            let near_track = EDGES.iter().any(|&(a, b)| {
                let edge = nodes[b] - nodes[a];
                let t = ((p - nodes[a]).dot(edge) / edge.length_squared()).clamp(0., 1.);
                p.distance_squared(nodes[a] + edge * t) < 30. * 30.
            });
            let near_yard = [nodes[0], nodes[7]]
                .iter()
                .any(|yard| (x - yard.x).abs() < 24. && y > yard.y && y < yard.y + 110.);
            if near_track || near_yard || (x - 400.).abs() < 40. {
                continue;
            }
            draw_poly(
                x + 3.,
                y + 5.,
                12,
                10.,
                0.,
                Color::new(0.04, 0.08, 0.07, 0.35),
            );
            draw_rectangle(x - 1., y + 1., 3., 9., Color::new(0.32, 0.24, 0.17, 1.));
            let (leaf, light) = if snow {
                (Color::new(0.80, 0.85, 0.82, 1.), CREAM)
            } else {
                (
                    Color::new(0.24, 0.42, 0.31, 1.),
                    Color::new(0.43, 0.61, 0.40, 1.),
                )
            };
            draw_poly(x, y, 16, 10., 0., leaf);
            draw_poly(x - 2.5, y - 2.5, 12, 6.5, 0., lighten(leaf, 0.08));
            draw_poly(x - 4., y - 4.5, 8, 3., 0., light);
        }
        // Weather drifts with the railway clock, so planning freezes it too.
        if rain || snow {
            for i in 0..34_u32 {
                let x = 30. + ((i * 97 + 13) % 684) as f32;
                let fall = (self.time * if snow { 1 } else { 5 } + i * 37) % 360;
                let y = 128. + fall as f32;
                if snow {
                    let sway = ((self.time / 20 + i) % 6) as f32 - 3.;
                    draw_rectangle(x + sway, y, 2., 2., Color::new(1., 1., 1., 0.7));
                } else {
                    draw_line(
                        x,
                        y,
                        x - 3.,
                        y + 10.,
                        1.,
                        Color::new(0.70, 0.82, 0.90, 0.35),
                    );
                }
            }
        }
        // The single-track bridge.
        let bridge_y = self.schedule().bridge_y;
        rounded(
            347.,
            bridge_y - 19.,
            105.,
            44.,
            4.,
            Color::new(0., 0., 0., 0.25),
        );
        panel(
            349.,
            bridge_y - 16.,
            101.,
            32.,
            Color::new(0.40, 0.37, 0.32, 1.),
        );
        for i in 0..13 {
            let x = 351. + i as f32 * 8.;
            draw_line(
                x,
                bridge_y - 15.,
                x,
                bridge_y + 15.,
                2.,
                Color::new(0.62, 0.55, 0.41, 1.),
            );
        }
        for side in [-18., 18.] {
            draw_line(
                350.,
                bridge_y + side,
                450.,
                bridge_y + side,
                3.,
                Color::new(0.66, 0.70, 0.66, 1.),
            );
        }
    }

    /// Track, stations with their platform pips, and the yard queues.
    fn railway(&self, ui: &Ui) {
        let nodes = self.nodes();
        let glint = ((self.time / 6) % 20) as f32 / 20.;
        let glint = 1. - (glint * 2. - 1.).abs();
        for (a, b) in EDGES {
            let active = match (a, b) {
                (2, 3) => self.points[0] == 0,
                (2, 8) => self.points[0] == 1,
                (2, 9) => self.points[0] == 2,
                (4, 5) => self.points[1] == 0,
                (5, 10) => self.points[1] == 1,
                (5, 11) => self.points[1] == 2,
                _ => false,
            };
            track(nodes[a], nodes[b], active, glint);
        }
        let trains = self.schedule().trains;
        let target = ui
            .focus
            .filter(|_| self.phase == Phase::Working)
            .map(|i| trains[i].destination);
        for station in 0..4 {
            let p = nodes[station + 8];
            let color = DESTINATIONS[station];
            if target == Some(station) {
                halo(p, 64. + 6. * pulse(ui.t, 0.9), color, 0.3);
            }
            // Platform, building and roof.
            panel(
                p.x - 48.,
                p.y - 12.,
                100.,
                42.,
                Color::new(0., 0., 0., 0.22),
            );
            panel(p.x - 50., p.y + 16., 100., 9., CREAM);
            for stripe in 0..12 {
                panel(
                    p.x - 47. + stripe as f32 * 8.,
                    p.y + 18.,
                    4.,
                    3.,
                    darken(color, 0.1),
                );
            }
            let body = Rect::new(p.x - 44., p.y - 12., 88., 29.);
            shapes::fill_rounded_rect(
                body,
                0.,
                Fill::vertical(
                    body,
                    Color::new(0.74, 0.69, 0.55, 1.),
                    Color::new(0.56, 0.52, 0.41, 1.),
                ),
            );
            rounded_gradient(
                p.x - 47.,
                p.y - 17.,
                94.,
                9.,
                3.,
                lighten(color, 0.15),
                darken(color, 0.15),
            );
            for window in 0..4 {
                let wx = p.x - 33. + window as f32 * 19.;
                panel(wx, p.y - 3., 9., 11., Color::new(0.15, 0.21, 0.23, 1.));
                panel(wx, p.y - 3., 9., 4., Color::new(0.98, 0.85, 0.55, 0.55));
            }
            // Name plate: lettered disc, name, then one pip per service due here.
            let name = STATIONS[station];
            let s = ink(12., CREAM);
            let name_w = text_width(name, s);
            let due = trains.iter().filter(|t| t.destination == station).count();
            let pips_w = if due > 0 { 6. + due as f32 * 10. } else { 0. };
            let w = name_w + 36. + pips_w;
            let plate = Rect::new(p.x - w * 0.5, p.y - 44., w, 22.);
            shapes::fill_rounded_rect(plate, 11., Color::new(0.04, 0.06, 0.07, 0.88));
            let disc = vec2(plate.x + 12., plate.y + 11.);
            destination_disc(disc, station, 8., 1.);
            draw_text_styled(name, plate.x + 26., s.centered_baseline(disc.y), s);
            let mut px = plate.x + 26. + name_w + 9.;
            for (i, _) in trains
                .iter()
                .enumerate()
                .filter(|(_, t)| t.destination == station)
            {
                let at = vec2(px + 3., disc.y);
                match self.trains[i].state {
                    TrainState::Delivered => shapes::fill_circle(at, 3.6, color),
                    TrainState::At(_) | TrainState::Moving { .. } => {
                        let k = pulse(ui.t + i as f32 * 0.21, 1.4);
                        shapes::fill_circle(at, 2.2 + 1.2 * k, with_alpha(color, 0.75));
                        shapes::stroke_circle(at, 3.6, 1.2, color);
                    }
                    TrainState::Waiting => {
                        shapes::stroke_circle(at, 3.4, 1.2, with_alpha(color, 0.5))
                    }
                }
                px += 10.;
            }
        }
        // Yards: a buffer stop and the queue of services still to enter.
        for (side, node) in [(Side::West, 0), (Side::East, 7)] {
            let p = nodes[node];
            rounded(p.x - 11., p.y + 11., 22., 5., 2., CREAM);
            let mut slot = 0;
            for (i, plan) in trains.iter().enumerate() {
                if plan.side != side || self.trains[i].state != TrainState::Waiting {
                    continue;
                }
                if slot == 3 {
                    break;
                }
                let at = vec2(p.x, p.y + 40. + slot as f32 * 25.);
                let release = plan.release * 60;
                shapes::fill_circle(at, 12.5, Color::new(0.03, 0.04, 0.05, 0.85));
                if self.time >= release {
                    // Released and waiting for the yard to clear.
                    let k = pulse(ui.t + i as f32 * 0.3, 1.2);
                    halo(at, 17. + 3. * k, BRASS, 0.35);
                    shapes::stroke_circle(at, 10.6, 2., BRASS);
                } else {
                    let fraction = self.time as f32 / release.max(1) as f32;
                    ring_meter(at.x, at.y, 10.6, 2., fraction, SOFT);
                }
                destination_disc(at, plan.destination, 7.5, 1.);
                slot += 1;
            }
        }
    }

    /// Signals, points and the bridge inspection, drawn over the routes.
    fn furniture(&self, ui: &Ui) {
        let nodes = self.nodes();
        let hint = self.hint(ui);
        let hovered = match ui.hover {
            Hover::Control(control) => Some(control),
            _ => None,
        };
        for (signal, node) in [(0, 1), (1, 3), (2, 4), (3, 6)] {
            let at = self.lamp(signal);
            let base = nodes[node];
            let up = at.y < base.y;
            let post_end = at + vec2(0., if up { 13. } else { -13. });
            line(post_end, base, 2.5, Color::new(0.80, 0.78, 0.68, 1.));
            let control = SIGNAL_CONTROL[signal];
            if hovered == Some(control) {
                halo(at, 30., CREAM, 0.25);
                shapes::stroke_circle(at, 19.5, 2., with_alpha(CREAM, 0.9));
            } else if self.selected == control {
                shapes::stroke_circle(at, 19., 1.5, with_alpha(CREAM, 0.55));
            }
            signal_head(at, self.signals[signal], 1., ui.t + signal as f32 * 0.4);
            if matches!(signal, 1 | 2) && self.bridge_closed() {
                marker(
                    Icon::Lock,
                    at + vec2(10., if up { -12. } else { 12. }),
                    7.,
                    RED_LAMP,
                );
            }
        }
        for (point, control) in [(0, 1), (1, 4)] {
            let p = self.junction(point);
            let selected = self.selected == control;
            let over = hovered == Some(control);
            let locked = self.points_locked(point);
            if hint == Some(point) {
                let k = pulse(ui.t, 1.1);
                halo(p, 30. + 6. * k, BRASS, 0.35 + 0.2 * k);
                shapes::stroke_circle(p, 17. + 4. * k, 2.2, with_alpha(BRASS, 0.55 + 0.4 * k));
            }
            if over {
                halo(p, 30., CREAM, 0.25);
            }
            draw_circle(p.x, p.y + 2.5, 12., Color::new(0., 0., 0., 0.35));
            shapes::fill_circle(p, 12., if selected || over { CREAM } else { BRASS });
            shapes::fill_circle(
                p,
                10.,
                Fill::radial(
                    p - vec2(3., 3.),
                    13.,
                    Color::new(0.34, 0.27, 0.20, 1.),
                    WOOD,
                ),
            );
            if over {
                shapes::stroke_circle(p, 17., 2., with_alpha(CREAM, 0.9));
            } else if selected {
                shapes::stroke_circle(p, 15.5, 1.5, with_alpha(CREAM, 0.6));
            }
            let direction = match self.points[point] {
                1 => vec2(0., -1.),
                2 => vec2(0., 1.),
                _ => Vec2::X,
            };
            shapes::fill_capsule(p - direction * 5., p + direction * 6., 2., BRASS);
            if locked {
                marker(
                    Icon::Lock,
                    p + vec2(12., -12.),
                    7.,
                    if over { RED_LAMP } else { SOFT },
                );
            }
        }
        // Bridge inspection: barriers and a lock that counts down.
        if let Some((start, end)) = self.schedule().closure {
            let bridge_y = self.schedule().bridge_y;
            let (start, end) = (start * 60, end * 60);
            let badge = vec2(400., bridge_y - 40.);
            if self.bridge_closed() {
                for x in [352., 448.] {
                    for k in 0..4 {
                        let y = bridge_y - 14. + k as f32 * 7.;
                        let tone = if k % 2 == 0 { RED_LAMP } else { CREAM };
                        rounded(x - 2.5, y, 5., 7., 1., tone);
                    }
                }
                let left = (end - self.time) as f32 / (end - start).max(1) as f32;
                shapes::fill_circle(badge, 15., Color::new(0.10, 0.04, 0.04, 0.92));
                ring_meter(badge.x, badge.y, 13., 2.5, left, RED_LAMP);
                icon(Icon::Lock, badge.x, badge.y, 13., RED_LAMP);
            } else if self.time < start && start - self.time <= 20 * 60 {
                let left = (start - self.time) as f32 / (20. * 60.);
                shapes::fill_circle(badge, 15., Color::new(0.10, 0.08, 0.04, 0.88));
                ring_meter(badge.x, badge.y, 13., 2.5, left, BRASS);
                icon(
                    Icon::Lock,
                    badge.x,
                    badge.y,
                    13.,
                    with_alpha(BRASS, 0.7 + 0.3 * pulse(ui.t, 1.)),
                );
            }
        }
    }

    /// Dashed previews of where trains will run with the current points; a
    /// hovered set of points previews the setting one click would make.
    fn routes(&self, ui: &Ui) {
        if self.phase != Phase::Working {
            return;
        }
        let preview = match ui.hover {
            Hover::Control(control) => CONTROL_POINTS[control].filter(|p| !self.points_locked(*p)),
            _ => None,
        };
        let mut points = self.points;
        if let Some(point) = preview {
            points[point] = (points[point] + 1) % 3;
            let nodes = self.nodes();
            let hub = if point == 0 { 2 } else { 5 };
            let to = match (point, points[point]) {
                (0, 0) => 3,
                (0, 1) => 8,
                (0, _) => 9,
                (_, 0) => 4,
                (_, 1) => 10,
                _ => 11,
            };
            // Ghost the branch one click would set.
            let k = pulse(ui.t, 1.5);
            let (a, b) = (nodes[hub], nodes[hub].lerp(nodes[to], 0.8));
            line(a, b, 16., with_alpha(CREAM, 0.16 + 0.12 * k));
            dashes(a, b, 0., ui.t * 30., 3., with_alpha(CREAM, 0.75));
        }
        let trains = self.schedule().trains;
        for (i, plan) in trains.iter().enumerate() {
            if ui.focus == Some(i) || !self.live(i) {
                continue;
            }
            if let Some(route) = self.route(i, self.points) {
                self.draw_route(&route, DESTINATIONS[plan.destination], false, ui.t);
            }
        }
        // The focus route goes on top, with any previewed points applied.
        if let Some(i) = ui.focus
            && let Some(route) = self.route(i, points)
        {
            self.draw_route(&route, DESTINATIONS[trains[i].destination], true, ui.t);
        }
    }

    fn draw_route(&self, route: &Route, color: Color, strong: bool, t: f32) {
        let alpha = if strong { 0.95 } else { 0.45 };
        let width = if strong { 4.5 } else { 3. };
        let mut along = 0.;
        for i in 0..route.len.saturating_sub(1) {
            let (a, b) = (route.pts[i], route.pts[i + 1]);
            let held = route.hold.is_some_and(|h| i >= h);
            let shade = if held { alpha * 0.55 } else { alpha };
            if strong {
                line(a, b, 12., with_alpha(color, 0.13 * shade));
            }
            let phase = if held { 0. } else { t * 28. };
            dashes(a, b, along, phase, width, with_alpha(color, shade));
            along += a.distance(b);
        }
        // Secondary routes mark their ends beside the stop, never over the
        // focus route's own badge.
        let end = route.pts[route.len - 1] + if strong { Vec2::ZERO } else { vec2(14., -14.) };
        let radius = if strong { 10. } else { 7. };
        match route.end {
            End::Home => {
                if strong {
                    shapes::fill_circle(end, radius + 2., Color::new(0.03, 0.05, 0.04, 0.85));
                    icon_badge(Icon::Check, end.x, end.y, radius * 2., GREEN_LAMP, INK);
                }
            }
            End::Wrong | End::Exit => {
                let k = pulse(t, 1.6);
                halo(end, radius * 2.6, RED_LAMP, 0.3 + 0.25 * k);
                shapes::fill_circle(end, radius + 2., Color::new(0.08, 0.03, 0.03, 0.9));
                icon_badge(Icon::Cross, end.x, end.y, radius * 2., RED_LAMP, CREAM);
            }
        }
    }

    /// Destination badge with a draining due ring; a late service glows red.
    fn train_badge(&self, at: Vec2, train: usize, radius: f32, t: f32) {
        let destination = self.schedule().trains[train].destination;
        shapes::fill_circle(at, radius + 4.5, Color::new(0.03, 0.04, 0.05, 0.92));
        if self.is_late(train) {
            let k = pulse(t, 1.6);
            halo(at, radius + 11., RED_LAMP, 0.25 + 0.3 * k);
            shapes::stroke_circle(at, radius + 2.5, 2.2, RED_LAMP);
        } else {
            let left = self.due_left(train);
            ring_meter(
                at.x,
                at.y,
                radius + 2.5,
                2.2,
                left,
                if left < 0.25 { BRASS } else { CREAM },
            );
        }
        destination_disc(at, destination, radius, 1.);
    }

    fn locomotives(&self, ui: &Ui) {
        for (index, plan) in self.schedule().trains.iter().enumerate() {
            let Some(position) = self.position(index) else {
                continue;
            };
            let direction = self.direction(index);
            let across = direction.perp();
            let color = DESTINATIONS[plan.destination];
            let focus = ui.focus == Some(index) && self.phase == Phase::Working;
            if focus {
                halo(position, 30., color, 0.35);
            }
            let rear = position - direction * 12.;
            let front = position + direction * 12.;
            shapes::fill_capsule(
                rear + vec2(2., 4.),
                front + vec2(2., 4.),
                9.,
                Color::new(0., 0., 0., 0.32),
            );
            shapes::fill_capsule(rear, front, 8.5, Color::new(0.06, 0.07, 0.08, 1.));
            shapes::fill_capsule(
                rear,
                front,
                6.5,
                Fill::linear(
                    position - across * 6.,
                    position + across * 6.,
                    lighten(color, 0.25),
                    darken(color, 0.2),
                ),
            );
            // Cab roof and window.
            let cab = position - direction * 7.;
            shapes::fill_capsule(
                cab - direction * 3.,
                cab + direction * 3.,
                5.,
                Color::new(0.88, 0.87, 0.78, 1.),
            );
            shapes::fill_capsule(
                cab - direction * 1.,
                cab + direction * 1.,
                2.6,
                Color::new(0.17, 0.32, 0.37, 1.),
            );
            let boiler = position + direction * 5.;
            draw_circle(boiler.x, boiler.y, 3.2, WOOD);
            let lamp = position + direction * 14.;
            halo(lamp, 7., Color::new(1., 0.93, 0.7, 1.), 0.5);
            draw_circle(lamp.x, lamp.y, 2.2, Color::new(1., 0.93, 0.7, 1.));
            if matches!(self.trains[index].state, TrainState::Moving { .. }) {
                for puff in 0..2_u32 {
                    let drift = ((self.time / 6 + index as u32 * 3 + puff * 4) % 9) as f32;
                    let smoke = position + vec2(-3. - drift * 0.8, -12. - drift * 1.2);
                    draw_circle(
                        smoke.x,
                        smoke.y,
                        2.5 + drift * 0.45,
                        Color::new(0.92, 0.90, 0.82, 0.32 - drift * 0.03),
                    );
                }
            }
            // A held train's badge bobs gently: it is waiting for you.
            let wait = if self.held_by(index).is_some() {
                bob(ui.t + index as f32 * 0.37, 1.1, 1.5)
            } else {
                0.
            };
            let radius = if focus { 9. } else { 8. };
            self.train_badge(position + vec2(0., -24. + wait), index, radius, ui.t);
        }
    }

    /// While planning, the railway rests under a cool veil with a pause mark.
    fn planning_veil(&self, ui: &Ui) {
        if self.phase != Phase::Working || self.dispatching {
            return;
        }
        let k = pulse(ui.t, 0.7);
        shapes::fill_rounded_rect(MAP, 12., Color::new(0.22, 0.38, 0.62, 0.14));
        shapes::stroke_rounded_rect(MAP, 12., 2., with_alpha(BRASS, 0.35 + 0.4 * k));
        let at = vec2(MAP.x + 36., MAP.y + 30.);
        halo(at, 30., BRASS, 0.2 + 0.15 * k);
        icon_badge(
            Icon::Pause,
            at.x,
            at.y,
            34.,
            Color::new(0.07, 0.09, 0.10, 0.95),
            BRASS,
        );
    }

    fn board(&self, ui: &Ui) {
        card(PANEL.x, PANEL.y, PANEL.w, PANEL.h, BOARD);
        let trains = self.schedule().trains;
        let h = row_height(trains.len());
        let right = PANEL.x + PANEL.w - 12.;
        let radius = (h * 0.3).clamp(6.5, 9.);
        for (i, plan) in trains.iter().enumerate() {
            let cy = ROWS_TOP + (i as f32 + 0.5) * h;
            let state = self.trains[i].state;
            let done = state == TrainState::Delivered;
            let live = self.live(i);
            let focus = ui.focus == Some(i) && self.phase == Phase::Working;
            let hovered = matches!(ui.hover, Hover::Row(j) | Hover::Train(j) if j == i);
            let color = DESTINATIONS[plan.destination];
            let row = Rect::new(PANEL.x + 6., cy - h * 0.5 + 1., PANEL.w - 12., h - 2.);
            if live || focus || hovered {
                let tint = 0.07
                    + if live { 0.05 } else { 0. }
                    + if focus { 0.05 } else { 0. }
                    + if hovered { 0.07 } else { 0. };
                shapes::fill_rounded_rect(row, 6., with_alpha(color, tint));
            } else if i % 2 == 0 {
                shapes::fill_rounded_rect(row, 6., Color::new(1., 1., 1., 0.025));
            }
            if focus || hovered {
                shapes::stroke_rounded_rect(row, 6., 1., with_alpha(color, 0.55));
            }
            let badge = vec2(PANEL.x + 22., cy);
            if done {
                destination_disc(badge, plan.destination, radius, 0.35);
            } else {
                self.train_badge(badge, i, radius, ui.t);
            }
            label_mid(
                plan.name,
                PANEL.x + 40.,
                cy,
                13.,
                if done { MUTED } else { CREAM },
            );
            let detail = if done { DIM } else { SOFT };
            let arrow = if plan.side == Side::West {
                Icon::ArrowRight
            } else {
                Icon::ArrowLeft
            };
            icon(arrow, PANEL.x + 76., cy, 12., detail);
            icon(service_icon(plan.service), PANEL.x + 94., cy, 13., detail);
            self.row_status(i, right, cy, ui.t);
        }
        self.outlook();
    }

    fn row_status(&self, train: usize, right: f32, cy: f32, t: f32) {
        let plan = self.schedule().trains[train];
        let due = plan.due * 60;
        match self.trains[train].state {
            TrainState::Delivered => {
                let late = self.trains[train]
                    .delivered_at
                    .unwrap_or(due)
                    .saturating_sub(due);
                if late == 0 {
                    icon(Icon::Check, right - 7., cy, 15., GREEN_LAMP);
                } else {
                    let w = label_end(&format!("+{}s", late.div_ceil(60)), right, cy, 12., BRASS);
                    icon(Icon::Check, right - w - 9., cy, 12., BRASS);
                }
            }
            TrainState::Waiting if self.time < plan.release * 60 => {
                let wait = (plan.release * 60 - self.time).div_ceil(60);
                let w = label_end(&format!("{wait}s"), right, cy, 12., MUTED);
                icon(Icon::Hourglass, right - w - 9., cy, 11., MUTED);
            }
            _ => {
                let w = if self.time > due {
                    let k = pulse(t, 1.6);
                    let late = (self.time - due).div_ceil(60);
                    let w = label_end(&format!("+{late}s"), right, cy, 13., RED_LAMP);
                    icon(
                        Icon::Alert,
                        right - w - 10.,
                        cy,
                        13.,
                        with_alpha(RED_LAMP, 0.6 + 0.4 * k),
                    );
                    w
                } else {
                    let left = (due - self.time).div_ceil(60);
                    let tone = if left <= 10 { BRASS } else { CREAM };
                    let w = label_end(&format!("{left}s"), right, cy, 13., tone);
                    icon(Icon::Clock, right - w - 10., cy, 12., tone);
                    w
                };
                // A train held at a red lamp shows the lamp holding it.
                if self.held_by(train).is_some() {
                    let at = vec2(right - w - 28., cy);
                    halo(at, 8., RED_LAMP, 0.45);
                    shapes::fill_circle(at, 3.6, RED_LAMP);
                }
            }
        }
    }

    /// The board's footer: the medal this shift still deserves, its delay
    /// against the silver allowance and the commission so far.
    fn outlook(&self) {
        let left = PANEL.x + 14.;
        let right = PANEL.x + PANEL.w - 12.;
        divider(left, 446., PANEL.w - 28., Color::new(1., 1., 1., 0.14));
        let delay = self.projected_delay();
        let stars = if delay == 0 {
            3
        } else if delay <= 45 * 60 {
            2
        } else {
            1
        };
        for k in 0..3 {
            let lit = k < stars;
            icon(
                Icon::Star,
                left + 8. + k as f32 * 21.,
                466.,
                17.,
                if lit { GOLD } else { DIM },
            );
        }
        let w = label_end(&grouped(self.provisional), right, 466., 15., CREAM);
        icon(Icon::Coin, right - w - 11., 466., 15., GOLD);
        let tone = match stars {
            3 => GREEN_LAMP,
            2 => BRASS,
            _ => RED_LAMP,
        };
        icon(Icon::Clock, left + 7., 489., 12., SOFT);
        meter(left + 20., 486., 108., 6., delay as f32 / (45. * 60.), tone);
        label_end(&format!("{}s", delay.div_ceil(60)), right, 489., 12., SOFT);
    }

    /// One switchboard control's points pictogram: the lit arm is the route.
    fn junction_glyph(&self, c: Vec2, point: usize, alpha: f32) {
        let route = self.points[point];
        let rail = with_alpha(Color::new(0.42, 0.44, 0.42, 1.), alpha);
        let bright = with_alpha(lighten(BRASS, 0.12), alpha);
        let (west, east) = (c - vec2(17., 0.), c + vec2(17., 0.));
        let (north, south) = (c - vec2(0., 13.), c + vec2(0., 13.));
        for tip in [west, east, north, south] {
            line(c, tip, 3., rail);
        }
        match route {
            1 => line(c, north, 3.6, bright),
            2 => line(c, south, 3.6, bright),
            _ => line(west, east, 3.6, bright),
        }
        shapes::fill_circle(c, 3.4, bright);
        let (up, down) = if point == 0 { (0, 1) } else { (2, 3) };
        destination_disc(north, up, 6., alpha * if route == 1 { 1. } else { 0.45 });
        destination_disc(south, down, 6., alpha * if route == 2 { 1. } else { 0.45 });
    }

    fn switchboard(&self, ui: &Ui) {
        let base = Rect::new(20., 513., 704., 55.);
        shapes::fill_rounded_rect(
            base,
            10.,
            Fill::vertical(base, Color::new(0.24, 0.19, 0.16, 1.), WOOD),
        );
        panel(30., 513.5, 684., 1., Color::new(1., 0.9, 0.7, 0.1));
        let working = self.phase == Phase::Working;
        let hint = self.hint(ui);
        for control in 0..6 {
            let at = Game::control_center(control);
            let selected = self.selected == control;
            let hovered = working && ui.hover == Hover::Control(control);
            let lift = if hovered { 2. } else { 0. };
            let r = Rect::new(at.x - 51., 518. - lift, 102., 45.);
            let (top, bottom) = if selected {
                (
                    Color::new(0.38, 0.32, 0.22, 1.),
                    Color::new(0.27, 0.22, 0.15, 1.),
                )
            } else if hovered {
                (
                    Color::new(0.25, 0.24, 0.21, 1.),
                    Color::new(0.17, 0.16, 0.14, 1.),
                )
            } else {
                (
                    Color::new(0.17, 0.17, 0.16, 1.),
                    Color::new(0.12, 0.12, 0.11, 1.),
                )
            };
            panel(
                r.x + 4.,
                518. + r.h - 3.,
                r.w - 8.,
                5.,
                Color::new(0., 0., 0., 0.45),
            );
            shapes::fill_rounded_rect(r, 7., Fill::vertical(r, top, bottom));
            if selected {
                shapes::stroke_rounded_rect(r, 7., 2., BRASS);
            } else if hovered {
                shapes::stroke_rounded_rect(r, 7., 1.5, with_alpha(CREAM, 0.6));
            } else {
                panel(
                    r.x + 7.,
                    r.y + 1.,
                    r.w - 14.,
                    1.,
                    Color::new(1., 0.92, 0.75, 0.1),
                );
            }
            let glyph = vec2(r.x + 25., r.y + r.h * 0.5);
            if let Some(signal) = CONTROL_SIGNAL[control] {
                signal_head(glyph, self.signals[signal], 0.8, ui.t + signal as f32 * 0.4);
                if matches!(signal, 1 | 2) && self.bridge_closed() {
                    marker(Icon::Lock, glyph + vec2(11., 9.), 6.5, RED_LAMP);
                }
            } else if let Some(point) = CONTROL_POINTS[control] {
                self.junction_glyph(glyph, point, 1.);
                if self.points_locked(point) {
                    marker(Icon::Lock, glyph + vec2(14., 10.), 6.5, SOFT);
                }
                if hint == Some(point) {
                    let k = pulse(ui.t, 1.1);
                    shapes::stroke_rounded_rect(
                        Rect::new(r.x - 2., r.y - 2., r.w + 4., r.h + 4.),
                        9.,
                        2.,
                        with_alpha(BRASS, 0.45 + 0.5 * k),
                    );
                }
            }
            label_mid(
                CAPTIONS[control],
                r.x + 47.,
                r.y + 27.,
                13.,
                if selected || hovered { CREAM } else { SOFT },
            );
            // Number keys 1-5 pick the first five controls.
            if control < 5 {
                let key = Rect::new(r.x + r.w - 17., r.y + 5., 12., 13.);
                shapes::fill_rounded_rect(key, 3., Color::new(0., 0., 0., 0.35));
                shapes::stroke_rounded_rect(key, 3., 1., Color::new(1., 1., 1., 0.1));
                let mut buffer = [0u8; 4];
                let digit = char::from(b'1' + control as u8).encode_utf8(&mut buffer);
                label_center(digit, key.x + 6., key.y + 6.5, 10., MUTED);
            }
        }
        // A hovered control is wired to its lamp or points on the map.
        if let Hover::Control(control) = ui.hover
            && working
        {
            let from = vec2(Game::control_center(control).x, 516.);
            let to = match (CONTROL_SIGNAL[control], CONTROL_POINTS[control]) {
                (Some(signal), _) => self.lamp(signal),
                (_, Some(point)) => self.junction(point),
                _ => from,
            };
            let gap = 21.;
            let to_edge = to + (from - to).normalize_or_zero() * gap;
            dashes(from, to_edge, 0., ui.t * 24., 1.8, with_alpha(CREAM, 0.6));
        }
    }

    fn clock_face(&self, ui: &Ui) {
        let r = Rect::new(734., 513., 206., 55.);
        let working = self.phase == Phase::Working;
        let live = working && self.dispatching;
        let planning = working && !self.dispatching;
        let hovered = ui.hover == Hover::Clock;
        let tint = if planning {
            Color::new(0.30, 0.25, 0.15, 1.)
        } else if live {
            Color::new(0.11, 0.24, 0.20, 1.)
        } else {
            Color::new(0.12, 0.14, 0.15, 1.)
        };
        let tint = if hovered { lighten(tint, 0.08) } else { tint };
        shapes::fill_rounded_rect(r, 10., Fill::vertical(r, lighten(tint, 0.06), tint));
        let k = pulse(ui.t, 0.8);
        let rim = if planning {
            with_alpha(BRASS, 0.4 + 0.5 * k)
        } else if hovered {
            with_alpha(CREAM, 0.55)
        } else {
            with_alpha(GREEN_LAMP, 0.3)
        };
        shapes::stroke_rounded_rect(r, 10., if planning { 2. } else { 1. }, rim);
        let closing = self.schedule().closing * 60;
        let elapsed = self.time as f32 / closing.max(1) as f32;
        shapes::fill_circle(DIAL, 21., Color::new(0.03, 0.04, 0.05, 0.6));
        ring_meter(
            DIAL.x,
            DIAL.y,
            17.,
            4.,
            elapsed,
            if planning { BRASS } else { GREEN_LAMP },
        );
        icon(
            if live { Icon::Pause } else { Icon::Play },
            DIAL.x,
            DIAL.y,
            14.,
            CREAM,
        );
        keycap("X", "", 791., 530.5, CREAM);
        let remaining = closing.saturating_sub(self.time).div_ceil(60);
        let urgent = working && remaining <= 20;
        let tone = if urgent {
            with_alpha(RED_LAMP, 0.7 + 0.3 * pulse(ui.t, 2.))
        } else {
            CREAM
        };
        icon(Icon::Bell, BELL.x, BELL.y, 17., tone);
        label_mid(&clock_text(remaining), BELL.x + 14., BELL.y, 22., tone);
    }

    fn scrim(&self) {
        shapes::fill_rounded_rect(MAP, 12., Color::new(0.01, 0.02, 0.03, 0.5));
    }

    fn briefing(&self, ui: &Ui) {
        self.scrim();
        let (x, y, w, h) = (74., 150., 625., 346.);
        card(x, y, w, h, BOARD);
        rounded(x, y, w, 4., 2., BRASS);
        let left = x + 24.;
        let schedule = self.schedule();
        let title = style(26., CREAM);
        draw_text_styled(schedule.name, left, y + 46., title);
        let (challenge, words) = CHALLENGES[self.shift];
        icon_chip(challenge, words, left, y + 62., 14., BRASS);
        // The whole timetable at a glance, in release order.
        let trains = schedule.trains;
        let gap = 6.;
        let count = trains.len() as f32;
        let chip_w = ((w - 48. - gap * (count - 1.)) / count).min(54.);
        let top = y + 112.;
        for (i, plan) in trains.iter().enumerate() {
            let cx = left + i as f32 * (chip_w + gap) + chip_w * 0.5;
            let color = DESTINATIONS[plan.destination];
            let tile = Rect::new(cx - chip_w * 0.5, top, chip_w, 70.);
            shapes::fill_rounded_rect(tile, 8., with_alpha(color, 0.12));
            shapes::stroke_rounded_rect(tile, 8., 1., with_alpha(color, 0.35));
            let rise = bob(ui.t + i as f32 * 0.18, 0.5, 1.2);
            destination_disc(vec2(cx, top + 21. + rise), plan.destination, 11., 1.);
            let arrow = if plan.side == Side::West {
                Icon::ArrowRight
            } else {
                Icon::ArrowLeft
            };
            icon(arrow, cx - 8., top + 50., 12., SOFT);
            icon(service_icon(plan.service), cx + 8., top + 50., 13., SOFT);
        }
        // Shift facts as icons and numbers.
        let cy = top + 108.;
        let mut fx = left;
        fx += icon_value(Icon::Train, &trains.len().to_string(), fx, cy, 18., CREAM) + 26.;
        fx += icon_value(
            Icon::Bell,
            &clock_text(schedule.closing),
            fx,
            cy,
            18.,
            CREAM,
        ) + 26.;
        if let Some((start, end)) = schedule.closure {
            let window = format!("{}-{}", clock_text(start), clock_text(end));
            fx += icon_value(Icon::Lock, &window, fx, cy, 18., RED_LAMP) + 26.;
        }
        if (schedule.speed - 1.).abs() > 0.001 {
            let speed = format!("{}%", (schedule.speed * 100.).round());
            fx += icon_value(speed_icon(self.shift), &speed, fx, cy, 18., SOFT) + 26.;
        }
        icon_value(Icon::Restart, &self.retries.to_string(), fx, cy, 18., BRASS);
        action_button(
            "Space",
            "Start",
            Color::new(0.20, 0.36, 0.29, 1.),
            ui.hover == Hover::Main,
        );
    }

    fn cleared(&self, ui: &Ui) {
        self.scrim();
        let (x, y, w, h) = (74., 200., 625., 296.);
        card(x, y, w, h, BOARD);
        rounded(x, y, w, 4., 2., BRASS);
        let left = x + 24.;
        let stars = self.medals[self.shift];
        let (tone, name) = medal(stars);
        for k in 0..3 {
            let at = vec2(left + 30. + k as f32 * 58., y + 66.);
            let lit = k < u32::from(stars);
            if lit {
                halo(
                    at,
                    34.,
                    GOLD,
                    0.25 + 0.1 * pulse(ui.t + k as f32 * 0.2, 0.8),
                );
            }
            icons::draw_styled(
                Icon::Star,
                at,
                46.,
                IconStyle::new(if lit { GOLD } else { DIM }).shadow(Color::new(0., 0., 0., 0.4)),
            );
        }
        label(self.schedule().name, left + 196., y + 52., 14., SOFT);
        draw_text_styled(name, left + 196., y + 88., style(32., tone));
        divider(left, y + 118., w - 48., Color::new(1., 1., 1., 0.12));
        let cy = y + 168.;
        let delay = self.delays.div_ceil(60);
        let earned = self.provisional + 300 + u32::from(stars) * 150;
        let mut fx = left;
        fx += icon_value(
            Icon::Train,
            &self.schedule().trains.len().to_string(),
            fx,
            cy,
            26.,
            CREAM,
        ) + 48.;
        let (delay_tone, delay_text) = if delay == 0 {
            (GREEN_LAMP, "0s".to_owned())
        } else {
            (BRASS, format!("+{delay}s"))
        };
        fx += icon_value(Icon::Clock, &delay_text, fx, cy, 26., delay_tone) + 48.;
        icon_value(
            Icon::Coin,
            &format!("+{}", grouped(earned)),
            fx,
            cy,
            26.,
            GOLD,
        );
        action_button(
            "Space",
            "Next shift",
            Color::new(0.20, 0.36, 0.29, 1.),
            ui.hover == Hover::Main,
        );
    }

    fn incident_card(&self, ui: &Ui) {
        self.scrim();
        let (x, y, w, h) = (74., 186., 625., 310.);
        card(x, y, w, h, BOARD);
        rounded(x, y, w, 4., 2., RED_LAMP);
        let left = x + 24.;
        let trains = self.schedule().trains;
        let headline = match self.incident {
            Some(Incident::WrongPlatform { .. }) => "Wrong platform",
            Some(Incident::LeftLine { .. }) => "Off the line",
            Some(Incident::Collision { .. }) => "Collision",
            Some(Incident::Bell { .. }) => "Out of time",
            None => "Line stopped",
        };
        icon(Icon::Alert, left + 14., y + 38., 28., RED_LAMP);
        draw_text_styled(headline, left + 38., y + 48., style(28., CREAM));
        // What happened, drawn as a little equation of badges.
        let cy = y + 112.;
        let named = |train: usize, at: Vec2| {
            let plan = trains[train];
            shapes::fill_circle(at, 26., Color::new(0.03, 0.04, 0.05, 0.9));
            destination_disc(at, plan.destination, 21., 1.);
            label_center(plan.name, at.x, at.y + 40., 15., SOFT);
        };
        let crossed = |at: Vec2| {
            let k = pulse(ui.t, 1.4);
            halo(at, 38. + 4. * k, RED_LAMP, 0.35);
        };
        let cross_badge = |at: Vec2| {
            icon_badge(Icon::Cross, at.x + 20., at.y - 20., 22., RED_LAMP, CREAM);
        };
        match self.incident {
            Some(Incident::WrongPlatform { train, station }) => {
                named(train, vec2(left + 46., cy));
                icon(Icon::ArrowRight, left + 116., cy, 28., SOFT);
                let at = vec2(left + 186., cy);
                crossed(at);
                shapes::fill_circle(at, 26., Color::new(0.03, 0.04, 0.05, 0.9));
                destination_disc(at, station, 21., 1.);
                label_center(STATIONS[station], at.x, at.y + 40., 15., SOFT);
                cross_badge(at);
            }
            Some(Incident::LeftLine { train, node }) => {
                named(train, vec2(left + 46., cy));
                let arrow = if node == 0 {
                    Icon::ArrowLeft
                } else {
                    Icon::ArrowRight
                };
                icon(arrow, left + 116., cy, 28., SOFT);
                let at = vec2(left + 186., cy);
                crossed(at);
                marker(Icon::Flag, at, 26., RED_LAMP);
                cross_badge(at);
            }
            Some(Incident::Collision { a, b, .. }) => {
                named(a, vec2(left + 46., cy));
                let k = pulse(ui.t, 1.6);
                halo(vec2(left + 116., cy), 40., RED_LAMP, 0.3 + 0.2 * k);
                icon(Icon::Bomb, left + 116., cy, 40., RED_LAMP);
                named(b, vec2(left + 186., cy));
            }
            Some(Incident::Bell { waiting }) => {
                icon(Icon::Bell, left + 46., cy, 46., RED_LAMP);
                icon_value(
                    Icon::Train,
                    &waiting.to_string(),
                    left + 104.,
                    cy,
                    32.,
                    CREAM,
                );
            }
            None => {}
        }
        // A retry restores the shift; its pending commission is forfeit.
        let ry = y + 206.;
        icon(Icon::Restart, left + 13., ry, 24., BRASS);
        for k in 0..3_u8 {
            let at = vec2(left + 46. + f32::from(k) * 24., ry);
            if k + 1 == self.retries {
                // The retry the button will spend.
                let p = pulse(ui.t, 1.2);
                shapes::fill_circle(at, 7., with_alpha(BRASS, 0.45 + 0.45 * p));
                shapes::stroke_circle(at, 7., 1.5, BRASS);
            } else if k < self.retries {
                shapes::fill_circle(at, 7., BRASS);
            } else {
                shapes::stroke_circle(at, 6.5, 1.5, with_alpha(BRASS, 0.4));
            }
        }
        if self.provisional > 0 {
            let x = left + 150.;
            let w = icon_value(
                Icon::Coin,
                &grouped(self.provisional),
                x,
                ry,
                20.,
                with_alpha(GOLD, 0.55),
            );
            line(
                vec2(x - 2., ry + 1.),
                vec2(x + w + 2., ry - 1.),
                2.5,
                RED_LAMP,
            );
        }
        action_button(
            "C",
            "Retry",
            Color::new(0.38, 0.17, 0.15, 1.),
            ui.hover == Hover::Main,
        );
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.tick(input)
    }
    fn draw(&self) {
        self.scene();
    }
    fn hud(&self) -> Hud {
        let trains = self.schedule().trains.len();
        Hud {
            stage: self.schedule().name.to_owned(),
            objective: format!("{}/{} trains delivered", self.delivered(), trains),
            status: match self.phase {
                Phase::Briefing => {
                    format!("{} {}", self.schedule().story, self.schedule().tip)
                }
                Phase::Incident | Phase::Lost => self.message.clone(),
                _ => format!(
                    "{} retries | {} stars | {} pending | {}",
                    self.retries,
                    self.stars(),
                    self.provisional,
                    if self.dispatching {
                        "Clock running"
                    } else {
                        "Clock stopped"
                    }
                ),
            },
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        if self.outcome() == Outcome::Won {
            format!(
                "{}: all 61 trains delivered with {} of 24 stars and {} commission.",
                self.rank(),
                self.stars(),
                self.score
            )
        } else {
            format!(
                "Career ended on shift {} with {} trains delivered and {} commission.",
                self.shift + 1,
                self.delivered_total,
                self.score
            )
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.bot_input(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.shift as u32 + 1, SHIFTS.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        let trains = self.schedule().trains;
        let nodes = self.nodes();
        let h = row_height(trains.len());
        let row = |i: usize| vec2(PANEL.x + 22., ROWS_TOP + (i as f32 + 0.5) * h);
        let above = vec2(0., -26.);
        for event in self.events.iter() {
            match *event {
                Event::Started => {
                    fx.ring(DIAL, GREEN_LAMP, 34.);
                    fx.callout(vec2(MAP.x + MAP.w * 0.5, 250.), "Go!", GREEN_LAMP);
                    for yard in [nodes[0], nodes[7]] {
                        fx.ring(yard, CREAM, 30.);
                    }
                }
                Event::Retried => {
                    fx.ring(DIAL, BRASS, 34.);
                    fx.popup_icon(DIAL, Icon::Restart, "", BRASS);
                }
                Event::Clock(running) => {
                    let (glyph, tone) = if running {
                        (Icon::Play, GREEN_LAMP)
                    } else {
                        (Icon::Pause, BRASS)
                    };
                    fx.ring(DIAL, tone, 30.);
                    fx.popup_icon(DIAL, glyph, "", tone);
                }
                Event::Signal { signal, green } => {
                    let at = self.lamp(signal);
                    if green {
                        fx.ring(at, GREEN_LAMP, 32.);
                        fx.burst(at, GREEN_LAMP, 8);
                    } else {
                        fx.ring(at, RED_LAMP, 24.);
                    }
                }
                Event::Points { point, .. } => {
                    let at = self.junction(point);
                    fx.ring(at, BRASS, 30.);
                    fx.burst(at, BRASS, 6);
                }
                Event::Locked { point } => {
                    let at = self.junction(point);
                    fx.ring(at, RED_LAMP, 26.);
                    fx.popup_icon(at + above, Icon::Lock, "", RED_LAMP);
                    fx.shake(0.14);
                }
                Event::Closed { signal } => {
                    let at = self.lamp(signal);
                    fx.ring(at, RED_LAMP, 26.);
                    fx.popup_icon(at + above, Icon::Lock, "", RED_LAMP);
                    fx.shake(0.14);
                }
                Event::Passed { signal } => fx.ring(self.lamp(signal), CREAM, 20.),
                Event::Entered { train } => {
                    let yard = if trains[train].side == Side::West {
                        nodes[0]
                    } else {
                        nodes[7]
                    };
                    fx.ring(yard, DESTINATIONS[trains[train].destination], 24.);
                }
                Event::Late { train } => {
                    let at = self.position(train).unwrap_or_else(|| row(train));
                    fx.ring(at, RED_LAMP, 26.);
                    fx.popup_icon(at + above, Icon::Clock, "Late", RED_LAMP);
                    fx.ring(row(train), RED_LAMP, 18.);
                }
                Event::Delivered {
                    train,
                    late,
                    earned,
                } => {
                    let destination = trains[train].destination;
                    let color = DESTINATIONS[destination];
                    let station = nodes[8 + destination];
                    fx.burst(station, color, 26);
                    fx.ring(station, color, 50.);
                    // Beside the building, clear of its name plate.
                    fx.popup_icon(
                        station + vec2(106., 2.),
                        Icon::Coin,
                        &format!("+{earned}"),
                        GOLD,
                    );
                    if late == 0 {
                        fx.sparkle(station + vec2(0., -4.), GOLD);
                    }
                    fx.ring(row(train), GREEN_LAMP, 18.);
                }
                Event::Bridge { open } => {
                    let at = vec2(400., self.schedule().bridge_y);
                    if open {
                        fx.ring(at, GREEN_LAMP, 60.);
                        fx.popup_icon(at + vec2(0., -40.), Icon::Unlock, "", GREEN_LAMP);
                    } else {
                        fx.ring(at, RED_LAMP, 60.);
                        fx.popup_icon(at + vec2(0., -40.), Icon::Lock, "", RED_LAMP);
                        fx.shake(0.1);
                    }
                }
                Event::Incident(incident) => {
                    let (at, glyph) = match incident {
                        Incident::WrongPlatform { station, .. } => {
                            (nodes[8 + station], Icon::Cross)
                        }
                        Incident::LeftLine { node, .. } => (nodes[node], Icon::Cross),
                        Incident::Collision { at, .. } => (at, Icon::Bomb),
                        Incident::Bell { .. } => (BELL, Icon::Bell),
                    };
                    fx.burst(at, RED_LAMP, 36);
                    fx.ring(at, RED_LAMP, 64.);
                    fx.popup_icon(at + above, glyph, "", RED_LAMP);
                    if matches!(incident, Incident::Collision { .. }) {
                        fx.burst(at, GOLD, 24);
                        fx.flash(Color::new(1., 0.55, 0.3, 0.22));
                        fx.shake(0.55);
                    } else {
                        fx.shake(0.3);
                    }
                }
                Event::Cleared { medal: stars } => {
                    let (tone, _) = medal(stars);
                    fx.callout(vec2(MAP.x + MAP.w * 0.5, 176.), "Shift clear!", tone);
                    fx.confetti(vec2(MAP.x + MAP.w * 0.5, 210.), 70);
                    // The card's stars pop as they light.
                    for k in 0..u32::from(stars) {
                        let star = vec2(128. + k as f32 * 58., 266.);
                        fx.sparkle(star, GOLD);
                        fx.burst(star, GOLD, 10);
                    }
                }
            }
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.objective(area)
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.result(area)
    }
}

impl Game {
    fn objective(&self, area: Rect) -> bool {
        let cy = area.y + area.h * 0.5;
        let t = clock();
        let trains = self.schedule().trains;
        // Services: one capsule each, filled once home.
        icon(Icon::Train, area.x + 10., cy, 18., CREAM);
        let mut x = area.x + 24.;
        for (i, plan) in trains.iter().enumerate() {
            let color = DESTINATIONS[plan.destination];
            let pip = Rect::new(x, cy - 5., 14., 10.);
            match self.trains[i].state {
                TrainState::Delivered => shapes::fill_rounded_rect(pip, 5., color),
                TrainState::At(_) | TrainState::Moving { .. } => {
                    let k = pulse(t + i as f32 * 0.2, 1.3);
                    shapes::fill_rounded_rect(pip, 5., with_alpha(color, 0.3 + 0.25 * k));
                    shapes::stroke_rounded_rect(pip, 5., 1.3, color);
                }
                TrainState::Waiting => {
                    shapes::stroke_rounded_rect(pip, 5., 1.2, with_alpha(color, 0.45))
                }
            }
            x += 18.;
        }
        // The shift timeline: due marks, the inspection window, now, the bell.
        let (from, to) = (area.x + 270., area.x + 690.);
        let closing = self.schedule().closing as f32 * 60.;
        let at = |ticks: f32| from + (to - from) * (ticks / closing).clamp(0., 1.);
        rounded(
            from,
            cy - 3.,
            to - from,
            6.,
            3.,
            Color::new(1., 1., 1., 0.1),
        );
        outline(
            from,
            cy - 3.,
            to - from,
            6.,
            3.,
            1.,
            Color::new(1., 1., 1., 0.08),
        );
        let now = at(self.time as f32);
        if now > from + 1. {
            rounded(from, cy - 3., now - from, 6., 3., with_alpha(BRASS, 0.85));
        }
        if let Some((start, end)) = self.schedule().closure {
            let (a, b) = (at(start as f32 * 60.), at(end as f32 * 60.));
            let closed = self.bridge_closed();
            rounded(
                a,
                cy - 8.,
                b - a,
                16.,
                4.,
                with_alpha(RED_LAMP, if closed { 0.95 } else { 0.6 }),
            );
            icon(Icon::Lock, (a + b) * 0.5, cy, 11., INK);
        }
        for (i, plan) in trains.iter().enumerate() {
            let color = DESTINATIONS[plan.destination];
            let done = self.trains[i].state == TrainState::Delivered;
            let dx = at(plan.due as f32 * 60.);
            rounded(
                dx - 1.,
                cy + 5.,
                2.5,
                7.,
                1.,
                with_alpha(color, if done { 0.35 } else { 1. }),
            );
        }
        let live = self.phase == Phase::Working && self.dispatching;
        shapes::fill_circle(vec2(now, cy + 1.5), 6.5, Color::new(0., 0., 0., 0.4));
        shapes::fill_circle(vec2(now, cy), 6., if live { CREAM } else { BRASS });
        shapes::stroke_circle(vec2(now, cy), 6., 1.2, INK);
        let remaining = (self.schedule().closing * 60).saturating_sub(self.time);
        let urgent = self.phase == Phase::Working && remaining <= 20 * 60;
        let bell = if urgent {
            with_alpha(RED_LAMP, 0.65 + 0.35 * pulse(t, 2.))
        } else {
            SOFT
        };
        icon(Icon::Bell, to + 16., cy, 17., bell);
        // Right: running speed when it differs, then career retries.
        let mut rx = area.x + area.w - 112.;
        let schedule = self.schedule();
        if (schedule.speed - 1.).abs() > 0.001 {
            let speed = format!("{}%", (schedule.speed * 100.).round());
            let w = measure(&speed, 15.) + 22.;
            icon_value(speed_icon(self.shift), &speed, rx - w - 16., cy, 15., SOFT);
        }
        icon(Icon::Restart, rx + 8., cy, 16., BRASS);
        rx += 26.;
        for k in 0..3 {
            let pip = vec2(rx + 5. + k as f32 * 14., cy);
            if k < self.retries {
                shapes::fill_circle(pip, 4.5, BRASS);
            } else {
                shapes::stroke_circle(pip, 4., 1.3, with_alpha(BRASS, 0.4));
            }
        }
        true
    }
    fn result(&self, area: Rect) -> bool {
        let won = self.outcome() == Outcome::Won;
        let x = area.x;
        // Rank for a finished career, otherwise the shift it ended on.
        let top = area.y + 20.;
        if won {
            icon(Icon::Crown, x + 15., top, 28., GOLD);
            draw_text_styled(self.rank(), x + 40., top + 9., style(24., CREAM));
        } else {
            icon(Icon::Flag, x + 15., top, 26., RED_LAMP);
            draw_text_styled(self.schedule().name, x + 40., top + 9., style(24., CREAM));
        }
        // The career ledger: one medal disc per shift.
        let my = area.y + 70.;
        for (k, &stars) in self.medals.iter().enumerate() {
            let at = vec2(x + 15. + k as f32 * 34., my);
            if stars > 0 {
                let (tone, _) = medal(stars);
                shapes::fill_circle(at + vec2(0., 1.5), 12., Color::new(0., 0., 0., 0.35));
                shapes::fill_circle(
                    at,
                    12.,
                    Fill::radial(at - vec2(4., 4.), 16., lighten(tone, 0.3), tone),
                );
                icon(Icon::Star, at.x, at.y, 13., darken(tone, 0.55));
            } else if !won && k == self.shift {
                shapes::fill_circle(at, 12., Color::new(0.30, 0.08, 0.07, 1.));
                icon(Icon::Cross, at.x, at.y, 13., RED_LAMP);
            } else {
                shapes::stroke_circle(at, 11., 1.5, DIM);
            }
        }
        // Big icon stats.
        let cy = area.y + 122.;
        let mut sx = x;
        sx += icon_value(Icon::Star, &self.stars().to_string(), sx, cy, 24., GOLD) + 40.;
        sx += icon_value(
            Icon::Train,
            &self.delivered_total.to_string(),
            sx,
            cy,
            24.,
            CREAM,
        ) + 40.;
        let incidents = if self.incidents == 0 {
            GREEN_LAMP
        } else {
            RED_LAMP
        };
        sx += icon_value(
            Icon::Alert,
            &self.incidents.to_string(),
            sx,
            cy,
            24.,
            incidents,
        ) + 40.;
        icon_value(Icon::Restart, &self.retries.to_string(), sx, cy, 24., BRASS);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot_until(game: &mut Game, mut done: impl FnMut(&Game) -> bool) {
        for tick in 0..130_000 {
            if done(game) {
                return;
            }
            let input = game.bot_input(tick);
            game.tick(&input);
        }
        panic!("condition never reached");
    }

    #[test]
    fn progress_counts_shifts_from_first_to_last() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 8)));
        bot_until(&mut game, |g| g.shift == 1);
        assert_eq!(game.progress(), Some((2, 8)));
        bot_until(&mut game, |g| g.outcome() != Outcome::Playing);
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((8, 8)));
        let summary = game.summary();
        assert!(summary.contains("all 61 trains delivered"), "{summary}");
        assert!(summary.starts_with(game.rank()), "{summary}");
        assert_eq!(summary.matches(". ").count(), 0, "one sentence: {summary}");
    }

    #[test]
    fn a_delivery_bursts_and_pays_at_its_platform() {
        let mut game = Game::new();
        bot_until(&mut game, |g| {
            g.events
                .iter()
                .any(|e| matches!(e, Event::Delivered { .. }))
        });
        let mut fx = Juice::new();
        game.effects(Cue::Good, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks >= 26, "{counts:?}");
        assert!(counts.popups >= 1 && counts.rings >= 2, "{counts:?}");
        // The coin popup carries what the delivery earned.
        let Some(Event::Delivered { earned, .. }) = game
            .events
            .iter()
            .copied()
            .find(|e| matches!(e, Event::Delivered { .. }))
        else {
            unreachable!("found above");
        };
        assert_eq!(earned, 380, "an on-time delivery pays the full commission");
        // A tick that accepted nothing shows nothing.
        let idle = Game::new();
        let mut quiet = Juice::new();
        idle.effects(Cue::None, &mut quiet);
        assert!(quiet.is_idle());
    }

    #[test]
    fn refusals_and_incidents_are_felt_where_they_happen() {
        // Locked points: a red ring and a lock popup, no cue.
        let mut game = Game::new();
        game.tick(&Input {
            primary: true,
            ..Input::default()
        });
        let click = |game: &mut Game, at: Vec2| {
            game.tick(&Input {
                click: true,
                pointer: at,
                ..Input::default()
            })
        };
        let (points, gate) = (game.junction(0), game.lamp(0));
        click(&mut game, points);
        click(&mut game, gate);
        while !game.points_locked(0) {
            game.tick(&Input::default());
        }
        assert_eq!(click(&mut game, points), Cue::None);
        let mut fx = Juice::new();
        game.effects(Cue::None, &mut fx);
        assert!(fx.counts().rings >= 1 && fx.counts().popups == 1);
        // A collision on the bridge throws sparks and shakes the view.
        let mut game = Game::new();
        game.tick(&Input {
            primary: true,
            ..Input::default()
        });
        click(&mut game, Game::control_center(0));
        for _ in 0..600 {
            game.tick(&Input::default());
        }
        click(&mut game, Game::control_center(5));
        for _ in 0..600 {
            game.tick(&Input::default());
        }
        click(&mut game, Game::control_center(2));
        click(&mut game, Game::control_center(3));
        let mut fx = Juice::new();
        while game.phase == Phase::Working {
            let cue = game.tick(&Input::default());
            game.effects(cue, &mut fx);
        }
        assert_eq!(game.phase, Phase::Incident);
        assert!(fx.counts().sparks >= 36);
        fx.update(1. / 60.);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
    }

    #[test]
    fn previews_follow_the_points_and_hint_the_fix() {
        let mut game = Game::new();
        game.tick(&Input {
            primary: true,
            ..Input::default()
        });
        // W1 needs A. Straight points run it off the east end of the line.
        let route = game.route(0, game.points).expect("W1 is out");
        assert_eq!(route.end, End::Exit);
        assert_eq!(route.fix, Some(0));
        assert_eq!(route.hold, Some(1), "the red west gate holds it");
        let ui = Ui {
            t: 0.,
            hover: Hover::None,
            focus: game.focus(Hover::None),
        };
        assert_eq!(ui.focus, Some(0));
        assert_eq!(game.hint(&ui), Some(0));
        // One click north previews home, and the hint goes away once set.
        let mut north = game.points;
        north[0] = 1;
        assert_eq!(game.route(0, north).map(|r| r.end), Some(End::Home));
        game.tick(&Input {
            click: true,
            pointer: game.junction(0),
            ..Input::default()
        });
        assert_eq!(game.route(0, game.points).map(|r| r.end), Some(End::Home));
        assert_eq!(game.hint(&ui), None);
        // Hovering a board row focuses that service.
        let row = vec2(PANEL.x + 40., ROWS_TOP + row_height(4) * 2.5);
        assert_eq!(game.hover_at(row), Hover::Row(2));
        assert_eq!(game.focus(Hover::Row(2)), Some(2));
        assert_eq!(game.hover_at(game.lamp(3)), Hover::Control(5));
    }

    #[test]
    fn hud_is_short_and_status_keeps_the_full_story_for_readers() {
        let mut game = Game::new();
        let hud = game.hud();
        assert_eq!(hud.stage, "The apprentice's bell");
        assert_eq!(hud.objective, "0/4 trains delivered");
        assert!(hud.status.contains("West points serve A/B"));
        let mut tick = 0;
        while game.phase != Phase::Working {
            game.tick(&game.bot_input(tick));
            tick += 1;
        }
        assert!(game.hud().status.contains("Clock running"));
    }

    #[test]
    fn meta_help_is_four_short_bullets() {
        let help = crate::META.help;
        assert!(help.len() <= 4);
        for bullet in help {
            assert!(bullet.split_whitespace().count() < 15, "{bullet}");
        }
        assert!(crate::META.subtitle.split_whitespace().count() <= 3);
    }
}
