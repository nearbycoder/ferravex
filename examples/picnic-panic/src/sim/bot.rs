//! Autoplay: a scripted but honest raccoon that only uses ordinary [`Input`].
//!
//! The bot plans on its own half-metre grid (raccoons can go under tables and
//! up the hill, unlike picnickers), works through the to-do list in a sensible
//! order, waits for owners to wander off, and runs for the nearest bush when a
//! chase starts. It never touches game state directly.
use super::folk::Mode;
use super::park::{PropKind, Role};
use super::pranks::Prank;
use super::props::{Holder, find};
use super::{Game, Input, flat, heading, yaw_of};
use ferravex_play::navigation::{Cell, Connectivity, Grid, SearchScratch, SearchStatus};
use ferravex_render::prelude::{Vec2, Vec3, vec2, vec3};

const RES: f32 = 0.5;
const SIZE: u32 = 120;
const ORIGIN: f32 = -30.;
const PATH: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    Bell,
    Cushion,
    Pie,
    Music,
    Marrow,
    Sled,
    Melon,
    Finale,
    Escape,
    Done,
}

impl Task {
    fn prank(self) -> Option<Prank> {
        Some(match self {
            Task::Bell => Prank::Bell,
            Task::Cushion => Prank::Cushion,
            Task::Pie => Prank::Pie,
            Task::Music => Prank::Music,
            Task::Marrow => Prank::Marrow,
            Task::Sled => Prank::Sled,
            Task::Melon => Prank::Melon,
            Task::Finale => Prank::Finale,
            Task::Escape => Prank::Escape,
            Task::Done => return None,
        })
    }
}

pub struct Bot {
    pub debug: bool,
    grid: Grid,
    search: SearchScratch,
    path: [Cell; PATH],
    len: usize,
    index: usize,
    goal: Option<Cell>,
    replan: f32,
    pub task: Task,
    stage: u8,
    timer: f32,
    task_time: f32,
    stuck: f32,
    last: Vec3,
    pressed: bool,
    hold: f32,
    jumps: u8,
    attempts: u8,
    hiding: f32,
    skipped: u8,
}

fn blocked(game: &Game, p: Vec3) -> bool {
    let park = &game.park;
    if p.x.abs() > 29.2 || p.z.abs() > 29.2 || park.in_pond(p) {
        return true;
    }
    // The hill is climbable only from its bottom edge.
    let side = (p.x > -6.9 && p.x < -5.2) || (p.x > 7.2 && p.x < 8.9);
    if side && p.z < -11.7 && p.z > -29. {
        return true;
    }
    for s in &park.solids {
        let bottom = s.center.y - s.half.y;
        let top = s.center.y + s.half.y;
        if top < 0.27 || bottom > 0.62 || s.half.y > 1.4 && s.center.y > 1. {
            continue;
        }
        let m = ferravex_render::prelude::Mat4::from_quat(s.rotation);
        let ext = vec2(
            m.x_axis.x.abs() * s.half.x + m.y_axis.x.abs() * s.half.y + m.z_axis.x.abs() * s.half.z,
            m.x_axis.z.abs() * s.half.x + m.y_axis.z.abs() * s.half.y + m.z_axis.z.abs() * s.half.z,
        );
        if (p.x - s.center.x).abs() < ext.x + 0.3 && (p.z - s.center.z).abs() < ext.y + 0.3 {
            return true;
        }
    }
    if park
        .trees
        .iter()
        .any(|t| flat(p - t.at).length() < 0.25 * t.scale + 0.35)
    {
        return true;
    }
    if park.poles.iter().any(|(q, _)| flat(p - *q).length() < 0.4) {
        return true;
    }
    let arch = park.deco(super::park::DecoKind::BellArch).at;
    if [-1.4_f32, 1.4]
        .iter()
        .any(|x| flat(p - (arch + vec3(*x, 0., 0.))).length() < 0.45)
    {
        return true;
    }
    let sign = park.deco(super::park::DecoKind::Sign).at;
    if flat(p - sign).length() < 0.4 {
        return true;
    }
    let seesaw = park.deco(super::park::DecoKind::Seesaw).at;
    if (p.x - seesaw.x).abs() < 2.7 && (p.z - seesaw.z).abs() < 0.75 {
        return true;
    }
    // Table legs are thin; the tabletops are a roof, not a wall.
    for t in &park.tables {
        for (sx, sz) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            let leg = t.center + vec3(sx * (t.top.x * 0.5 - 0.14), 0., sz * (t.top.z * 0.5 - 0.1));
            if flat(p - leg).length() < 0.3 {
                return true;
            }
        }
    }
    game.props
        .iter()
        .any(|q| q.kind == PropKind::Crate && flat(p - q.home).length() < 0.7)
}

fn center(c: Cell) -> Vec3 {
    vec3(
        ORIGIN + (c.x as f32 + 0.5) * RES,
        0.,
        ORIGIN + (c.y as f32 + 0.5) * RES,
    )
}

impl Bot {
    pub fn new(game: &Game) -> Self {
        let mut grid = Grid::new(SIZE, SIZE).expect("bot grid");
        for y in 0..SIZE {
            for x in 0..SIZE {
                let c = Cell { x, y };
                grid.set_blocked(c, blocked(game, center(c))).expect("cell");
            }
        }
        Self {
            debug: false,
            grid,
            search: SearchScratch::new((SIZE * SIZE) as usize).expect("bot search"),
            path: [Cell { x: 0, y: 0 }; PATH],
            len: 0,
            index: 0,
            goal: None,
            replan: 0.,
            task: Task::Bell,
            stage: 0,
            timer: 0.,
            task_time: 0.,
            stuck: 0.,
            last: game.coon.pos(),
            pressed: false,
            hold: 0.,
            jumps: 0,
            attempts: 0,
            hiding: 0.,
            skipped: 0,
        }
    }

    fn cell(&self, p: Vec3) -> Option<Cell> {
        let x = ((p.x - ORIGIN) / RES).floor() as i32;
        let y = ((p.z - ORIGIN) / RES).floor() as i32;
        for r in 0_i32..=4 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (cx, cy) = (x + dx, y + dy);
                    if cx >= 0 && cy >= 0 && cx < SIZE as i32 && cy < SIZE as i32 {
                        let c = Cell {
                            x: cx as u32,
                            y: cy as u32,
                        };
                        if self.grid.walkable(c) {
                            return Some(c);
                        }
                    }
                }
            }
        }
        None
    }

    /// Movement intent (world XZ) toward `goal`, following a grid path.
    fn steer(&mut self, game: &Game, goal: Vec3) -> Vec3 {
        let pos = game.coon.pos();
        if flat(goal - pos).length() < 0.6 {
            return flat(goal - pos);
        }
        let goal_cell = self.cell(goal);
        self.replan -= super::DT;
        if goal_cell != self.goal || self.replan <= 0. {
            self.goal = goal_cell;
            self.replan = 1.5;
            self.len = 0;
            self.index = 0;
            if let (Some(start), Some(end)) = (self.cell(pos), goal_cell)
                && self
                    .search
                    .begin(&self.grid, start, end, Connectivity::Eight)
                    .is_ok()
                && self.search.step(&self.grid, 60_000).status == SearchStatus::Found
                && let Ok(n) = self.search.copy_path(&mut self.path)
            {
                self.len = n;
            }
        }
        while self.index < self.len {
            let p = center(self.path[self.index]);
            if flat(p - pos).length() < RES * 0.9 && self.index + 1 < self.len {
                self.index += 1;
            } else {
                break;
            }
        }
        let target = if self.index < self.len && self.index + 2 < self.len {
            center(self.path[self.index])
        } else {
            goal
        };
        flat(target - pos).normalize_or_zero()
    }

    fn input_toward(&mut self, game: &Game, goal: Vec3, slow: bool) -> Input {
        let dir = self.steer(game, goal);
        let m = vec2(dir.x, -dir.z).normalize_or_zero();
        let mut input = Input {
            movement: if slow { m * 0.5 } else { m },
            ..Input::default()
        };
        // Hop when stuck against something small.
        if game.coon.speed < 0.4 && m.length() > 0.2 {
            self.stuck += super::DT;
            if self.stuck > 0.5 {
                input.jump = true;
                self.stuck = -0.4;
                self.replan = 0.;
            }
        } else {
            self.stuck = self.stuck.min(0.);
        }
        input
    }

    fn face(&self, game: &Game, at: Vec3) -> Input {
        let d = flat(at - game.coon.pos()).normalize_or_zero() * 0.12;
        Input {
            movement: vec2(d.x, -d.z),
            ..Input::default()
        }
    }

    fn arrive(game: &Game, at: Vec3, tolerance: f32) -> bool {
        flat(game.coon.pos() - at).length() < tolerance
    }

    fn press(&mut self) -> bool {
        // A clean edge: press on one tick, release on the next.
        self.pressed = !self.pressed;
        self.pressed
    }

    fn advance(&mut self, game: &Game) {
        const ORDER: [Task; 7] = [
            Task::Bell,
            Task::Cushion,
            Task::Pie,
            Task::Music,
            Task::Marrow,
            Task::Sled,
            Task::Melon,
        ];
        self.stage = 0;
        self.timer = 0.;
        self.task_time = 0.;
        self.attempts = 0;
        self.jumps = 0;
        self.hold = 0.;
        let cushion_set = {
            let c = game.props[find(game, PropKind::Cushion)];
            let seat = game.park.seat(0).map_or(Vec3::ZERO, |s| s.at);
            c.holder == Holder::None && flat(c.pos - seat).length() < 0.45
        };
        let pending = |t: Task| {
            !t.prank().is_some_and(|p| game.todo.done(p)) && (t != Task::Cushion || !cushion_set)
        };
        let from = ORDER
            .iter()
            .position(|t| *t == self.task)
            .map_or(0, |i| i + 1);
        if let Some(t) = (0..ORDER.len())
            .map(|k| ORDER[(from + k) % ORDER.len()])
            .find(|t| pending(*t))
        {
            self.task = t;
        } else if !game.todo.done(Prank::Finale) {
            self.task = Task::Finale;
        } else if !game.todo.done(Prank::Escape) {
            self.task = Task::Escape;
        } else {
            self.task = Task::Done;
        }
    }

    fn danger(&self, game: &Game) -> Option<Vec3> {
        let pos = game.coon.pos();
        // Hugs and licks are only a short delay: run from grown-ups only.
        game.folk
            .iter()
            .filter(|f| f.kind.adult())
            .filter(|f| {
                (f.mode == Mode::Chase && flat(f.pos - pos).length() < 9.)
                    || (f.state() == super::folk::ALARMED && flat(f.pos - pos).length() < 5.)
            })
            .map(|f| f.pos)
            .next()
    }

    fn watcher(&self, game: &Game) -> Option<Vec3> {
        let pos = game.coon.pos();
        game.folk
            .iter()
            .filter(|f| {
                f.kind.adult()
                    && f.state() == super::folk::CURIOUS
                    && f.sees
                    && flat(f.pos - pos).length() < 8.
            })
            .map(|f| f.pos)
            .next()
    }

    fn hideout(&self, game: &Game, threat: Vec3) -> Vec3 {
        let pos = game.coon.pos();
        let mut best = game.park.dens[0];
        let mut score = f32::INFINITY;
        for b in &game.park.bushes {
            let mine = flat(b.at - pos).length();
            let theirs = flat(b.at - threat).length();
            let s = mine - theirs * 0.8;
            if theirs > mine * 0.9 && s < score {
                score = s;
                best = b.at;
            }
        }
        best
    }

    /// One tick of ordinary input.
    pub fn input(&mut self, game: &Game) -> Input {
        let mut input = self.decide(game);
        input.jump_held = input.jump;
        input.grab_held |= input.grab;
        input
    }

    fn decide(&mut self, game: &Game) -> Input {
        self.task_time += super::DT;
        self.timer += super::DT;
        let pos = game.coon.pos();
        let moved = flat(pos - self.last).length();
        self.last = pos;
        let _ = moved;
        if game.is_over() || self.task == Task::Done {
            return Input::default();
        }
        if game.coon.scruffed.is_some() {
            // Wriggle! (one press every few ticks)
            return Input {
                jump: self.press() && game.tick.is_multiple_of(4),
                ..Input::default()
            };
        }
        if let Some(p) = self.task.prank()
            && game.todo.done(p)
        {
            self.advance(game);
        }
        if self.task_time > 140. && self.skipped < 12 && self.task != Task::Escape {
            // Give up on a stubborn prank for now; come back to it later.
            self.skipped += 1;
            self.advance(game);
        }
        // Run and hide when someone is after us (except during the final dash).
        if self.task != Task::Escape
            && let Some(threat) = self.danger(game)
        {
            self.hiding = 2.5;
            let held_heavy = game
                .coon
                .held
                .is_some_and(|h| game.props[h].heft() != super::props::Heft::Light);
            if held_heavy {
                return Input {
                    grab: self.press(),
                    ..Input::default()
                };
            }
            let spot = self.hideout(game, threat);
            return self.input_toward(game, spot, false);
        }
        if self.task != Task::Escape
            && let Some(eyes) = self.watcher(game)
        {
            // Someone said "?": slip away out of their cone.
            let away = flat(game.coon.pos() - eyes).normalize_or_zero();
            return self.input_toward(game, game.coon.pos() + away * 4., false);
        }
        if self.hiding > 0. {
            self.hiding -= super::DT;
            if game.coon.hidden() {
                return Input::default();
            }
        }
        match self.task {
            Task::Bell => self.bell(game),
            Task::Cushion => self.cushion(game),
            Task::Pie => self.pie(game),
            Task::Music => self.music(game),
            Task::Marrow => self.marrow(game),
            Task::Sled => self.sled(game),
            Task::Melon => self.melon(game),
            Task::Finale => self.finale(game),
            Task::Escape => self.input_toward(game, game.park.dens[0], false),
            Task::Done => Input::default(),
        }
    }

    fn fetch(&mut self, game: &Game, kind: PropKind, approach: Vec3) -> Option<Input> {
        let i = find(game, kind);
        if game.coon.held == Some(i) {
            return None;
        }
        if let Some(h) = game.coon.held {
            let _ = h;
            return Some(Input {
                grab: self.press(),
                ..Input::default()
            });
        }
        let p = game.props[i];
        if p.holder != Holder::None {
            return Some(Input::default());
        }
        // Approach from the authored side when the prop is home; else directly.
        let near = if p.displaced() {
            p.pos - flat(p.pos - game.coon.pos()).normalize_or_zero() * 0.7
        } else {
            approach
        };
        let dist = flat(p.pos - game.coon.pos()).length();
        let reach = 0.62 + p.radius();
        if !Self::arrive(game, near, 0.3) && dist > reach {
            return Some(self.input_toward(game, near, false));
        }
        if dist > reach {
            // At the authored spot but still out of reach: shuffle closer.
            return Some(self.input_toward(game, p.pos, true));
        }
        let facing = heading(game.coon.yaw).dot(flat(p.pos - game.coon.pos()).normalize_or_zero());
        if facing < 0.8 {
            return Some(self.face(game, p.pos));
        }
        Some(Input {
            grab: self.press(),
            ..Input::default()
        })
    }

    fn deliver(&mut self, game: &Game, at: Vec3, face: Vec3) -> Input {
        if !Self::arrive(game, at, 0.3) {
            return self.input_toward(game, at, false);
        }
        let dir = flat(face - game.coon.pos()).normalize_or_zero();
        if heading(game.coon.yaw).dot(dir) < 0.9 {
            return self.face(game, face);
        }
        Input {
            grab: self.press(),
            ..Input::default()
        }
    }

    fn bell(&mut self, game: &Game) -> Input {
        let rope = game.park.zone(Role::Rope).at;
        if !Self::arrive(game, rope, 0.5) {
            return self.input_toward(game, rope, false);
        }
        self.hold += super::DT;
        Input {
            grab: self.hold < 0.05,
            grab_held: true,
            ..Input::default()
        }
    }

    fn cushion(&mut self, game: &Game) -> Input {
        let cushion = find(game, PropKind::Cushion);
        let seat = game.park.seat(0).expect("authored seat").at;
        let c = game.props[cushion];
        if c.holder == Holder::None && flat(c.pos - seat).length() < 0.45 {
            // Placed; Nana will find it in her own time.
            self.advance(game);
            return Input::default();
        }
        if let Some(input) = self.fetch(game, PropKind::Cushion, vec3(6., 0., 20.8)) {
            return input;
        }
        let nana = &game.folk[0];
        if flat(nana.pos - seat).length() < 9. {
            // Lurk behind the blue table until Nana wanders off.
            return self.input_toward(game, vec3(-1., 0., 10.5), true);
        }
        self.deliver(game, seat + vec3(0.82, 0., 0.), seat)
    }

    fn pie(&mut self, game: &Game) -> Input {
        let pie = find(game, PropKind::Pie);
        let nana = &game.folk[0];
        if game.coon.held != Some(pie) && flat(nana.pos - game.props[pie].home).length() < 9. {
            // Wait for Nana to wander off (behind the blue table).
            return self.input_toward(game, vec3(-1., 0., 10.5), true);
        }
        if let Some(input) = self.fetch(game, PropKind::Pie, vec3(-7.3, 0., 7.)) {
            return input;
        }
        let den = game.park.dens[0];
        self.deliver(game, den + vec3(1.0, 0., -1.0), den)
    }

    fn music(&mut self, game: &Game) -> Input {
        let sheet = find(game, PropKind::Sheet);
        let trumble = &game.folk[2];
        let stand = game.park.zone(Role::Stand).at;
        if game.coon.held != Some(sheet)
            && (trumble.spot == 0 && flat(trumble.pos - stand).length() < 4.)
        {
            return self.input_toward(game, vec3(-14.6, 0., -0.4), true);
        }
        if let Some(input) = self.fetch(game, PropKind::Sheet, vec3(-18., 0.22, -0.45)) {
            return input;
        }
        self.input_toward(game, vec3(-10.2, 0., 3.6), false)
    }

    fn marrow(&mut self, game: &Game) -> Input {
        let marrow = find(game, PropKind::Marrow);
        let plinth = game.park.zone(Role::Plinth).at;
        let pumble = &game.folk[1];
        let on_plinth =
            flat(game.props[marrow].pos - plinth).length() < 0.6 && game.props[marrow].pos.y > 0.7;
        let busy = pumble.spot == 0 && flat(pumble.pos - plinth).length() < 5.;
        if on_plinth || game.coon.held == Some(marrow) {
            if game.coon.held != Some(marrow) && busy {
                return self.input_toward(game, vec3(10.6, 0., -6.8), true);
            }
            if let Some(input) = self.fetch(game, PropKind::Marrow, vec3(14., 0., -1.95)) {
                return input;
            }
            return self.deliver(game, vec3(11., 0., -0.8), vec3(10., 0., -0.8));
        }
        if let Some(input) = self.fetch(game, PropKind::Pinecone, vec3(20.3, 0., -6.9)) {
            return input;
        }
        if busy {
            return self.input_toward(game, vec3(10.6, 0., -6.8), true);
        }
        self.deliver(game, vec3(14., 0., -1.95), plinth)
    }

    fn sled(&mut self, game: &Game) -> Input {
        let tray = find(game, PropKind::Tray);
        let t = game.props[tray];
        let crest = vec3(1., 3., -22.7);
        if !t.loaded {
            let at_crest = flat(t.pos - crest).length() < 1.4 && t.pos.y > 2.8;
            if !at_crest {
                if self.stage == 0 && !Self::arrive(game, vec3(1., 0., -10.6), 1.) {
                    return self.input_toward(game, vec3(1., 0., -10.6), false);
                }
                self.stage = 1;
                if let Some(input) = self.fetch(game, PropKind::Tray, vec3(-2., 3., -23.15)) {
                    return input;
                }
                return self.deliver(game, vec3(1., 3., -23.5), vec3(1., 3., -21.5));
            }
            if let Some(input) = self.fetch(game, PropKind::Blanket, vec3(3.2, 3., -24.4)) {
                return input;
            }
            let behind = t.pos - vec3(0., 0., 0.95);
            return self.deliver(game, behind, t.pos);
        }
        // Loaded: give it a determined shove downhill.
        if game.coon.held.is_some() {
            return Input {
                grab: self.press(),
                ..Input::default()
            };
        }
        if t.pos.z > -21.6 && t.vel.length() > 0.5 {
            return self.input_toward(game, vec3(1., 0., -9.), false);
        }
        let behind = t.pos - vec3(0., 0., 0.75);
        if !Self::arrive(game, behind, 0.3) {
            return self.input_toward(game, behind, true);
        }
        Input {
            movement: vec2(0., -1.),
            slide: self.timer % 1. < 0.05,
            ..Input::default()
        }
    }

    fn melon(&mut self, game: &Game) -> Input {
        let melon = find(game, PropKind::Melon);
        let m = game.props[melon];
        let pivot = game.mech.seesaw_pivot;
        let low_end = pivot + vec3(-1.5, 0., 0.);
        let on_seesaw = flat(m.pos - low_end).length() < 0.8 && m.holder == Holder::None;
        if !on_seesaw {
            if let Some(input) = self.fetch(game, PropKind::Melon, vec3(20.2, 0., 11.05)) {
                return input;
            }
            // Approach from the side so the plank never lands on our head.
            let spot = low_end + vec3(-0.3, 0., -0.95);
            if !Self::arrive(game, spot, 0.15) {
                return self.input_toward(game, spot, true);
            }
            return self.deliver(game, spot, spot + Vec3::Z);
        }
        // Climb the hay bales and drop in from a great height.
        let start = vec3(18.2, 0., 19.);
        let y = game.coon.pos().y;
        let x = game.coon.pos().x;
        let grounded = game.coon.motor.grounded;
        if self.debug {
            println!(
                "entry stage {} x {x:.2} y {y:.2} timer {:.1} yaw {:.2}",
                self.stage, self.timer, game.coon.yaw
            );
        }
        if self.stage == 0 {
            if !Self::arrive(game, start, 0.2) {
                return self.input_toward(game, start, true);
            }
            if heading(game.coon.yaw).dot(Vec3::NEG_X) < 0.97 {
                return self.face(game, start - Vec3::X);
            }
            self.stage = 1;
            self.timer = 0.;
            return Input::default();
        }
        if self.timer > 8. || (grounded && y < 0.3 && x < 16.5) {
            // Missed (or landed on the grass): line up again.
            self.stage = 0;
            self.attempts += 1;
            return Input::default();
        }
        let pulse = self.press();
        let (speed, jump) = if y < 0.3 {
            (0.7, grounded && x < 18.1)
        } else if y < 0.9 {
            (0.7, grounded && x < 17.05)
        } else if y > 1.1 && x > 15.4 {
            // Top bale: a gentle hop west onto the raised end of the seesaw.
            (0.45, grounded && x < 16.1)
        } else {
            (0., false)
        };
        if self.debug {
            println!(
                "melon stage {} x {x:.2} y {y:.2} grounded {grounded} speed {speed} jump {jump} pulse {pulse} vel {:?}",
                self.stage, game.coon.motor.velocity
            );
        }
        Input {
            movement: vec2(-speed, 0.),
            jump: jump && pulse,
            slide: !grounded && y > 1.3 && x < 15.1 && game.coon.motor.velocity.y < 0.,
            ..Input::default()
        }
    }

    fn finale(&mut self, game: &Game) -> Input {
        let cloth = game.park.zone(Role::Cloth).at;
        if !Self::arrive(game, cloth, 0.55) {
            return self.input_toward(game, cloth, false);
        }
        Input {
            grab: game.tick.is_multiple_of(90),
            grab_held: true,
            ..Input::default()
        }
    }
}

/// Stateless helper for attract mode: steer toward a goal with no planning.
pub fn wander(game: &Game, goal: Vec3) -> Input {
    let d = flat(goal - game.coon.pos());
    let m = if d.length() > 0.5 {
        d.normalize()
    } else {
        Vec3::ZERO
    };
    Input {
        movement: Vec2::new(m.x, -m.z) * 0.8,
        cam_yaw: yaw_of(Vec3::NEG_Z),
        ..Input::default()
    }
}
