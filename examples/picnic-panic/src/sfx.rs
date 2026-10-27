//! Picnic Panic's own synthesized sound kit and adaptive score.
//!
//! Every clip is generated once at startup from oscillators and noise. World
//! sounds are positional (inverse-square falloff and panning around the
//! listener at the camera). The score is three phase-aligned loops: a gentle
//! pizzicato-and-glockenspiel stroll, a sneaky chase layer that swells on bar
//! lines (through the engine's [`MusicConductor`]) while someone is after the
//! raccoon, and the village brass band, playing positionally from the
//! bandstand — and wobbling hopelessly off-key once its sheet music is gone.
use ferravex_render::audio::{
    self, Bus, Clip, GameAudio, MusicConductor, Sound, Spatial, Surface, VoiceHandle, VoiceParams,
};
use ferravex_render::prelude::Vec3;

const RATE: u32 = 24_000;
const TRACKED: usize = 32;
pub const BPM: u16 = 112;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Sfx {
    Hop,
    Pat,
    Grab,
    Plop,
    Whoosh,
    Chitter,
    Boing,
    Knock,
    Clink,
    Thud,
    Splash,
    Ding,
    Alert,
    Huh,
    Harrumph,
    Squeak,
    Bark,
    Rustle,
    Bell,
    Pfft,
    Whee,
    Fanfare,
    Scribble,
    Twinkle,
    Drumroll,
    Victory,
    Click,
    Select,
    Page,
    Bird,
}

const COUNT: usize = Sfx::Bird as usize + 1;

struct Lcg(u32);
impl Lcg {
    fn noise(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.
    }
}

fn render(seconds: f32, mut f: impl FnMut(f32, &mut Lcg) -> f32) -> Sound {
    let frames = (seconds * RATE as f32) as usize;
    let mut rng = Lcg(0x5eed_1234);
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        let tail = ((seconds - t) / 0.012).clamp(0., 1.);
        let v = (f(t, &mut rng) * tail).clamp(-1., 1.);
        out.push([v, v]);
    }
    Sound::new(Clip {
        sample_rate: RATE,
        samples: out.into(),
    })
    .expect("synthesized clip")
}

fn sine(p: f32) -> f32 {
    (p * std::f32::consts::TAU).sin()
}
fn tri(p: f32) -> f32 {
    4. * (p.fract() - 0.5).abs() - 1.
}
fn square(p: f32) -> f32 {
    if p.fract() < 0.5 { 1. } else { -1. }
}
fn saw(p: f32) -> f32 {
    p.fract() * 2. - 1.
}
fn env(t: f32, attack: f32, decay: f32) -> f32 {
    if t < 0. {
        0.
    } else {
        (t / attack).min(1.) * (-decay * t).exp()
    }
}
fn sweep(t: f32, f0: f32, f1: f32, d: f32) -> f32 {
    let k = (f1 / f0).ln() / d;
    if k.abs() < 1e-6 {
        f0 * t
    } else {
        f0 * ((k * t).exp() - 1.) / k
    }
}
fn midi(n: f32) -> f32 {
    440. * 2_f32.powf((n - 69.) / 12.)
}
fn notes(t: f32, freqs: &[f32], step: f32, decay: f32) -> f32 {
    let mut v = 0.;
    for (i, &f) in freqs.iter().enumerate() {
        let lt = t - i as f32 * step;
        if lt >= 0. {
            v += (tri(f * lt) * 0.6 + sine(f * 2. * lt) * 0.25) * env(lt, 0.004, decay);
        }
    }
    v
}

fn build() -> Vec<Sound> {
    let mut s = Vec::with_capacity(COUNT);
    // Hop: a tiny rising "bwip".
    s.push(render(0.12, |t, _| {
        sine(sweep(t, 300., 700., 0.12)) * env(t, 0.003, 22.) * 0.35
    }));
    // Pat: soft paw landing.
    s.push(render(0.08, |t, r| {
        (r.noise() * 0.4 + sine(110. * t) * 0.6) * env(t, 0.001, 55.) * 0.4
    }));
    // Grab: cheerful "bloop".
    s.push(render(0.14, |t, _| {
        (sine(sweep(t, 420., 880., 0.14)) * 0.6 + sine(sweep(t, 840., 1760., 0.14)) * 0.15)
            * env(t, 0.004, 18.)
            * 0.4
    }));
    // Plop: set something down.
    s.push(render(0.12, |t, _| {
        sine(sweep(t, 520., 180., 0.12)) * env(t, 0.002, 28.) * 0.45
    }));
    let mut lp = 0.;
    // Whoosh.
    s.push(render(0.32, |t, r| {
        let shape = (t / 0.32 * std::f32::consts::PI).sin();
        lp += (r.noise() - lp) * (0.05 + 0.25 * shape);
        lp * shape * 1.1
    }));
    // Chitter: a flurry of squeaky raccoon chirps.
    s.push(render(0.42, |t, r| {
        let mut v = 0.;
        for k in 0..5 {
            let lt = t - k as f32 * 0.075;
            if lt >= 0. {
                let f = 1500. + (k % 3) as f32 * 260. + r.noise() * 30.;
                v += sine(sweep(lt, f, f * 1.45, 0.06)) * env(lt, 0.002, 38.);
            }
        }
        v * 0.3
    }));
    // Boing: spring.
    s.push(render(0.5, |t, _| {
        let vib = 1. + 0.25 * (t * 38.).sin() * (-t * 5.).exp();
        sine(sweep(t, 160., 420., 0.5) * vib) * env(t, 0.003, 6.) * 0.45
    }));
    // Knock: wood.
    s.push(render(0.12, |t, r| {
        (sine(sweep(t, 240., 160., 0.12)) * 0.7 + r.noise() * 0.25 * (-t * 90.).exp())
            * env(t, 0.001, 35.)
            * 0.5
    }));
    // Clink: crockery.
    s.push(render(0.3, |t, _| {
        (sine(2350. * t) + 0.6 * sine(3720. * t) + 0.3 * sine(5100. * t))
            * env(t, 0.001, 16.)
            * 0.22
    }));
    // Thud: fruit.
    s.push(render(0.18, |t, r| {
        (sine(sweep(t, 140., 70., 0.18)) * 0.8 + r.noise() * 0.15) * env(t, 0.002, 20.) * 0.55
    }));
    let mut lp2 = 0.;
    // Splash.
    s.push(render(0.7, |t, r| {
        lp2 += (r.noise() - lp2) * (0.3 - t * 0.3).max(0.05);
        let drops = sine(sweep(t, 900., 1500., 0.7)) * 0.1 * (t * 14.).sin().max(0.);
        (lp2 * 1.4 + drops) * env(t, 0.004, 4.5) * 0.7
    }));
    // Ding: bottle cap.
    s.push(render(0.4, |t, _| {
        (sine(1975. * t) + 0.5 * sine(2960. * t)) * env(t, 0.002, 9.) * 0.25
    }));
    // Alert: "!" stinger (two-note rise).
    s.push(render(0.35, |t, _| {
        let f = if t < 0.1 { 587. } else { 880. };
        (square(f * t) * 0.25 + tri(f * t) * 0.4) * env(t % 0.1 + 0.001, 0.003, 9.) * 0.4
    }));
    // Huh: curious rising "hm?".
    s.push(render(0.38, |t, _| {
        let f = sweep(t, 180., 290., 0.38);
        (tri(f) * 0.6 + sine(f * 2.) * 0.2) * env(t, 0.03, 5.) * 0.4
    }));
    // Harrumph: falling grumble.
    s.push(render(0.5, |t, _| {
        let f = sweep(t, 220., 120., 0.5);
        (saw(f) * 0.3 + tri(f) * 0.4) * env(t, 0.02, 4.) * 0.4
    }));
    // Squeak.
    s.push(render(0.2, |t, _| {
        sine(sweep(t, 1400., 2300., 0.2)) * env(t, 0.004, 12.) * 0.3
    }));
    // Bark: woof.
    s.push(render(0.22, |t, r| {
        let f = sweep(t, 520., 240., 0.22);
        (saw(f) * 0.4 + r.noise() * 0.2) * env(t, 0.006, 14.) * 0.5
    }));
    let mut lp3 = 0.;
    // Rustle: leaves.
    s.push(render(0.35, |t, r| {
        lp3 += (r.noise() - lp3) * 0.45;
        let crackle = if r.noise() > 0.92 { r.noise() } else { 0. };
        (lp3 * 0.5 + crackle * 0.4) * env(t, 0.02, 7.) * 0.6
    }));
    // Bell: a village bell, rich and long.
    s.push(render(3.2, |t, _| {
        let p = [1., 2.0, 2.4, 3.0, 4.1];
        let a = [0.5, 0.3, 0.22, 0.14, 0.08];
        let mut v = 0.;
        for k in 0..5 {
            v += sine(392. * p[k] * t) * a[k] * (-t * (0.9 + k as f32 * 0.5)).exp();
        }
        v * env(t, 0.002, 0.25) * 0.55
    }));
    // Pfft: the whoopee cushion. Of course.
    s.push(render(1.1, |t, r| {
        let f = sweep(t, 140., 70., 1.1) * (1. + 0.12 * (t * 31.).sin());
        let buzz = saw(f) * 0.6 + square(f * 0.5) * 0.2 + r.noise() * 0.15;
        buzz * env(t, 0.01, 2.2) * 0.55
    }));
    // Whee: sliding whistle.
    s.push(render(0.9, |t, _| {
        let f = sweep(t, 600., 1500., 0.9) * (1. + 0.02 * (t * 30.).sin());
        sine(f) * env(t, 0.03, 2.5) * 0.35
    }));
    // Fanfare: a kazoo-and-glock ta-daa.
    s.push(render(1.0, |t, _| {
        let n = notes(t, &[523.25, 659.25, 783.99, 1046.5], 0.08, 4.);
        let kazoo = if t > 0.32 {
            let lt = t - 0.32;
            saw(783.99 * lt * (1. + 0.01 * (lt * 34.).sin())) * env(lt, 0.02, 3.) * 0.18
        } else {
            0.
        };
        (n * 0.3 + kazoo) * 0.9
    }));
    let mut lp4 = 0.;
    // Scribble: pencil crossing a line off the list.
    s.push(render(0.55, |t, r| {
        lp4 += (r.noise() - lp4) * 0.6;
        let strokes = (t * 26.).sin().abs();
        lp4 * strokes * env(t, 0.01, 3.5) * 0.8
    }));
    // Twinkle: bonus found.
    s.push(render(0.9, |t, _| {
        notes(t, &[1046.5, 1318.5, 1568., 2093.], 0.07, 5.) * 0.22
    }));
    let mut lp5 = 0.;
    // Drumroll: the finale is open.
    s.push(render(1.4, |t, r| {
        lp5 += (r.noise() - lp5) * 0.5;
        let roll = 0.5 + 0.5 * (t * 90.).sin();
        let crash = if t > 1.15 {
            r.noise() * env(t - 1.15, 0.002, 6.)
        } else {
            0.
        };
        (lp5 * roll * (t / 1.15).min(1.) * 0.7 + crash * 0.6) * 0.7
    }));
    // Victory: a jaunty little march.
    s.push(render(2.2, |t, _| {
        notes(
            t,
            &[523.25, 523.25, 659.25, 783.99, 659.25, 783.99, 1046.5],
            0.16,
            3.,
        ) * 0.3
    }));
    // Click.
    s.push(render(0.03, |t, _| {
        sine(1900. * t) * env(t, 0.0005, 120.) * 0.3
    }));
    // Select.
    s.push(render(0.13, |t, _| {
        (tri(660. * t) * 0.5 + tri(990. * t) * 0.3) * env(t, 0.002, 22.) * 0.35
    }));
    let mut lp6 = 0.;
    // Page: notepad flip.
    s.push(render(0.25, |t, r| {
        lp6 += (r.noise() - lp6) * 0.3;
        lp6 * env(t, 0.01, 12.) * 0.7
    }));
    // Bird: a two-note chirp.
    s.push(render(0.32, |t, _| {
        let a = sine(sweep(t, 2800., 3600., 0.1)) * env(t, 0.003, 30.);
        let lt = t - 0.14;
        let b = if lt > 0. {
            sine(sweep(lt, 3300., 2600., 0.12)) * env(lt, 0.003, 26.)
        } else {
            0.
        };
        (a + b) * 0.16
    }));
    debug_assert_eq!(s.len(), COUNT);
    s
}

/// Minimum seconds between two starts of the same sound.
fn spacing(s: Sfx) -> f32 {
    match s {
        Sfx::Bell | Sfx::Pfft | Sfx::Victory | Sfx::Drumroll => 0.6,
        Sfx::Knock | Sfx::Clink | Sfx::Thud | Sfx::Plop => 0.06,
        Sfx::Pat => 0.05,
        _ => 0.08,
    }
}

const BARS: usize = 8;
const MELODY: [u8; 64] = [
    76, 79, 84, 79, 76, 0, 74, 0, 72, 76, 81, 76, 72, 0, 71, 0, 69, 72, 77, 81, 79, 77, 76, 0, 74,
    79, 83, 86, 83, 0, 79, 0, 79, 0, 76, 79, 84, 0, 83, 81, 81, 0, 77, 81, 84, 0, 81, 77, 79, 77,
    76, 74, 71, 74, 79, 0, 84, 0, 79, 76, 72, 0, 0, 0,
];
const ROOTS: [u8; BARS] = [48, 45, 41, 43, 48, 41, 43, 48];
/// Major (0) or minor (1) triad per bar.
const MINOR: [bool; BARS] = [false, true, false, false, false, false, false, false];

fn chord(bar: usize) -> [f32; 3] {
    let r = ROOTS[bar] as f32 + 12.;
    let third = if MINOR[bar] { 3. } else { 4. };
    [midi(r), midi(r + third), midi(r + 7.)]
}

/// Three phase-aligned loops: stroll, chase layer, brass band.
fn stems() -> [Sound; 3] {
    let beat = 60. / BPM as f32;
    let seconds = beat * 4. * BARS as f32;
    let eighth = beat * 0.5;
    let stroll = render(seconds, |t, _| {
        let bar = ((t / (beat * 4.)) as usize).min(BARS - 1);
        let in_bar = t - bar as f32 * beat * 4.;
        let b = (in_bar / beat) as usize;
        let lt = in_bar - b as f32 * beat;
        // Pizzicato oom-pah.
        let root = midi(ROOTS[bar] as f32);
        let bass = if b.is_multiple_of(2) {
            let f = if b == 0 { root } else { root * 1.5 };
            (tri(f * lt) * 0.7 + sine(f * 2. * lt) * 0.2) * env(lt, 0.004, 9.)
        } else {
            let c = chord(bar);
            c.iter().map(|f| tri(f * lt)).sum::<f32>() * 0.25 * env(lt, 0.004, 14.)
        };
        // Glockenspiel melody on eighths.
        let step = (t / eighth) as usize % MELODY.len();
        let st = t - (t / eighth).floor() * eighth;
        let n = MELODY[step];
        let glock = if n > 0 {
            let f = midi(n as f32);
            (sine(f * st) + 0.35 * sine(f * 2.76 * st) + 0.12 * sine(f * 5.4 * st))
                * env(st, 0.002, 7.)
        } else {
            0.
        };
        bass * 0.32 + glock * 0.16
    });
    let chase = render(seconds, |t, r| {
        let bar = ((t / (beat * 4.)) as usize).min(BARS - 1);
        let sixteenth = beat * 0.25;
        let k = (t / sixteenth) as usize;
        let st = t - k as f32 * sixteenth;
        let c = chord(bar);
        let up = [0, 1, 2, 1];
        let f = c[up[k % 4]] * 2.;
        let arp = (square(f * st) * 0.3 + tri(f * st) * 0.5) * env(st, 0.002, 30.);
        let tick = if k.is_multiple_of(2) {
            r.noise() * env(st, 0.0005, 80.) * 0.5
        } else {
            0.
        };
        let b = (t / beat) as usize;
        let lt = t - b as f32 * beat;
        let tuba = saw(midi(ROOTS[bar] as f32 - 12.) * lt) * env(lt, 0.01, 6.) * 0.35;
        arp * 0.18 + tick * 0.22 + tuba * 0.3
    });
    let band = render(seconds, |t, _| {
        let bar = ((t / (beat * 4.)) as usize).min(BARS - 1);
        let b = (t / beat) as usize;
        let lt = t - b as f32 * beat;
        let root = midi(ROOTS[bar] as f32 - 12.);
        let tuba = if b.is_multiple_of(2) {
            (saw(root * lt) * 0.5 + sine(root * lt) * 0.5) * env(lt, 0.012, 5.)
        } else {
            0.
        };
        let step = (t / eighth) as usize % MELODY.len();
        let st = t - (t / eighth).floor() * eighth;
        let n = MELODY[step];
        let horn = if n > 0 {
            let f = midi(n as f32 - 12.) * (1. + 0.004 * (st * 30.).sin());
            (saw(f * st) * 0.35 + square(f * st) * 0.15 + sine(f * st) * 0.4) * env(st, 0.02, 3.)
        } else {
            0.
        };
        let c = chord(bar);
        let pah = if b % 2 == 1 {
            c.iter().map(|f| saw(f * lt)).sum::<f32>() * 0.12 * env(lt, 0.01, 10.)
        } else {
            0.
        };
        tuba * 0.32 + horn * 0.26 + pah
    });
    [stroll, chase, band]
}

pub struct Score {
    sounds: [Sound; 3],
    voices: [Option<VoiceHandle>; 3],
    conductor: MusicConductor,
    level: f32,
    detune: f32,
    t: f32,
}

impl Score {
    fn new() -> Self {
        Self {
            sounds: stems(),
            voices: [None; 3],
            conductor: MusicConductor::new(BPM).expect("authored tempo"),
            level: 0.,
            detune: 0.,
            t: 0.,
        }
    }

    fn update(&mut self, dt: f32, chase: f32, fade: f32, band: Vec3, offkey: bool) {
        let dt = dt.clamp(0., 0.25);
        self.t += dt;
        if !self.voices.iter().all(|v| v.is_some_and(audio::playing)) {
            for v in &mut self.voices {
                if let Some(h) = v.take() {
                    audio::stop(h, 0);
                }
            }
            self.conductor = MusicConductor::new(BPM).expect("authored tempo");
            for (k, (sound, voice)) in self.sounds.iter().zip(&mut self.voices).enumerate() {
                *voice = audio::play_sound(
                    sound,
                    VoiceParams {
                        bus: if k == 2 { Bus::Ambience } else { Bus::Music },
                        gain: 0.,
                        looping: true,
                        priority: 9,
                        ..VoiceParams::default()
                    },
                );
            }
        }
        // The chase layer joins/leaves on bar lines.
        let frame = self.conductor.update(dt, chase.clamp(0., 1.)).unwrap_or(
            ferravex_render::audio::MusicFrame {
                beat: 0.,
                bar_changed: false,
                gains: [0.22, 0.],
            },
        );
        self.level += (fade - self.level) * (1. - (-dt * 2.).exp());
        self.detune += ((if offkey { 1. } else { 0. }) - self.detune) * (1. - (-dt).exp());
        let wobble =
            1. + self.detune * (0.035 * (self.t * 2.3).sin() + 0.02 * (self.t * 5.1).sin());
        let gains = [
            frame.gains[0] * 1.1 * self.level,
            frame.gains[1] * 1.6 * self.level,
            0.75 * self.level,
        ];
        for (k, v) in self.voices.iter().enumerate() {
            if let Some(h) = v {
                let params = if k == 2 {
                    VoiceParams {
                        position: band.to_array(),
                        spatial: Spatial::Attenuated {
                            min_distance: 3.,
                            max_distance: 34.,
                            rolloff: 1.,
                        },
                        pitch: wobble,
                        gain: gains[2],
                        bus: Bus::Ambience,
                        looping: true,
                        priority: 9,
                        ramp_frames: 1024,
                        ..VoiceParams::default()
                    }
                } else {
                    VoiceParams {
                        gain: gains[k],
                        bus: Bus::Music,
                        looping: true,
                        priority: 9,
                        ramp_frames: 1024,
                        ..VoiceParams::default()
                    }
                };
                audio::set_params(*h, params);
            }
        }
    }

    fn stop(&mut self) {
        for v in &mut self.voices {
            if let Some(h) = v.take() {
                audio::stop(h, 256);
            }
        }
    }
}

impl Drop for Score {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct SoundKit {
    sounds: Vec<Sound>,
    voices: [Option<VoiceHandle>; TRACKED],
    next: usize,
    last: [f32; COUNT],
    time: f32,
    paused: bool,
    score: Score,
    pub steps: GameAudio,
    bird_timer: f32,
    bird_seed: u32,
}

impl Default for SoundKit {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundKit {
    pub fn new() -> Self {
        Self {
            sounds: build(),
            voices: [None; TRACKED],
            next: 0,
            last: [f32::NEG_INFINITY; COUNT],
            time: 0.,
            paused: false,
            score: Score::new(),
            steps: GameAudio::new(),
            bird_timer: 2.,
            bird_seed: 0x77,
        }
    }

    pub fn age(&mut self, dt: f32) {
        self.time += dt.max(0.);
    }

    fn start(&mut self, sfx: Sfx, params: VoiceParams) {
        let ui = matches!(sfx, Sfx::Click | Sfx::Select | Sfx::Page);
        if self.paused && !ui {
            return;
        }
        let i = sfx as usize;
        if self.time - self.last[i] < spacing(sfx) {
            return;
        }
        self.last[i] = self.time;
        let Some(handle) = audio::play_sound(&self.sounds[i], params) else {
            return;
        };
        if !ui {
            self.voices[self.next] = Some(handle);
            self.next = (self.next + 1) % TRACKED;
        }
    }

    /// Non-positional (UI and to-do list) sound.
    pub fn play(&mut self, sfx: Sfx, gain: f32, pitch: f32) {
        let ui = matches!(sfx, Sfx::Click | Sfx::Select | Sfx::Page);
        self.start(
            sfx,
            VoiceParams {
                gain: gain.clamp(0., 2.),
                pitch: pitch.clamp(0.25, 4.),
                bus: if ui { Bus::Ui } else { Bus::Effects },
                priority: 6,
                ramp_frames: 0,
                ..VoiceParams::default()
            },
        );
    }

    /// A sound placed in the world, attenuated and panned around the listener.
    pub fn at(&mut self, sfx: Sfx, at: Vec3, gain: f32, pitch: f32) {
        let big = matches!(sfx, Sfx::Bell | Sfx::Pfft | Sfx::Splash);
        self.start(
            sfx,
            VoiceParams {
                position: at.to_array(),
                spatial: Spatial::Attenuated {
                    min_distance: if big { 6. } else { 2.5 },
                    max_distance: if big { 70. } else { 34. },
                    rolloff: 1.,
                },
                gain: gain.clamp(0., 2.),
                pitch: pitch.clamp(0.25, 4.),
                bus: Bus::Effects,
                priority: if big { 7 } else { 3 },
                ramp_frames: 0,
                ..VoiceParams::default()
            },
        );
    }

    pub fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.paused = true;
        self.steps.pause();
        for v in &mut self.voices {
            if let Some(h) = v.take() {
                audio::stop(h, 64);
            }
        }
    }

    pub fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            self.steps.resume();
        }
    }

    pub fn reset(&mut self) {
        for v in &mut self.voices {
            if let Some(h) = v.take() {
                audio::stop(h, 64);
            }
        }
        self.steps.reset();
        self.last = [f32::NEG_INFINITY; COUNT];
    }

    /// Music, band and birdsong. `chase` in 0..=1; `fade` scales everything.
    pub fn ambience(&mut self, dt: f32, chase: f32, offkey: bool, listener: Vec3, trees: &[Vec3]) {
        let fade = if self.paused { 0.35 } else { 1. };
        let band = Vec3::new(-18., 1.8, -3.);
        self.score.update(dt, chase, fade, band, offkey);
        self.bird_timer -= dt;
        if self.bird_timer <= 0. && !self.paused && !trees.is_empty() {
            self.bird_seed = self
                .bird_seed
                .wrapping_mul(1_103_515_245)
                .wrapping_add(12_345);
            let k = (self.bird_seed >> 8) as usize;
            let tree = trees[k % trees.len()] + Vec3::Y * 4.;
            if tree.distance(listener) < 30. {
                let pitch = 0.85 + (k % 7) as f32 * 0.06;
                self.at(Sfx::Bird, tree, 0.55, pitch);
            }
            self.bird_timer = 1.5 + (k % 9) as f32 * 0.45;
        }
    }
}

/// Pentatonic ladder for chained bottle caps.
pub fn ladder(step: u32) -> f32 {
    const L: [f32; 8] = [0., 2., 4., 7., 9., 12., 14., 16.];
    2f32.powf(L[(step as usize).min(L.len() - 1)] / 12.)
}

pub fn walk_surface() -> Surface {
    Surface::Grass
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_and_stem_is_synthesized_and_aligned() {
        let s = build();
        assert_eq!(s.len(), COUNT);
        assert!(s.iter().all(|x| !x.clip().samples.is_empty()));
        let stems = stems();
        let n = stems[0].clip().samples.len();
        assert!(
            stems.iter().all(|x| x.clip().samples.len() == n),
            "loops stay phase-aligned"
        );
        assert!(ladder(4) > ladder(1));
    }
}
