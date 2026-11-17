//! The chart: water, reefs, hazards, air bells, the dock and the boat, plus
//! the host hooks. Decorative motion follows the presentation clock; anything
//! that reflects a rule (hazard position, vent phase, hit flashes) follows
//! simulation ticks.
use crate::{
    Game,
    art::*,
    content::{HazardKind, TOTAL_DIVERS},
    model::{BASE, DOCK_RANGE, LOW_AIR, Phase, RescueState, Trail, WINCH_RANGE, WINCH_SPEED},
};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, draw_text_styled};
use ferravex_arcade::*;
use std::f32::consts::{FRAC_PI_2, TAU};

/// The water window: simulation units map one-to-one onto this rectangle.
pub(crate) const SEA_X: f32 = 28.;
pub(crate) const SEA_Y: f32 = 152.;
pub(crate) const SEA_W: f32 = 640.;
pub(crate) const SEA_H: f32 = 400.;
pub(crate) const SEA_BOTTOM: f32 = SEA_Y + SEA_H;
/// Approach distance at which reach rings and prompts appear.
const APPROACH: f32 = 72.;

/// Chart point to canvas point.
pub(crate) fn sea(p: Vec2) -> Vec2 {
    p + vec2(SEA_X, SEA_Y)
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.tick(input)
    }
    fn draw(&self) {
        // One route search per frame serves the trail and the air mark.
        let home = (self.phase == Phase::Diving).then(|| self.trail(BASE));
        self.draw_sea(home.as_ref());
        self.draw_instruments(home.as_ref());
        if self.phase != Phase::Diving {
            self.draw_overlay();
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.chart().name.to_string(),
            objective: format!("{} / {TOTAL_DIVERS} divers home", self.rescued()),
            // Never drawn during play: the narration is for status readers.
            status: match self.phase {
                Phase::Briefing => {
                    let chart = self.chart();
                    format!(
                        "{} {} {} credits, {} tugs.",
                        chart.story, chart.advice, self.credits, self.recovery
                    )
                }
                Phase::Wreck => format!("{} {} tugs left.", self.failure, self.recovery),
                _ => format!(
                    "{} {} credits, {} tugs.",
                    self.notice, self.credits, self.recovery
                ),
            },
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        format!(
            "{} of {TOTAL_DIVERS} divers home with {} points and {} gold medals.",
            self.rescued(),
            self.score,
            self.medals.iter().filter(|&&m| m == 3).count(),
        )
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.sortie() as u32, 6))
    }
    fn effects(&self, cue: Cue, fx: &mut Juice) {
        self.feedback(cue, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.draw_rescue_row(area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.draw_results(area);
        true
    }
}

impl Game {
    fn depth_fraction(&self) -> f32 {
        self.mission as f32 / 5.
    }
    /// Height of the decorative seabed above the window floor at sea-local `x`.
    fn seabed(&self, x: f32) -> f32 {
        let m = self.mission as f32;
        (5. + 3. * (x * 0.021 + m * 1.3).sin() + 2. * (x * 0.067 + m * 0.7).sin()).max(1.5)
    }
    /// The colour of the route home when the boat should take it now: gold
    /// for a full (or final) load, coral when air is down to the trip's need.
    pub(crate) fn homeward(&self, trail: &Trail) -> Option<Color> {
        if self.phase != Phase::Diving {
            None
        } else if self.oxygen <= self.air_reserve(trail.distance) {
            Some(CORAL)
        } else if self.onboard() >= self.capacity() || self.onboard() > 0 && !self.waiting() {
            Some(GOLD)
        } else {
            None
        }
    }

    fn draw_sea(&self, home: Option<&Trail>) {
        // Decorative motion in tick units, from the presentation clock.
        let t = clock() * 60.;
        let deep = self.depth_fraction();
        surface(16., 112., 672., 456.);
        self.draw_strip();

        // Water column: brighter near the surface, darker on deeper charts.
        let top = mix(
            Color::new(0.07, 0.36, 0.42, 1.),
            Color::new(0.04, 0.17, 0.26, 1.),
            deep,
        );
        let bottom = mix(
            Color::new(0.015, 0.075, 0.12, 1.),
            Color::new(0.01, 0.025, 0.06, 1.),
            deep,
        );
        rounded_gradient(SEA_X, SEA_Y, SEA_W, SEA_H, 6., top, bottom);
        // Sunlight shafts, fading with depth.
        let lit = Color::new(0.65, 0.97, 1., 0.085 * (1. - deep * 0.65));
        let clear = with_alpha(lit, 0.);
        shade((0..4).flat_map(|i| {
            let sway = (t / 260. + i as f32 * 1.7).sin() * 16.;
            let x0 = SEA_X + 40. + i as f32 * 140. + sway;
            let w0 = 24. + (i % 2) as f32 * 16.;
            let y1 = SEA_Y + 230. + (i % 3) as f32 * 40.;
            quad(
                (vec2(x0, SEA_Y), lit),
                (vec2(x0 + w0, SEA_Y), lit),
                (vec2(x0 + 50., y1), clear),
                (vec2(x0 + 50. + w0 * 1.8, y1), clear),
            )
        }));
        // Surface shimmer, rippling.
        let shimmer = Rect::new(SEA_X, SEA_Y, SEA_W, 18.);
        shapes::fill_rounded_rect(
            shimmer,
            0.,
            Fill::vertical(
                shimmer,
                Color::new(0.7, 1., 1., 0.1 * (1. - deep * 0.7)),
                Color::new(0.7, 1., 1., 0.),
            ),
        );
        for i in 0..12 {
            let x = SEA_X + 20. + i as f32 * 53. + (t / 40. + i as f32).sin() * 9.;
            let w = 18. + (hash(i) % 14) as f32;
            panel(
                x,
                SEA_Y + 3. + (t / 33. + i as f32 * 2.1).sin().abs() * 3.,
                w,
                1.,
                Color::new(0.8, 1., 1., 0.12 * (1. - deep * 0.6)),
            );
        }
        // Marine snow drifting through the beam.
        let drift = (t / 5.) as usize;
        for i in 0..38 {
            let x = SEA_X + 7. + ((i * 113 + 29) % 624) as f32;
            let y = SEA_Y + 6. + ((i * 71 + drift) % 380) as f32;
            let s = if i % 3 == 0 { 2. } else { 1.2 };
            panel(x, y, s, s, Color::new(0.6, 0.9, 0.95, 0.16));
        }
        self.draw_seabed(t);
        if self.mission >= 4 {
            let y = SEA_Y + 315.;
            let inside = self.position.y > 315. && self.upgrades[1] == 0;
            let tint = Color::new(0.45, 0.12, 0.55, if inside { 0.32 } else { 0.2 });
            shade(quad(
                (vec2(SEA_X, y), with_alpha(tint, 0.)),
                (vec2(SEA_X + SEA_W, y), with_alpha(tint, 0.)),
                (vec2(SEA_X, SEA_BOTTOM), tint),
                (vec2(SEA_X + SEA_W, SEA_BOTTOM), tint),
            ));
            let crawl = (t * 0.2) % 20.;
            for k in 0..33 {
                let x = SEA_X - 16. + k as f32 * 20. + crawl;
                if x >= SEA_X && x + 10. <= SEA_X + SEA_W {
                    panel(x, y, 10., 1., Color::new(0.75, 0.5, 0.9, 0.4));
                }
            }
        }
        self.draw_current(t);
        self.draw_reefs();
        self.draw_dock(t);
        self.draw_hazards(t);
        if let Some(trail) = home
            && let Some(color) = self.homeward(trail)
        {
            self.draw_trail(trail, color);
        }
        self.draw_divers(t);
        self.draw_sub(t);
        self.draw_lights(t);
        self.draw_prompts();
        if self.phase == Phase::Diving && self.oxygen < LOW_AIR {
            // Breathing red edges as the air runs out.
            let urgency = 1. - self.oxygen / LOW_AIR;
            let a = (0.12 + 0.16 * urgency) * (0.55 + 0.45 * pulse(clock(), 1.3));
            let edge = Color::new(1., 0.25, 0.3, a);
            let none = with_alpha(edge, 0.);
            let (x0, x1, y0, y1) = (SEA_X, SEA_X + SEA_W, SEA_Y, SEA_BOTTOM);
            let d = 36.;
            shade(
                quad(
                    (vec2(x0, y0), edge),
                    (vec2(x0 + d, y0), none),
                    (vec2(x0, y1), edge),
                    (vec2(x0 + d, y1), none),
                )
                .into_iter()
                .chain(quad(
                    (vec2(x1 - d, y0), none),
                    (vec2(x1, y0), edge),
                    (vec2(x1 - d, y1), none),
                    (vec2(x1, y1), edge),
                ))
                .chain(quad(
                    (vec2(x0, y0), edge),
                    (vec2(x1, y0), edge),
                    (vec2(x0, y0 + d), none),
                    (vec2(x1, y0 + d), none),
                ))
                .chain(quad(
                    (vec2(x0, y1 - d), none),
                    (vec2(x1, y1 - d), none),
                    (vec2(x0, y1), edge),
                    (vec2(x1, y1), edge),
                )),
            );
        }
    }

    fn draw_seabed(&self, t: f32) {
        const STEPS: usize = 40;
        let step = SEA_W / STEPS as f32;
        let rim = Color::new(0.1, 0.24, 0.26, 1.);
        let floor = Color::new(0.01, 0.035, 0.055, 1.);
        // Kelp sways behind the sand line.
        for i in 0..15 {
            let local = 18. + i as f32 * 43. + (hash(i) % 17) as f32;
            let base = vec2(SEA_X + local, SEA_BOTTOM - self.seabed(local) + 2.);
            let height = 46. + (hash(i + 40) % 50) as f32;
            let phase = t / 110. + i as f32 * 0.9;
            let mut points = [Vec2::ZERO; 5];
            for (j, point) in points.iter_mut().enumerate() {
                let k = j as f32 / 4.;
                *point = base + vec2((phase + k * 1.6).sin() * 7. * k * k, -height * k);
            }
            let c = if i % 2 == 0 {
                Color::new(0.1, 0.42, 0.34, 0.7)
            } else {
                Color::new(0.08, 0.32, 0.3, 0.55)
            };
            for pair in points.windows(2) {
                draw_line(
                    pair[0].x,
                    pair[0].y,
                    pair[1].x,
                    pair[1].y,
                    3. - pair[0].distance(base) / height,
                    c,
                );
            }
            for leaf in [1, 3] {
                let p = points[leaf];
                let side = if (i + leaf) % 2 == 0 { 1. } else { -1. };
                draw_triangle(p, p + vec2(side * 7., -5.), p + vec2(side * 2., -9.), c);
            }
        }
        shade((0..STEPS).flat_map(|i| {
            let xa = i as f32 * step;
            let xb = xa + step;
            let (ya, yb) = (SEA_BOTTOM - self.seabed(xa), SEA_BOTTOM - self.seabed(xb));
            quad(
                (vec2(SEA_X + xa, ya), rim),
                (vec2(SEA_X + xb, yb), rim),
                (vec2(SEA_X + xa, SEA_BOTTOM), floor),
                (vec2(SEA_X + xb, SEA_BOTTOM), floor),
            )
        }));
        let mut ridge = [Vec2::ZERO; STEPS + 1];
        for (i, p) in ridge.iter_mut().enumerate() {
            let x = i as f32 * step;
            *p = vec2(SEA_X + x, SEA_BOTTOM - self.seabed(x));
        }
        for pair in ridge.windows(2) {
            draw_line(
                pair[0].x,
                pair[0].y,
                pair[1].x,
                pair[1].y,
                1.2,
                Color::new(0.4, 0.75, 0.72, 0.3),
            );
        }
    }

    /// Streaks drifting with the cross current through the water it moves
    /// (below chart depth 170), faster for stronger currents.
    fn draw_current(&self, t: f32) {
        let current = self.chart().current;
        if current == 0. {
            return;
        }
        let dir = current.signum();
        let speed = 0.35 + current.abs() * 0.06;
        let span = SEA_W + 60.;
        let tone = Color::new(0.72, 0.96, 1., 0.13 + current.abs() * 0.006);
        for i in 0..18 {
            let n = hash(i * 13 + 5);
            let y = SEA_Y + 182. + (n % 196) as f32;
            let len = 14. + (n % 4) as f32 * 7.;
            let x = SEA_X - 30. + ((n % 640) as f32 + dir * t * speed).rem_euclid(span);
            let (a, b) = (x.max(SEA_X), (x + len).min(SEA_X + SEA_W));
            if b - a < 4. {
                continue;
            }
            draw_line(a, y, b, y, 1.2, tone);
            let head = if dir > 0. { b } else { a };
            if head - 4. > SEA_X && head + 4. < SEA_X + SEA_W {
                let back = head - dir * 4.;
                draw_line(head, y, back, y - 3., 1.2, tone);
                draw_line(head, y, back, y + 3., 1.2, tone);
            }
        }
    }

    fn draw_reefs(&self) {
        let rock_top = Color::new(0.2, 0.31, 0.35, 1.);
        let rock_bottom = Color::new(0.06, 0.1, 0.14, 1.);
        let corals = [
            Color::new(1., 0.48, 0.5, 0.9),
            Color::new(1., 0.66, 0.36, 0.85),
            Color::new(0.82, 0.5, 0.95, 0.85),
        ];
        for (i, reef) in self.chart().reefs.iter().enumerate() {
            let p = sea(vec2(reef.x, reef.y));
            let (w, h) = (reef.w, reef.h);
            // Soft contact shadow, kept inside the water window.
            let sh = (h + 2.).min(SEA_BOTTOM - p.y - 6.).max(0.);
            let sw = w.min(SEA_X + SEA_W - p.x - 4.);
            rounded(
                p.x + 4.,
                p.y + 6.,
                sw,
                sh,
                8.,
                Color::new(0., 0.02, 0.04, 0.4),
            );
            rounded_gradient(p.x, p.y, w, h, 7., rock_top, rock_bottom);
            // Shaded flank for a sculpted, three-dimensional read.
            let (fx0, fx1) = (p.x + w * 0.62, p.x + w - 3.);
            let (fy0, fy1) = (p.y + 5., p.y + h - 5.);
            let clear = Color::new(0., 0.02, 0.04, 0.);
            let dusk = Color::new(0., 0.02, 0.04, 0.35);
            shade([
                [
                    (vec2(fx0, fy0), clear),
                    (vec2(fx1, fy0), dusk),
                    (vec2(fx0, fy1), clear),
                ],
                [
                    (vec2(fx0, fy1), clear),
                    (vec2(fx1, fy0), dusk),
                    (vec2(fx1, fy1), dusk),
                ],
            ]);
            let mut y = p.y + 22.;
            let mut k = 0;
            while y < p.y + h - 10. {
                let tilt = if (i + k) % 2 == 0 { 5. } else { -4. };
                draw_line(
                    p.x + 6.,
                    y,
                    p.x + w - 8.,
                    y + tilt,
                    1.,
                    Color::new(0.03, 0.07, 0.1, 0.55),
                );
                y += 26. + (hash(i * 7 + k) % 14) as f32;
                k += 1;
            }
            panel(
                p.x + 5.,
                p.y + 1.5,
                w - 10.,
                1.5,
                Color::new(0.6, 0.92, 0.9, 0.35),
            );
            // Coral tufts along the lit ledge.
            let count = (w / 15.) as usize;
            for k in 0..count {
                let n = hash(i * 31 + k);
                if n % 5 >= 3 {
                    continue;
                }
                let x = p.x + 8. + k as f32 * 15. + (n % 6) as f32;
                if x > p.x + w - 6. {
                    break;
                }
                let r = 2.2 + (n % 3) as f32 * 0.9;
                draw_circle(x, p.y + 1., r, corals[n % 3]);
            }
        }
    }

    /// Ring colour for a reach zone the boat is inside: green when a tap
    /// would work now, amber when it must brake first.
    fn reach_color(&self) -> Color {
        if self.velocity.length() <= WINCH_SPEED {
            MINT
        } else {
            AMBER
        }
    }

    fn draw_dock(&self, t: f32) {
        let d = sea(BASE);
        let diving = self.phase == Phase::Diving;
        // Tether to the surface support ship.
        rounded_gradient(
            d.x - 3.,
            SEA_Y,
            6.,
            d.y - 30. - SEA_Y,
            2.,
            Color::new(0.45, 0.5, 0.52, 1.),
            Color::new(0.22, 0.26, 0.3, 1.),
        );
        let carrying = self.onboard() > 0 && diving;
        let distance = self.position.distance(BASE);
        // Worth a stop: someone to land, or real air or hull to restore.
        let work = diving
            && (carrying
                || self.oxygen < self.max_air() * 0.9
                || self.hull < self.max_hull() * 0.9);
        let inside = work && distance < DOCK_RANGE;
        let beat = pulse(clock(), 1.1);
        // The docking zone at its true reach.
        let zone = if inside { self.reach_color() } else { GOLD };
        let strength = if inside {
            0.85
        } else if carrying || work && distance < APPROACH {
            0.45 + 0.35 * beat
        } else {
            0.32
        };
        shapes::fill_circle(d, DOCK_RANGE, with_alpha(zone, 0.05 + 0.06 * strength));
        dashed_ring(
            d,
            DOCK_RANGE,
            1.6,
            14,
            t * 0.003,
            with_alpha(zone, strength),
        );
        // Docking collar with marker lamps.
        rounded_gradient(
            d.x - 30.,
            d.y - 34.,
            60.,
            9.,
            4.,
            lighten(GOLD, 0.2),
            darken(GOLD, 0.25),
        );
        panel(d.x - 26., d.y - 33., 52., 1.5, Color::new(1., 1., 1., 0.35));
        let on = ((clock() * 2.) as u32).is_multiple_of(2);
        for side in [-1., 1.] {
            let lamp = vec2(d.x + side * 25., d.y - 24.);
            draw_circle(
                lamp.x,
                lamp.y,
                2.5,
                if on { PAPER } else { darken(GOLD, 0.3) },
            );
        }
        // Home marker: it bobs while someone aboard is waiting to land.
        let lift = if carrying { bob(clock(), 1.1, 2.5) } else { 0. };
        let badge = vec2(d.x + 46., d.y - 30. + lift);
        if carrying {
            glow(badge.x, badge.y, 22., with_alpha(GOLD, 0.25 + 0.2 * beat));
        }
        icon_badge(Icon::Home, badge.x, badge.y, 22., GOLD, INK);
    }

    fn draw_hazards(&self, t: f32) {
        for hazard in self.chart().hazards {
            let p = sea(hazard.position(self.elapsed));
            let active = hazard.active(self.elapsed);
            if self.sonar > 0 {
                let c = if active { CORAL } else { MUTED };
                shapes::fill_circle(p, hazard.radius() + 11., with_alpha(c, 0.07));
                shapes::stroke_circle(p, hazard.radius() + 11., 1.5, with_alpha(c, 0.85));
            }
            match hazard.kind {
                HazardKind::Jelly => {
                    let beat = (t / 13. + hazard.phase as f32).sin();
                    for j in -2..=2 {
                        let x = p.x + j as f32 * 3.6;
                        let wave = t / 17. + j as f32;
                        let pts = [
                            vec2(x, p.y + 1.),
                            vec2(x + wave.sin() * 2.5, p.y + 9.),
                            vec2(x + (wave + 1.).sin() * 4., p.y + 17.),
                            vec2(x + (wave + 2.).sin() * 5., p.y + 24. + beat * 2.),
                        ];
                        for pair in pts.windows(2) {
                            draw_line(
                                pair[0].x,
                                pair[0].y,
                                pair[1].x,
                                pair[1].y,
                                1.1,
                                Color::new(0.8, 0.6, 1., 0.6),
                            );
                        }
                    }
                    // Dome: a lit half-disc fan over a frilled rim.
                    let r = 11. + beat * 0.8;
                    let core = vec2(p.x - 2., p.y - 4.);
                    let inner = Color::new(0.98, 0.86, 1., 0.95);
                    let rim = Color::new(0.6, 0.34, 0.86, 0.8);
                    shade((0..10).map(|k| {
                        let a0 = std::f32::consts::PI * (1. + k as f32 / 10.);
                        let a1 = std::f32::consts::PI * (1. + (k + 1) as f32 / 10.);
                        let base = vec2(p.x, p.y + 1.);
                        [
                            (core, inner),
                            (base + vec2(a0.cos(), a0.sin() * 1.05) * r, rim),
                            (base + vec2(a1.cos(), a1.sin() * 1.05) * r, rim),
                        ]
                    }));
                    shade([[
                        (core, inner),
                        (vec2(p.x + r, p.y + 1.), rim),
                        (vec2(p.x - r, p.y + 1.), rim),
                    ]]);
                    panel(p.x - r, p.y, r * 2., 3., Color::new(0.52, 0.3, 0.8, 0.95));
                }
                HazardKind::Mine => {
                    // Mooring chain down to the seabed.
                    let local = p.x - SEA_X;
                    let floor = SEA_BOTTOM - self.seabed(local);
                    let mut y = p.y + 10.;
                    while y < floor - 2. {
                        panel(p.x - 0.7, y, 1.4, 3., Color::new(0.4, 0.42, 0.45, 0.6));
                        y += 5.;
                    }
                    for i in 0..8 {
                        let a = i as f32 * TAU / 8. + 0.2;
                        let dir = vec2(a.cos(), a.sin());
                        let tip = p + dir * 15.;
                        draw_line(
                            p.x,
                            p.y,
                            tip.x,
                            tip.y,
                            2.2,
                            Color::new(0.42, 0.36, 0.38, 1.),
                        );
                        panel(tip.x - 1.6, tip.y - 1.6, 3.2, 3.2, CORAL);
                    }
                    shapes::fill_circle(
                        p,
                        9.5,
                        Fill::radial(
                            p - vec2(3., 3.),
                            12.,
                            Color::new(0.56, 0.48, 0.5, 1.),
                            Color::new(0.16, 0.11, 0.13, 1.),
                        ),
                    );
                    draw_circle(
                        p.x,
                        p.y,
                        2.6,
                        if mine_lamp(t) {
                            CORAL
                        } else {
                            darken(CORAL, 0.55)
                        },
                    );
                }
                HazardKind::Vent => {
                    let cycle = ((self.elapsed + hazard.phase) % 300) as f32;
                    let rock_a = Color::new(0.38, 0.24, 0.2, 1.);
                    let rock_b = Color::new(0.12, 0.08, 0.09, 1.);
                    shade(quad(
                        (vec2(p.x - 6., p.y - 2.), rock_a),
                        (vec2(p.x + 6., p.y - 2.), rock_a),
                        (vec2(p.x - 16., p.y + 15.), rock_b),
                        (vec2(p.x + 16., p.y + 15.), rock_b),
                    ));
                    if active {
                        for i in 0..6 {
                            let rise = ((t as usize + i * 9) % 30) as f32;
                            let fade = 1. - rise / 30.;
                            draw_circle(
                                p.x + (i as f32 - 2.5) * 5. + (rise / 5. + i as f32).sin() * 3.,
                                p.y - 4. - rise * 1.1,
                                3. + rise * 0.12,
                                Color::new(1., 0.6, 0.3, 0.55 * fade),
                            );
                        }
                    } else {
                        // Warming ember and a countdown arc to the next burst.
                        let warm = cycle / 180.;
                        shapes::stroke_arc(
                            vec2(p.x, p.y + 4.),
                            15.,
                            -FRAC_PI_2,
                            warm * TAU,
                            Stroke::new(2., Color::new(1., 0.72, 0.3, 0.55)).cap(Cap::Round),
                        );
                    }
                    rounded(
                        p.x - 6.,
                        p.y - 3.,
                        12.,
                        3.,
                        1.5,
                        if active {
                            Color::new(1., 0.65, 0.35, 1.)
                        } else {
                            Color::new(0.85, 0.5, 0.25, 1.)
                        },
                    );
                }
            }
        }
    }

    /// Marching dots along the pilot's route to the dock.
    fn draw_trail(&self, trail: &Trail, color: Color) {
        if trail.len == 0 {
            return;
        }
        let at = |i: usize| -> Vec2 {
            if i == 0 {
                self.position
            } else {
                trail.points[(i - 1).min(trail.len - 1)]
            }
        };
        let flow = clock() * 2.2;
        let boat = sea(self.position);
        let mut first = None;
        for i in 1..=trail.len {
            // Smooth the grid staircase for display.
            let p = sea((at(i - 1) + at(i) * 2. + at(i + 1)) * 0.25);
            if p.distance(boat) < 26. {
                continue;
            }
            first.get_or_insert(p);
            let wave = 0.5 + 0.5 * ((i as f32 * 0.55 - flow) * 2.).sin();
            draw_circle(p.x, p.y, 1.8 + wave, with_alpha(color, 0.3 + 0.5 * wave));
        }
        // A chevron at the bow points the way.
        if let Some(next) = first {
            let dir = (next - boat).normalize_or_zero();
            if dir != Vec2::ZERO {
                let side = vec2(-dir.y, dir.x);
                let tip = boat + dir * (30. + bob(clock(), 1.6, 2.));
                shapes::fill_triangle(
                    tip,
                    tip - dir * 9. + side * 6.,
                    tip - dir * 9. - side * 6.,
                    with_alpha(color, 0.9),
                );
            }
        }
    }

    /// Names and Space prompts, drawn over the boat on the side away from it.
    fn draw_prompts(&self) {
        if self.phase != Phase::Diving {
            return;
        }
        let slow = self.velocity.length() <= WINCH_SPEED;
        let room = self.onboard() < self.capacity();
        let hover = pointer();
        let listed = crate::panel::manifest_at(hover, self.chart().divers.len());
        let boat = sea(self.position);
        let prompt = |at: Vec2, from: Vec2| {
            let away = (at - from).try_normalize().unwrap_or(vec2(0., 1.));
            let c = at + away * 38.;
            let w = keycap_w("Space");
            keycap("Space", "", c.x - w * 0.5, c.y - 10., PAPER);
        };
        for (index, (point, name)) in self.chart().divers.iter().enumerate() {
            if self.divers[index] != RescueState::Waiting {
                continue;
            }
            let p = sea(*point);
            let distance = self.position.distance(*point);
            if room && slow && distance < WINCH_RANGE {
                prompt(p, boat);
                continue;
            }
            if distance < APPROACH || listed == Some(index) || hover.distance(p) < 22. {
                // Above the bell, unless the boat is coming from above.
                let y = if boat.y < p.y - 4. {
                    p.y + 32.
                } else {
                    p.y - 22.
                };
                let a = if room { 1. } else { 0.5 };
                draw_text_styled(
                    name,
                    p.x,
                    y,
                    style(12., with_alpha(lighten(AQUA, 0.3), a)).with_align(TextAlign::Center),
                );
            }
        }
        let dock = sea(BASE);
        let worth = self.onboard() > 0
            || self.oxygen < self.max_air() * 0.9
            || self.hull < self.max_hull() * 0.9;
        if worth && slow && self.position.distance(BASE) < DOCK_RANGE {
            prompt(dock + vec2(0., 6.), boat);
        }
    }

    fn draw_divers(&self, t: f32) {
        let diving = self.phase == Phase::Diving;
        let room = self.onboard() < self.capacity();
        let hover = pointer();
        let listed = crate::panel::manifest_at(hover, self.chart().divers.len());
        for (index, (point, _)) in self.chart().divers.iter().enumerate() {
            if self.divers[index] != RescueState::Waiting {
                continue;
            }
            let p = sea(*point);
            let distance = self.position.distance(*point);
            // Bells fade back while every berth is taken.
            let a = if room || !diving { 1. } else { 0.5 };
            let glint = (t / 28. + index as f32).sin();
            // Air bell: glass dome on a weighted skirt.
            let dome = Rect::new(p.x - 12., p.y - 14., 24., 26.);
            shapes::fill_rounded_rect(
                dome,
                12.,
                Fill::radial(
                    vec2(p.x - 4., p.y - 8.),
                    18.,
                    Color::new(0.55, 1., 0.96, 0.4 * a),
                    Color::new(0.15, 0.55, 0.6, 0.22 * a),
                ),
            );
            shapes::stroke_rounded_rect(
                dome,
                12.,
                1.4,
                Color::new(0.45, 0.98, 0.94, (0.65 + 0.3 * glint) * a),
            );
            panel(
                p.x - 14.,
                p.y + 10.,
                28.,
                5.,
                Color::new(0.26, 0.32, 0.36, 1.),
            );
            panel(
                p.x - 14.,
                p.y + 10.,
                28.,
                1.5,
                Color::new(0.55, 0.64, 0.68, 1.),
            );
            // Diver silhouette waving from inside.
            let wave = (t / 9. + index as f32).sin() * 3.;
            let body = with_alpha(AQUA, a);
            draw_circle(p.x, p.y - 5., 3.2, body);
            draw_line(p.x, p.y - 2., p.x, p.y + 7., 4., body);
            draw_line(p.x, p.y, p.x + 6., p.y - 5. + wave, 1.8, body);
            draw_line(p.x, p.y, p.x - 5., p.y + 3., 1.8, body);
            panel(p.x - 7., p.y - 9., 2., 5., Color::new(1., 1., 1., 0.4 * a));
            // Bubbles escaping from the dome.
            for b in 0..2 {
                let rise = ((t as usize + b * 23 + index * 11) % 46) as f32;
                draw_circle(
                    p.x + 4. + (rise / 6.).sin() * 2.,
                    p.y - 15. - rise * 0.6,
                    1.2 + b as f32 * 0.4,
                    Color::new(0.7, 1., 1., 0.45 * (1. - rise / 46.) * a),
                );
            }
            if !diving {
                continue;
            }
            // Reach ring on approach: the winch works inside it.
            let near = distance < APPROACH;
            if near && room {
                let inside = distance < WINCH_RANGE;
                let c = if inside { self.reach_color() } else { AQUA };
                let strength = if inside {
                    0.9
                } else {
                    0.25 + 0.5 * (1. - distance / APPROACH)
                };
                shapes::fill_circle(p, WINCH_RANGE, with_alpha(c, 0.06 * strength));
                dashed_ring(p, WINCH_RANGE, 1.4, 12, -t * 0.004, with_alpha(c, strength));
            }
            let linked = listed == Some(index);
            if linked {
                // Hovering a name in the manifest points at its bell.
                let r = 21. + 3. * pulse(clock(), 1.5);
                shapes::stroke_circle(p, r, 2., with_alpha(lighten(AQUA, 0.3), 0.9));
            }
            if !(near || linked || hover.distance(p) < 22.) && room {
                // A beacon chevron calls the boat over.
                let y = p.y - 27. + bob(clock() + index as f32 * 0.37, 0.8, 2.5);
                icon(Icon::ArrowDown, p.x, y, 12., with_alpha(AQUA, 0.85));
            }
        }
    }

    fn draw_sub(&self, t: f32) {
        let p = sea(self.position);
        let f = self.facing;
        let nose = p + vec2(f * 19., 1.);
        // Headlight cone fading into the water, clipped to the window.
        let reach = 150.;
        let end_x = (nose.x + f * reach).clamp(SEA_X, SEA_X + SEA_W);
        let spread = (end_x - nose.x).abs() / reach * 46.;
        let beam = Color::new(1., 0.93, 0.62, 0.2);
        let fade = with_alpha(beam, 0.);
        shade([[
            (nose, beam),
            (vec2(end_x, (p.y - spread).max(SEA_Y)), fade),
            (vec2(end_x, (p.y + spread).min(SEA_BOTTOM)), fade),
        ]]);
        if self.sonar > 0 {
            let radius = (180 - self.sonar) as f32 * 1.4;
            let alpha = 0.5 * (self.sonar as f32 / 180.);
            for ring in 0..2 {
                let r = radius - ring as f32 * 20.;
                if r > 0. {
                    // Clip the wave to the water using short segments.
                    for segment in 0..90 {
                        let a = segment as f32 * TAU / 90.;
                        let b = (segment + 1) as f32 * TAU / 90.;
                        let aa = p + vec2(a.cos(), a.sin()) * r;
                        let bb = p + vec2(b.cos(), b.sin()) * r;
                        if [aa, bb].iter().all(|v| {
                            v.x >= SEA_X
                                && v.x <= SEA_X + SEA_W
                                && v.y >= SEA_Y
                                && v.y <= SEA_BOTTOM
                        }) {
                            draw_line(
                                aa.x,
                                aa.y,
                                bb.x,
                                bb.y,
                                1.5 - ring as f32 * 0.5,
                                Color::new(0.3, 1., 0.8, alpha),
                            );
                        }
                    }
                }
            }
        }
        // Wake bubbles from the propeller when moving.
        let speed = (self.velocity.length() / 90.).min(1.);
        if speed > 0.1 {
            for b in 0..4 {
                let age = ((t as usize + b * 7) % 28) as f32;
                let q = p + vec2(
                    -f * (24. + age * 1.3),
                    (age / 4. + b as f32).sin() * 3. - age * 0.25,
                );
                draw_circle(
                    q.x,
                    q.y,
                    1.3 + age * 0.06,
                    Color::new(0.75, 1., 1., 0.4 * speed * (1. - age / 28.)),
                );
            }
        }
        let hit = self.impact_cooldown > 0 && self.elapsed % 8 < 4;
        let body = if hit { CORAL } else { GOLD };
        let light_body = lighten(body, 0.28);
        let dark_body = darken(body, 0.38);
        // Tail fins.
        let tail = p.x - f * 17.;
        draw_triangle(
            vec2(tail, p.y - 2.),
            vec2(tail - f * 7., p.y - 10.),
            vec2(tail - f * 7., p.y - 1.),
            dark_body,
        );
        draw_triangle(
            vec2(tail, p.y + 2.),
            vec2(tail - f * 7., p.y + 10.),
            vec2(tail - f * 7., p.y + 1.),
            dark_body,
        );
        // Spinning propeller.
        let spin = (t * (0.15 + speed * 0.5)).sin().abs();
        let prop = vec2(p.x - f * 23., p.y);
        draw_ellipse(
            prop.x,
            prop.y,
            1.6,
            2. + 6. * spin,
            0.,
            Color::new(0.85, 0.85, 0.8, 0.9),
        );
        draw_line(p.x - f * 18., p.y, prop.x, prop.y, 2., dark_body);
        // Conning tower and periscope.
        rounded_gradient(p.x - f * 2. - 6., p.y - 16., 12., 9., 3., light_body, body);
        draw_line(
            p.x - f * 1.,
            p.y - 16.,
            p.x - f * 1.,
            p.y - 22.,
            2.,
            dark_body,
        );
        draw_line(
            p.x - f * 1.,
            p.y - 22.,
            p.x + f * 3.,
            p.y - 22.,
            2.,
            dark_body,
        );
        // Hull.
        let hull = Rect::new(p.x - 19., p.y - 10., 38., 20.);
        shapes::fill_rounded_rect(hull, 10., Fill::vertical(hull, light_body, dark_body));
        panel(p.x - 15., p.y + 3., 30., 4., Color::new(0., 0., 0., 0.18));
        panel(p.x - 11., p.y - 8., 22., 1.5, Color::new(1., 1., 1., 0.45));
        // One porthole per berth, lit while someone rides behind it.
        let berths = self.capacity();
        let aboard = self.onboard();
        let (spacing, r) = match berths {
            2 => (11., 3.8),
            3 => (9., 3.8),
            _ => (7.6, 3.3),
        };
        for i in 0..berths {
            let dx = (i as f32 - (berths as f32 - 1.) * 0.5) * spacing + 1.;
            let c = vec2(p.x + dx * f, p.y - 1.);
            draw_circle(c.x, c.y, r, Color::new(0.08, 0.12, 0.16, 1.));
            if i < aboard {
                draw_circle(c.x, c.y, r - 1.1, lighten(AQUA, 0.25));
                draw_circle(c.x, c.y + 0.9, (r - 1.1) * 0.45, darken(AQUA, 0.55));
            } else {
                draw_circle(c.x, c.y, r - 1.1, Color::new(0.14, 0.3, 0.34, 1.));
            }
            panel(c.x - 1.4, c.y - 1.6, 1.2, 1.2, Color::new(1., 1., 1., 0.7));
        }
        // Headlamp.
        draw_circle(nose.x, nose.y, 2.4, PAPER);
    }

    /// Every additive light in one contiguous pass, so they share a draw.
    fn draw_lights(&self, t: f32) {
        let d = sea(BASE);
        if self.onboard() > 0 && self.phase == Phase::Diving {
            let pulse = 0.5 + 0.5 * (t / 14.).sin();
            light(
                vec2(d.x, d.y),
                36.,
                Color::new(1., 0.75, 0.3, 0.08 + 0.06 * pulse),
            );
        }
        for (index, (point, _)) in self.chart().divers.iter().enumerate() {
            if self.divers[index] == RescueState::Waiting {
                let breathe = 0.5 + 0.5 * (t / 30. + index as f32 * 1.3).sin();
                light(
                    sea(*point) - vec2(0., 2.),
                    24.,
                    Color::new(0.4, 1., 0.95, 0.1 + 0.08 * breathe),
                );
            }
        }
        for hazard in self.chart().hazards {
            let p = sea(hazard.position(self.elapsed));
            match hazard.kind {
                HazardKind::Jelly => light(vec2(p.x, p.y - 3.), 26., Color::new(0.7, 0.4, 1., 0.3)),
                HazardKind::Mine if mine_lamp(t) => {
                    light(vec2(p.x, p.y), 14., Color::new(1., 0.3, 0.3, 0.7));
                }
                HazardKind::Mine => {}
                HazardKind::Vent if hazard.active(self.elapsed) => {
                    light(vec2(p.x, p.y - 8.), 34., Color::new(1., 0.42, 0.18, 0.55));
                }
                HazardKind::Vent => {
                    let warm = ((self.elapsed + hazard.phase) % 300) as f32 / 180.;
                    light(
                        vec2(p.x, p.y - 2.),
                        12. + warm * 6.,
                        Color::new(1., 0.5, 0.2, 0.15 + 0.35 * warm),
                    );
                }
            }
        }
        let p = sea(self.position);
        light(
            vec2(p.x + self.facing * 19., p.y + 1.),
            16.,
            Color::new(1., 0.9, 0.6, 0.55),
        );
    }
}

/// Mine warning lamps blink one beat in three.
fn mine_lamp(t: f32) -> bool {
    ((t / 24.) as u32).is_multiple_of(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Event;
    fn press() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn felt(g: &mut Game, input: &Input) -> ferravex_arcade::juice::JuiceCounts {
        let cue = g.tick(input);
        let mut fx = Juice::new();
        g.effects(cue, &mut fx);
        fx.counts()
    }
    #[test]
    fn progress_pips_span_the_whole_campaign() {
        let mut g = Game::new();
        assert_eq!(g.progress(), Some((1, 6)));
        for tick in 0..120_000 {
            let input = g.pilot(tick);
            g.tick(&input);
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won);
        assert_eq!(g.progress(), Some((6, 6)));
        assert_eq!(g.summary().matches('.').count(), 1, "{}", g.summary());
    }
    #[test]
    fn header_shows_the_station_and_a_short_goal() {
        let g = Game::new();
        let hud = g.hud();
        assert_eq!(hud.stage, "The Kelp Nursery");
        assert_eq!(hud.objective, "0 / 27 divers home");
        assert!(hud.objective.split_whitespace().count() <= 5);
        // Help stays short: four bullets, each under fifteen words.
        assert!(crate::META.help.len() <= 4);
        assert!(
            crate::META
                .help
                .iter()
                .all(|line| line.split_whitespace().count() < 15)
        );
    }
    #[test]
    fn rescues_docking_and_hits_are_felt_where_they_happen() {
        let mut g = Game::new();
        let launch = felt(&mut g, &press());
        assert!(launch.rings > 0 && launch.sparks > 0, "{launch:?}");
        // Winching a diver: sparkle, bubbles, rings and a person popup.
        g.balanced = true;
        g.position = g.chart().divers[0].0;
        let winch = felt(&mut g, &press());
        assert!(
            winch.popups >= 1 && winch.sparks > 10 && winch.rings >= 2,
            "{winch:?}"
        );
        // Landing at the dock: a person popup at the dock and pay at the purse.
        g.position = BASE;
        let dock = felt(&mut g, &press());
        assert!(dock.popups >= 2 && dock.rings >= 3, "{dock:?}");
        // A hard reef strike: debris and a hull popup.
        g.position = vec2(208., 170.);
        g.velocity = vec2(90., 0.);
        let strike = felt(
            &mut g,
            &Input {
                right: true,
                ..Input::default()
            },
        );
        assert!(matches!(g.events.as_slice(), [Event::Impact { .. }]));
        assert!(strike.sparks > 0 && strike.popups == 1, "{strike:?}");
        // A quiet tick adds nothing.
        let quiet = felt(&mut g, &Input::default());
        assert_eq!((quiet.sparks, quiet.popups, quiet.rings), (0, 0, 0));
    }
    #[test]
    fn refused_refits_shake_their_card_and_fitted_ones_burst() {
        let mut g = Game::new();
        let poor = felt(
            &mut g,
            &Input {
                slot: Some(3),
                ..Input::default()
            },
        );
        assert!(poor.rings == 1 && poor.popups == 1, "{poor:?}");
        g.credits = 100;
        let bought = felt(
            &mut g,
            &Input {
                slot: Some(1),
                ..Input::default()
            },
        );
        assert!(bought.sparks > 20 && bought.popups == 1 && bought.rings == 2);
    }
}
