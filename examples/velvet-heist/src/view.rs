//! Read-only heist art and the shared arcade adapter.
//!
//! Floor tiles stay flat quads with a one-unit grout gap; walls gain a
//! three-quarter face and gold trim. Objects, actors and previews are the
//! anti-aliased sprites in [`crate::sprites`]. The floor plan explains the
//! rules visually: guard sight is tinted strongly where it would catch the
//! thief in the current posture, Space prompts float over the reachable
//! object, standing footsteps and coins draw their hearing reach, and the
//! pointer previews a coin's arc and which guards would come.
use crate::content::HEISTS;
use crate::model::{COLS, Game, Guard, MAP_X, MAP_Y, Phase, Pos, ROWS, TILE};
use crate::sprites::*;
use crate::sprites::{GOLD, RED};
use ferravex_arcade::typography::{TextStyle, text_width};
use ferravex_arcade::*;

pub(crate) const MAP_W: f32 = TILE * COLS as f32;
pub(crate) const MAP_H: f32 = TILE * ROWS as f32;

pub(crate) fn center(p: Pos) -> Vec2 {
    vec2(
        MAP_X + (f32::from(p.x) + 0.5) * TILE,
        MAP_Y + (f32::from(p.y) + 0.5) * TILE,
    )
}
/// Where a guard's speech bubble floats.
pub(crate) fn head(p: Pos) -> Vec2 {
    center(p) + vec2(11., -27.)
}
/// How exposed the thief is right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exposure {
    Seen,
    Hidden,
    Exposed,
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, i: &Input) -> Cue {
        self.step(i)
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        Game::summary(self)
    }
    fn bot_input(&self, t: u64) -> Input {
        self.pilot(t)
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.content().name.to_owned(),
            objective: self.objective().to_owned(),
            status: self.status(),
            score: self.score,
        }
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.heist() as u32, HEISTS.len() as u32))
    }
    fn effects(&self, cue: Cue, fx: &mut Juice) {
        self.feedback(cue, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.draw_goal(area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.draw_results(area);
        true
    }
    fn draw(&self) {
        rounded_gradient(
            16.,
            112.,
            928.,
            456.,
            0.,
            rgba(0.06, 0.04, 0.08, 1.),
            rgba(0.035, 0.025, 0.05, 1.),
        );
        self.draw_map();
        self.draw_sidebar();
        if matches!(
            self.phase,
            Phase::Briefing | Phase::Debrief | Phase::Setback
        ) {
            self.draw_card();
        }
    }
}
impl Game {
    /// A short goal, published for status readers; the header draws it.
    pub(crate) fn objective(&self) -> &'static str {
        match self.phase {
            Phase::Briefing | Phase::Infiltrating if self.stolen < self.needed => {
                "Take every record, then exit"
            }
            Phase::Briefing | Phase::Infiltrating => "Escape through the exit",
            Phase::Debrief => "Heist complete",
            Phase::Setback => "Retry with a favor",
            Phase::Escaped => "History returned",
            Phase::Caught => "Career over",
        }
    }
    /// The full narration for screen readers; the play screen shows it as art.
    fn status(&self) -> String {
        match self.phase {
            Phase::Briefing => format!("{}. {}", self.content().object, self.content().briefing),
            Phase::Infiltrating => {
                let left = self.seconds_left();
                format!(
                    "{} {}:{:02} left, alarm {}%.",
                    self.note,
                    left / 60,
                    left % 60,
                    self.alarm / 10
                )
            }
            _ => self.note.to_owned(),
        }
    }
    pub(crate) fn exposure(&self) -> Exposure {
        if self.seen {
            Exposure::Seen
        } else if self.crouched && self.shadow(self.player) {
            Exposure::Hidden
        } else {
            Exposure::Exposed
        }
    }
    /// True when `guard` sees the thief this frame (only during play).
    pub(crate) fn watching(&self, guard: &Guard) -> bool {
        self.phase == Phase::Infiltrating && self.cone(*guard, self.player, self.crouched)
    }
    /// The extraction tile.
    pub(crate) fn exit(&self) -> Option<Pos> {
        self.tiles
            .iter()
            .position(|&t| t == b'E')
            .map(|i| Pos::new((i % COLS) as u8, (i / COLS) as u8))
    }
    fn draw_map(&self) {
        card(16., 116., 642., 448., rgba(0.07, 0.05, 0.09, 1.));
        self.draw_strip();
        draw_rectangle(
            MAP_X - 1.,
            MAP_Y - 1.,
            MAP_W + 1.,
            MAP_H + 1.,
            rgba(0.03, 0.02, 0.04, 1.),
        );
        let t = clock();
        for y in 0..ROWS {
            for x in 0..COLS {
                self.draw_tile(Pos::new(x as u8, y as u8), t);
            }
        }
        let live = self.phase == Phase::Infiltrating;
        if live {
            self.draw_aim(t);
            self.draw_prompt(t);
        }
        self.draw_sound();
        for g in &self.guards[..self.guard_count] {
            self.draw_guard(g);
        }
        self.draw_thief(t);
        self.draw_work();
        self.draw_bubbles(t);
        if live && self.stolen >= self.needed {
            self.draw_beacon(t);
        }
        if live {
            self.draw_prompt_key(t);
        }
    }
    fn draw_tile(&self, p: Pos, t: f32) {
        let (x, y) = (usize::from(p.x), usize::from(p.y));
        let tile = self.tile(p);
        let c = center(p);
        let (px, py) = (c.x - TILE / 2., c.y - TILE / 2.);
        if tile == b'#' {
            let below = p.shifted(0, 1).is_some_and(|b| self.tile(b) != b'#');
            let above = p.shifted(0, -1).is_some_and(|b| self.tile(b) != b'#');
            draw_rectangle(px, py, TILE, TILE, rgba(0.115, 0.095, 0.15, 1.));
            draw_rectangle(px, py, TILE, 1., rgba(0.17, 0.14, 0.21, 1.));
            draw_rectangle(px + TILE - 1., py, 1., TILE, rgba(0.07, 0.055, 0.09, 1.));
            if above {
                draw_rectangle(px, py, TILE, 2., with_alpha(GOLD, 0.55));
            }
            if below {
                // Wainscot face with a gold rail.
                draw_rectangle(px, py + 25., TILE, 14., rgba(0.07, 0.05, 0.09, 1.));
                draw_rectangle(px, py + 24., TILE, 1.5, GOLD);
                draw_rectangle(px, py + 30., TILE, 1., rgba(0.36, 0.27, 0.16, 1.));
                for i in 0..2 {
                    draw_rectangle(
                        px + 9. + i as f32 * 19.,
                        py + 32.,
                        1.,
                        6.,
                        rgba(0.2, 0.15, 0.12, 1.),
                    );
                }
            } else if (x + y) % 3 == 0 {
                // Deco medallion on the cap.
                let m = vec2(c.x, c.y - 2.);
                draw_poly(m.x, m.y, 4, 5., 45., rgba(0.2, 0.16, 0.2, 1.));
                draw_poly(m.x, m.y, 4, 2.5, 45., rgba(0.32, 0.25, 0.2, 1.));
            }
            return;
        }
        let shadow = self.shadow(p);
        let base = if shadow {
            if (x + y) % 2 == 0 {
                rgba(0.17, 0.10, 0.23, 1.)
            } else {
                rgba(0.155, 0.09, 0.21, 1.)
            }
        } else if (x + y) % 2 == 0 {
            rgba(0.27, 0.15, 0.20, 1.)
        } else {
            rgba(0.235, 0.13, 0.175, 1.)
        };
        draw_rectangle(px, py, TILE - 1., TILE - 1., base);
        draw_rectangle(px, py, TILE - 1., 1., lighten(base, 0.08));
        if shadow {
            draw_line(
                px + 4.,
                py + TILE - 6.,
                px + TILE - 6.,
                py + 4.,
                1.,
                rgba(0.25, 0.16, 0.33, 1.),
            );
        } else if (x + y) % 2 == 0 {
            // Carpet diamond.
            draw_poly(c.x, c.y, 4, 4., 0., darken(base, 0.18));
        }
        // Occlusion under a wall face.
        if p.shifted(0, -1).is_some_and(|a| self.tile(a) == b'#') {
            draw_rectangle(px, py, TILE - 1., 5., rgba(0., 0., 0., 0.3));
            draw_rectangle(px, py + 5., TILE - 1., 4., rgba(0., 0., 0., 0.12));
        }
        // Guard sight: faint where a standing thief would be seen, strong
        // where the thief would be seen in the current posture.
        for g in &self.guards[..self.guard_count] {
            if self.cone(*g, p, false) {
                let danger = !self.crouched || self.cone(*g, p, true);
                let alpha = match (g.search > 0, danger) {
                    (true, true) => 0.26,
                    (false, true) => 0.17,
                    _ => 0.055,
                };
                let tint = if g.search > 0 {
                    rgba(1., 0.32, 0.2, alpha)
                } else {
                    rgba(1., 0.82, 0.34, alpha)
                };
                draw_rectangle(px, py, TILE - 1., TILE - 1., tint);
            }
        }
        let bob = (t * 2.4 + f32::from(p.x) * 0.9).sin() * 1.5;
        let shine = pulse(t + f32::from(p.y) * 0.3, 0.8);
        match tile {
            b'=' => door(c, 1., !self.key, if self.key { shine } else { 0. }),
            b'E' => {
                let ready = self.stolen >= self.needed;
                if ready {
                    glow(c.x, c.y, 30. + 6. * pulse(t, 1.2), with_alpha(MINT, 0.34));
                }
                exit_sign(c, 30., 32., ready, t);
            }
            b'K' => {
                glow(c.x, c.y, 16. + 3. * shine, with_alpha(GOLD, 0.24));
                draw_ellipse(c.x, c.y + 12., 9., 2.5, 0., rgba(0., 0., 0., 0.4));
                key(c.x, c.y + bob, 1.05, GOLD, rgba(0.2, 0.12, 0.16, 1.));
            }
            b'L' => {
                glow(c.x, c.y - 1., 17. + 3. * shine, rgba(1., 0.95, 0.85, 0.22));
                draw_ellipse(c.x, c.y + 13., 10., 2.5, 0., rgba(0., 0., 0., 0.4));
                record(c.x, c.y - 1.5 + bob, 1., 1.);
            }
            b'$' => {
                glow(c.x, c.y, 16. + 2. * shine, with_alpha(GOLD, 0.14));
                draw_ellipse(c.x, c.y + 13., 11., 2.5, 0., rgba(0., 0., 0., 0.4));
                painting(c.x, c.y + 0.5 + bob * 0.5, 24., 19., 1.);
            }
            b'T' => {
                let live = !self.disabled;
                if live {
                    glow(c.x, c.y, 18. + 3. * shine, with_alpha(MINT, 0.16));
                }
                console(c, 1., live, (t * 1.5).fract() < 0.5);
            }
            b'!' => {
                let flicker = 0.75 + 0.25 * (t * 30. + f32::from(p.x)).sin();
                beam(c, TILE - 6., !self.disabled, flicker);
            }
            b'H' => {
                let peek = (self.player == p && self.crouched && !self.seen)
                    .then(|| 1. - pulse(t * 0.25, 1.).powi(12));
                cabinet(c, 1., peek);
            }
            _ => {}
        }
    }
    /// Coin preview under the pointer and the marked landing: a dotted arc,
    /// a reticle and a ghost "?" over each guard the noise would draw.
    fn draw_aim(&self, t: f32) {
        let mouse = pointer();
        if let Some(h) = Game::pointer(mouse.x, mouse.y)
            && Some(h) != self.aim
            && !self.solid(h)
        {
            let c = center(h);
            outline(
                c.x - TILE / 2. + 1.,
                c.y - TILE / 2. + 1.,
                TILE - 3.,
                TILE - 3.,
                5.,
                1.5,
                rgba(1., 1., 1., 0.3),
            );
            if self.coins > 0 {
                self.draw_throw(h, 0.5, t);
            }
        }
        if let Some(aim) = self.aim {
            self.draw_throw(aim, 1., t);
        }
    }
    fn draw_throw(&self, target: Pos, a: f32, t: f32) {
        let c = center(target);
        if !(self.coins > 0 && self.throwable(target)) {
            cross(c, 6., fade(RED, 0.85 * a));
            reticle(c, 12., fade(RED, 0.6 * a));
            return;
        }
        let from = center(self.player) - vec2(0., 10.);
        let lift = from.distance(c) * 0.32 + 10.;
        let mid = (from + c) * 0.5 - vec2(0., lift);
        for i in 1..14 {
            let s = i as f32 / 14.;
            let p = from * (1. - s) * (1. - s) + mid * 2. * s * (1. - s) + c * s * s;
            shapes::fill_circle(p, 1.8, fade(GOLD, 0.85 * a));
        }
        for g in &self.guards[..self.guard_count] {
            if g.pos.distance(target) <= 6 && g.search == 0 && !self.watching(g) {
                bubble(head(g.pos), "?", AMBER, 0.35 + 0.35 * a);
            }
        }
        reticle(c, 11. + 1.5 * pulse(t, 1.5), fade(GOLD, a));
    }
    /// Pulse the object Space would act on.
    fn draw_prompt(&self, t: f32) {
        let Some((p, kind)) = self.interaction().filter(|_| self.work.is_none()) else {
            return;
        };
        let c = center(p);
        let color = kind_color(kind);
        let k = pulse(t, 1.6);
        glow(c.x, c.y, 22. + 4. * k, with_alpha(color, 0.16 + 0.12 * k));
        outline(
            c.x - TILE / 2. + 1.5,
            c.y - TILE / 2. + 1.5,
            TILE - 4.,
            TILE - 4.,
            6.,
            2.,
            with_alpha(color, 0.5 + 0.4 * k),
        );
    }
    /// The Space key cap floating over the object it would act on.
    fn draw_prompt_key(&self, t: f32) {
        let Some((p, _)) = self.interaction().filter(|_| self.work.is_none()) else {
            return;
        };
        let c = center(p);
        let w = text_width("Space", TextStyle::modern(13., PAPER)) + 12.;
        keycap(
            "Space",
            "",
            c.x - w * 0.5,
            c.y - TILE * 0.5 - 25. + bob(t, 1.2, 2.),
            PAPER,
        );
    }
    /// Standing footsteps carry two tiles; a thrown coin carries six.
    fn draw_sound(&self) {
        if self.phase == Phase::Infiltrating && !self.crouched && self.move_wait > 0 {
            let k = 1. - f32::from(self.move_wait) / 9.;
            diamond(
                center(self.player),
                TILE * (0.7 + 1.8 * k),
                2.,
                with_alpha(AMBER, 0.42 * (1. - k)),
            );
        }
        if let Some((p, left)) = self.noise {
            let c = center(p);
            let age = 180 - left;
            if age < 40 {
                let k = f32::from(age) / 40.;
                diamond(
                    c,
                    TILE * (0.5 + 6. * k),
                    2.5,
                    with_alpha(GOLD, 0.5 * (1. - k)),
                );
            }
            for ring in 0..2 {
                let phase = (f32::from(age) + ring as f32 * 18.) % 36. / 36.;
                shapes::stroke_circle(
                    c,
                    5. + phase * 16.,
                    1.5,
                    with_alpha(GOLD, 0.9 * (1. - phase)),
                );
            }
            coin(c.x, c.y, 5.5, 1.);
        }
    }
    fn draw_guard(&self, g: &Guard) {
        let c = center(g.pos);
        let (fx, fy) = (f32::from(g.facing.0), f32::from(g.facing.1));
        let searching = g.search > 0;
        draw_ellipse(c.x, c.y + 12., 12., 4., 0., rgba(0., 0., 0., 0.5));
        // Flashlight beam hint.
        let tip = vec2(c.x + fx * 16., c.y - 3. + fy * 16.);
        draw_line(
            c.x + fx * 6.,
            c.y - 3. + fy * 6.,
            tip.x,
            tip.y,
            4.,
            if searching {
                rgba(1., 0.4, 0.3, 0.45)
            } else {
                rgba(1., 0.88, 0.5, 0.4)
            },
        );
        // Coat, then head and peaked cap.
        let coat = Rect::new(c.x - 9., c.y - 2., 18., 15.);
        shapes::fill_rounded_rect(
            coat,
            5.,
            shapes::Fill::vertical(coat, rgba(0.16, 0.19, 0.25, 1.), rgba(0.07, 0.08, 0.11, 1.)),
        );
        draw_rectangle(c.x - 1., c.y, 2., 10., GOLD);
        guard_head(vec2(c.x, c.y - 8.), 1., (fx, fy), 1.);
    }
    fn draw_thief(&self, t: f32) {
        let c = center(self.player);
        let exposure = self.exposure();
        if self.tile(self.player) == b'H' && self.crouched && exposure != Exposure::Seen {
            // Tucked inside the cabinet: only the eyes show.
            return;
        }
        let hidden = exposure == Exposure::Hidden;
        let ring = match exposure {
            Exposure::Seen => RED,
            Exposure::Hidden => ORCHID,
            Exposure::Exposed => VELVET,
        };
        draw_ellipse(
            c.x,
            c.y + 12.,
            15.,
            5.,
            0.,
            with_alpha(
                ring,
                if exposure == Exposure::Seen {
                    0.45
                } else {
                    0.28
                },
            ),
        );
        draw_ellipse(c.x, c.y + 12., 11., 3.5, 0., rgba(0., 0., 0., 0.55));
        if hidden {
            glow(
                c.x,
                c.y,
                22.,
                with_alpha(ORCHID, 0.1 + 0.06 * pulse(t, 0.7)),
            );
        }
        let lowered = if self.crouched { 5. } else { 0. } + (t * 4.2).sin() * 0.8;
        thief(
            c,
            1.,
            lowered,
            f32::from(self.facing.0),
            if hidden { 0.55 } else { 1. },
        );
        if exposure == Exposure::Seen {
            let k = pulse(t, 2.5);
            shapes::stroke_circle(c, 18. + k * 3., 2., with_alpha(RED, 0.55 + k * 0.45));
        }
    }
    /// A progress ring around the object being worked.
    fn draw_work(&self) {
        let Some(w) = self.work else {
            return;
        };
        let c = center(w.target);
        let color = kind_color(w.kind);
        let done = 1. - f32::from(w.remaining) / f32::from(w.total);
        glow(c.x, c.y, 26., with_alpha(color, 0.22));
        ring_meter(c.x, c.y, 18., 4., done, color);
    }
    /// "!" over a guard who sees the thief, "?" over one who searches.
    fn draw_bubbles(&self, t: f32) {
        for (i, g) in self.guards[..self.guard_count].iter().enumerate() {
            let (glyph, color) = if self.watching(g) {
                ("!", RED)
            } else if g.search > 0 {
                ("?", AMBER)
            } else {
                continue;
            };
            let lift = bob(t + i as f32 * 0.4, 1.6, 1.5);
            bubble(head(g.pos) + vec2(0., lift), glyph, color, 1.);
        }
    }
    /// A bobbing arrow over the open exit once every record is packed.
    fn draw_beacon(&self, t: f32) {
        let Some(exit) = self.exit() else {
            return;
        };
        let c = center(exit);
        let y = c.y - TILE * 0.5 - 8. + bob(t, 1.4, 3.);
        glow(c.x, y - 2., 14., with_alpha(MINT, 0.3));
        shapes::fill_triangle(
            vec2(c.x - 8., y - 7.),
            vec2(c.x + 8., y - 7.),
            vec2(c.x, y + 3.),
            MINT,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Event;
    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    /// The foyer, entered, with its patrols sent home.
    fn entered() -> Game {
        let mut g = Game::new();
        assert_eq!(g.step(&primary()), Cue::Action);
        g.guard_count = 0;
        g
    }
    fn effects_of(g: &Game, cue: Cue) -> juice::JuiceCounts {
        let mut fx = Juice::new();
        ArcadeGame::effects(g, cue, &mut fx);
        fx.counts()
    }
    #[test]
    fn progress_pips_follow_the_six_heists() {
        let mut g = Game::new();
        assert_eq!(g.progress(), Some((1, 6)));
        for t in 0..120_000 {
            let input = g.pilot(t);
            g.step(&input);
            let (current, total) = g.progress().unwrap();
            assert!((1..=total).contains(&current) && total == 6);
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won);
        assert_eq!(g.progress(), Some((6, 6)));
    }
    #[test]
    fn hud_names_the_heist_and_keeps_the_goal_short() {
        let mut g = Game::new();
        assert_eq!(g.hud().stage, "The Velvet Foyer");
        for phase in [
            Phase::Briefing,
            Phase::Infiltrating,
            Phase::Debrief,
            Phase::Setback,
            Phase::Escaped,
            Phase::Caught,
        ] {
            g.phase = phase;
            let hud = g.hud();
            assert!(
                hud.objective.split_whitespace().count() <= 5,
                "{}",
                hud.objective
            );
            assert!(hud.objective.is_ascii() && hud.stage.is_ascii());
            assert!(!hud.status.is_empty());
        }
        g.phase = Phase::Infiltrating;
        g.stolen = g.needed;
        assert_eq!(g.objective(), "Escape through the exit");
        assert_eq!(g.summary().matches(". ").count(), 0);
    }
    #[test]
    fn taking_a_record_bursts_and_pops_its_value_on_the_display() {
        let mut g = entered();
        let display = g.player.shifted(1, 0).unwrap();
        g.tiles[display.index()] = b'L';
        assert_eq!(g.step(&primary()), Cue::Action);
        assert_eq!(g.events.as_slice(), &[Event::WorkStart(display, b'L')]);
        assert_eq!(effects_of(&g, Cue::Action).rings, 1);
        let mut cue = Cue::None;
        while g.work.is_some() {
            cue = g.step(&Input::default());
        }
        assert_eq!(cue, Cue::Good);
        assert!(
            g.events
                .as_slice()
                .contains(&Event::WorkDone(display, b'L'))
        );
        let counts = effects_of(&g, cue);
        assert!(counts.popups >= 1 && counts.sparks >= 14, "{counts:?}");
        // The next quiet tick requests nothing new.
        g.step(&Input::default());
        assert_eq!(effects_of(&g, Cue::None), juice::JuiceCounts::default());
    }
    #[test]
    fn coins_and_sightings_are_felt_where_they_happen() {
        let mut g = entered();
        g.guard_count = 1;
        g.guards[0] = crate::model::Guard {
            pos: g.player.shifted(4, -1).unwrap(),
            facing: (0, -1),
            wait: 1000,
            ..Default::default()
        };
        g.facing = (1, 0);
        assert_eq!(
            g.step(&Input {
                tertiary: true,
                ..Input::default()
            }),
            Cue::Action
        );
        let counts = effects_of(&g, Cue::Action);
        // A ring and burst at the landing, the coin, and a "?" over the guard.
        assert!(counts.rings >= 1 && counts.popups >= 2 && counts.sparks > 0);
        // Out of coins: a refusal is a small red ring and the coin popup.
        g.coins = 0;
        g.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        let refused = effects_of(&g, Cue::None);
        assert_eq!((refused.rings, refused.popups), (1, 1));
        // Turn the guard toward the thief: "!" and a red ring at the thief.
        g.guards[0].pos = g.player.shifted(3, 0).unwrap();
        g.guards[0].facing = (-1, 0);
        g.crouched = false;
        g.step(&Input::default());
        assert!(g.seen);
        let spotted = effects_of(&g, Cue::None);
        assert!(spotted.popups >= 1 && spotted.rings >= 1);
    }
}
