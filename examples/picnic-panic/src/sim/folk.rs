//! Picnickers: routines, readable perception and very polite pursuit.
//!
//! Each picnicker runs the engine's AI pieces on fixed ticks: a [`Sight`] cone
//! plus [`Hearing`] feed an expiring [`Memory`]; a shared [`DecisionGraph`]
//! turns sensor flags into awareness (calm → "?" → "!" → searching → calm);
//! a budgeted [`PathPlanner`] over a 60 × 60 grid routes them around tables,
//! hedges, the pond and the hill. The host layer adds the picnic-specific
//! behaviour: routines, sitting, fetching displaced belongings, chasing,
//! carrying the raccoon back to its burrow, hugs (Pip) and licks (Biscuit).
use super::park::{Act, FOLK, FOLK_NAMES, Park, Role};
use super::props::{Holder, place, set_carried};
use super::raccoon::{Hide, set_down};
use super::{Event, Game, L_OCCLUDE, flat, heading, yaw_of};
use ferravex_physics::d3::{PhysicsWorld, QueryOptions, RigidBodyPose};
use ferravex_play::ai::{
    Candidate, DecisionEdge, DecisionGraph, DecisionState, Detection, Hearing, Memory,
    MemorySettings, Noise, NoiseBuffer, Observer, Sense, Sight, VisibilityQuery, WorkBudget,
};
use ferravex_play::navigation::{
    Cell, Connectivity, Grid, PathPlanner, PathRequest, Route3, WaypointFollower,
};
use ferravex_render::prelude::{Vec3, vec3};

pub const GRID: u32 = 60;
pub const CELL: f32 = 1.0;
const ORIGIN: f32 = -30.;
const ROUTE: usize = 160;
pub const RACCOON_ID: u64 = 1;

/// Awareness states in the shared decision graph.
pub const CALM: u16 = 0;
pub const CURIOUS: u16 = 1;
pub const ALARMED: u16 = 2;
pub const SEARCHING: u16 = 3;

const SEE: u64 = 1;
const HEARD: u64 = 2;
const MEMORY: u64 = 4;
const CLOSE: u64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Nana,
    Pumble,
    Trumble,
    Pip,
    Biscuit,
}

impl Kind {
    pub fn adult(self) -> bool {
        matches!(self, Self::Nana | Self::Pumble | Self::Trumble)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Nana => "Nana Hollis",
            Self::Pumble => "Mr. Pumble",
            Self::Trumble => "Maestro Trumble",
            Self::Pip => "Pip",
            Self::Biscuit => "Biscuit",
        }
    }
    fn walk(self) -> f32 {
        match self {
            Self::Nana => 1.35,
            Self::Pumble => 1.5,
            Self::Trumble => 1.45,
            Self::Pip => 1.9,
            Self::Biscuit => 2.4,
        }
    }
    fn run(self) -> f32 {
        match self {
            Self::Nana => 3.7,
            Self::Pumble => 4.1,
            Self::Trumble => 4.25,
            Self::Pip => 3.9,
            Self::Biscuit => 5.0,
        }
    }
    /// Seconds of sprinting before they need a sit-down and a biscuit.
    fn stamina(self) -> f32 {
        match self {
            Self::Nana => 7.,
            Self::Pumble => 8.5,
            Self::Trumble => 8.,
            Self::Pip => 6.,
            Self::Biscuit => 4.5,
        }
    }
    pub fn sight_range(self) -> f32 {
        match self {
            Self::Pip => 8.5,
            Self::Biscuit => 7.,
            _ => 10.5,
        }
    }
    pub fn fov(self) -> f32 {
        match self {
            Self::Pip => 125_f32.to_radians(),
            Self::Biscuit => 150_f32.to_radians(),
            _ => 100_f32.to_radians(),
        }
    }
    pub fn eye(self) -> f32 {
        match self {
            Self::Pip => 1.0,
            Self::Biscuit => 0.55,
            _ => 1.55,
        }
    }
    fn hearing(self) -> f32 {
        if self == Self::Biscuit { 14. } else { 11. }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Routine,
    Sit,
    Fetch(usize),
    Return(usize),
    Chase,
    Search,
    Carry,
    React(f32),
    Puffed(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bubble {
    None,
    Question,
    Alarm,
    Heart,
    Huff,
    Note,
    Star,
    Sweat,
    Shock,
}

/// What their body is doing, for animation and HUD flavour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Walk,
    Run,
    Sit,
    Act(Act),
    Look,
    Carry,
    Startled,
    Cheer,
    Puff,
}

pub struct Folk {
    pub kind: Kind,
    pub pos: Vec3,
    pub prev: Vec3,
    pub yaw: f32,
    pub prev_yaw: f32,
    pub speed: f32,
    pub mode: Mode,
    pub pose: Pose,
    pub spot: usize,
    pub wait: f32,
    pub arrived: bool,
    pub awareness: DecisionState,
    pub memory: Memory,
    pub route: Route3,
    pub follower: WaypointFollower,
    pub goal: Vec3,
    pub planned: Vec3,
    pub replan: f32,
    pub has_route: bool,
    pub bubble: Bubble,
    pub bubble_age: f32,
    /// 0..1 how close they are to a full "!" (drives the vision cone tint).
    pub alert: f32,
    pub sees: bool,
    pub chase_time: f32,
    /// Seconds spent poking at the bush/table the raccoon is hiding in.
    pub poke: f32,
    /// They found the raccoon in its hiding place ("Aha!").
    pub found: bool,
    /// Pip and Biscuit are content for a while after a hug or a lick.
    pub content: f32,
    pub seat: Option<usize>,
    pub look_yaw: f32,
    pub fetch_check: f32,
    pub carry_time: f32,
    pub award_pending: bool,
    pub chase_limit: f32,
}

impl Folk {
    pub fn new(index: usize, park: &Park) -> Self {
        let kind = [
            Kind::Nana,
            Kind::Pumble,
            Kind::Trumble,
            Kind::Pip,
            Kind::Biscuit,
        ][index];
        let start = park.routines[index][0].at;
        let yaw = (index as f32 * 1.7).sin() * 2.;
        Self {
            kind,
            pos: start,
            prev: start,
            yaw,
            prev_yaw: yaw,
            speed: 0.,
            mode: Mode::Routine,
            pose: Pose::Idle,
            spot: 0,
            wait: park.routines[index][0].wait * (0.4 + 0.15 * index as f32),
            arrived: true,
            awareness: graph().initial(),
            memory: Memory::new(MemorySettings {
                capacity: 4,
                sight_seconds: 3.,
                hearing_seconds: 4.,
            })
            .expect("folk memory"),
            route: Route3::new(ROUTE).expect("route storage"),
            follower: WaypointFollower::default(),
            goal: start,
            planned: Vec3::splat(f32::MAX),
            replan: 0.,
            has_route: false,
            bubble: Bubble::None,
            bubble_age: 0.,
            alert: 0.,
            sees: false,
            chase_time: 0.,
            poke: 0.,
            found: false,
            content: 0.,
            seat: None,
            look_yaw: yaw,
            fetch_check: index as f32 * 0.1,
            carry_time: 0.,
            award_pending: false,
            chase_limit: kind.stamina(),
        }
    }

    pub fn state(&self) -> u16 {
        self.awareness.state()
    }

    pub fn chasing(&self) -> bool {
        self.mode == Mode::Chase
    }

    pub fn hands(&self) -> Vec3 {
        let up = match self.kind {
            Kind::Pip => 0.75,
            Kind::Biscuit => 0.45,
            _ => 1.05,
        };
        self.pos + heading(self.yaw) * 0.38 + Vec3::Y * up
    }

    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * self.kind.eye()
    }

    /// Effective sight range this tick (sneaking raccoons are harder to spot).
    pub fn range(&self, sneaking: bool) -> f32 {
        let r = self.kind.sight_range();
        if sneaking { r * 0.6 } else { r }
    }

    fn say(&mut self, bubble: Bubble) {
        if self.bubble != bubble {
            self.bubble = bubble;
            self.bubble_age = 0.;
        }
    }
}

fn graph() -> DecisionGraph {
    let e = |from: u16, to: u16, all: u64, none: u64, after: f32| DecisionEdge {
        from: Some(from),
        to,
        all,
        none,
        after,
    };
    DecisionGraph::new(
        4,
        CALM,
        &[
            e(CALM, CURIOUS, SEE, 0, 0.),
            e(CALM, CURIOUS, HEARD, 0, 0.),
            e(CURIOUS, ALARMED, SEE | CLOSE, 0, 0.5),
            e(CURIOUS, ALARMED, SEE, 0, 1.1),
            e(CURIOUS, SEARCHING, 0, SEE, 1.1),
            e(ALARMED, SEARCHING, 0, SEE, 0.7),
            e(SEARCHING, ALARMED, SEE, 0, 0.2),
            e(SEARCHING, CALM, 0, SEE | MEMORY, 2.5),
            e(SEARCHING, CALM, 0, SEE, 8.),
        ],
    )
    .expect("authored awareness graph")
}

/// Line of sight through occluding colliders (hedges, trees, the hill).
pub struct SightLine<'a>(pub &'a PhysicsWorld);
impl VisibilityQuery for SightLine<'_> {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool {
        let d = to - from;
        let length = d.length();
        if length < 1e-3 {
            return true;
        }
        let options = QueryOptions::new((length - 0.1).max(0.)).layers(L_OCCLUDE);
        self.0
            .raycast_with_options(from.to_array(), (d / length).to_array(), options)
            .is_none()
    }
}

pub struct FolkAi {
    pub grid: Grid,
    pub planner: PathPlanner,
    pub noises: NoiseBuffer,
    pub graph: DecisionGraph,
    pub noise_serial: u64,
    scratch: [Vec3; ROUTE],
}

fn cell_center(cell: Cell) -> Vec3 {
    vec3(
        ORIGIN + (cell.x as f32 + 0.5) * CELL,
        0.,
        ORIGIN + (cell.y as f32 + 0.5) * CELL,
    )
}

fn blocked_at(park: &Park, p: Vec3) -> bool {
    let inside = |center: Vec3, half: Vec3, pad: f32| {
        (p.x - center.x).abs() < half.x + pad && (p.z - center.z).abs() < half.z + pad
    };
    if p.x.abs() > 28.8 || p.z.abs() > 28.8 || park.on_hill(p) {
        return true;
    }
    let pond = park.zone(Role::Pond);
    if flat(p - pond.at).length() < pond.radius + 0.5 {
        return true;
    }
    if park.dens.iter().any(|d| flat(p - *d).length() < 2.6) {
        return true;
    }
    for s in &park.solids {
        if !s.nav {
            continue;
        }
        // Conservative world AABB of a possibly rotated box.
        let m = ferravex_render::prelude::Mat4::from_quat(s.rotation);
        let ext = vec3(
            m.x_axis.x.abs() * s.half.x + m.y_axis.x.abs() * s.half.y + m.z_axis.x.abs() * s.half.z,
            0.,
            m.x_axis.z.abs() * s.half.x + m.y_axis.z.abs() * s.half.y + m.z_axis.z.abs() * s.half.z,
        );
        if s.half.y * 2. > 0.3 && inside(s.center, ext, 0.42) {
            return true;
        }
    }
    if park
        .tables
        .iter()
        .any(|t| inside(t.center, t.top * 0.5, 0.35))
    {
        return true;
    }
    if park
        .trees
        .iter()
        .any(|t| flat(p - t.at).length() < 0.55 * t.scale + 0.3)
    {
        return true;
    }
    if park
        .bushes
        .iter()
        .any(|b| flat(p - b.at).length() < b.radius * 0.85)
    {
        return true;
    }
    let seesaw = park.deco(super::park::DecoKind::Seesaw).at;
    if inside(seesaw, vec3(2.4, 0., 0.45), 0.25) {
        return true;
    }
    let arch = park.deco(super::park::DecoKind::BellArch).at;
    if [-1.4, 1.4]
        .iter()
        .any(|x| flat(p - arch.at_x(*x)).length() < 0.5)
    {
        return true;
    }
    park.poles
        .iter()
        .any(|(pole, _)| flat(p - *pole).length() < 0.45)
}

trait ArchAt {
    fn at_x(self, x: f32) -> Vec3;
}
impl ArchAt for Vec3 {
    fn at_x(self, x: f32) -> Vec3 {
        self + vec3(x, 0., 0.)
    }
}

impl FolkAi {
    pub fn new(park: &Park) -> Self {
        let mut grid = Grid::new(GRID, GRID).expect("park grid");
        for y in 0..GRID {
            for x in 0..GRID {
                let cell = Cell { x, y };
                grid.set_blocked(cell, blocked_at(park, cell_center(cell)))
                    .expect("in grid");
            }
        }
        Self {
            grid,
            planner: PathPlanner::new((GRID * GRID) as usize, 8).expect("planner"),
            noises: NoiseBuffer::new(8).expect("noise buffer"),
            graph: graph(),
            noise_serial: 100,
            scratch: [Vec3::ZERO; ROUTE],
        }
    }

    pub fn cell_of(&self, p: Vec3) -> Option<Cell> {
        let x = ((p.x - ORIGIN) / CELL).floor() as i32;
        let y = ((p.z - ORIGIN) / CELL).floor() as i32;
        for radius in 0_i32..=3 {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let (cx, cy) = (x + dx, y + dy);
                    if cx >= 0 && cy >= 0 && cx < GRID as i32 && cy < GRID as i32 {
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

    pub fn walkable(&self, p: Vec3) -> bool {
        let x = ((p.x - ORIGIN) / CELL).floor() as i32;
        let y = ((p.z - ORIGIN) / CELL).floor() as i32;
        x >= 0
            && y >= 0
            && x < GRID as i32
            && y < GRID as i32
            && self.grid.walkable(Cell {
                x: x as u32,
                y: y as u32,
            })
    }

    /// Is a straight walk between two points clear on the grid?
    pub fn straight(&self, a: Vec3, b: Vec3) -> bool {
        let d = flat(b - a);
        let steps = (d.length() / (CELL * 0.5)).ceil().max(1.) as usize;
        (0..=steps).all(|k| self.walkable(a + d * (k as f32 / steps as f32)))
    }
}

/// Publish an accepted sound (chitter, bell, splash, crash) for hearing.
pub fn noise(game: &mut Game, at: Vec3, radius: f32, source: u64) {
    game.ai.noise_serial += 1;
    let _ = game.ai.noises.push(
        Noise {
            source: source + game.ai.noise_serial % 64,
            position: at,
            groups: 1,
            radius,
        },
        0.4,
    );
}

/// Called when the raccoon grabs something: its owner gasps if watching.
pub fn notice_theft(game: &mut Game, prop: usize) {
    let at = game.coon.pos();
    noise(game, at, 4.5, 300);
    let _ = prop;
}

/// Every picnicker drops what they are doing to look at `at` (the bell, the
/// tablecloth). `alarm` sends them straight into a chase.
pub fn rouse(game: &mut Game, at: Vec3, radius: f32, alarm: bool) {
    let target = game.coon.pos();
    for i in 0..FOLK {
        let f = &mut game.folk[i];
        if f.mode == Mode::Carry || flat(f.pos - at).length() > radius {
            continue;
        }
        if alarm {
            let _ = f.memory.observe(
                Candidate {
                    id: RACCOON_ID,
                    position: target,
                    groups: 1,
                },
                Sense::Sight,
            );
            f.awareness.reset(&game.ai.graph);
            force_state(f, &game.ai.graph, ALARMED);
            f.mode = Mode::Chase;
            f.chase_limit = f.kind.stamina() + 9.;
            f.chase_time = 0.;
            f.seat = None;
            f.say(Bubble::Alarm);
        }
    }
    if !alarm {
        noise(game, at, radius, 400);
    }
}

fn force_state(f: &mut Folk, graph: &DecisionGraph, state: u16) {
    // Drive the shared graph through its authored edges with synthetic flags;
    // never write private state directly.
    f.awareness.reset(graph);
    let steps: &[(u64, f32)] = match state {
        CURIOUS => &[(SEE, 0.)],
        ALARMED => &[(SEE, 0.), (SEE | CLOSE, 0.31)],
        _ => &[],
    };
    for &(flags, dt) in steps {
        let _ = f.awareness.update(graph, flags, dt);
    }
}

pub fn wriggle_free(game: &mut Game, carrier: usize) {
    let f = &mut game.folk[carrier];
    let at = f.pos + heading(f.yaw) * 0.8;
    let yaw = f.yaw;
    f.mode = Mode::React(1.6);
    f.pose = Pose::Startled;
    f.say(Bubble::Shock);
    f.awareness.reset(&game.ai.graph);
    f.memory.clear();
    set_down(game, vec3(at.x, game.park.ground_y(at), at.z), yaw);
    game.coon.grace = 1.4;
    game.emit(Event::WriggledFree { folk: carrier, at });
}

fn request_route(ai: &mut FolkAi, index: usize, f: &mut Folk, goal: Vec3) {
    f.goal = goal;
    if f.replan > 0. && f.planned.distance(goal) < 1.2 && f.has_route {
        return;
    }
    if let (Some(start), Some(end)) = (ai.cell_of(f.pos), ai.cell_of(goal)) {
        let status = ai.planner.request(PathRequest {
            agent: index as u64,
            start,
            end,
            connectivity: Connectivity::Eight,
        });
        if status != ferravex_play::navigation::RequestStatus::Full {
            f.planned = goal;
            f.replan = 0.45;
        }
    }
}

/// Step toward `goal` along the planned route (or directly when clear).
fn walk(ai: &FolkAi, park: &Park, f: &mut Folk, goal: Vec3, speed: f32) -> bool {
    let to_goal = flat(goal - f.pos);
    if to_goal.length() < 0.35 {
        f.speed = 0.;
        return true;
    }
    // Prefer the planned route; walk straight only when it is short and clear.
    let straight = to_goal.length() < 4. && ai.straight(f.pos, goal);
    let target = if straight {
        Some(goal)
    } else if f.has_route {
        f.follower
            .target(&f.route, f.pos, 0.55, 4)
            .ok()
            .flatten()
            .or(Some(goal))
    } else if ai.straight(f.pos, goal) {
        Some(goal)
    } else {
        None
    };
    let Some(target) = target else {
        f.speed = 0.;
        return false;
    };
    let d = flat(target - f.pos);
    let step = (speed * super::DT).min(d.length());
    let mut moved = 0.;
    if d.length() > 1e-4 {
        let dir = d / d.length();
        let start = f.pos;
        // Slide along blocked cells instead of sticking to their corners.
        for candidate in [
            f.pos + dir * step,
            f.pos + vec3(dir.x, 0., 0.) * step,
            f.pos + vec3(0., 0., dir.z) * step,
        ] {
            if ai.walkable(candidate) || !ai.walkable(f.pos) {
                f.pos = candidate;
                break;
            }
        }
        moved = flat(f.pos - start).length();
        if moved < step * 0.2 {
            // Wedged: drop the straight-line shortcut and ask for a route.
            f.has_route = false;
            f.replan = 0.;
        }
        let want = yaw_of(dir);
        f.yaw = super::lerp_angle(f.yaw, want, (9. * super::DT).min(1.));
    }
    f.pos.y = park.ground_y(f.pos);
    f.speed = moved / super::DT;
    false
}

fn turn_toward(f: &mut Folk, at: Vec3, rate: f32) {
    let d = flat(at - f.pos);
    if d.length() > 0.05 {
        f.yaw = super::lerp_angle(f.yaw, yaw_of(d), (rate * super::DT).min(1.));
    }
}

/// Bounded per-tick event staging while picnickers are mutably borrowed.
pub struct Outbox {
    items: [Option<Event>; 16],
    n: usize,
}
impl Outbox {
    fn new() -> Self {
        Self {
            items: [None; 16],
            n: 0,
        }
    }
    fn push(&mut self, e: Event) {
        if self.n < self.items.len() {
            self.items[self.n] = Some(e);
            self.n += 1;
        }
    }
}

pub fn update(game: &mut Game) {
    let _ = game.ai.noises.advance(super::DT);
    let mut budget = WorkBudget::new(32, 16, 900);
    let coon_pos = game.coon.pos() + Vec3::Y * 0.3;
    let coon_visible = game.coon.visible_to_folk() && game.phase == super::Phase::Playing;
    let hidden = game.coon.hide;
    let sneaking = game.coon.sneaking || game.coon.slide > 0.;
    let mut out = Outbox::new();
    for i in 0..FOLK {
        let f = &mut game.folk[i];
        f.prev = f.pos;
        f.prev_yaw = f.yaw;
        f.replan = (f.replan - super::DT).max(0.);
        f.bubble_age += super::DT;
        f.content = (f.content - super::DT).max(0.);
        if f.bubble_age > 3.2 && !matches!(f.mode, Mode::Chase | Mode::Search) {
            f.bubble = Bubble::None;
        }
        let _ = f.memory.advance(super::DT);
        // --- Perception.
        f.sees = false;
        let busy = matches!(f.mode, Mode::Carry | Mode::React(_) | Mode::Puffed(_));
        if coon_visible && !busy && f.content <= 0. {
            let d = coon_pos - f.eye();
            let tucked = matches!(hidden, Hide::Bush(_) | Hide::Table(_));
            let near = flat(d).length() < 1.25;
            if tucked && near && matches!(f.mode, Mode::Search | Mode::Chase) {
                f.poke += super::DT;
                if f.poke > 1.1 {
                    f.found = true;
                }
            } else if !near {
                f.poke = 0.;
                if flat(d).length() > 2.2 {
                    f.found = false;
                }
            }
            let concealed = tucked && !f.found;
            if !concealed {
                let range = f.range(sneaking);
                if let Ok(sight) = Sight::new(range, f.kind.fov(), 1) {
                    // The cone is judged at the raccoon's height so a picnicker
                    // looking at their own feet still sees what is there.
                    let eye = f.eye();
                    let level = vec3(eye.x, coon_pos.y.max(eye.y - 1.2), eye.z);
                    let seen = sight.test(
                        Observer {
                            position: level,
                            forward: heading(f.yaw),
                        },
                        Candidate {
                            id: RACCOON_ID,
                            position: coon_pos,
                            groups: 1,
                        },
                        &mut budget,
                        &mut SightLine(&game.world),
                    );
                    if seen == Ok(Detection::Detected) {
                        f.sees = true;
                        let _ = f.memory.observe(
                            Candidate {
                                id: RACCOON_ID,
                                position: coon_pos,
                                groups: 1,
                            },
                            Sense::Sight,
                        );
                    }
                }
            }
        }
        let mut heard = false;
        if !busy && let Ok(hearing) = Hearing::new(f.kind.hearing(), 1, 0.55) {
            for noise in game.ai.noises.iter() {
                if hearing.test(
                    f.eye(),
                    noise,
                    &mut budget,
                    Some(&mut SightLine(&game.world)),
                ) == Ok(Detection::Detected)
                {
                    heard = true;
                    let _ = f.memory.observe(
                        Candidate {
                            id: noise.source,
                            position: noise.position,
                            groups: 1,
                        },
                        Sense::Hearing,
                    );
                }
            }
        }
        // --- Awareness through the shared decision graph.
        let close = f.sees && flat(coon_pos - f.pos).length() < 4.5;
        let mut flags = 0;
        if f.sees {
            flags |= SEE;
        }
        if heard {
            flags |= HEARD;
        }
        if f.memory.best(1).is_some() {
            flags |= MEMORY;
        }
        if close {
            flags |= CLOSE;
        }
        let before = f.state();
        if !busy {
            let _ = f.awareness.update(&game.ai.graph, flags, super::DT);
        }
        let after = f.state();
        f.alert = match after {
            CALM => (f.alert - super::DT * 1.5).max(0.),
            CURIOUS => (f.alert + super::DT * if close { 2.6 } else { 1.3 }).min(0.95),
            ALARMED => 1.,
            _ => (f.alert - super::DT * 0.25).max(0.45),
        };
        let at = f.memory.best(1).map_or(f.pos, |t| t.position);
        if before != after {
            match after {
                CURIOUS => {
                    f.say(Bubble::Question);
                    out.push(Event::Suspicious { folk: i, at });
                }
                ALARMED => {
                    f.say(Bubble::Alarm);
                    if before == CURIOUS {
                        f.chase_time = 0.;
                    }
                    if !matches!(f.mode, Mode::Chase) {
                        // Whatever they were holding goes down where they stand.
                        if let Mode::Return(prop) | Mode::Fetch(prop) = f.mode
                            && game.props[prop].holder == Holder::Folk(i)
                        {
                            game.props[prop].holder = Holder::None;
                            set_carried(&mut game.world, game.props[prop].body, false);
                        }
                        f.mode = Mode::Chase;
                        f.seat = None;
                    }
                    out.push(Event::Spotted { folk: i, at });
                    if f.kind == Kind::Biscuit {
                        out.push(Event::Bark { folk: i, at: f.pos });
                    }
                }
                SEARCHING => {
                    if before == ALARMED {
                        out.push(Event::Lost { folk: i, at });
                    }
                    f.say(Bubble::Question);
                    if !matches!(f.mode, Mode::Search) {
                        if let Mode::Return(prop) | Mode::Fetch(prop) = f.mode
                            && game.props[prop].holder == Holder::Folk(i)
                        {
                            game.props[prop].holder = Holder::None;
                            set_carried(&mut game.world, game.props[prop].body, false);
                        }
                        f.mode = Mode::Search;
                        f.seat = None;
                    }
                }
                _ => {
                    f.chase_time = 0.;
                    f.found = false;
                    f.poke = 0.;
                    if matches!(f.mode, Mode::Chase | Mode::Search) {
                        f.mode = Mode::Routine;
                        f.arrived = false;
                        f.say(Bubble::Huff);
                        out.push(Event::GaveUp { folk: i, at: f.pos });
                    }
                }
            }
        }
        // Pip tells the grown-ups. Loudly.
        if f.kind == Kind::Pip && after == ALARMED && before != ALARMED {
            game.ai.noise_serial += 1;
            let _ = game.ai.noises.push(
                Noise {
                    source: 500,
                    position: coon_pos,
                    groups: 1,
                    radius: 12.,
                },
                0.5,
            );
        }
    }
    for i in 0..FOLK {
        behave(game, i, &mut out);
    }
    // Planner: one result per tick, adopted only by its current agent.
    game.ai.planner.step(&game.ai.grid, &mut budget);
    if let Some((request, result)) = game.ai.planner.result() {
        let agent = request.agent as usize;
        if agent < FOLK
            && let Ok(cells) = result
        {
            let n = cells.len().min(ROUTE);
            let ground = &game.park;
            for (k, cell) in cells.iter().rev().take(n).enumerate() {
                let p = cell_center(*cell);
                game.ai.scratch[k] = vec3(p.x, ground.ground_y(p), p.z);
            }
            let f = &mut game.folk[agent];
            if n > 0 && f.route.set(&game.ai.scratch[..n]).is_ok() {
                f.follower.reset();
                f.has_route = true;
            }
        }
        game.ai.planner.acknowledge();
    }
    separate(game);
    for e in out.items.into_iter().flatten() {
        game.emit(e);
    }
}

fn behave(game: &mut Game, i: usize, out: &mut Outbox) {
    let coon = game.coon.pos();
    let kind = game.folk[i].kind;
    match game.folk[i].mode {
        Mode::Routine => routine(game, i, out),
        Mode::Sit => {
            let f = &mut game.folk[i];
            f.wait -= super::DT;
            f.pose = Pose::Sit;
            f.speed = 0.;
            if let Some(seat) = f.seat.and_then(|s| game.park.seat(s)) {
                let target = vec3(seat.at.x, 0., seat.at.z);
                f.pos = f.pos.lerp(target, (6. * super::DT).min(1.));
                f.pos.y = 0.;
            }
            if f.state() == CURIOUS {
                if let Some(t) = f.memory.best(1) {
                    let p = t.position;
                    turn_toward(f, p, 3.);
                }
            } else {
                turn_toward(f, f.pos + heading(f.look_yaw), 4.);
            }
            if f.wait <= 0. {
                f.mode = Mode::Routine;
                f.seat = None;
                next_spot(game, i);
            }
        }
        Mode::Chase => {
            let target = game.folk[i]
                .memory
                .get(RACCOON_ID)
                .map_or(coon, |t| t.position);
            let f = &mut game.folk[i];
            f.chase_time += super::DT;
            f.pose = Pose::Run;
            f.say(Bubble::Alarm);
            if f.chase_time > f.chase_limit {
                f.mode = Mode::Puffed(2.4);
                f.pose = Pose::Puff;
                f.say(Bubble::Sweat);
                f.awareness.reset(&game.ai.graph);
                f.memory.clear();
                f.chase_limit = f.kind.stamina();
                f.chase_time = 0.;
                f.found = false;
                out.push(Event::GaveUp { folk: i, at: f.pos });
                return;
            }
            let speed = kind.run();
            let direct = f.sees && game.ai.straight(f.pos, target);
            if !direct {
                request_route(&mut game.ai, i, &mut game.folk[i], target);
            }
            let f = &mut game.folk[i];
            walk(&game.ai, &game.park, f, target, speed);
            // Caught?
            let reach = if kind.adult() { 0.78 } else { 0.7 };
            let c = &game.coon;
            if f.sees
                && c.visible_to_folk()
                && (!c.hidden() || f.found)
                && flat(coon - f.pos).length() < reach
                && (coon.y - f.pos.y).abs() < 1.3
                && game.phase == super::Phase::Playing
            {
                catch(game, i, out);
            }
        }
        Mode::Search => {
            let target = game.folk[i].memory.best(1).map(|t| t.position);
            let f = &mut game.folk[i];
            f.pose = Pose::Walk;
            if let Some(target) = target {
                let arrived = flat(target - f.pos).length() < 0.9;
                if arrived {
                    f.speed = 0.;
                    f.pose = Pose::Look;
                    let wobble = (game.time * 1.7 + i as f32).sin() * 1.3;
                    f.yaw = super::lerp_angle(f.yaw, f.look_yaw + wobble, (3. * super::DT).min(1.));
                } else {
                    f.look_yaw = f.yaw;
                    if !game.ai.straight(f.pos, target) {
                        request_route(&mut game.ai, i, &mut game.folk[i], target);
                    }
                    let f = &mut game.folk[i];
                    walk(&game.ai, &game.park, f, target, kind.walk() * 1.6);
                }
            } else {
                f.speed = 0.;
                f.pose = Pose::Look;
                f.yaw += super::DT * 1.2;
            }
        }
        Mode::Carry => carry(game, i, out),
        Mode::Fetch(prop) => {
            let p = game.props[prop];
            if p.holder != Holder::None || p.settled {
                game.folk[i].mode = Mode::Routine;
                game.folk[i].arrived = false;
                return;
            }
            let goal = p.pos;
            request_route(&mut game.ai, i, &mut game.folk[i], goal);
            let f = &mut game.folk[i];
            f.pose = Pose::Walk;
            let reached = walk(&game.ai, &game.park, f, goal, kind.walk() * 1.35)
                || flat(goal - f.pos).length() < 0.75;
            if reached {
                game.props[prop].holder = Holder::Folk(i);
                set_carried(&mut game.world, game.props[prop].body, true);
                game.folk[i].mode = Mode::Return(prop);
                game.folk[i].say(Bubble::Huff);
            }
        }
        Mode::Return(prop) => {
            let home = game.props[prop].home;
            let goal = vec3(home.x, 0., home.z);
            let f = &game.folk[i];
            let approach = if game.ai.walkable(goal) {
                goal
            } else {
                // Stand beside the table/stand rather than inside it.
                let side = flat(f.pos - goal).normalize_or_zero();
                goal + side * 1.1
            };
            request_route(&mut game.ai, i, &mut game.folk[i], approach);
            let f = &mut game.folk[i];
            f.pose = Pose::Carry;
            let reached = walk(&game.ai, &game.park, f, approach, kind.walk())
                || flat(approach - f.pos).length() < 0.6;
            if reached {
                let p = game.props[prop];
                game.props[prop].holder = Holder::None;
                set_carried(&mut game.world, p.body, false);
                place(
                    game,
                    prop,
                    RigidBodyPose {
                        translation: p.home + Vec3::Y * 0.04,
                        rotation: p.home_rot,
                    },
                    Vec3::ZERO,
                );
                game.stats.fetches += 1;
                let f = &mut game.folk[i];
                f.mode = Mode::React(1.2);
                f.pose = Pose::Act(Act::Fuss);
                f.say(Bubble::Heart);
                out.push(Event::Returned {
                    folk: i,
                    prop,
                    at: p.home,
                });
            }
        }
        Mode::React(t) => {
            let f = &mut game.folk[i];
            f.speed = 0.;
            let t = t - super::DT;
            if t <= 0. {
                f.mode = Mode::Routine;
                f.arrived = false;
                f.pose = Pose::Idle;
            } else {
                f.mode = Mode::React(t);
            }
        }
        Mode::Puffed(t) => {
            let f = &mut game.folk[i];
            f.speed = 0.;
            f.pose = Pose::Puff;
            let t = t - super::DT;
            if t <= 0. {
                f.mode = Mode::Routine;
                f.arrived = false;
            } else {
                f.mode = Mode::Puffed(t);
            }
        }
    }
}

fn routine(game: &mut Game, i: usize, out: &mut Outbox) {
    let spots = &game.park.routines[i];
    let spot = spots[game.folk[i].spot % spots.len()];
    let kind = game.folk[i].kind;
    // A curious picnicker pauses to look toward whatever caught their eye.
    if game.folk[i].state() == CURIOUS {
        let f = &mut game.folk[i];
        f.speed = 0.;
        f.pose = Pose::Look;
        if let Some(t) = f.memory.best(1) {
            let p = t.position;
            turn_toward(f, p, 5.);
        }
        return;
    }
    if !game.folk[i].arrived {
        request_route(&mut game.ai, i, &mut game.folk[i], spot.at);
        let f = &mut game.folk[i];
        f.pose = Pose::Walk;
        if walk(&game.ai, &game.park, f, spot.at, kind.walk())
            || flat(spot.at - f.pos).length() < 0.5
        {
            f.arrived = true;
            f.wait = spot.wait;
            f.has_route = false;
            f.look_yaw = activity_yaw(&game.park, i, spot.at, spot.act);
            if spot.act == Act::Sit {
                let seat = nearest_seat(&game.park, spot.at);
                f.seat = seat;
                f.mode = Mode::Sit;
                f.pose = Pose::Sit;
                out.push(Event::Sat { folk: i, at: f.pos });
                super::pranks::sat(game, i);
            }
        }
        return;
    }
    let f = &mut game.folk[i];
    f.speed = 0.;
    f.pose = Pose::Act(spot.act);
    turn_toward(f, f.pos + heading(f.look_yaw), 4.);
    f.wait -= super::DT;
    if f.award_pending && kind == Kind::Pumble && spot.act == Act::Inspect {
        f.award_pending = false;
        f.mode = Mode::React(3.);
        f.pose = Pose::Cheer;
        f.say(Bubble::Star);
        let plinth = game.park.zone(Role::Plinth).at;
        out.push(Event::Prize { at: plinth });
        return;
    }
    // Owners keep an eye on their belongings.
    f.fetch_check -= super::DT;
    if f.fetch_check <= 0. {
        f.fetch_check = 0.5;
        if let Some(prop) = displaced_belonging(game, i) {
            let f = &mut game.folk[i];
            f.mode = Mode::Fetch(prop);
            f.say(Bubble::Huff);
            f.has_route = false;
            out.push(Event::Fetching { folk: i, prop });
            return;
        }
    }
    let f = &mut game.folk[i];
    if f.wait <= 0. {
        next_spot(game, i);
    }
}

fn activity_yaw(park: &Park, i: usize, at: Vec3, act: Act) -> f32 {
    let face = |p: Vec3| yaw_of(flat(p - at));
    match act {
        Act::Sit => yaw_of(vec3(1., 0., 0.)),
        Act::Fuss => face(park.tables[0].center),
        Act::Inspect => face(park.zone(Role::Plinth).at),
        Act::Conduct => yaw_of(vec3(0., 0., 1.)),
        Act::Feed => face(park.zone(Role::Pond).at),
        Act::Chat | Act::Snack => face(vec3(7., 0., -4.)),
        _ => (i as f32 * 2.1).sin() * 3.,
    }
}

fn next_spot(game: &mut Game, i: usize) {
    let n = game.park.routines[i].len();
    let jitter = (game.rng().next_u64() % 3) as usize;
    let f = &mut game.folk[i];
    // Mostly in order, sometimes skipping ahead, never repeating.
    f.spot = (f.spot + 1 + usize::from(jitter == 0 && n > 3)) % n;
    f.arrived = false;
    f.has_route = false;
}

fn nearest_seat(park: &Park, at: Vec3) -> Option<usize> {
    park.zones
        .iter()
        .filter(|z| z.role == Role::Seat)
        .min_by(|a, b| {
            flat(a.at - at)
                .length()
                .total_cmp(&flat(b.at - at).length())
        })
        .filter(|z| flat(z.at - at).length() < 2.)
        .map(|z| z.index)
}

fn displaced_belonging(game: &Game, i: usize) -> Option<usize> {
    let f = &game.folk[i];
    let eye = f.eye();
    game.props.iter().position(|p| {
        p.owner == Some(i)
            && !p.settled
            && p.holder == Holder::None
            && p.displaced()
            && !p.stashed
            && p.still > 1.2
            && !game.park.on_hill(p.pos)
            && p.pos.y < 1.6
            && game.ai.cell_of(p.pos).is_some()
            && flat(p.pos - f.pos).length() < 12.
            && {
                let mut budget = WorkBudget::new(1, 1, 0);
                Sight::new(12., 160_f32.to_radians(), 1).is_ok_and(|s| {
                    s.test(
                        Observer {
                            position: vec3(eye.x, p.pos.y + 0.3, eye.z),
                            forward: heading(f.yaw),
                        },
                        Candidate {
                            id: 900,
                            position: p.pos + Vec3::Y * 0.1,
                            groups: 1,
                        },
                        &mut budget,
                        &mut SightLine(&game.world),
                    ) == Ok(Detection::Detected)
                })
            }
    })
}

fn catch(game: &mut Game, i: usize, out: &mut Outbox) {
    let at = game.coon.pos();
    if game.coon.held.is_some() {
        super::raccoon::drop_held(game, Vec3::ZERO);
    }
    game.coon.slide = 0.;
    game.stats.scruffs += 1;
    let kind = game.folk[i].kind;
    game.folk[i].found = false;
    game.folk[i].chase_time = 0.;
    match kind {
        Kind::Pip | Kind::Biscuit => {
            game.coon.stun = 1.3;
            game.coon.grace = 2.4;
            let f = &mut game.folk[i];
            f.mode = Mode::React(2.2);
            f.pose = Pose::Cheer;
            f.content = 24.;
            f.say(Bubble::Heart);
            f.awareness.reset(&game.ai.graph);
            f.memory.clear();
            if kind == Kind::Pip {
                out.push(Event::Hugged { folk: i, at });
            } else {
                out.push(Event::Licked { folk: i, at });
            }
        }
        _ => {
            game.coon.scruffed = Some(i);
            game.coon.wriggle = 0;
            let f = &mut game.folk[i];
            f.mode = Mode::Carry;
            f.carry_time = 0.;
            f.has_route = false;
            f.say(Bubble::Huff);
            out.push(Event::Caught { folk: i, at });
        }
    }
    // Everyone else relaxes while the culprit is dealt with.
    for (k, other) in game.folk.iter_mut().enumerate() {
        if k != i && other.mode == Mode::Chase {
            other.awareness.reset(&game.ai.graph);
            other.memory.clear();
            other.mode = Mode::Routine;
            other.arrived = false;
            other.say(Bubble::Huff);
        }
    }
}

fn carry(game: &mut Game, i: usize, out: &mut Outbox) {
    let den = game.dens.respawn();
    // Walk to a spot just outside the burrow's no-go ring.
    let toward = flat(den - game.folk[i].pos).normalize_or_zero();
    let goal = den - toward * 2.9;
    request_route(&mut game.ai, i, &mut game.folk[i], goal);
    let f = &mut game.folk[i];
    f.carry_time += super::DT;
    f.pose = Pose::Carry;
    let reached = walk(&game.ai, &game.park, f, goal, f.kind.walk() * 1.6)
        || flat(goal - f.pos).length() < 0.7;
    if reached || f.carry_time > 14. {
        let yaw = yaw_of(toward);
        let drop = f.pos + toward * 0.9;
        f.mode = Mode::React(1.4);
        f.pose = Pose::Act(Act::Bow);
        f.say(Bubble::Huff);
        f.awareness.reset(&game.ai.graph);
        f.memory.clear();
        let name = FOLK_NAMES[i];
        let _ = name;
        set_down(game, vec3(drop.x, game.park.ground_y(drop), drop.z), yaw);
        game.coon.grace = 2.6;
        out.push(Event::Shooed { folk: i, at: drop });
    }
}

fn separate(game: &mut Game) {
    for a in 0..FOLK {
        for b in a + 1..FOLK {
            let d = flat(game.folk[b].pos - game.folk[a].pos);
            let len = d.length();
            if len > 1e-4 && len < 0.7 {
                let push = d / len * (0.7 - len) * 0.5;
                if game.ai.walkable(game.folk[a].pos - push) {
                    game.folk[a].pos -= push;
                }
                if game.ai.walkable(game.folk[b].pos + push) {
                    game.folk[b].pos += push;
                }
            }
        }
    }
}
