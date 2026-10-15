//! Offline demonstration of original surface, weapon and variable-pitch sounds.
use ferravex_play::audio::*;
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/ferravex-audio-bank.wav".into());
    let bank = SoundBank::procedural(24000).unwrap();
    let mut mixer = Mixer::new(48000, 32).unwrap();
    let mut output = Vec::new();
    let mut block = [[0.; 2]; 1024];
    mixer.set_master_gain(0.7);
    for cue in [
        Cue::Footstep(Surface::Stone),
        Cue::Footstep(Surface::Metal),
        Cue::Footstep(Surface::Wood),
        Cue::Footstep(Surface::Sand),
        Cue::Footstep(Surface::Grass),
        Cue::Footstep(Surface::Water),
        Cue::Impact(Surface::Metal),
        Cue::Fire,
        Cue::Reload,
        Cue::Pickup,
    ] {
        mixer.start(bank.get(cue), VoiceParams::default());
        for _ in 0..20 {
            mixer.render(&mut block);
            output.extend_from_slice(&block);
        }
    }
    let engine = mixer
        .start(
            bank.get(Cue::Engine),
            VoiceParams {
                looping: true,
                bus: Bus::Ambience,
                ..Default::default()
            },
        )
        .unwrap();
    for i in 0..100 {
        mixer.set_params(
            engine,
            VoiceParams {
                looping: true,
                gain: 0.5,
                pitch: 0.6 + i as f32 * 0.02,
                ..Default::default()
            },
        );
        mixer.render(&mut block);
        output.extend_from_slice(&block);
    }
    mixer.stop(engine, 512);
    mixer.render(&mut block);
    output.extend_from_slice(&block);
    std::fs::write(&path, encode_wav(48000, &output).unwrap()).unwrap();
    println!(
        "{} frames / {:.2}s / {} source-bank bytes -> {path}",
        output.len(),
        output.len() as f64 / 48000.,
        bank.decoded_bytes()
    );
}
