use ferravex_play::audio::*;
#[test]
fn room_filter_audibly_attenuates_high_frequencies_without_allocation_in_render() {
    let sound = Sound::new(Clip {
        sample_rate: 8000,
        samples: (0..256)
            .map(|i| [if i % 2 == 0 { 0.5 } else { -0.5 }; 2])
            .collect::<Vec<_>>()
            .into(),
    })
    .unwrap();
    let mut clear = Mixer::new(8000, 2).unwrap();
    let mut room = Mixer::new(8000, 2).unwrap();
    let p = VoiceParams {
        ramp_frames: 0,
        ..Default::default()
    };
    clear.start(&sound, p).unwrap();
    room.start(
        &sound,
        VoiceParams {
            low_pass_hz: 100.,
            ..p
        },
    )
    .unwrap();
    let mut a = [[0.; 2]; 128];
    let mut b = a;
    clear.render(&mut a);
    room.render(&mut b);
    let energy = |v: &[[f32; 2]]| v.iter().map(|s| s[0] * s[0]).sum::<f32>();
    assert!(energy(&b) < energy(&a) * 0.01);
    assert_eq!(a[0], [0.5; 2]);
}
