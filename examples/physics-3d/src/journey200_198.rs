use super::*;

#[test]
fn narrow_inspector_reserves_complete_fixed_size_fields_and_readable_buttons() {
    for width in [296., 320., 366., 390., 516.] {
        let (panel, buttons) = footer_layout(width, 640.);
        let text_width = panel.w - 24.;
        for text in [
            "PLAYING 180/180 tick 18446744073709551615",
            "Contacts 21528 | awake 180 | sleep 180",
            "Ray 8/214 capped | near 60.0m",
            "Body limit reached. RESET yard.",
            "Resume to pulse. DROP/STEP work.",
            "Stepped 8.33 ms; still paused.",
        ] {
            let lines: Vec<_> = hud_lines(text, text_width, 14).collect();
            assert!(lines.len() <= 2, "{width}: {text}: {lines:?}");
            for line in lines {
                assert!(
                    get_default_font()
                        .try_metrics(14, 1.)
                        .unwrap()
                        .measure_line(line)
                        .unwrap()
                        .width
                        <= text_width
                );
            }
        }
        for (button, label) in buttons.iter().zip([
            "CRATE", "BALL", "DROP", "PULSE", "RESUME", "STEP", "VIEW", "RESET",
        ]) {
            let advance = get_default_font()
                .try_metrics(17, 1.)
                .unwrap()
                .measure_line(label)
                .unwrap()
                .width;
            let size = 17. * ((button.w - 12.) / advance).min(1.);
            assert!(size >= 14., "{width}: {label} at {size}");
            assert!(button.y >= panel.y + 44.);
            assert!(button.y + button.h <= panel.y + panel.h);
        }
        assert!(scene_rect(width, panel).h >= 190.);
    }
}

#[test]
fn oversized_and_unbound_binding_hints_are_complete_or_explicit() {
    let mut input = PlayerInput::transient("journey200-198-hints");
    input
        .action("step", "Step", ActionKind::Button, Vec::new())
        .unwrap();
    let mut buffer = [0; 128];
    let unbound = binding_hint(&input, "step", &mut buffer);
    assert_eq!(unbound, "Unbound");
    let copy = format!("{unbound}: step");
    assert_eq!(inspector_hint(&copy, 248., true), copy);
    let long = "PageUp / PageDown / Enter / Space: drop crate | Unbound: pulse recharging";
    assert_eq!(inspector_hint(long, 248., true), "Bindings: Controls");
    assert_eq!(inspector_hint(long, 248., false), "Bindings: Controls");
    assert_eq!(
        hud_lines("éééééééé", 21., 14).collect::<Vec<_>>(),
        vec!["éé", "éé", "éé", "éé"]
    );
}

#[test]
fn narrow_world_and_scene_input_share_reserved_band_without_changing_pose_or_depth() {
    let mut lab = Lab::new();
    let pose = lab.orbit.view(0.);
    let camera = Camera3D {
        position: pose.position,
        target: pose.target,
        up: pose.up,
        fovy: pose.fov_y,
        z_near: 0.1,
        z_far: 100.,
        ..Default::default()
    };
    for width in [296., 320., 366., 390.] {
        let height = 640.;
        let (panel, _) = footer_layout(width, height);
        let scene = scene_rect(width, panel);
        assert_eq!(scene.y, 44.);
        assert_eq!(scene.y + scene.h, panel.y);
        assert!(!scene.contains(vec2(20., 30.)));
        assert!(!scene.contains(vec2(20., panel.y + 1.)));
        let fitted = SceneCamera { camera, scene };
        assert!(fitted.depth());
        assert_eq!(fitted.position(), Some(pose.position));
        assert_eq!(
            (
                fitted.camera.target,
                fitted.camera.z_near,
                fitted.camera.z_far
            ),
            (pose.target, 0.1, 100.)
        );
        let matrix = fitted.matrix(width, height);
        // All authored dynamic centers and mechanism anchors fit the reserved
        // scene at the unchanged default orbit. This is projection geometry;
        // browser captures separately establish actual rendered visibility.
        let points = lab
            .sandbox
            .bodies
            .iter()
            .map(|b| lab.sandbox.physics.bodies[b.handle].translation())
            .chain(
                lab.sandbox
                    .links
                    .iter()
                    .flat_map(|id| lab.sandbox.physics.joint_anchors(*id).unwrap()),
            );
        for point in points {
            let clip = matrix * point.extend(1.);
            let screen = vec2(
                (clip.x / clip.w + 1.) * width / 2.,
                (1. - clip.y / clip.w) * height / 2.,
            );
            assert!(
                scene.contains(screen),
                "{width}: {point:?} -> {screen:?}, {scene:?}"
            );
        }
    }
    let (panel, _) = footer_layout(1280., 608.);
    let fitted = SceneCamera {
        camera,
        scene: scene_rect(1280., panel),
    };
    assert_eq!(fitted.matrix(1280., 608.), camera.matrix(1280., 608.));
}
