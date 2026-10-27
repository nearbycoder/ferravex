//! Quiet, readable overlay helpers shared by the examples.
//!
//! [`ButtonOptions`] separates application selection from interaction focus.
//! Use [`Ui::button_with_options`] or [`Ui::hud_button_with_options`] for bounded,
//! clipped menus with the existing pointer capture and keyboard/gamepad handling.
//! The free [`button_with_options`] helper retains legacy click-on-press behavior.
use crate::prelude::*;

pub const BACKGROUND: Color = Color::from_rgba(14, 22, 35, 255);
pub const PANEL: Color = Color::from_rgba(20, 31, 47, 242);
pub const LINE: Color = Color::from_rgba(43, 59, 77, 255);
pub const INK: Color = Color::from_rgba(228, 238, 242, 255);
pub const MUTED: Color = Color::from_rgba(139, 160, 177, 255);
pub const AMBER: Color = Color::from_rgba(247, 181, 83, 255);
pub const CYAN: Color = Color::from_rgba(91, 213, 216, 255);

/// Glyph geometry is static and needs no texture atlas or DPI prewarming.
pub fn warm_up_font() {}
pub fn warm_up_font_sizes(_sizes: &[u16]) {}

/// Compact lab title and controls; dimensions/backend details live in the host.
pub fn header(_section: &str, title: &str, subtitle: &str) {
    draw_rectangle(0.0, 0.0, screen_width(), 104.0, PANEL);
    fitted_text(title, 24.0, 45.0, screen_width() - 48.0, 28.0, INK);
    fitted_text(subtitle, 25.0, 78.0, screen_width() - 49.0, 16.0, MUTED);
    draw_line(24.0, 103.0, screen_width() - 24.0, 103.0, 1.0, LINE);
}

pub fn panel(rect: Rect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, PANEL);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, LINE);
}

/// Click on press; the platform host maps the primary touch to the pointer.
pub fn button(rect: Rect, text: &str, selected: bool) -> bool {
    button_with_options(
        rect,
        text,
        ButtonOptions {
            selected,
            ..Default::default()
        },
    )
}

/// Legacy click-on-press button with an explicit disabled state.
/// Disabled buttons use muted text, ignore hover/selection, and never activate.
/// Enabled buttons keep [`button`]'s hit rectangle and appearance. This helper
/// does not own keyboard focus or pointer capture; use [`Ui`] for those menus.
///
/// ```
/// use ferravex_render::{Rect, ui::{ButtonOptions, button_with_options}};
/// let options = ButtonOptions { enabled: false, selected: true };
/// assert!(!button_with_options(Rect::new(20., 20., 100., 40.), "LOCKED", options));
/// ```
pub fn button_with_options(rect: Rect, text: &str, options: ButtonOptions) -> bool {
    let hovered = options.enabled && rect.contains(Vec2::from(mouse_position()));
    let selected = options.enabled && options.selected;
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        if !options.enabled {
            LINE
        } else if selected {
            AMBER
        } else if hovered {
            LINE
        } else {
            PANEL
        },
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        if selected { AMBER } else { LINE },
    );
    // Keep one rasterized size across responsive layouts. Scaling existing
    // glyphs avoids atlas growth midway through a frame after a resize.
    let font_scale =
        ((rect.w - 12.0) / measure_text(text, None, 17, 1.0).width.max(1.0)).clamp(0.1, 1.0);
    let dimensions = measure_text(text, None, 17, font_scale);
    draw_text_ex(
        text,
        rect.x + (rect.w - dimensions.width) * 0.5,
        rect.y + rect.h * 0.5 + 6.0,
        TextParams {
            font_size: 17,
            font_scale,
            color: if !options.enabled {
                MUTED
            } else if selected {
                BACKGROUND
            } else {
                INK
            },
        },
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

fn fitted_text(text: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let measured = measure_text(text, None, size as u16, 1.0).width;
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: size as u16,
            font_scale: (width.max(1.0) / measured.max(1.0)).min(1.0),
            color,
        },
    );
}

pub fn stat(x: f32, y: f32, label: &str, value: &str) {
    draw_text(label, x, y, 14.0, MUTED);
    draw_text(value, x, y + 24.0, 25.0, INK);
}

mod shared;
pub use shared::*;
pub mod cards;
pub mod inspector;
pub mod kit;
pub mod meters;
pub mod scroll;
pub mod stats;
pub mod text_layout;

/// Application-owned button availability and selection. Focus remains owned by
/// [`Ui`]; selection never changes tab order, hit regions, or activation.
/// Disabled takes precedence over selected. The default is enabled/unselected.
///
/// ```
/// use ferravex_render::ui::ButtonOptions;
/// let selected = ButtonOptions { selected: true, ..Default::default() };
/// assert!(selected.enabled);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonOptions {
    pub enabled: bool,
    pub selected: bool,
}

impl Default for ButtonOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            selected: false,
        }
    }
}

impl Ui {
    /// Queue a button with consistent selected/disabled visuals using the
    /// existing [`Self::button`] interaction and storage budgets.
    ///
    /// Enabled/unselected uses `style` unchanged. Selected uses `style.focus`
    /// as fill and `style.fill` as text and focus-border color, keeping focus
    /// distinguishable from selection. Disabled uses `style.disabled` as fill
    /// and blends text RGB halfway toward that fill, preserving text alpha.
    /// Custom palettes should keep fill and focus colors contrasting.
    /// All original style fields are validated, including inactive colors.
    /// Rebuild with options each frame; no extra commands or widgets are added.
    ///
    /// ```
    /// use ferravex_render::ui::{ButtonOptions, ButtonStyle, Ui, UiRect, Viewport};
    /// let mut ui = Ui::new();
    /// ui.begin(Viewport::responsive([320., 240.], [0.; 4])?);
    /// ui.button_with_options(
    ///     1, UiRect { min: [20.; 2], size: [120., 40.] }, "EASY",
    ///     ButtonOptions { selected: true, ..Default::default() },
    ///     ButtonStyle::default(),
    /// )?;
    /// ui.finish(true); // Process input after all widgets have been queued.
    /// let chose_easy = ui.activated(1);
    /// ui.paint();
    /// # let _ = chose_easy;
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn button_with_options(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        options: ButtonOptions,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        let style = option_style(style, options)?;
        self.button(id, rect, text, options.enabled, style)
    }

    /// Ellipsized, fixed-size-label counterpart of [`Self::button_with_options`].
    /// Uses [`Self::hud_button`]'s padding, clipping and bounded text arena.
    ///
    /// ```
    /// use ferravex_render::ui::{ButtonOptions, ButtonStyle, Ui, UiRect, Viewport};
    /// let mut ui = Ui::new();
    /// ui.begin(Viewport::responsive([320., 240.], [0.; 4])?);
    /// ui.hud_button_with_options(
    ///     1, UiRect { min: [20.; 2], size: [120., 40.] }, "CONTINUE EXPEDITION",
    ///     ButtonOptions { enabled: false, selected: false }, ButtonStyle::default(),
    /// )?;
    /// ui.finish(true);
    /// assert!(!ui.activated(1));
    /// ui.paint();
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn hud_button_with_options(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        options: ButtonOptions,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        let style = option_style(style, options)?;
        self.hud_button(id, rect, text, options.enabled, style)
    }
}

fn option_style(style: ButtonStyle, options: ButtonOptions) -> Result<ButtonStyle, UiError> {
    // Validate before choosing a palette so invalid inactive fields cannot be
    // hidden by selection/disablement. Match the existing Ui button contract.
    let finite = |c: Color| [c.r, c.g, c.b, c.a].into_iter().all(f32::is_finite);
    if ![style.fill, style.ink, style.focus, style.disabled]
        .into_iter()
        .all(finite)
        || !style.size.is_finite()
        || !(1. ..=256.).contains(&style.size)
    {
        return Err(UiError::InvalidLayout);
    }
    if !options.enabled {
        // Widen before averaging: the existing color contract accepts finite
        // HDR values, including opposite signs and f32::MAX.
        let mix = |ink: f32, fill: f32| ((ink as f64 + fill as f64) * 0.5) as f32;
        Ok(ButtonStyle {
            ink: Color::new(
                mix(style.ink.r, style.disabled.r),
                mix(style.ink.g, style.disabled.g),
                mix(style.ink.b, style.disabled.b),
                style.ink.a,
            ),
            ..style
        })
    } else if options.selected {
        Ok(ButtonStyle {
            fill: style.focus,
            ink: style.fill,
            focus: style.fill,
            ..style
        })
    } else {
        Ok(style)
    }
}

#[cfg(test)]
mod widget_tests {
    use super::*;

    const RECT: UiRect = UiRect {
        min: [20.; 2],
        size: [100., 40.],
    };

    fn reset() {
        crate::edit(|s| {
            *s = crate::State::default();
            s.w = 200;
            s.h = 200;
        });
        crate::set_default_camera();
    }

    fn next() {
        crate::edit(|s| {
            s.frame += 1;
            s.input.finish_frame();
            s.pressed.clear();
            s.clicks.clear();
            s.vertices.clear();
            s.batches.clear();
            s.retained.commands.clear();
        });
    }

    fn begin(ui: &mut Ui) {
        ui.begin(Viewport::responsive([200.; 2], [0.; 4]).unwrap());
    }

    fn queue(ui: &mut Ui, id: WidgetId, options: ButtonOptions) {
        ui.button_with_options(id, RECT, "PLAY", options, ButtonStyle::default())
            .unwrap();
    }

    fn tap(key: u32) {
        crate::key(key, true);
        crate::key(key, false);
    }

    fn color(vertex: &[f32; 8]) -> Color {
        Color::new(vertex[4], vertex[5], vertex[6], vertex[7])
    }

    #[test]
    fn legacy_enabled_options_preserve_geometry_colors_and_inclusive_hit_edges() {
        let rect = Rect::new(20., 20., 100., 40.);
        for selected in [false, true] {
            for point in [[19., 20.], [20., 20.], [120., 60.], [121., 60.]] {
                reset();
                crate::pointer(point[0], point[1], 0., 0.);
                crate::button(1, true);
                let old = button(rect, "PLAY", selected);
                let vertices = crate::read(|s| s.vertices.clone());
                let fill = if selected {
                    AMBER
                } else if rect.contains(Vec2::from(point)) {
                    LINE
                } else {
                    PANEL
                };
                assert!(vertices[..6].iter().all(|v| color(v) == fill));
                assert!(
                    vertices[6..30]
                        .iter()
                        .all(|v| color(v) == if selected { AMBER } else { LINE })
                );
                assert!(
                    vertices[30..]
                        .iter()
                        .all(|v| color(v) == if selected { BACKGROUND } else { INK })
                );
                crate::edit(|s| {
                    s.vertices.clear();
                    s.batches.clear();
                });
                let new = button_with_options(
                    rect,
                    "PLAY",
                    ButtonOptions {
                        selected,
                        ..Default::default()
                    },
                );
                assert_eq!(old, new);
                assert_eq!(new, rect.contains(Vec2::from(point)));
                crate::read(|s| assert_eq!(s.vertices, vertices));
            }
        }
    }

    #[test]
    fn disabled_legacy_button_ignores_press_hover_and_selection() {
        reset();
        let rect = Rect::new(20., 20., 100., 40.);
        let options = ButtonOptions {
            enabled: false,
            selected: false,
        };
        assert!(!button_with_options(rect, "PLAY", options));
        let vertices = crate::read(|s| s.vertices.clone());
        assert!(vertices.iter().any(|v| color(v) == MUTED));
        next();
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        assert!(!button_with_options(
            rect,
            "PLAY",
            ButtonOptions {
                selected: true,
                ..options
            }
        ));
        crate::read(|s| assert_eq!(s.vertices, vertices));
        assert!(is_mouse_button_pressed(MouseButton::Left));
    }

    #[test]
    fn selection_and_focus_have_distinct_paint_without_changing_tab_order() {
        reset();
        let mut ui = Ui::new();
        let selected = ButtonOptions {
            selected: true,
            ..Default::default()
        };
        begin(&mut ui);
        queue(&mut ui, 7, selected);
        ui.finish(true);
        assert_eq!(ui.focused(), None);
        ui.paint();
        let unfocused = crate::read(|s| {
            assert!(s.vertices[..6].iter().all(|v| color(v) == AMBER));
            s.vertices.len()
        });

        next();
        begin(&mut ui);
        queue(&mut ui, 7, selected);
        tap(0xff09);
        ui.finish(true);
        assert_eq!(ui.focused(), Some(7));
        assert!(!ui.activated(7));
        ui.paint();
        crate::read(|s| {
            assert_eq!(s.vertices.len(), unfocused + 24);
            assert!(s.vertices[..6].iter().all(|v| color(v) == AMBER));
            assert!(s.vertices[6..30].iter().all(|v| color(v) == PANEL));
            assert!(s.batches.iter().all(|batch| !batch.depth));
        });

        // Selection changes preserve focus. Tab follows submission order even
        // when the next button is selected and the first becomes unselected.
        next();
        begin(&mut ui);
        queue(&mut ui, 7, ButtonOptions::default());
        queue(&mut ui, 2, selected);
        ui.finish(true);
        assert_eq!(ui.focused(), Some(7));
        next();
        begin(&mut ui);
        queue(&mut ui, 7, ButtonOptions::default());
        queue(&mut ui, 2, selected);
        tap(0xff09);
        tap(0xff0d);
        ui.finish(true);
        assert_eq!(ui.focused(), Some(2));
        assert!(ui.activated(2));
        assert!(!ui.activated(7));
    }

    #[test]
    fn disabling_a_captured_selected_button_clears_focus_and_prevents_late_activation() {
        reset();
        let mut ui = Ui::new();
        let selected = ButtonOptions {
            selected: true,
            ..Default::default()
        };
        begin(&mut ui);
        queue(&mut ui, 1, selected);
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        ui.finish(true);
        assert_eq!(ui.focused(), Some(1));
        assert!(!ui.activated(1));

        next();
        begin(&mut ui);
        queue(
            &mut ui,
            1,
            ButtonOptions {
                enabled: false,
                ..selected
            },
        );
        crate::button(1, false);
        tap(0xff0d);
        ui.finish(true);
        assert_eq!(ui.focused(), None);
        assert!(!ui.activated(1));
        ui.paint();
        crate::read(|s| {
            assert!(s.vertices[..6].iter().all(|v| color(v) == LINE));
            assert!(s.vertices[6..].iter().all(|v| color(v) != INK));
            assert!(s.vertices.iter().all(|v| color(v) != AMBER));
        });

        next();
        begin(&mut ui);
        queue(&mut ui, 1, selected);
        crate::button(1, false);
        ui.finish(true);
        assert!(!ui.activated(1));
        next();
        begin(&mut ui);
        queue(&mut ui, 1, selected);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(true);
        assert!(ui.activated(1));
    }

    #[test]
    fn disabled_overlay_blocks_pointer_without_taking_keyboard_focus() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        queue(&mut ui, 1, ButtonOptions::default());
        queue(
            &mut ui,
            2,
            ButtonOptions {
                enabled: false,
                selected: true,
            },
        );
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(true);
        assert!(!ui.activated(1));
        assert!(!ui.activated(2));
        assert_eq!(ui.focused(), None);
        next();
        begin(&mut ui);
        queue(&mut ui, 1, ButtonOptions::default());
        queue(
            &mut ui,
            2,
            ButtonOptions {
                enabled: false,
                selected: true,
            },
        );
        tap(0xff09);
        tap(0xff0d);
        ui.finish(true);
        assert_eq!(ui.focused(), Some(1));
        assert!(ui.activated(1));
        assert!(!ui.activated(2));
    }

    #[test]
    fn options_preserve_scaled_clips_half_open_hits_and_tiny_widget_paint() {
        reset();
        let mut ui = Ui::new();
        let viewport = Viewport::fit([200.; 2], [100.; 2], [0.; 4]).unwrap();
        let clip = UiRect {
            min: [30.; 2],
            size: [40., 20.],
        };
        for selected in [false, true] {
            for enabled in [false, true] {
                for (point, inside) in
                    [([30., 30.], true), ([70., 30.], false), ([30., 50.], false)]
                {
                    next();
                    ui.begin(viewport);
                    ui.push_clip(clip).unwrap();
                    ui.hud_button_with_options(
                        1,
                        RECT,
                        "PLAY",
                        ButtonOptions { enabled, selected },
                        ButtonStyle::default(),
                    )
                    .unwrap();
                    crate::pointer(point[0] * 2., point[1] * 2., 0., 0.);
                    crate::button(1, true);
                    crate::button(1, false);
                    ui.finish(true);
                    assert_eq!(ui.activated(1), enabled && inside);
                    ui.paint();
                    crate::read(|s| {
                        assert!(!s.vertices.is_empty());
                        for vertex in &s.vertices {
                            let logical = [(vertex[0] + 1.) * 50., (1. - vertex[1]) * 50.];
                            for (axis, coordinate) in logical.into_iter().enumerate() {
                                assert!(coordinate >= clip.min[axis] - 0.001);
                                assert!(coordinate <= clip.min[axis] + clip.size[axis] + 0.001);
                            }
                        }
                    });
                }
            }
        }
        for size in [[1., 1.], [40., 4.], [0., 0.]] {
            next();
            begin(&mut ui);
            ui.hud_button_with_options(
                1,
                UiRect { size, ..RECT },
                "PLAY",
                ButtonOptions {
                    selected: true,
                    ..Default::default()
                },
                ButtonStyle::default(),
            )
            .unwrap();
            tap(0xff09);
            ui.finish(true);
            assert_eq!(ui.focused(), (size[0] > 0.).then_some(1));
            ui.paint();
            crate::read(|s| {
                assert_eq!(s.vertices.is_empty(), size[0] == 0.);
                for vertex in &s.vertices {
                    let pixel = [(vertex[0] + 1.) * 100., (1. - vertex[1]) * 100.];
                    for (axis, coordinate) in pixel.into_iter().enumerate() {
                        assert!(coordinate >= RECT.min[axis] - 0.001);
                        assert!(coordinate <= RECT.min[axis] + size[axis] + 0.001);
                    }
                }
            });
        }
    }

    #[test]
    fn invalid_inactive_styles_reject_atomically_and_finite_extremes_stay_finite() {
        reset();
        let mut ui = Ui::new();
        let bad_color = Color::new(f32::NAN, 0., 0., 1.);
        for selected in [false, true] {
            for enabled in [false, true] {
                let options = ButtonOptions { enabled, selected };
                for style in [
                    ButtonStyle {
                        fill: bad_color,
                        ..Default::default()
                    },
                    ButtonStyle {
                        ink: bad_color,
                        ..Default::default()
                    },
                    ButtonStyle {
                        focus: bad_color,
                        ..Default::default()
                    },
                    ButtonStyle {
                        disabled: bad_color,
                        ..Default::default()
                    },
                    ButtonStyle {
                        size: f32::INFINITY,
                        ..Default::default()
                    },
                    ButtonStyle {
                        size: 0.,
                        ..Default::default()
                    },
                    ButtonStyle {
                        size: 257.,
                        ..Default::default()
                    },
                ] {
                    next();
                    begin(&mut ui);
                    assert_eq!(
                        ui.button_with_options(1, RECT, "BAD", options, style),
                        Err(UiError::InvalidLayout)
                    );
                    assert_eq!(
                        ui.hud_button_with_options(1, RECT, "BAD", options, style),
                        Err(UiError::InvalidLayout)
                    );
                    ui.paint();
                    crate::read(|s| assert!(s.vertices.is_empty()));
                    queue(&mut ui, 1, ButtonOptions::default());
                }
            }
        }
        next();
        begin(&mut ui);
        ui.button_with_options(
            1,
            RECT,
            "HDR",
            ButtonOptions {
                enabled: false,
                selected: true,
            },
            ButtonStyle {
                ink: Color::new(f32::MAX, -f32::MAX, f32::MAX, 0.75),
                disabled: Color::new(f32::MAX, f32::MAX, -f32::MAX, 1.),
                ..Default::default()
            },
        )
        .unwrap();
        ui.paint();
        crate::read(|s| {
            assert!(s.vertices.iter().flatten().all(|v| v.is_finite()));
            assert!(
                s.vertices[6..]
                    .iter()
                    .all(|v| color(v) == Color::new(f32::MAX, 0., 0., 0.75))
            );
        });
    }

    #[test]
    fn options_reuse_budgets_reject_duplicates_and_keep_hud_labels_bounded() {
        reset();
        let mut ui = Ui::new();
        let reserved = ui.reserved_bytes();
        begin(&mut ui);
        let options = ButtonOptions {
            selected: true,
            ..Default::default()
        };
        assert_eq!(
            ui.button_with_options(
                1,
                UiRect {
                    min: [f32::NAN, 0.],
                    ..RECT
                },
                "BAD",
                options,
                ButtonStyle::default()
            ),
            Err(UiError::InvalidLayout)
        );
        let huge = "é".repeat(100_000);
        assert_eq!(
            ui.button_with_options(1, RECT, &huge, options, ButtonStyle::default()),
            Err(UiError::Capacity)
        );
        ui.hud_button_with_options(1, RECT, &huge, options, ButtonStyle::default())
            .unwrap();
        assert_eq!(
            ui.button_with_options(1, RECT, "DUPLICATE", options, ButtonStyle::default()),
            Err(UiError::DuplicateId)
        );
        // One button still consumes exactly one command/widget and no extra
        // focus geometry reservation. Empty labels leave the vertex budget free.
        for id in 2..=1024 {
            ui.button_with_options(id, RECT, "", options, ButtonStyle::default())
                .unwrap();
        }
        assert_eq!(
            ui.button_with_options(1025, RECT, "", options, ButtonStyle::default()),
            Err(UiError::Capacity)
        );
        for frame in 0..12 {
            next();
            begin(&mut ui);
            ui.hud_button_with_options(
                1,
                RECT,
                &huge,
                ButtonOptions {
                    enabled: frame % 2 == 0,
                    selected: frame % 3 == 0,
                },
                ButtonStyle::default(),
            )
            .unwrap();
            ui.finish(true);
            ui.paint();
            assert_eq!(ui.reserved_bytes(), reserved);
            crate::read(|s| assert!(s.vertices.len() < 1024));
        }
    }
}
