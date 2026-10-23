//! A presentation lab with a native window and a display-free assertion mode.
//! Run `cargo run -p ferravex-render --example journey200_hud --offline --locked
//! -j 2 -- --headless`. See docs/JOURNEY_200_PRESENTATION.md for the composition.
//! Preview is a global UI action; this example has no gameplay or simulation.

use ferravex_play::feedback::{FeedbackController, FeedbackEvent, FeedbackPolicy};
use ferravex_render::{
    self as render, BitmapFontMetrics, Mat4, Mesh, Vec2, Vec3, Vertex,
    input::{ActionKind, Binding, HintDevice, InputCode, PlayerInput},
    preferences,
    retained::{self, ResidentMesh, RetainedTelemetry},
    ui::{self, ButtonOptions, ButtonStyle, TextStyle, Ui, UiRect, Viewport},
};

const LABELS: [&str; 4] = ["PREVIEW", "REDUCE MOTION", "REDUCE FLASH", "REBIND PREVIEW"];
const FONT_SIZE: u16 = 16;
const GAP: f32 = 8.;
const EMPTY: UiRect = UiRect {
    min: [0.; 2],
    size: [0.; 2],
};
type Result<T> = std::result::Result<T, String>;

// Keep errors from checked APIs visible, including APIs whose errors implement
// Debug but not std::error::Error. No failure silently becomes a default layout.
fn checked<T, E: std::fmt::Debug>(result: std::result::Result<T, E>) -> Result<T> {
    result.map_err(|error| format!("{error:?}"))
}

#[derive(Clone, Copy)]
struct Layout {
    viewport: Viewport,
    title: UiRect,
    hint: UiRect,
    buttons: [UiRect; 4],
    diagnostics: UiRect,
    preview: UiRect,
}

fn row(remaining: &mut UiRect, height: f32) -> UiRect {
    let result = UiRect {
        size: [remaining.size[0], height.min(remaining.size[1])],
        ..*remaining
    };
    let consumed = (height + GAP).min(remaining.size[1]);
    remaining.min[1] += consumed;
    remaining.size[1] -= consumed;
    result
}

// Pure layout: host logical pixels, finite checked dimensions, no renderer I/O.
// Metrics match these ASCII labels at scale 1; UI fallback/wrapping is separate.
fn layout(screen: [f32; 2], metrics: BitmapFontMetrics) -> Result<Layout> {
    let viewport = checked(Viewport::responsive(screen, [12.; 4]))?;
    let mut remaining = viewport.bounds();
    let title = row(&mut remaining, metrics.line_height() + 4.);
    let hint = row(&mut remaining, metrics.line_height() + 4.);
    let mut minimum_width = 1_f32;
    for label in LABELS {
        minimum_width = minimum_width.max(checked(metrics.measure_line(label))?.width + 16.);
    }
    let mut buttons = [EMPTY; 4];
    for (target, rect) in buttons.iter_mut().zip(checked(ui::hud_slots(
        remaining,
        LABELS.len(),
        minimum_width,
        44.,
        GAP,
    ))?) {
        *target = rect;
    }
    let last = buttons[3];
    let controls_height = last.min[1] + last.size[1] - remaining.min[1];
    row(&mut remaining, controls_height);
    let diagnostics = row(&mut remaining, 3. * metrics.line_height() + 2. * GAP);
    Ok(Layout {
        viewport,
        title,
        hint,
        buttons,
        diagnostics,
        preview: remaining,
    })
}

fn preview_bindings(key: u16) -> Vec<Binding> {
    vec![
        Binding::digital(InputCode::Key(key)),
        Binding::digital(InputCode::PadButton(2)),
    ]
}

struct Presentation {
    input: PlayerInput,
    feedback: FeedbackController<8>,
    ui: Ui,
    metrics: BitmapFontMetrics,
    mesh: ResidentMesh,
    event_id: u64,
    alternate_key: bool,
}

impl Presentation {
    fn new() -> Result<Self> {
        // No saved profile/preferences are read or written by this lab.
        let mut input = PlayerInput::transient("journey200-hud");
        checked(input.global_action(
            "preview",
            "Preview impact",
            ActionKind::Button,
            preview_bindings(32),
        ))?;
        let mesh = Mesh {
            vertices: [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
                .map(|(x, y)| Vertex::new2(Vec3::new(x, y, 0.), Vec2::ZERO, ui::CYAN))
                .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: None,
        };
        let mut ui = Ui::new();
        ui.set_auto_focus(false);
        Ok(Self {
            input,
            feedback: checked(FeedbackController::new(Default::default(), 180))?,
            ui,
            metrics: checked(render::get_default_font().try_metrics(FONT_SIZE, 1.))?,
            mesh: checked(ResidentMesh::try_new(&mesh))?,
            event_id: 0,
            alternate_key: false,
        })
    }

    fn rebind(&mut self) -> Result<()> {
        let alternate = !self.alternate_key;
        checked(self.input.rebind(
            "preview",
            preview_bindings(if alternate { 106 } else { 32 }),
        ))?;
        self.alternate_key = alternate;
        Ok(())
    }

    fn apply_policy(&mut self, motion: bool, flash: bool) -> Result<()> {
        let policy = checked(FeedbackPolicy::new(
            motion,
            flash,
            preferences::feedback_policy().intensity(),
        ))?;
        // Start from actual underlying choices; never save an effective override.
        checked(preferences::try_apply_with_feedback(
            preferences::current(),
            policy,
        ))?;
        preferences::sync_feedback(&mut self.feedback);
        Ok(())
    }

    fn preview(&mut self) -> Result<()> {
        preferences::sync_feedback(&mut self.feedback);
        let id = self
            .event_id
            .checked_add(1)
            .ok_or("preview event IDs exhausted")?;
        let event = checked(FeedbackEvent::hurt(Vec3::ZERO, 0.7))?;
        if checked(self.feedback.emit(id, event))?.is_some() {
            self.event_id = id;
        }
        Ok(())
    }

    fn controls(&mut self, layout: Layout) -> Result<()> {
        self.ui.begin(layout.viewport);
        let policy = preferences::feedback_policy();
        for (index, (label, rect)) in LABELS.into_iter().zip(layout.buttons).enumerate() {
            let selected = match index {
                1 => policy.reduced_motion(),
                2 => policy.reduced_flash(),
                3 => self.alternate_key,
                _ => false,
            };
            checked(self.ui.hud_button_with_options(
                index as u64,
                rect,
                label,
                ButtonOptions {
                    enabled: true,
                    selected,
                },
                ButtonStyle {
                    size: FONT_SIZE as f32,
                    ..Default::default()
                },
            ))?;
        }
        Ok(())
    }

    fn paint(&mut self, layout: Layout, dt: f32) -> Result<RetainedTelemetry> {
        // Also runs on idle/menu frames. Clearing policy channels precedes both
        // sampling and drawing; presentation delta never controls game time.
        preferences::sync_feedback(&mut self.feedback);
        let (_, impact) = checked(self.feedback.advance_with_impact(dt))?;
        let preview = layout.preview;
        if preview.size.into_iter().all(|size| size > 0.) {
            let origin = layout.viewport.pixels().min;
            let x = origin[0] + preview.min[0];
            let y = origin[1] + preview.min[1];
            render::draw_rectangle(x, y, preview.size[0], preview.size[1], ui::PANEL);
            // 0.5 * 1.3 leaves the whole pulse inside the preview rectangle.
            let size = preview.size[0].min(preview.size[1]) * 0.5 * (1. + 0.3 * impact.emphasis);
            let center = Vec3::new(x + preview.size[0] * 0.5, y + preview.size[1] * 0.5, 0.);
            checked(retained::draw_resident_checked(
                &self.mesh,
                Mat4::from_translation(center) * Mat4::from_scale(Vec3::new(size, size, 1.)),
            ))?;
            if impact.flash > 0. {
                render::draw_rectangle(
                    x,
                    y,
                    preview.size[0],
                    preview.size[1],
                    render::Color::new(1., 0.7, 0.2, impact.flash * 0.25),
                );
            }
        }
        let sample = retained::telemetry_snapshot();
        let style = TextStyle {
            size: FONT_SIZE as f32,
            ..Default::default()
        };
        checked(self.ui.label(layout.title, "PRESENTATION LAB", style))?;
        let mut bytes = [0_u8; 128];
        let hint = checked(self.input.format_hint("preview", &mut bytes))?;
        // The fixed action has two short bindings. If expanded, this error must
        // remain visible rather than silently replacing a truncated control hint.
        checked(
            self.ui
                .text_fmt(layout.hint, style, format_args!("Preview: {hint}")),
        )?;
        let mut diagnostics = layout.diagnostics;
        checked(self.ui.text_fmt(
            row(&mut diagnostics, self.metrics.line_height()),
            style,
            format_args!("CPU meshes {} / {}", sample.live_meshes, sample.mesh_limit),
        ))?;
        checked(self.ui.text_fmt(
            row(&mut diagnostics, self.metrics.line_height()),
            style,
            format_args!("Instances {}  batches {}", sample.instances, sample.draws),
        ))?;
        checked(self.ui.text_fmt(
            row(&mut diagnostics, self.metrics.line_height()),
            style,
            format_args!("Draw rejects {}", sample.rejections.draw_validation),
        ))?;
        self.ui.paint();
        Ok(sample)
    }
}

fn verify_layout(layout: Layout) {
    let bounds = layout.viewport.bounds();
    let mut rects = vec![layout.title, layout.hint];
    rects.extend(layout.buttons);
    rects.extend([layout.diagnostics, layout.preview]);
    for (index, rect) in rects.iter().enumerate() {
        assert!(rect.valid());
        for axis in 0..2 {
            assert!(rect.min[axis] >= bounds.min[axis]);
            assert!(rect.min[axis] + rect.size[axis] <= bounds.size[axis] + 0.001);
        }
        for other in &rects[index + 1..] {
            assert!(rect.intersect(*other).is_none(), "HUD regions overlap");
        }
    }
}

fn headless() -> Result<()> {
    let mut presentation = Presentation::new()?;
    let reserved = presentation.ui.reserved_bytes();
    for screen in [
        [960., 640.],
        [320., 568.],
        [568., 320.],
        [120., 120.],
        [1., 1.],
        [0., 0.],
    ] {
        let layout = layout(screen, presentation.metrics)?;
        verify_layout(layout);
        render::clear_background(ui::BACKGROUND);
        presentation.controls(layout)?;
        presentation.ui.finish(false);
        let sample = presentation.paint(layout, 0.)?;
        assert_eq!(
            sample.instances,
            usize::from(layout.preview.size.into_iter().all(|v| v > 0.))
        );
        assert_eq!(sample.rejections.draw_validation, 0);
        assert_eq!(presentation.ui.reserved_bytes(), reserved);
        println!(
            "{screen:?}: contained HUD; {} retained instance(s)",
            sample.instances
        );
    }
    let mut bytes = [0; 128];
    assert_eq!(
        presentation
            .input
            .format_hint("preview", &mut bytes)
            .unwrap(),
        "Space"
    );
    presentation.rebind()?;
    assert_eq!(
        presentation
            .input
            .format_hint("preview", &mut bytes)
            .unwrap(),
        "J"
    );
    assert_eq!(
        presentation
            .input
            .format_hint_for("preview", Some(HintDevice::Touch), &mut bytes)
            .unwrap(),
        "Unbound"
    );
    presentation.apply_policy(false, false)?;
    presentation.preview()?;
    assert!(presentation.feedback.impact().emphasis > 0.);
    assert!(presentation.feedback.impact().flash > 0.);
    presentation.apply_policy(true, false)?;
    assert_eq!(presentation.feedback.impact().emphasis, 0.);
    assert!(presentation.feedback.impact().flash > 0.);
    presentation.apply_policy(false, false)?;
    assert_eq!(
        presentation.feedback.impact().emphasis,
        0.,
        "cleared pulse must not revive"
    );
    presentation.apply_policy(false, true)?;
    presentation.preview()?;
    assert!(presentation.feedback.impact().emphasis > 0.);
    assert_eq!(presentation.feedback.impact().flash, 0.);
    presentation.apply_policy(true, true)?;
    assert_eq!(presentation.feedback.impact(), Default::default());
    let before = preferences::current();
    let policy = preferences::feedback_policy();
    assert!(
        preferences::try_apply_with_feedback(
            preferences::Preferences {
                master: f32::NAN,
                ..before
            },
            FeedbackPolicy::default()
        )
        .is_err()
    );
    assert_eq!(preferences::current(), before);
    assert_eq!(preferences::feedback_policy(), policy);
    println!(
        "Live hints, independent policy, cleared effects and invalid-snapshot rollback passed."
    );
    Ok(())
}

async fn window(frame_limit: Option<u32>) -> Result<()> {
    let mut presentation = Presentation::new()?;
    let mut frames = 0_u32;
    loop {
        presentation.input.sample(false); // Only global UI actions are enabled.
        preferences::sync_feedback(&mut presentation.feedback);
        let layout = layout(
            [render::screen_width(), render::screen_height()],
            presentation.metrics,
        )?;
        render::clear_background(ui::BACKGROUND);
        render::set_default_camera();
        presentation.controls(layout)?;
        let enabled = !presentation.input.controls_open();
        presentation.ui.finish(enabled);
        let shortcut = presentation.input.pressed("preview");
        if enabled {
            if presentation.ui.activated(3) {
                presentation.rebind()?;
            }
            let policy = preferences::feedback_policy();
            let motion = policy.reduced_motion() ^ presentation.ui.activated(1);
            let flash = policy.reduced_flash() ^ presentation.ui.activated(2);
            if motion != policy.reduced_motion() || flash != policy.reduced_flash() {
                presentation.apply_policy(motion, flash)?;
            }
            if shortcut || presentation.ui.activated(0) {
                presentation.preview()?;
            }
        }
        // The queued button styling reflects changes on the next frame. The
        // effect policy and hint below already observe the accepted action.
        presentation.paint(layout, render::get_frame_time().clamp(0., 0.25))?;
        frames = frames.saturating_add(1);
        if frame_limit.is_some_and(|limit| frames >= limit) {
            return Ok(());
        }
        render::next_frame().await;
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Help,
    Headless,
    Window(Option<u32>),
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Mode> {
    let mut args = args.into_iter();
    let mode = match args.next().as_deref() {
        None => Mode::Window(None),
        Some("--help") => Mode::Help,
        Some("--headless") => Mode::Headless,
        Some("--frames") => Mode::Window(Some(
            args.next()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value > 0)
                .ok_or("--frames requires a positive u32")?,
        )),
        Some(other) => return Err(format!("unknown argument: {other}")),
    };
    if args.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    Ok(mode)
}

fn main() {
    let result = parse(std::env::args().skip(1)).and_then(|mode| match mode {
        Mode::Help => {
            println!(concat!(
                "Journey200 HUD\n",
                "  (no arguments)  open native presentation lab\n",
                "  --headless      run geometry and composition assertions without a display\n",
                "  --frames N      close native window after N frames\n",
                "  --help          show this help"
            ));
            Ok(())
        }
        Mode::Headless => headless(),
        Mode::Window(limit) => {
            #[cfg(not(target_arch = "wasm32"))]
            render::run(
                render::Conf {
                    window_title: "Journey200 Presentation Lab".into(),
                    window_width: 960,
                    window_height: 640,
                    ..Default::default()
                },
                async move {
                    if let Err(error) = window(limit).await {
                        eprintln!("journey200_hud: {error}");
                        std::process::exit(1);
                    }
                },
            );
            #[cfg(target_arch = "wasm32")]
            {
                let _ = limit;
                return Err("this Cargo example has no browser host".into());
            }
            Ok(())
        }
    });
    if let Err(error) = result {
        eprintln!("journey200_hud: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_scenarios() {
        headless().unwrap();
    }

    #[test]
    fn layout_reflows_without_changing_font_size_and_rejects_invalid_extents() {
        let metrics = render::get_default_font()
            .try_metrics(FONT_SIZE, 1.)
            .unwrap();
        let wide = layout([960., 640.], metrics).unwrap();
        assert_eq!(wide.buttons[0].min[1], wide.buttons[3].min[1]);
        let narrow = layout([320., 568.], metrics).unwrap();
        for pair in narrow.buttons.windows(2) {
            assert!(pair[1].min[1] > pair[0].min[1]);
        }
        assert!(narrow.buttons.iter().all(|rect| rect.size[1] == 44.));
        assert_eq!(metrics.line_height(), 16.);
        for size in [[-1., 50.], [f32::NAN, 50.], [50., f32::INFINITY]] {
            assert!(layout(size, metrics).is_err());
        }
    }

    #[test]
    fn hint_capacity_error_preserves_the_callers_buffer_and_pad_binding() {
        let mut presentation = Presentation::new().unwrap();
        let mut short = [b'!'; 2];
        assert!(
            presentation
                .input
                .format_hint("preview", &mut short)
                .is_err()
        );
        assert_eq!(short, [b'!'; 2]);
        let mut bytes = [0; 128];
        let pad = presentation
            .input
            .format_hint_for("preview", Some(HintDevice::Gamepad), &mut bytes)
            .unwrap()
            .to_owned();
        presentation.rebind().unwrap();
        assert_eq!(
            presentation
                .input
                .format_hint_for("preview", Some(HintDevice::Gamepad), &mut bytes)
                .unwrap(),
            pad
        );
    }

    #[test]
    fn arguments_are_explicit_and_bounded() {
        let args = |values: &[&str]| parse(values.iter().map(|value| (*value).to_owned()));
        assert_eq!(args(&[]).unwrap(), Mode::Window(None));
        assert_eq!(args(&["--headless"]).unwrap(), Mode::Headless);
        assert_eq!(args(&["--frames", "2"]).unwrap(), Mode::Window(Some(2)));
        for values in [
            vec!["--frames"],
            vec!["--frames", "0"],
            vec!["--frames", "4294967296"],
            vec!["--headless", "extra"],
            vec!["--unknown"],
        ] {
            assert!(args(&values).is_err());
        }
    }
}
