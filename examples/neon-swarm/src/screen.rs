//! Game-owned screen composition. World fitting and simulation stay independent.
use crate::{CORAL, CYAN, FLOOR, INK, MUTED, PAPER, game::*, hint};
use ferravex_render::{
    canvas::Canvas2D,
    input::{
        PlayerInput,
        touch::{Control, Output, Region},
    },
    prelude::*,
    ui::{Align, Anchor, ButtonStyle, TextStyle, Ui, UiRect, Viewport, hud_slots},
};

const TEXT: f32 = 14.;
const ROW: f32 = 20.;

pub(crate) struct ScreenLayout {
    pub canvas: Canvas2D,
    pub compact: bool,
    pub viewport: Viewport,
    header: UiRect,
    #[cfg(test)]
    hints: UiRect,

    card: UiRect,
    action: UiRect,
    dock: Option<UiRect>,
}

impl ScreenLayout {
    pub fn new(screen: [f32; 2]) -> Self {
        // Host dimensions are logical pixels (CSS pixels on the web). DPR only
        // changes the GPU framebuffer, never this viewport or pointer coordinates.
        let screen = screen.map(|v| {
            if v.is_finite() {
                v.clamp(1., 65536.)
            } else {
                1.
            }
        });
        let canvas = Canvas2D::fit(vec2(1280., 800.), Vec2::from(screen));
        let compact = screen[0] < 900. || screen[1] < 480.;
        let viewport = if compact {
            Viewport::responsive(screen, [0.; 4]).unwrap()
        } else {
            Viewport::fit(screen, [1280., 800.], [0.; 4]).unwrap()
        };
        let bounds = UiRect {
            min: [0.; 2],
            size: screen,
        };
        let safe = bounds.inset([8.; 4]).unwrap();
        let arena_bottom = canvas.project(vec2(ARENA.x, ARENA.bottom())).y;
        // Keep the entire arena visible in portrait. In a very short window,
        // clip secondary rows rather than shrink text or cover the whole arena.
        let header_height = 80_f32.min(safe.size[1]);
        let header = UiRect {
            min: safe.min,
            size: [safe.size[0], header_height],
        };
        let hints_y = (arena_bottom + 8.).min(screen[1] - safe.min[1]);
        let hints = UiRect {
            min: [safe.min[0], hints_y],
            size: [safe.size[0], (screen[1] - safe.min[1] - hints_y).max(0.)],
        };
        let dock = (compact && screen[0] >= 240. && hints.size[1] >= 156.).then(|| {
            Anchor::BottomCenter
                .place(hints, [hints.size[0], 96.])
                .unwrap()
        });
        let card = Anchor::Center.place(safe, [440., 208.]).unwrap();
        let inside = card.inset([12.; 4]).unwrap();
        let action_height = 48_f32.min(inside.size[1]);
        let action = UiRect {
            min: [
                inside.min[0],
                inside.min[1] + (inside.size[1] - 56.).max(0.),
            ],
            size: [inside.size[0], action_height],
        };
        Self {
            canvas,
            compact,
            viewport,
            header,
            #[cfg(test)]
            hints,
            card,
            action,
            dock,
        }
    }

    /// The same geometry feeds both the default touch surface and its drawing.
    /// Output channels/order match the existing saved touch-layout slots.
    pub fn touch_controls(&self) -> [Control; 2] {
        if let Some(dock) = self.dock {
            let center = vec2(dock.min[0] + 48., dock.min[1] + 48.);
            return [
                Control {
                    region: Region::Circle {
                        center,
                        radius: 44.,
                    },
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 36.,
                        floating: false,
                    },
                },
                Control {
                    region: Region::Rect(Rect::new(
                        dock.min[0] + dock.size[0] - 104.,
                        dock.min[1] + 24.,
                        96.,
                        56.,
                    )),
                    output: Output::Button(7),
                },
            ];
        }
        let canvas = self.canvas;
        [
            Control {
                region: Region::Circle {
                    center: canvas.project(vec2(202., 600.)),
                    radius: 210. * canvas.scale,
                },
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: (66. * canvas.scale).max(0.001),
                    floating: false,
                },
            },
            Control {
                region: Region::Rect(Rect::new(
                    canvas.project(vec2(640., 0.)).x,
                    canvas.offset.y,
                    640. * canvas.scale,
                    800. * canvas.scale,
                )),
                output: Output::Button(7),
            },
        ]
    }

    pub fn draw_touch(&self, movement: Vec2) -> bool {
        let Some(_) = self.dock else {
            return false;
        };
        let [stick, button] = self.touch_controls();
        let Region::Circle { center, radius } = stick.region else {
            unreachable!()
        };
        let Region::Rect(button) = button.region else {
            unreachable!()
        };
        set_default_camera();
        draw_circle_lines(center.x, center.y, radius, 2., CYAN);
        let knob = center + movement * 24.;
        draw_circle(knob.x, knob.y, 12., CYAN);
        draw_rectangle(button.x, button.y, button.w, button.h, FLOOR);
        draw_rectangle_lines(button.x, button.y, button.w, button.h, 2., CYAN);
        crate::label("DASH", button.x + 11., button.y + 34., 14, CYAN);
        set_camera(&self.canvas.camera());
        ferravex_render::set_world_colors(false);
        true
    }

    pub fn hud(&self, ui: &mut Ui, game: &Game, _input: &PlayerInput) {
        ui.push_clip(self.header).unwrap();
        ui.panel(self.header, INK).unwrap();
        let inner = self.header.inset([8.; 4]).unwrap();
        let y = inner.min[1];
        line(
            ui,
            rect(inner.min[0], y, inner.size[0] - 64., 20.),
            &format!("WAVE {}/3", game.wave),
            TEXT,
            CYAN,
        );
        let stats = rect(inner.min[0], y + 20., inner.size[0], 40.);
        let mut slots = hud_slots(stats, 4, ((inner.size[0] - 8.) * 0.5).max(1.), ROW, 0.).unwrap();
        let cells: [UiRect; 4] = std::array::from_fn(|_| slots.next().unwrap());
        // Fixed-size labels ellipsize at absurdly small widths; no font fitting.
        line(
            ui,
            cells[0],
            &format!("HEALTH {}", game.health),
            15.,
            if game.health <= 30 { CORAL } else { CYAN },
        );
        line(
            ui,
            cells[1],
            &format!("SHIELD {}", game.shield.value().ceil() as u32),
            15.,
            CYAN,
        );
        let ammo = if game.magazine.reloading() {
            format!("RELOAD {:.1}s", game.magazine.remaining())
        } else {
            format!("AMMO {}/12", game.magazine.loaded())
        };
        line(ui, cells[2], &ammo, TEXT, PAPER);
        let dash = game.dash_availability();
        let dash = if dash.check().is_ok() {
            "DASH READY".to_owned()
        } else if game.phase != Phase::Playing {
            "DASH OFFLINE".to_owned()
        } else {
            format!("DASH {:.1}s", dash.cooldown_remaining)
        };
        line(ui, cells[3], &dash, TEXT, CYAN);
        ui.pop_clip();
    }

    pub fn overlay(
        &self,
        ui: &mut Ui,
        title: &str,
        detail: &str,
        action: &str,
        input: &PlayerInput,
    ) {
        ui.panel(self.card, Color::new(0.025, 0.055, 0.07, 0.97))
            .unwrap();
        ui.push_clip(self.card).unwrap();
        let inner = self.card.inset([12.; 4]).unwrap();
        if inner.size[1] >= 96. {
            ui.label(
                rect(inner.min[0], inner.min[1] + 4., inner.size[0], 28.),
                title,
                TextStyle {
                    size: 24.,
                    color: PAPER,
                    align: Align::Center,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        if inner.size[1] >= 140. {
            ui.text(
                rect(inner.min[0], inner.min[1] + 42., inner.size[0], 44.),
                detail,
                TextStyle {
                    size: 16.,
                    color: PAPER,
                    align: Align::Center,
                    wrap: true,
                    ..Default::default()
                },
            )
            .unwrap();
            if title == "PAUSED" {
                binding(
                    ui,
                    rect(inner.min[0], inner.min[1] + 96., inner.size[0], ROW),
                    input,
                    "restart",
                    "RESTART",
                );
            }
        }
        ui.hud_button(
            1,
            self.action,
            action,
            true,
            ButtonStyle {
                size: 16.,
                fill: FLOOR,
                ink: CYAN,
                focus: CYAN,
                ..Default::default()
            },
        )
        .unwrap();
        ui.pop_clip();
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> UiRect {
    UiRect {
        min: [x, y],
        size: [width.max(0.), height.max(0.)],
    }
}

fn line(ui: &mut Ui, rect: UiRect, text: &str, size: f32, color: Color) {
    ui.label(
        rect,
        text,
        TextStyle {
            size,
            color,
            ..Default::default()
        },
    )
    .unwrap();
}

fn binding(ui: &mut Ui, rect: UiRect, input: &PlayerInput, action: &str, name: &str) {
    let mut buffer = [0; 128];
    line(
        ui,
        rect,
        &format!("{name}: {}", hint(input, action, &mut buffer)),
        TEXT,
        MUTED,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::input::{
        ActionMap, Binding, InputCode,
        touch::{Event, Phase as TouchPhase, TouchSurface},
    };
    use ferravex_render::input::{host_frame, touch_event};

    const PORTRAIT: [[f32; 2]; 4] = [[320., 533.], [390., 608.], [320., 740.], [390., 844.]];

    fn inside(outer: UiRect, inner: UiRect) {
        assert!(inner.valid());
        for axis in 0..2 {
            assert!(inner.min[axis] >= outer.min[axis] - 0.001);
            assert!(
                inner.min[axis] + inner.size[axis] <= outer.min[axis] + outer.size[axis] + 0.001
            );
        }
    }

    #[test]
    fn portrait_uses_letterboxes_for_readable_hud_and_large_touch_targets() {
        // First two heights are the rounded 72svh canvases at 320x740/390x844.
        for size in PORTRAIT {
            let layout = ScreenLayout::new(size);
            assert!(layout.compact);
            assert_eq!(layout.viewport.scale(), 1.);
            assert_eq!(layout.viewport.bounds().size, size);
            assert_eq!(layout.header.size[1], 80.);
            assert!(layout.hints.size[1] >= 156.);
            assert!(
                layout.header.min[1] + layout.header.size[1] + 8.
                    <= layout.canvas.project(vec2(ARENA.x, ARENA.y)).y + 0.001
            );
            assert!(
                layout.hints.min[1]
                    >= layout.canvas.project(vec2(ARENA.x, ARENA.bottom())).y + 7.99
            );
            for rect in [
                layout.header,
                layout.hints,
                layout.card,
                layout.action,
                layout.dock.unwrap(),
            ] {
                inside(layout.viewport.bounds(), rect);
            }
            assert!(layout.action.size[0] >= 44. && layout.action.size[1] >= 44.);
            let controls = layout.touch_controls();
            let report = ferravex_play::input::touch::TouchLayoutSettings::diagnose_layout(
                &controls,
                Vec2::from(size),
                Rect::new(0., layout.hints.min[1], size[0], layout.hints.size[1]),
            )
            .unwrap();
            assert!(
                report.is_clear(),
                "dock must neither overlap nor cover play"
            );
            let Region::Rect(button) = controls[1].region else {
                panic!()
            };
            assert!(button.w >= 44. && button.h >= 44.);
            let metrics = get_default_font().try_metrics(TEXT as u16, 1.).unwrap();
            assert_eq!(metrics.line_height(), 14.);
            // Useful current hints stay whole at the narrowest supported width.
            for hint in [
                "FIRE: MOUSE 1",
                "DASH: SPACE",
                "PAUSE: ESCAPE",
                "F2 / TOOLBAR: CONTROLS",
            ] {
                assert!(metrics.measure_line(hint).unwrap().width <= layout.hints.size[0] - 16.);
            }
        }
    }

    #[test]
    fn world_camera_and_pointer_mapping_are_unchanged_across_layout_and_dpr() {
        for size in
            PORTRAIT
                .into_iter()
                .chain([[899., 600.], [900., 600.], [1280., 800.], [1280., 576.]])
        {
            let expected = Canvas2D::fit(vec2(1280., 800.), Vec2::from(size));
            for dpr in [1., 2., 3.] {
                ferravex_render::set_display_scale(dpr);
                let layout = ScreenLayout::new(size);
                assert_eq!(layout.canvas.scale, expected.scale);
                assert_eq!(layout.canvas.offset, expected.offset);
                let matrix = layout.canvas.camera().matrix(size[0], size[1]);
                for world in [vec2(72., 112.), vec2(640., 414.), vec2(1208., 716.)] {
                    let logical = layout.canvas.project(world);
                    assert!(layout.canvas.unproject(logical).distance(world) < 0.001);
                    let ndc = matrix.project_point3(world.extend(0.));
                    let framebuffer = vec2(
                        (ndc.x + 1.) * size[0] * dpr * 0.5,
                        (1. - ndc.y) * size[1] * dpr * 0.5,
                    );
                    assert!((framebuffer / dpr).distance(logical) < 0.001);
                }
                if layout.compact {
                    assert_eq!(layout.viewport.unproject([30., 40.]), Some([30., 40.]));
                } else {
                    assert_eq!(layout.viewport.scale(), expected.scale);
                }
            }
        }
        ferravex_render::set_display_scale(1.);
    }

    #[test]
    fn actual_menu_touch_activation_tracks_css_rect_and_respects_controls_blocking() {
        for size in PORTRAIT {
            let layout = ScreenLayout::new(size);
            for dpr in [1., 2., 3.] {
                ferravex_render::set_display_scale(dpr);
                let mut ui = Ui::new();
                let mut input = PlayerInput::transient("neon-portrait-menu");
                let center = Vec2::from(layout.action.min) + Vec2::from(layout.action.size) * 0.5;
                for (title, action) in [
                    ("NEON SWARM", "START"),
                    ("PAUSED", "RESUME"),
                    ("SIGNAL LOST", "PLAY AGAIN"),
                    ("SECTOR SECURED", "PLAY AGAIN"),
                ] {
                    for (blocked, end, expect) in [
                        (false, center, true),
                        (false, vec2(1., 1.), false),
                        (true, center, false),
                    ] {
                        input.set_controls_open(blocked);
                        for (phase, position) in
                            [(TouchPhase::Start, center), (TouchPhase::End, end)]
                        {
                            touch_event(Event {
                                id: 991,
                                position,
                                phase,
                            });
                            host_frame(0., size[0] as u32, size[1] as u32, || {
                                set_camera(&layout.canvas.camera());
                                ui.begin(layout.viewport);
                                // Call the production dispatcher, including its private layout.
                                crate::overlay(
                                    &mut ui,
                                    &layout,
                                    &input,
                                    title,
                                    "Run frozen. Resume when ready.",
                                    action,
                                );
                                ui.finish_player(&mut input, true);
                                ui.paint();
                                assert_eq!(ui.activated(1), phase == TouchPhase::End && expect);
                            });
                        }
                        input.set_controls_open(false);
                    }
                }
            }
        }
        ferravex_render::set_display_scale(1.);
    }

    #[test]
    fn dock_uses_existing_touch_channels_and_resize_releases_captured_outputs() {
        let mut map = ActionMap::default();
        let move_x = map
            .try_action(vec![Binding::digital(InputCode::Touch(0))])
            .unwrap();
        let dash = map
            .try_action(vec![Binding::digital(InputCode::Touch(7))])
            .unwrap();
        let mut surface = TouchSurface::default();
        let controls = ScreenLayout::new(PORTRAIT[0]).touch_controls();
        surface.set_layout(&controls, &mut map).unwrap();
        let Region::Circle { center, .. } = controls[0].region else {
            panic!()
        };
        let Region::Rect(button) = controls[1].region else {
            panic!()
        };
        for (id, position) in [(11, center), (12, button.center())] {
            surface
                .handle(
                    Event {
                        id,
                        position,
                        phase: TouchPhase::Start,
                    },
                    &mut map,
                )
                .unwrap();
        }
        surface
            .handle(
                Event {
                    id: 11,
                    position: center + vec2(36., 0.),
                    phase: TouchPhase::Move,
                },
                &mut map,
            )
            .unwrap();
        assert_eq!(map.peek(move_x).value, 1.);
        assert!(map.consume_tick(dash).pressed);
        surface
            .set_layout(&ScreenLayout::new(PORTRAIT[1]).touch_controls(), &mut map)
            .unwrap();
        assert_eq!(map.peek(move_x).value, 0.);
        assert_eq!(map.peek(dash).value, 0.);
        assert!(map.consume_tick(dash).released);
    }

    #[test]
    fn small_and_invalid_sizes_stay_finite_and_production_ui_stays_bounded() {
        let game = Game::new();
        let mut input = PlayerInput::transient("neon-portrait-bounds");
        input
            .action(
                "boost",
                "Dash",
                ferravex_play::input::ActionKind::Button,
                vec![Binding::digital(InputCode::Key(106))],
            )
            .unwrap();
        let mut ui = Ui::new();
        let reserved = ui.reserved_bytes();
        for width in [
            0.,
            1.,
            15.,
            64.,
            240.,
            320.,
            390.,
            899.,
            f32::NAN,
            f32::INFINITY,
            f32::MAX,
        ] {
            for height in [0., 1., 15., 64., 160., 288., 533., 608., 844., f32::NAN] {
                let layout = ScreenLayout::new([width, height]);
                for rect in [layout.header, layout.hints, layout.card, layout.action] {
                    assert!(rect.valid());
                }
                assert!(layout.canvas.camera().zoom.is_finite());
                TouchSurface::default()
                    .set_layout(&layout.touch_controls(), &mut ActionMap::default())
                    .unwrap();
                host_frame(
                    0.,
                    layout.canvas.screen.x as u32,
                    layout.canvas.screen.y as u32,
                    || {
                        ui.begin(layout.viewport);
                        layout.hud(&mut ui, &game, &input);
                        layout.overlay(
                            &mut ui,
                            "NEON SWARM",
                            "Clear three waves. Amber rings warn of attacks; dash to evade.",
                            "START",
                            &input,
                        );
                        ui.finish(false);
                        ui.paint();
                    },
                );
                assert_eq!(ui.reserved_bytes(), reserved);
            }
        }
    }
}
