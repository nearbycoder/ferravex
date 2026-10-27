//! Glowtide's own synthesized sound kit. Every clip is generated once at
//! startup from oscillators and noise (no files), then replayed with pitch,
//! gain and a bounded set of tracked voices that pause/reset with the run.
use ferravex_render::audio::{self, AdaptiveScore, Bus, Clip, Sound, VoiceHandle, VoiceParams};

const RATE: u32 = 24_000;
const TRACKED: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Sfx {
    Chime,
    Hit,
    Crit,
    Pop,
    PopBig,
    Zap,
    Whoosh,
    Boom,
    LevelUp,
    Chest,
    Hurt,
    Roar,
    Coin,
    Click,
    Pew,
    Bubble,
    Lash,
    Evolve,
    Warning,
    Victory,
    Defeat,
    Heal,
    Magnet,
    Select,
    Shimmer,
}

const COUNT: usize = Sfx::Shimmer as usize + 1;

/// Minimum seconds between two starts of the same sound.
const SPACING: [f32; COUNT] = [
    0.03, 0.035, 0.05, 0.028, 0.08, 0.06, 0.05, 0.07, 0.2, 0.2, 0.08, 0.5, 0.05, 0.02, 0.05, 0.06,
    0.07, 0.3, 0.3, 0.5, 0.5, 0.1, 0.2, 0.03, 0.15,
];

struct Lcg(u32);
impl Lcg {
    fn noise(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.
    }
}

fn render(seconds: f32, mut f: impl FnMut(f32, &mut Lcg) -> f32) -> Sound {
    let frames = (seconds * RATE as f32) as usize;
    let mut rng = Lcg(0x1234_5678);
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        // Short fade-out avoids clicks at the end of every clip.
        let tail = ((seconds - t) / 0.01).clamp(0., 1.);
        let v = (f(t, &mut rng) * tail).clamp(-1., 1.);
        out.push([v, v]);
    }
    Sound::new(Clip {
        sample_rate: RATE,
        samples: out.into(),
    })
    .expect("synthesized clip")
}

fn sine(phase: f32) -> f32 {
    (phase * std::f32::consts::TAU).sin()
}

fn tri(phase: f32) -> f32 {
    let p = phase.fract();
    4. * (p - 0.5).abs() - 1.
}

fn env(t: f32, attack: f32, decay: f32) -> f32 {
    (t / attack).min(1.) * (-decay * t).exp()
}

/// Sweep frequency from `f0` to `f1` exponentially over `dur`: returns phase.
fn sweep_phase(t: f32, f0: f32, f1: f32, dur: f32) -> f32 {
    let k = (f1 / f0).ln() / dur;
    if k.abs() < 1e-6 {
        f0 * t
    } else {
        f0 * ((k * t).exp() - 1.) / k
    }
}

fn notes(t: f32, freqs: &[f32], step: f32, decay: f32, bright: f32) -> f32 {
    let mut v = 0.;
    for (i, &f) in freqs.iter().enumerate() {
        let start = i as f32 * step;
        if t >= start {
            let lt = t - start;
            let e = env(lt, 0.004, decay);
            v += (tri(f * lt) * 0.6 + sine(f * 2. * lt) * 0.25 * bright) * e;
        }
    }
    v
}

fn build() -> Vec<Sound> {
    let mut lp = 0.;
    let mut sounds = Vec::with_capacity(COUNT);
    // Chime: glassy bell partials.
    sounds.push(render(0.32, |t, _| {
        let e = env(t, 0.002, 14.);
        (sine(1568. * t) + 0.4 * sine(3136. * 1.003 * t) + 0.18 * sine(4704. * t)) * e * 0.32
    }));
    // Hit: soft thud + click.
    sounds.push(render(0.07, |t, r| {
        let e = env(t, 0.001, 60.);
        (sine(sweep_phase(t, 260., 110., 0.07)) * 0.6 + r.noise() * 0.3 * (-120. * t).exp())
            * e
            * 0.5
    }));
    // Crit: bright tink.
    sounds.push(render(0.14, |t, _| {
        let e = env(t, 0.001, 32.);
        (sine(2637. * t) + 0.5 * sine(3951. * t)) * e * 0.28
    }));
    // Pop: bubbly descending blip.
    sounds.push(render(0.11, |t, r| {
        let e = env(t, 0.002, 30.);
        (sine(sweep_phase(t, 720., 180., 0.11)) * 0.7 + r.noise() * 0.12 * (-80. * t).exp())
            * e
            * 0.45
    }));
    // PopBig: elite/boss burst.
    sounds.push(render(0.45, |t, r| {
        let e = env(t, 0.002, 7.);
        (sine(sweep_phase(t, 380., 60., 0.45)) * 0.8 + r.noise() * 0.35 * (-14. * t).exp())
            * e
            * 0.6
    }));
    // Zap: buzzy crackle.
    sounds.push(render(0.16, |t, r| {
        let e = env(t, 0.001, 18.);
        let buzz = if (t * 95.).fract() < 0.5 { 1. } else { -1. };
        (buzz * 0.25 + r.noise() * 0.5) * e * 0.35
    }));
    // Whoosh: filtered noise swell.
    sounds.push(render(0.28, |t, r| {
        let shape = (t / 0.28 * std::f32::consts::PI).sin();
        let k = 0.04 + 0.25 * shape;
        lp += (r.noise() - lp) * k;
        lp * shape * 0.9
    }));
    let mut lp2 = 0.;
    // Boom: low thump with rumble.
    sounds.push(render(0.55, |t, r| {
        lp2 += (r.noise() - lp2) * 0.05;
        let e = env(t, 0.003, 6.);
        (sine(sweep_phase(t, 95., 38., 0.55)) * 0.85 + lp2 * 1.6) * e * 0.6
    }));
    // LevelUp: sparkly arpeggio.
    sounds.push(render(0.75, |t, _| {
        notes(t, &[523.25, 659.25, 783.99, 1046.5], 0.075, 6., 1.) * 0.32
    }));
    // Chest: fanfare.
    sounds.push(render(1.1, |t, _| {
        let f = notes(t, &[392., 523.25, 659.25, 783.99, 1046.5], 0.09, 3.5, 1.);
        let shimmer = sine(2093. * t) * 0.12 * env(t - 0.4, 0.2, 3.).max(0.) * f32::from(t > 0.4);
        (f + shimmer) * 0.3
    }));
    // Hurt: squelchy buzz.
    sounds.push(render(0.22, |t, r| {
        let e = env(t, 0.002, 12.);
        let sq = if (sweep_phase(t, 190., 80., 0.22)).fract() < 0.5 {
            1.
        } else {
            -1.
        };
        (sq * 0.35 + r.noise() * 0.2) * e * 0.45
    }));
    let mut lp3 = 0.;
    // Roar: low growl with vibrato.
    sounds.push(render(1.5, |t, r| {
        lp3 += (r.noise() - lp3) * 0.08;
        let vib = 1. + 0.04 * sine(7. * t);
        let saw = (52. * vib * t).fract() * 2. - 1.;
        let e = env(t, 0.15, 1.6);
        (saw * 0.35 + lp3 * 1.2 + sine(sweep_phase(t, 70., 45., 1.5)) * 0.4) * e * 0.55
    }));
    // Coin: two-tone ding.
    sounds.push(render(0.22, |t, _| {
        let f = if t < 0.06 { 1975.5 } else { 2637. };
        sine(f * t) * env(t, 0.001, 16.) * 0.25
    }));
    // Click: UI tick.
    sounds.push(render(0.03, |t, _| {
        sine(1800. * t) * env(t, 0.0005, 120.) * 0.3
    }));
    // Pew: soft wisp shot.
    sounds.push(render(0.08, |t, _| {
        sine(sweep_phase(t, 1300., 520., 0.08)) * env(t, 0.002, 30.) * 0.18
    }));
    // Bubble: rising blip.
    sounds.push(render(0.12, |t, _| {
        sine(sweep_phase(t, 280., 900., 0.12)) * env(t, 0.004, 20.) * 0.3
    }));
    let mut lp4 = 0.;
    // Lash: swish.
    sounds.push(render(0.14, |t, r| {
        lp4 += (r.noise() - lp4) * (0.1 + t * 3.);
        lp4 * env(t, 0.01, 22.) * 1.3
    }));
    // Evolve: rising shimmer sweep.
    sounds.push(render(1.4, |t, _| {
        let p = sweep_phase(t, 220., 1760., 1.2);
        let e = (t / 0.1).min(1.) * ((1.4 - t) / 0.4).clamp(0., 1.);
        (tri(p) * 0.3 + sine(p * 1.5) * 0.2 + sine(p * 2.) * 0.12) * e * 0.4
    }));
    // Warning: wub-wub siren.
    sounds.push(render(0.9, |t, _| {
        let f = if (t * 3.3).fract() < 0.5 { 330. } else { 247. };
        tri(f * t) * 0.3 * (1. - t / 0.9)
    }));
    // Victory: major arpeggio and chord.
    sounds.push(render(1.8, |t, _| {
        notes(
            t,
            &[523.25, 659.25, 783.99, 1046.5, 1318.5, 1568.],
            0.1,
            2.2,
            1.,
        ) * 0.26
    }));
    // Defeat: falling minor line.
    sounds.push(render(1.4, |t, _| {
        notes(t, &[440., 392., 349.23, 293.66], 0.22, 3., 0.4) * 0.3
    }));
    // Heal: soft rising triad.
    sounds.push(render(0.45, |t, _| {
        notes(t, &[587.33, 739.99, 880.], 0.06, 8., 0.6) * 0.22
    }));
    // Magnet: upward whoosh with sparkle.
    sounds.push(render(0.6, |t, _| {
        let p = sweep_phase(t, 300., 2400., 0.6);
        sine(p) * (t / 0.6) * (1. - t / 0.6).max(0.) * 4. * 0.12
    }));
    // Select: chunky confirm.
    sounds.push(render(0.12, |t, _| {
        (tri(660. * t) * 0.5 + tri(990. * t) * 0.3) * env(t, 0.002, 22.) * 0.35
    }));
    // Shimmer: ambient sparkle for tide shifts.
    sounds.push(render(1.2, |t, _| {
        let mut v = 0.;
        for (i, f) in [1046.5, 1318.5, 1568., 2093.].into_iter().enumerate() {
            let start = i as f32 * 0.12;
            if t > start {
                v += sine(f * t) * env(t - start, 0.05, 3.);
            }
        }
        v * 0.12
    }));
    debug_assert_eq!(sounds.len(), COUNT);
    sounds
}

pub struct SoundKit {
    sounds: Vec<Sound>,
    voices: [Option<VoiceHandle>; TRACKED],
    next: usize,
    last: [f32; COUNT],
    time: f32,
    paused: bool,
    score: Option<AdaptiveScore>,
    intensity: f32,
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
            score: AdaptiveScore::new(96, 0x6c75),
            intensity: 0.,
        }
    }

    pub fn age(&mut self, dt: f32) {
        self.time += dt.max(0.);
    }

    /// Play `sfx` if its spacing allows. `pitch` 1.0 = authored.
    pub fn play(&mut self, sfx: Sfx, gain: f32, pitch: f32) {
        let ui = matches!(sfx, Sfx::Click | Sfx::Select);
        if self.paused && !ui {
            return;
        }
        let i = sfx as usize;
        if self.time - self.last[i] < SPACING[i] {
            return;
        }
        self.last[i] = self.time;
        let params = VoiceParams {
            gain: gain.clamp(0., 2.),
            pitch: pitch.clamp(0.25, 4.),
            bus: if ui { Bus::Ui } else { Bus::Effects },
            priority: match sfx {
                Sfx::Hit | Sfx::Pew | Sfx::Pop => 1,
                Sfx::Chime | Sfx::Zap | Sfx::Lash | Sfx::Bubble => 2,
                _ => 5,
            },
            ramp_frames: 0,
            ..VoiceParams::default()
        };
        let Some(handle) = audio::play_sound(&self.sounds[i], params) else {
            return;
        };
        if !ui {
            // The oldest tracked voice keeps playing; the mixer bounds total voices.
            self.voices[self.next] = Some(handle);
            self.next = (self.next + 1) % TRACKED;
        }
    }

    /// Stop gameplay voices and suppress new ones (menus, focus loss).
    pub fn pause(&mut self) {
        if self.paused {
            return;
        }
        self.paused = true;
        self.stop_voices();
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn reset(&mut self) {
        self.stop_voices();
        self.last = [f32::NEG_INFINITY; COUNT];
    }

    fn stop_voices(&mut self) {
        for v in &mut self.voices {
            if let Some(h) = v.take() {
                audio::stop(h, 64);
            }
        }
    }

    /// Adaptive score: `target` 0..=1 swells the second stem with the horde.
    pub fn music(&mut self, dt: f32, target: f32) {
        let target = if self.paused {
            0.
        } else {
            target.clamp(0., 1.)
        };
        self.intensity += (target - self.intensity) * (1. - (-1.5 * dt.max(0.)).exp());
        if let Some(score) = &mut self.score {
            score.update(dt.clamp(0., 0.25), self.intensity.clamp(0., 1.));
        }
    }
}

/// Pentatonic pitch ladder for chained mote pickups.
pub fn chime_pitch(step: u32) -> f32 {
    const LADDER: [f32; 10] = [0., 2., 4., 7., 9., 12., 14., 16., 19., 21.];
    let s = LADDER[(step as usize).min(LADDER.len() - 1)];
    2f32.powf(s / 12.)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_synthesizes_every_sound_and_chimes_climb() {
        let sounds = build();
        assert_eq!(sounds.len(), COUNT);
        assert!(sounds.iter().all(|s| !s.clip().samples.is_empty()));
        assert!(chime_pitch(3) > chime_pitch(1));
        assert_eq!(chime_pitch(99), chime_pitch(9));
    }
}
