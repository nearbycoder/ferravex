//! Renderer-independent, seeded fixed-tick rules for Picnic Panic.
//!
//! [`Game::update`] advances exactly one 60 Hz tick from an [`Input`]
//! snapshot. It steps the rigid-body world, the raccoon's character motor,
//! the picnickers' perception/decision/navigation brains and the to-do list,
//! then appends accepted [`Event`]s for presentation to drain. Nothing here
//! reads a clock, the platform, or the renderer.
pub mod bot;
pub mod folk;
pub mod park;
pub mod pranks;
pub mod props;
pub mod raccoon;
#[cfg(test)]
mod tests;

use ferravex_physics::d3::PhysicsWorld;
use ferravex_play::gameplay::checkpoint::Checkpoints;
use ferravex_render::prelude::{Quat, Vec2, Vec3, vec3};
use folk::{Folk, FolkAi};
use park::Park;
use pranks::{Bonus, Prank, Todo};
use props::{Mechanisms, Prop};
use raccoon::Raccoon;

pub const TICK_HZ: f64 = 60.;
pub const DT: f32 = 1. / 60.;
pub const MAX_EVENTS: usize = 128;

/// Collider membership bits shared by physics, character and sight queries.
pub const L_STATIC: u32 = 1;
pub const L_PROP: u32 = 2;
pub const L_OCCLUDE: u32 = 4;
pub const L_DECOR: u32 = 8;

/// One tick of player (or bot) intent. Movement is camera-relative:
/// `x` right, `y` forward; `cam_yaw` is the camera heading (radians, 0 = -Z).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub movement: Vec2,
    pub cam_yaw: f32,
    pub jump: bool,
    pub jump_held: bool,
    /// Grab / drop / use (edge). Holding it continues bell and tablecloth pulls.
    pub grab: bool,
    pub grab_held: bool,
    pub throw: bool,
    pub chitter: bool,
    pub slide: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    /// The finale escape reached the den. Results are shown.
    Won,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Wood,
    Soft,
    Crockery,
    Fruit,
    Metal,
}

/// Accepted game facts. Presentation turns these into sound, particles, camera
/// framing and UI; it never decides them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Jumped {
        at: Vec3,
    },
    Landed {
        at: Vec3,
        speed: f32,
    },
    Grabbed {
        prop: usize,
        at: Vec3,
    },
    Dropped {
        prop: usize,
        at: Vec3,
    },
    Thrown {
        prop: usize,
        at: Vec3,
    },
    Chitter {
        at: Vec3,
    },
    Slide {
        at: Vec3,
    },
    Stomp {
        at: Vec3,
        strength: f32,
    },
    Bump {
        at: Vec3,
        strength: f32,
        surface: Surface,
    },
    Splash {
        at: Vec3,
        big: bool,
    },
    Shiny {
        at: Vec3,
        count: u32,
    },
    Spotted {
        folk: usize,
        at: Vec3,
    },
    Suspicious {
        folk: usize,
        at: Vec3,
    },
    Lost {
        folk: usize,
        at: Vec3,
    },
    GaveUp {
        folk: usize,
        at: Vec3,
    },
    Caught {
        folk: usize,
        at: Vec3,
    },
    Wriggle {
        at: Vec3,
        count: u8,
    },
    WriggledFree {
        folk: usize,
        at: Vec3,
    },
    Shooed {
        folk: usize,
        at: Vec3,
    },
    Hugged {
        folk: usize,
        at: Vec3,
    },
    Licked {
        folk: usize,
        at: Vec3,
    },
    Bark {
        folk: usize,
        at: Vec3,
    },
    Hid {
        at: Vec3,
        basket: bool,
    },
    Unhid {
        at: Vec3,
    },
    Fetching {
        folk: usize,
        prop: usize,
    },
    Returned {
        folk: usize,
        prop: usize,
        at: Vec3,
    },
    Sat {
        folk: usize,
        at: Vec3,
    },
    Bell {
        at: Vec3,
    },
    Pfft {
        folk: usize,
        at: Vec3,
    },
    Launch {
        at: Vec3,
    },
    Sled {
        at: Vec3,
    },
    OffKey {
        at: Vec3,
    },
    Prize {
        at: Vec3,
    },
    Yank {
        at: Vec3,
    },
    Prank {
        prank: Prank,
        at: Vec3,
    },
    Bonus {
        bonus: Bonus,
        at: Vec3,
    },
    DenFound {
        den: usize,
        at: Vec3,
    },
    Unlocked {
        at: Vec3,
    },
    Victory {
        at: Vec3,
    },
}

/// Small deterministic generator (xorshift64*), owned by the rules.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    /// Uniform in `0..1`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunConfig {
    pub seed: u64,
    /// Pranks already crossed off in a saved to-do list (continue).
    pub completed: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub scruffs: u32,
    pub chitters: u32,
    pub slides: u32,
    pub jumps: u32,
    pub grabs: u32,
    pub throws: u32,
    pub hides: u32,
    pub wriggles: u32,
    pub distance: f32,
    pub max_chasers: u32,
    pub fetches: u32,
}

pub struct Game {
    pub config: RunConfig,
    pub tick: u64,
    pub time: f32,
    pub phase: Phase,
    pub park: Park,
    pub world: PhysicsWorld,
    pub props: Vec<Prop>,
    pub mech: Mechanisms,
    pub coon: Raccoon,
    pub folk: [Folk; park::FOLK],
    pub ai: FolkAi,
    pub todo: Todo,
    pub dens: Checkpoints<Vec3, 3>,
    pub dens_found: u8,
    pub shinies: Vec<(Vec3, bool)>,
    pub stats: Stats,
    pub wind: Vec3,
    pub band_offkey: bool,
    pub band_paused: f32,
    rng: Rng,
    events: Vec<Event>,
}

impl Game {
    pub fn new(config: RunConfig) -> Self {
        let park = Park::authored();
        let mut world = PhysicsWorld::new([0., -9.81, 0.]);
        let (props, mech) = props::build(&park, &mut world);
        let coon = Raccoon::new(&park, &world);
        let folk = std::array::from_fn(|i| Folk::new(i, &park));
        let ai = FolkAi::new(&park);
        let dens = Checkpoints::new(0x6465_6e73, park.dens).expect("three authored dens");
        let shinies = park.shinies.iter().map(|p| (*p, false)).collect();
        let mut game = Self {
            config,
            tick: 0,
            time: 0.,
            phase: Phase::Playing,
            park,
            world,
            props,
            mech,
            coon,
            folk,
            ai,
            todo: Todo::new(),
            dens,
            dens_found: 1,
            shinies,
            stats: Stats::default(),
            wind: Vec3::ZERO,
            band_offkey: false,
            band_paused: 0.,
            rng: Rng::new(config.seed),
            events: Vec::with_capacity(MAX_EVENTS),
        };
        // Physics impacts feed bump sounds; record a bounded set per tick.
        let _ = game
            .world
            .set_impact_settings(Some(ferravex_physics::ImpactSettings {
                capacity: 16,
                min_normal_impulse: 0.35,
                min_approach_speed: 1.4,
            }));
        if config.completed != 0 {
            pranks::restore_completed(&mut game, config.completed);
        }
        // Let stacks and the bunting settle before the first visible tick.
        for _ in 0..90 {
            props::wind(&mut game, 0.);
            game.world.step(DT);
        }
        props::sync_poses(&mut game, true);
        game.world.clear_impacts();
        game
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Won
    }

    /// Accepted events since the last drain, in tick order.
    pub fn drain_events(&mut self, out: &mut Vec<Event>) {
        out.append(&mut self.events);
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub(crate) fn emit(&mut self, event: Event) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        }
    }

    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// Advance exactly one fixed tick.
    pub fn update(&mut self, input: Input) {
        self.tick += 1;
        self.time += DT;
        let previous = self.coon.motor.position;
        props::remember_poses(self);
        raccoon::begin_tick(self);
        if self.phase == Phase::Playing {
            raccoon::actions(self, input);
        }
        props::hold_carried(self);
        props::wind(self, self.time);
        self.world.step(DT);
        props::sync_poses(self, false);
        raccoon::movement(self, input);
        props::hold_carried(self);
        folk::update(self);
        props::rules(self);
        pranks::update(self);
        props::impacts(self);
        raccoon::collect_shinies(self);
        raccoon::dens(self);
        let moved = (self.coon.motor.position - previous) * vec3(1., 0., 1.);
        if moved.length() < 2. {
            self.stats.distance += moved.length();
        }
        let chasers = self.folk.iter().filter(|f| f.chasing()).count() as u32;
        self.stats.max_chasers = self.stats.max_chasers.max(chasers);
    }

    /// Interpolated raccoon pose for rendering.
    pub fn coon_pose(&self, alpha: f32) -> (Vec3, f32) {
        let a = alpha.clamp(0., 1.);
        let p = self.coon.prev_pos.lerp(self.coon.motor.position, a);
        (p, lerp_angle(self.coon.prev_yaw, self.coon.yaw, a))
    }

    pub fn prop_pose(&self, i: usize, alpha: f32) -> (Vec3, Quat) {
        let p = &self.props[i];
        let a = alpha.clamp(0., 1.);
        (
            p.prev_pos.lerp(p.pos, a),
            p.prev_rot.slerp(p.rot, a).normalize(),
        )
    }

    pub fn completed(&self) -> u8 {
        self.todo.pranks_done()
    }

    pub fn shinies_found(&self) -> u32 {
        self.coon.pouch.count(raccoon::SHINY)
    }

    /// FNV-1a hash of the authoritative state that drives later ticks.
    /// Used to prove render-cadence independence and for ghost contexts.
    pub fn state_hash(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325_u64;
        let mut feed = |v: f32| {
            for b in v.to_bits().to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
            }
        };
        let c = &self.coon.motor;
        for v in [c.position, c.velocity] {
            feed(v.x);
            feed(v.y);
            feed(v.z);
        }
        feed(self.coon.yaw);
        for f in &self.folk {
            feed(f.pos.x);
            feed(f.pos.z);
            feed(f.yaw);
        }
        for p in &self.props {
            feed(p.pos.x);
            feed(p.pos.y);
            feed(p.pos.z);
        }
        feed(self.time);
        feed(f32::from(self.todo.pranks_done()));
        h
    }
}

pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    a + d * t
}

pub fn flat(v: Vec3) -> Vec3 {
    vec3(v.x, 0., v.z)
}

/// Unit heading vector for a yaw (0 = -Z, positive turns toward +X).
pub fn heading(yaw: f32) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    vec3(s, 0., -c)
}

pub fn yaw_of(v: Vec3) -> f32 {
    v.x.atan2(-v.z)
}
