//! Presentation-only juice, created exclusively from accepted game events.
//!
//! Particles (petals, golden dust motes, grass puffs, crumbs, confetti, pond
//! splashes, music notes and sparkles), ground trails for the belly-slide and
//! the tea-tray sled, camera framing for set pieces, toasts and speech lines.
//! A [`FeedbackController`] synced with the shared engine preferences decides
//! shake and flash: reduced motion removes shake, set-piece swoops and moving
//! ambience; reduced flash removes screen flashes and caps bloom.
use crate::sfx::{Sfx, SoundKit};
use crate::sim::folk::Kind;
use crate::sim::park::PropKind;
use crate::sim::pranks::{Bonus, Prank};
use crate::sim::props::Holder;
use crate::sim::{Event, Game, Surface, flat};
use ferravex_play::feedback::{FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackPolicy};
use ferravex_play::particles::{
    DirectionCone, Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion,
    ParticlePool, SpawnShape, Trail, TrailConfig, TrailSample,
};
use ferravex_play::ui::notices::{Notice, NoticeDuplicate, NoticePriority, NoticeQueue};
use ferravex_render::preferences;
use ferravex_render::prelude::{Vec3, vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Toast {
    Prank(Prank),
    Bonus(Bonus),
    Den,
    Shiny(u32),
    Hat(crate::models::Hat),
    Unlocked,
    Line(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Speech {
    pub text: &'static str,
    pub age: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Showcase {
    pub at: Vec3,
    pub prop: Option<usize>,
    pub age: f32,
    pub life: f32,
}

impl Showcase {
    /// 0 → 1 → 0 blend weight with eased ends.
    pub fn weight(&self) -> f32 {
        let a = (self.age / 0.6).min(1.);
        let b = ((self.life - self.age) / 0.7).clamp(0., 1.);
        let w = a.min(b);
        w * w * (3. - 2. * w)
    }
}

fn desc(start: [f32; 4], end: [f32; 4], size: f32, life: f32) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            lifetime: life,
            start_color: start,
            end_color: end,
            start_size: size,
            end_size: size * 0.3,
            ..ParticleDesc::default()
        },
        motion: ParticleMotion {
            drag: 1.5,
            angular_velocity: 5.,
            ..ParticleMotion::default()
        },
        lifetime_jitter: 0.35,
        size_jitter: 0.4,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..EmitterDesc::default()
    }
}

pub struct Fx {
    feedback: FeedbackController<64>,
    next_id: u64,
    pub soft: ParticlePool,
    pub glow: ParticlePool,
    petals: Emitter,
    motes: Emitter,
    puff: Emitter,
    crumbs: Emitter,
    confetti: Emitter,
    splash: Emitter,
    sparkle: Emitter,
    notes: Emitter,
    pub slide_trail: Trail,
    pub sled_trail: Trail,
    pub toasts: NoticeQueue<4, Toast, u8>,
    pub speech: [Option<Speech>; 5],
    pub showcase: Option<Showcase>,
    pub shake: Vec3,
    pub flash: f32,
    pub jelly: f32,
    pub time: f32,
    pub strike: [f32; 9],
    pub pop: f32,
    pub bonus_seen: u8,
    pub caught_by: Option<(usize, f32)>,
}

const SEED: u64 = 0x7069_636e;

impl Default for Fx {
    fn default() -> Self {
        Self::new()
    }
}

impl Fx {
    pub fn new() -> Self {
        let mut petals = desc([1., 0.86, 0.9, 0.95], [1., 0.95, 0.95, 0.], 0.07, 6.);
        petals.shape = SpawnShape::Box {
            half: [16., 3., 16.],
        };
        petals.particle.acceleration = [0.35, -0.12, 0.15];
        petals.velocity_jitter = [0.4, 0.15, 0.4];
        petals.motion.drag = 0.4;
        petals.motion.angular_velocity = 2.;
        petals.particle.end_size = 0.06;
        petals.rate = 7.;
        let mut motes = desc([1., 0.86, 0.55, 0.65], [1., 0.8, 0.4, 0.], 0.035, 5.);
        motes.shape = SpawnShape::Box {
            half: [12., 2.5, 12.],
        };
        motes.particle.acceleration = [0.05, 0.03, 0.02];
        motes.velocity_jitter = [0.12, 0.08, 0.12];
        motes.motion.drag = 0.6;
        motes.rate = 10.;
        let mut puff = desc([0.86, 0.82, 0.64, 0.7], [0.9, 0.86, 0.7, 0.], 0.18, 0.7);
        puff.velocity_cone = Some(DirectionCone {
            axis: [0., 1., 0.],
            angle: 1.3,
            speed: [0.6, 1.8],
        });
        puff.particle.end_size = 0.36;
        puff.motion.drag = 3.;
        let mut crumbs = desc([0.92, 0.73, 0.42, 1.], [0.85, 0.62, 0.35, 0.], 0.05, 0.9);
        crumbs.velocity_cone = Some(DirectionCone {
            axis: [0., 1., 0.],
            angle: 0.9,
            speed: [1., 2.6],
        });
        crumbs.particle.acceleration = [0., -9., 0.];
        crumbs.motion.drag = 0.5;
        let mut confetti = desc([1., 1., 1., 1.], [1., 1., 1., 0.], 0.09, 2.4);
        confetti.velocity_cone = Some(DirectionCone {
            axis: [0., 1., 0.],
            angle: 0.75,
            speed: [3.5, 7.5],
        });
        confetti.particle.acceleration = [0., -5.5, 0.];
        confetti.particle.end_size = 0.08;
        confetti.motion.drag = 1.6;
        confetti.motion.angular_velocity = 9.;
        let mut splash = desc([0.75, 0.9, 1., 0.9], [0.8, 0.95, 1., 0.], 0.1, 0.9);
        splash.velocity_cone = Some(DirectionCone {
            axis: [0., 1., 0.],
            angle: 0.6,
            speed: [2.5, 5.],
        });
        splash.particle.acceleration = [0., -10., 0.];
        splash.motion.drag = 0.3;
        let mut sparkle = desc([1., 0.95, 0.6, 1.], [1., 0.7, 0.3, 0.], 0.09, 0.9);
        sparkle.shape = SpawnShape::Sphere { radius: 0.5 };
        sparkle.velocity_jitter = [1., 1.4, 1.];
        sparkle.motion.drag = 2.5;
        let mut notes = desc([0.2, 0.18, 0.3, 0.9], [0.3, 0.25, 0.45, 0.], 0.12, 2.2);
        notes.shape = SpawnShape::Sphere { radius: 0.6 };
        notes.particle.velocity = [0., 0.7, 0.];
        notes.velocity_jitter = [0.5, 0.2, 0.5];
        notes.motion.drag = 0.2;
        notes.motion.angular_velocity = 0.5;
        let trail = |lifetime: f32| {
            Trail::new(TrailConfig {
                capacity: 160,
                lifetime,
                spacing: 0.12,
                teleport_distance: 3.,
                max_per_update: 64,
                fade_width: true,
            })
            .expect("authored trail")
        };
        Self {
            feedback: FeedbackController::new(FeedbackConfig::default(), SEED)
                .expect("feedback controller"),
            next_id: 1,
            soft: ParticlePool::new(900),
            glow: ParticlePool::new(400),
            petals: Emitter::new(petals, SEED + 1).expect("petals"),
            motes: Emitter::new(motes, SEED + 2).expect("motes"),
            puff: Emitter::new(puff, SEED + 3).expect("puff"),
            crumbs: Emitter::new(crumbs, SEED + 4).expect("crumbs"),
            confetti: Emitter::new(confetti, SEED + 5).expect("confetti"),
            splash: Emitter::new(splash, SEED + 6).expect("splash"),
            sparkle: Emitter::new(sparkle, SEED + 7).expect("sparkle"),
            notes: Emitter::new(notes, SEED + 8).expect("notes"),
            slide_trail: trail(2.4),
            sled_trail: trail(3.5),
            toasts: NoticeQueue::new(),
            speech: [None; 5],
            showcase: None,
            shake: Vec3::ZERO,
            flash: 0.,
            jelly: 0.,
            time: 0.,
            strike: [0.; 9],
            pop: 0.,
            bonus_seen: 0,
            caught_by: None,
        }
    }

    pub fn sync(&mut self) {
        preferences::sync_feedback(&mut self.feedback);
    }

    pub fn policy(&self) -> FeedbackPolicy {
        self.feedback.policy()
    }

    /// 1 normally; 0 under reduced motion (scales sway, bob and drift).
    pub fn motion(&self) -> f32 {
        if self.policy().reduced_motion() {
            0.
        } else {
            1.
        }
    }

    pub fn reduced_flash(&self) -> bool {
        self.policy().reduced_flash()
    }

    pub fn reset(&mut self) {
        self.feedback.clear();
        self.soft.clear();
        self.glow.clear();
        self.slide_trail.clear();
        self.sled_trail.clear();
        self.toasts.clear();
        self.speech = [None; 5];
        self.showcase = None;
        self.shake = Vec3::ZERO;
        self.flash = 0.;
        self.jelly = 0.;
        self.strike = [0.; 9];
        self.bonus_seen = 0;
        self.caught_by = None;
        for (k, e) in [
            &mut self.petals,
            &mut self.motes,
            &mut self.puff,
            &mut self.crumbs,
            &mut self.confetti,
            &mut self.splash,
            &mut self.sparkle,
            &mut self.notes,
        ]
        .into_iter()
        .enumerate()
        {
            e.reset(SEED + k as u64 + 1);
        }
    }

    fn accept(
        &mut self,
        event: Result<FeedbackEvent, ferravex_play::feedback::FeedbackError>,
    ) -> bool {
        let Ok(event) = event else { return false };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        matches!(self.feedback.emit(id, event), Ok(Some(_)))
    }

    fn burst(emitter: &mut Emitter, pool: &mut ParticlePool, at: Vec3, n: u32) {
        let _ = emitter.burst(n, EmitterTransform::at(at.to_array()), pool);
    }

    fn toast(&mut self, toast: Toast, key: u8, priority: NoticePriority, seconds: f64) {
        let _ = self.toasts.insert(
            Notice {
                key: Some(key),
                priority,
                message: toast,
                lifetime_seconds: seconds,
            },
            NoticeDuplicate::Replace,
        );
    }

    fn say(&mut self, folk: usize, text: &'static str) {
        if folk < self.speech.len() {
            self.speech[folk] = Some(Speech { text, age: 0. });
        }
    }

    /// Turn accepted simulation events into presentation (and sounds).
    pub fn consume(&mut self, events: &[Event], game: &Game, sound: &mut SoundKit, quiet: bool) {
        let reduced = self.policy().reduced_motion();
        for e in events {
            match *e {
                Event::Jumped { at } => sound.at(Sfx::Hop, at, 0.5, 1.),
                Event::Landed { at, speed } => {
                    if speed > 4. && self.accept(FeedbackEvent::land(at, (speed / 10.).min(1.))) {
                        Self::burst(&mut self.puff, &mut self.soft, at, 8);
                    }
                    sound.at(Sfx::Pat, at, (speed / 8.).clamp(0.2, 0.6), 1.);
                }
                Event::Grabbed { prop, at } => {
                    self.accept(FeedbackEvent::pickup(at, 0.5));
                    sound.at(Sfx::Grab, at, 0.7, 1.);
                    if game.props[prop].kind == PropKind::Pie {
                        Self::burst(&mut self.crumbs, &mut self.soft, at, 10);
                    }
                }
                Event::Dropped { at, .. } => sound.at(Sfx::Plop, at, 0.5, 1.),
                Event::Thrown { at, .. } => sound.at(Sfx::Whoosh, at, 0.6, 1.15),
                Event::Chitter { at } => {
                    sound.at(Sfx::Chitter, at, 0.9, 1.);
                    Self::burst(&mut self.sparkle, &mut self.glow, at + Vec3::Y * 0.7, 3);
                }
                Event::Slide { at } => {
                    sound.at(Sfx::Whoosh, at, 0.5, 0.8);
                    Self::burst(&mut self.puff, &mut self.soft, at, 6);
                }
                Event::Stomp { at, strength } => {
                    if self.accept(FeedbackEvent::hit(at, (strength / 40.).min(1.))) {
                        Self::burst(&mut self.puff, &mut self.soft, at, 10);
                    }
                    sound.at(Sfx::Boing, at, 0.8, 1.);
                }
                Event::Bump {
                    at,
                    strength,
                    surface,
                } => {
                    let g = (strength / 8.).clamp(0.15, 0.7);
                    let s = match surface {
                        Surface::Wood => Sfx::Knock,
                        Surface::Crockery => Sfx::Clink,
                        Surface::Fruit => Sfx::Thud,
                        Surface::Soft | Surface::Metal => Sfx::Plop,
                    };
                    sound.at(s, at, g, 0.9 + (strength % 1.) * 0.2);
                }
                Event::Splash { at, big } => {
                    if self.accept(FeedbackEvent::explosion(at, if big { 0.5 } else { 0.25 })) {
                        Self::burst(
                            &mut self.splash,
                            &mut self.soft,
                            at,
                            if big { 26 } else { 14 },
                        );
                    }
                    sound.at(Sfx::Splash, at, 0.8, if big { 0.8 } else { 1.1 });
                }
                Event::Shiny { at, count } => {
                    Self::burst(&mut self.sparkle, &mut self.glow, at, 10);
                    sound.play(Sfx::Ding, 0.45, 1. + count as f32 * 0.06);
                    self.toast(Toast::Shiny(count), 6, NoticePriority::Low, 1.8);
                }
                Event::Spotted { folk, at } => {
                    sound.at(Sfx::Alert, at, 0.8, folk_pitch(game, folk));
                    self.say(folk, spot_line(game.folk[folk].kind));
                }
                Event::Suspicious { folk, at } => {
                    sound.at(Sfx::Huh, at, 0.5, folk_pitch(game, folk));
                }
                Event::Lost { folk, .. } => self.say(folk, "Where'd it go?"),
                Event::GaveUp { folk, at } => {
                    sound.at(Sfx::Harrumph, at, 0.5, folk_pitch(game, folk));
                    self.say(folk, giveup_line(game.folk[folk].kind));
                }
                Event::Caught { folk, at } => {
                    if self.accept(FeedbackEvent::hurt(at, 0.7)) {
                        Self::burst(&mut self.puff, &mut self.soft, at, 12);
                    }
                    sound.at(Sfx::Squeak, at, 0.9, 1.);
                    self.say(folk, caught_line(game.folk[folk].kind));
                    self.caught_by = Some((folk, 0.));
                }
                Event::Wriggle { at, count } => {
                    sound.at(Sfx::Squeak, at, 0.4, 1. + count as f32 * 0.08);
                    if !reduced {
                        self.shake += vec3(0.04, 0.02, 0.);
                    }
                }
                Event::WriggledFree { folk, at } => {
                    sound.at(Sfx::Boing, at, 0.7, 1.2);
                    self.say(folk, "Ow! It wriggled!");
                    Self::burst(&mut self.puff, &mut self.soft, at, 8);
                    self.caught_by = None;
                }
                Event::Shooed { folk, at } => {
                    sound.at(Sfx::Harrumph, at, 0.6, folk_pitch(game, folk));
                    self.say(folk, shoo_line(game.folk[folk].kind));
                    self.caught_by = None;
                }
                Event::Hugged { folk, at } => {
                    sound.at(Sfx::Squeak, at, 0.7, 1.3);
                    self.say(folk, "SO FLUFFY!");
                    Self::burst(&mut self.sparkle, &mut self.glow, at + Vec3::Y * 0.8, 8);
                }
                Event::Licked { folk, at } => {
                    sound.at(Sfx::Bark, at, 0.7, 1.2);
                    self.say(folk, "*slurp*");
                    Self::burst(&mut self.sparkle, &mut self.glow, at + Vec3::Y * 0.6, 6);
                }
                Event::Bark { at, .. } => sound.at(Sfx::Bark, at, 0.8, 1.),
                Event::Hid { at, basket } => {
                    sound.at(if basket { Sfx::Plop } else { Sfx::Rustle }, at, 0.5, 1.);
                }
                Event::Unhid { at } => sound.at(Sfx::Rustle, at, 0.3, 1.2),
                Event::Fetching { folk, .. } => self.say(folk, "Now how did THAT get there?"),
                Event::Returned { folk, at, .. } => {
                    sound.at(Sfx::Plop, at, 0.4, 0.9);
                    self.say(folk, "There. Back where it belongs.");
                }
                Event::Sat { folk, .. } => {
                    if game.folk[folk].kind == Kind::Nana {
                        self.say(folk, "Ahh, my bench.");
                    }
                }
                Event::Bell { at } => {
                    sound.at(Sfx::Bell, at, 1., 1.);
                    if self.accept(FeedbackEvent::explosion(at, 0.3)) {
                        Self::burst(&mut self.sparkle, &mut self.glow, at, 12);
                    }
                }
                Event::Pfft { folk, at } => {
                    sound.at(Sfx::Pfft, at, 1., 1.);
                    self.say(folk, "...I beg your pardon!");
                    Self::burst(&mut self.puff, &mut self.soft, at, 14);
                }
                Event::Launch { at } => {
                    sound.at(Sfx::Boing, at, 1., 0.7);
                    self.accept(FeedbackEvent::explosion(at, 0.6));
                }
                Event::Sled { at } => sound.at(Sfx::Whee, at, 0.9, 1.),
                Event::OffKey { .. } => self.say(2, "Hmm? Where is my... ah, nevermind."),
                Event::Prize { at } => {
                    sound.at(Sfx::Fanfare, at, 0.6, 1.1);
                    self.say(1, "A magnificent specimen! First prize!");
                    Self::burst(&mut self.confetti, &mut self.soft, at, 30);
                }
                Event::Yank { at } => {
                    sound.at(Sfx::Whoosh, at, 1., 0.6);
                    sound.at(Sfx::Clink, at, 1., 0.8);
                    self.jelly = 1.;
                    if self.accept(FeedbackEvent::explosion(at, 1.)) {
                        Self::burst(&mut self.confetti, &mut self.soft, at, 60);
                    }
                    for (i, f) in game.folk.iter().enumerate() {
                        self.say(i, yank_line(f.kind));
                    }
                }
                Event::Prank { prank, at } => {
                    self.strike[prank.slot()] = 0.001;
                    self.pop = 1.;
                    sound.play(Sfx::Scribble, 0.8, 1.);
                    if prank != Prank::Escape {
                        sound.play(Sfx::Fanfare, 0.7, 1.);
                    }
                    if self.accept(FeedbackEvent::explosion(at, 0.55)) {
                        Self::burst(&mut self.confetti, &mut self.soft, at + Vec3::Y * 0.5, 50);
                        Self::burst(&mut self.sparkle, &mut self.glow, at + Vec3::Y * 0.6, 18);
                    }
                    self.toast(Toast::Prank(prank), 1, NoticePriority::High, 4.);
                    if !reduced && !quiet {
                        let prop = match prank {
                            Prank::Melon => Some(crate::sim::props::find(game, PropKind::Melon)),
                            Prank::Sled => Some(crate::sim::props::find(game, PropKind::Tray)),
                            _ => None,
                        };
                        self.showcase = Some(Showcase {
                            at,
                            prop,
                            age: 0.,
                            life: if prop.is_some() { 3.2 } else { 2.4 },
                        });
                    }
                }
                Event::Bonus { bonus, at } => {
                    sound.play(Sfx::Twinkle, 0.7, 1.);
                    Self::burst(&mut self.sparkle, &mut self.glow, at + Vec3::Y * 0.6, 20);
                    self.toast(Toast::Bonus(bonus), 2, NoticePriority::Normal, 3.5);
                    self.bonus_seen |= bonus.bit();
                }
                Event::DenFound { den, at } => {
                    if den > 0 {
                        sound.at(Sfx::Twinkle, at, 0.5, 0.8);
                        self.toast(Toast::Den, 3, NoticePriority::Normal, 3.);
                    }
                }
                Event::Unlocked { .. } => {
                    sound.play(Sfx::Drumroll, 0.8, 1.);
                    self.toast(Toast::Unlocked, 4, NoticePriority::High, 5.);
                }
                Event::Victory { at } => {
                    sound.play(Sfx::Victory, 0.9, 1.);
                    Self::burst(&mut self.confetti, &mut self.soft, at + Vec3::Y, 90);
                }
            }
        }
    }

    /// Age everything by presentation time; ambient emitters follow the camera.
    pub fn advance(&mut self, dt: f32, game: &Game, focus: Vec3, running: bool) {
        let dt = dt.clamp(0., 0.1);
        self.time += dt;
        let frame = self.feedback.advance(dt).unwrap_or_default();
        let s = frame.shake;
        let target = s.translation * 0.6;
        self.shake = self.shake * (1. - (dt * 14.).min(1.)) + target;
        self.flash = if self.reduced_flash() {
            0.
        } else {
            frame.flash.max(self.flash - dt * 3.)
        };
        self.jelly = (self.jelly - dt * 0.35).max(0.);
        self.pop = (self.pop - dt * 1.4).max(0.);
        for s in &mut self.strike {
            if *s > 0. {
                *s = (*s + dt * 1.6).min(1.);
            }
        }
        for sp in self.speech.iter_mut() {
            if let Some(s) = sp {
                s.age += dt;
                if s.age > 2.8 {
                    *sp = None;
                }
            }
        }
        if let Some((_, age)) = &mut self.caught_by {
            *age += dt;
        }
        let _ = self.toasts.advance(f64::from(dt));
        if let Some(sc) = &mut self.showcase {
            sc.age += dt;
            if sc.age >= sc.life {
                self.showcase = None;
            }
        }
        let _ = self.soft.update(dt);
        let _ = self.glow.update(dt);
        let motion = self.motion();
        if running {
            let at = EmitterTransform::at((focus + Vec3::Y * 2.).to_array());
            let _ = self.petals.set_rate(7. * motion);
            let _ = self.motes.set_rate(9. * motion);
            let _ = self.petals.update(dt, at, &mut self.soft);
            let _ = self.motes.update(dt, at, &mut self.glow);
            // The band plays (gamely, and eventually off-key) at the bandstand.
            let band = vec3(-18., 2.4, -2.6);
            let near = flat(band - focus).length() < 24.;
            let _ = self
                .notes
                .set_rate(if near && motion > 0. { 1.6 } else { 0. });
            let _ = self
                .notes
                .update(dt, EmitterTransform::at(band.to_array()), &mut self.soft);
        }
        // Trails: the belly-slide's grass skid and the tea tray's glorious run.
        let c = &game.coon;
        let skid = (c.slide > 0. && c.motor.grounded).then(|| TrailSample {
            position: (c.pos() + Vec3::Y * 0.02).to_array(),
            color: [0.55, 0.72, 0.35, 0.7],
            width: 0.32,
        });
        let _ = self.slide_trail.update(dt, skid);
        let tray = crate::sim::props::find(game, PropKind::Tray);
        let t = &game.props[tray];
        let sled =
            (t.loaded && t.holder == Holder::None && t.vel.length() > 1.2).then(|| TrailSample {
                position: (t.pos - Vec3::Y * 0.02).to_array(),
                color: [0.98, 0.9, 0.72, 0.8],
                width: 0.62,
            });
        let _ = self.sled_trail.update(dt, sled);
    }

    /// Offset applied to the camera for shake (zero under reduced motion).
    pub fn camera_shake(&self) -> Vec3 {
        if self.policy().reduced_motion() {
            Vec3::ZERO
        } else {
            self.shake
        }
    }
}

fn folk_pitch(game: &Game, folk: usize) -> f32 {
    match game.folk[folk].kind {
        Kind::Nana => 1.2,
        Kind::Pumble => 0.8,
        Kind::Trumble => 0.95,
        Kind::Pip => 1.5,
        Kind::Biscuit => 1.1,
    }
}

fn spot_line(kind: Kind) -> &'static str {
    match kind {
        Kind::Nana => "Good heavens, a RACCOON!",
        Kind::Pumble => "Oi! You there!",
        Kind::Trumble => "Fortissimo! Seize it!",
        Kind::Pip => "RACCOON!!! Can I keep it?",
        Kind::Biscuit => "WOOF! (friend?)",
    }
}

fn caught_line(kind: Kind) -> &'static str {
    match kind {
        Kind::Nana => "Back to your hedge, young man.",
        Kind::Pumble => "Not on my watch, bandit.",
        Kind::Trumble => "And... exeunt raccoon!",
        _ => "Gotcha!",
    }
}

fn shoo_line(kind: Kind) -> &'static str {
    match kind {
        Kind::Nana => "Shoo! And stay shooed.",
        Kind::Pumble => "Go on. Scram.",
        Kind::Trumble => "Encore? NO encore.",
        _ => "Bye bye!",
    }
}

fn giveup_line(kind: Kind) -> &'static str {
    match kind {
        Kind::Nana => "My knees aren't what they were.",
        Kind::Pumble => "Hmph. Slippery little...",
        Kind::Trumble => "I need a sit-down. And a scone.",
        Kind::Pip => "Aww, come back!",
        Kind::Biscuit => "*pant pant*",
    }
}

fn yank_line(kind: Kind) -> &'static str {
    match kind {
        Kind::Nana => "MY TABLECLOTH!",
        Kind::Pumble => "Was that... a magic trick?",
        Kind::Trumble => "BRAVO! I mean: STOP!",
        Kind::Pip => "AGAIN! DO IT AGAIN!",
        Kind::Biscuit => "WOOF WOOF WOOF!",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showcase_eases_in_and_out() {
        let mut s = Showcase {
            at: Vec3::ZERO,
            prop: None,
            age: 0.,
            life: 2.,
        };
        assert_eq!(s.weight(), 0.);
        s.age = 1.;
        assert!(s.weight() > 0.99);
        s.age = 2.;
        assert_eq!(s.weight(), 0.);
    }
}
