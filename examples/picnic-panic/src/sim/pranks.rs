//! The to-do list: seven pranks, a secret finale and a few hidden bonuses.
//!
//! Completion is decided here from authoritative physics and AI state, then
//! fed to the engine's [`ObjectiveGraph`] as ordered accepted events. The
//! finale only unlocks once all seven pranks are crossed off, and the escape
//! only once the finale happened. Snapshots of the graph are saved by the host.
use super::folk::{Kind, Mode, rouse};
use super::park::{PropKind, Role};
use super::props::{Holder, find, set_carried};
use super::{Event, Game, flat};
use ferravex_play::objective::{
    ObjectiveGraph, ObjectiveKind, ObjectiveSnapshot, ObjectiveSpec, ObjectiveState,
};
use ferravex_render::prelude::{Vec3, vec3};

pub const PRANKS: usize = 7;
pub const SLOTS: usize = 9;
/// Fingerprint of the authored list; bump when its rules change.
pub const RULES: u64 = 0x7069_636e_6963_0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prank {
    Pie,
    Marrow,
    Music,
    Sled,
    Bell,
    Cushion,
    Melon,
    Finale,
    Escape,
}

impl Prank {
    pub const ALL: [Prank; SLOTS] = [
        Prank::Pie,
        Prank::Marrow,
        Prank::Music,
        Prank::Sled,
        Prank::Bell,
        Prank::Cushion,
        Prank::Melon,
        Prank::Finale,
        Prank::Escape,
    ];
    pub fn slot(self) -> usize {
        self as usize
    }
    /// Notepad line, in a raccoon's careful handwriting.
    pub fn line(self) -> &'static str {
        match self {
            Prank::Pie => "Steal a whole pie",
            Prank::Marrow => "Swap the marrow for a pinecone",
            Prank::Music => "Make the band play off-key",
            Prank::Sled => "Send a blanket sledding",
            Prank::Bell => "Ring the village bell",
            Prank::Cushion => "Whoopee-cushion a picnicker",
            Prank::Melon => "Launch a melon off the seesaw",
            Prank::Finale => "Do the tablecloth trick",
            Prank::Escape => "Make a grand escape home",
        }
    }
    /// A tiny hint shown under an unfinished line when it is selected.
    pub fn hint(self) -> &'static str {
        match self {
            Prank::Pie => "Nana's pie is cooling on the red table. Your den is very hungry.",
            Prank::Marrow => "Pinecones grow under the pines. Prize marrows sit on plinths.",
            Prank::Music => "Borrow the sheet music from the bandstand. Keep walking.",
            Prank::Sled => "Tea tray on the hilltop + folded blanket + one good push.",
            Prank::Bell => "Hold the grab button at the bell rope by the gate.",
            Prank::Cushion => "Somebody's toy box. Somebody's favourite bench.",
            Prank::Melon => "Melon on the low end. Raccoon from a great height.",
            Prank::Finale => "The long banquet table. Hold the grab button. Commit.",
            Prank::Escape => "Run home with your cape. Everyone is watching.",
        }
    }
    pub fn toast(self) -> &'static str {
        match self {
            Prank::Pie => "Pie acquired. Pie is yours now.",
            Prank::Marrow => "First prize: one (1) pinecone.",
            Prank::Music => "The band has lost its place. And its key.",
            Prank::Sled => "Wheeee! Blanket-sledding is now a sport.",
            Prank::Bell => "DONG. Everyone checks their watch.",
            Prank::Cushion => "PFFFRRRT. Nobody will speak of this.",
            Prank::Melon => "Watermelon has achieved orbit.",
            Prank::Finale => "The tablecloth trick! (Mostly.)",
            Prank::Escape => "Picnic thoroughly panicked.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bonus {
    Basket,
    Splash,
    Crates,
    Popular,
    Slide,
    Magpie,
}

impl Bonus {
    pub const ALL: [Bonus; 6] = [
        Bonus::Basket,
        Bonus::Splash,
        Bonus::Crates,
        Bonus::Popular,
        Bonus::Slide,
        Bonus::Magpie,
    ];
    pub fn bit(self) -> u8 {
        1 << (self as u8)
    }
    pub fn title(self) -> &'static str {
        match self {
            Bonus::Basket => "Just a basket",
            Bonus::Splash => "Splash zone",
            Bonus::Crates => "Crate expectations",
            Bonus::Popular => "Very popular raccoon",
            Bonus::Slide => "Slip 'n' slide",
            Bonus::Magpie => "Magpie of the year",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Bonus::Basket => "Hid in the basket while being looked for.",
            Bonus::Splash => "Dunked something in the duck pond.",
            Bonus::Crates => "Toppled the crate tower.",
            Bonus::Popular => "Chased by three people at once.",
            Bonus::Slide => "Belly-slid six metres in one go.",
            Bonus::Magpie => "Collected every shiny bottle cap.",
        }
    }
}

pub struct Todo {
    pub graph: ObjectiveGraph<SLOTS>,
    serial: u64,
    pub bonus: u8,
    /// Tick-ordered record of completions for presentation (strike-through order).
    pub done_order: [Option<Prank>; SLOTS],
}

fn specs() -> [ObjectiveSpec; SLOTS] {
    let all_pranks = (1_u64 << PRANKS) - 1;
    std::array::from_fn(|i| ObjectiveSpec {
        id: i as u16 + 1,
        all: match i {
            7 => all_pranks,
            8 => 1 << 7,
            _ => 0,
        },
        any: 0,
        exclusive_group: 0,
        kind: ObjectiveKind::Event {
            id: i as u16 + 1,
            count: 1,
        },
    })
}

impl Default for Todo {
    fn default() -> Self {
        Self::new()
    }
}

impl Todo {
    pub fn new() -> Self {
        Self {
            graph: ObjectiveGraph::new(RULES, specs()).expect("authored to-do list"),
            serial: 0,
            bonus: 0,
            done_order: [None; SLOTS],
        }
    }
    pub fn done(&self, prank: Prank) -> bool {
        self.graph.complete(prank.slot())
    }
    pub fn state(&self, prank: Prank) -> ObjectiveState {
        self.graph
            .state(prank.slot())
            .unwrap_or(ObjectiveState::Locked)
    }
    /// Bit `i` set when prank `i` (of the seven) is crossed off.
    pub fn pranks_done(&self) -> u8 {
        (0..PRANKS)
            .filter(|i| self.graph.complete(*i))
            .fold(0, |m, i| m | 1 << i)
    }
    pub fn count(&self) -> usize {
        self.pranks_done().count_ones() as usize
    }
    pub fn finale_open(&self) -> bool {
        self.state(Prank::Finale) == ObjectiveState::Ready
    }
    pub fn snapshot(&self) -> ObjectiveSnapshot<SLOTS> {
        self.graph.snapshot()
    }
    /// Feed one accepted completion. Returns true only for a new crossing-off.
    fn accept(&mut self, prank: Prank) -> bool {
        self.serial = self.graph.last_event_serial() + 1;
        let before = self.graph.complete(prank.slot());
        let _ = self.graph.event(prank.slot() as u16 + 1, 1, self.serial);
        let now = self.graph.complete(prank.slot());
        if now && !before {
            if let Some(slot) = self.done_order.iter_mut().find(|s| s.is_none()) {
                *slot = Some(prank);
            }
            true
        } else {
            false
        }
    }
}

pub fn complete(game: &mut Game, prank: Prank, at: Vec3) {
    if game.todo.accept(prank) {
        game.coon.celebrate = 1.6;
        game.emit(Event::Prank { prank, at });
        if prank == Prank::Escape {
            game.phase = super::Phase::Won;
            game.emit(Event::Victory { at });
        } else if game.todo.finale_open() && prank != Prank::Finale {
            game.emit(Event::Unlocked {
                at: game.park.zone(Role::Cloth).at,
            });
        }
    }
}

pub fn bonus(game: &mut Game, bonus: Bonus, at: Vec3) {
    if game.todo.bonus & bonus.bit() == 0 {
        game.todo.bonus |= bonus.bit();
        game.emit(Event::Bonus { bonus, at });
    }
}

/// Continue a saved list: cross the pranks off again and put their props
/// where a finished prank would have left them.
pub fn restore_completed(game: &mut Game, mask: u8) {
    for (i, prank) in Prank::ALL.iter().take(PRANKS).enumerate() {
        if mask & (1 << i) == 0 {
            continue;
        }
        game.todo.accept(*prank);
        let settle = |game: &mut Game, kind: PropKind, at: Vec3| {
            let p = find(game, kind);
            super::props::place(game, p, super::props::hold_pose(at, 0.), Vec3::ZERO);
            game.props[p].settled = true;
            game.props[p].home = at;
        };
        let den = game.park.dens[0];
        match prank {
            Prank::Pie => settle(game, PropKind::Pie, den + vec3(0.8, 0.1, -0.6)),
            Prank::Marrow => {
                let plinth = game.park.zone(Role::Plinth).at;
                settle(game, PropKind::Pinecone, plinth + Vec3::Y * 0.12);
                settle(game, PropKind::Marrow, vec3(12.4, 0.12, -1.5));
            }
            Prank::Music => {
                settle(game, PropKind::Sheet, vec3(-10.5, 0.03, 3.4));
                game.band_offkey = true;
            }
            Prank::Sled => {
                let tray = find(game, PropKind::Tray);
                settle(game, PropKind::Tray, vec3(2., 0.05, -7.6));
                let blanket = find(game, PropKind::Blanket);
                game.props[tray].loaded = true;
                game.props[blanket].holder = Holder::Tray(tray);
                game.props[blanket].settled = true;
                set_carried(&mut game.world, game.props[blanket].body, true);
            }
            Prank::Cushion => {
                let seat = game.park.seat(0).map_or(Vec3::ZERO, |s| s.at);
                settle(game, PropKind::Cushion, seat + Vec3::Y * 0.06);
            }
            Prank::Melon => settle(game, PropKind::Melon, vec3(9.5, 0.3, 17.)),
            _ => {}
        }
    }
    game.todo.done_order = [None; SLOTS];
    let mut k = 0;
    for prank in Prank::ALL {
        if game.todo.done(prank) {
            game.todo.done_order[k] = Some(prank);
            k += 1;
        }
    }
}

fn plinth_holds(game: &Game, i: usize) -> bool {
    let z = game.park.zone(Role::Plinth);
    let p = &game.props[i];
    p.holder == Holder::None
        && flat(p.pos - z.at).length() < z.radius
        && (p.pos.y - z.at.y) > -0.15
        && (p.pos.y - z.at.y) < 0.55
}

pub fn update(game: &mut Game) {
    let t = &game.todo;
    // Pie: resting (or at least dropped) inside the den.
    let pie = find(game, PropKind::Pie);
    if !t.done(Prank::Pie) && game.props[pie].stashed {
        game.props[pie].settled = true;
        let at = game.props[pie].pos;
        complete(game, Prank::Pie, at);
    }
    // Marrow swap: pinecone on the plinth while the marrow is elsewhere.
    let marrow = find(game, PropKind::Marrow);
    let cone = find(game, PropKind::Pinecone);
    if !game.todo.done(Prank::Marrow)
        && plinth_holds(game, cone)
        && !plinth_holds(game, marrow)
        && game.props[marrow].holder != Holder::Folk(1)
    {
        game.props[marrow].settled = true;
        game.props[cone].settled = true;
        game.folk[1].award_pending = true;
        let at = game.props[cone].pos;
        complete(game, Prank::Marrow, at);
    }
    // Sheet music: carried (or flung) well away from the music stand.
    let sheet = find(game, PropKind::Sheet);
    let stand = game.park.zone(Role::Stand).at;
    let away = flat(game.props[sheet].pos - stand).length();
    if !game.todo.done(Prank::Music) && away > 7. {
        game.props[sheet].settled = true;
        game.band_offkey = true;
        let at = game.folk[2].pos;
        game.emit(Event::OffKey { at });
        let f = &mut game.folk[2];
        if matches!(f.mode, Mode::Routine | Mode::Sit) {
            f.mode = Mode::React(2.5);
            f.pose = super::folk::Pose::Startled;
        }
        complete(game, Prank::Music, at);
    }
    // Tea-tray sled: a loaded tray arrives at the bottom of the hill at speed.
    let tray = find(game, PropKind::Tray);
    let p = game.props[tray];
    let sled_end = game.park.zone(Role::SledEnd);
    if !game.todo.done(Prank::Sled)
        && p.loaded
        && p.holder == Holder::None
        && p.pos.z > -11.4
        && flat(p.pos - sled_end.at).length() < sled_end.radius
        && p.pos.y < 0.7
        && p.vel.length() > 1.6
    {
        game.props[tray].settled = true;
        let blanket = find(game, PropKind::Blanket);
        game.props[blanket].settled = true;
        game.emit(Event::Sled { at: p.pos });
        complete(game, Prank::Sled, p.pos);
    }
    // Watermelon: properly airborne (not carried, well above the seesaw).
    let melon = find(game, PropKind::Melon);
    let m = game.props[melon];
    if !game.todo.done(Prank::Melon)
        && m.holder == Holder::None
        && m.vel.y > 3.2
        && m.pos.y > 1.1
        && flat(m.pos - game.mech.seesaw_pivot).length() < 4.5
    {
        game.props[melon].settled = true;
        game.emit(Event::Launch { at: m.pos });
        complete(game, Prank::Melon, m.pos);
    }
    bonuses(game);
}

fn bonuses(game: &mut Game) {
    let pos = game.coon.pos();
    if game.coon.hide == super::raccoon::Hide::Basket
        && game.folk.iter().any(|f| {
            matches!(f.mode, Mode::Search | Mode::Chase) && flat(f.pos - pos).length() < 3.
        })
    {
        bonus(game, Bonus::Basket, pos);
    }
    let chasers = game.folk.iter().filter(|f| f.chasing()).count();
    if chasers >= 3 {
        bonus(game, Bonus::Popular, pos);
    }
    let mut fallen = 0;
    for p in &game.props {
        if p.kind == PropKind::Crate && p.holder == Holder::None && p.pos.distance(p.home) > 0.6 {
            fallen += 1;
        }
    }
    if fallen >= 3 {
        let at = game.props[find(game, PropKind::Crate)].pos;
        bonus(game, Bonus::Crates, at);
    }
}

pub fn splash(game: &mut Game, prop: usize) {
    let at = game.props[prop].pos;
    super::folk::noise(game, at, 8., 600);
    bonus(game, Bonus::Splash, at);
}

pub fn slide_finished(game: &mut Game, distance: f32) {
    if distance >= 6. {
        let at = game.coon.pos();
        bonus(game, Bonus::Slide, at);
    }
}

pub fn ring_bell(game: &mut Game) {
    let bell = game.mech.bell;
    let at = game.mech.bell_pivot - Vec3::Y * 0.4;
    game.world.bodies[bell].apply_torque_impulse(vec3(9., 0., 0.), true);
    game.emit(Event::Bell { at });
    rouse(game, at, 24., false);
    complete(game, Prank::Bell, at);
}

/// Somebody sat down: is there a cushion waiting for them?
pub fn sat(game: &mut Game, folk: usize) {
    let Some(seat) = game.folk[folk].seat.and_then(|s| game.park.seat(s)) else {
        return;
    };
    let cushion = find(game, PropKind::Cushion);
    let c = game.props[cushion];
    if c.holder == Holder::None
        && flat(c.pos - seat.at).length() < seat.radius + 0.15
        && (c.pos.y - seat.at.y).abs() < 0.35
    {
        let at = seat.at;
        game.emit(Event::Pfft { folk, at });
        let f = &mut game.folk[folk];
        f.mode = Mode::React(2.6);
        f.pose = super::folk::Pose::Startled;
        f.seat = None;
        if !game.todo.done(Prank::Cushion) {
            game.props[cushion].settled = true;
            complete(game, Prank::Cushion, at);
        }
    }
}

/// The finale: dishes fly, the cake wobbles, the raccoon wears a cape.
pub fn yank_cloth(game: &mut Game) {
    if !game.todo.finale_open() {
        return;
    }
    let at = game.park.zone(Role::Cloth).at;
    game.coon.cape = true;
    let table = vec3(7., 0.9, -4.);
    for i in 0..game.props.len() {
        let p = game.props[i];
        if flat(p.pos - table).length() > 3.2 || p.holder != Holder::None {
            continue;
        }
        let away = flat(p.pos - at).normalize_or_zero();
        let lift = match p.kind {
            PropKind::Cake => 2.2,
            PropKind::Jelly => 3.6,
            _ => 3.,
        };
        let jitter = (i as f32 * 2.3).sin();
        let impulse = (away * 1.6 + Vec3::Y * lift + vec3(0., 0., jitter * 0.8))
            * game.world.bodies[p.body].mass();
        game.world.bodies[p.body].apply_impulse(impulse, true);
        game.world.bodies[p.body].apply_torque_impulse(vec3(jitter, 0.3, -jitter) * 0.2, true);
    }
    game.emit(Event::Yank { at });
    rouse(game, at, 80., true);
    complete(game, Prank::Finale, at);
}

pub fn reached_den(game: &mut Game) {
    if game.todo.done(Prank::Finale) && !game.todo.done(Prank::Escape) {
        let at = game.coon.pos();
        complete(game, Prank::Escape, at);
    }
}

/// Who would award first prize to a pinecone? Mr. Pumble, apparently.
pub fn judge() -> Kind {
    Kind::Pumble
}
