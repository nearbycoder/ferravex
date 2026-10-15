use super::*;
fn code_label(code: InputCode) -> String {
    match code {
        InputCode::Key(32) => "Space".into(),
        InputCode::Key(0xff1b) => "Esc".into(),
        InputCode::Key(0xff0d) => "Enter".into(),
        InputCode::Key(0xff51) => "Left".into(),
        InputCode::Key(0xff53) => "Right".into(),
        InputCode::Key(0xff52) => "Up".into(),
        InputCode::Key(0xff54) => "Down".into(),
        InputCode::Key(0xffe1) | InputCode::Key(0xffe2) => "Shift".into(),
        InputCode::Key(0xffe3) | InputCode::Key(0xffe4) => "Ctrl".into(),
        InputCode::Key(n) if (32..=126).contains(&n) => {
            (n as u8 as char).to_ascii_uppercase().to_string()
        }
        InputCode::Key(n) => format!("Key {n}"),
        InputCode::Mouse(n) => format!("Mouse {n}"),
        InputCode::MouseDelta(0) => "Mouse X".into(),
        InputCode::MouseDelta(_) => "Mouse Y".into(),
        InputCode::Wheel(n) => format!("Wheel {n}"),
        InputCode::PadButton(n) => format!("Pad {n}"),
        InputCode::PadAxis(n) => format!("Pad axis {n}"),
        InputCode::Touch(n) => format!("Touch {n}"),
        InputCode::TouchDelta(n) => format!("Touch look {n}"),
    }
}
fn fit(text: &str, width: f32, size: u16) -> String {
    if measure_text(text, None, size, 1.).width <= width {
        return text.into();
    }
    let mut text = text.to_string();
    while !text.is_empty() && measure_text(&(text.clone() + "..."), None, size, 1.).width > width {
        text.pop();
    }
    text + "..."
}
impl PlayerInput {
    fn ui_button(&mut self, rect: Rect, label: &str, selected: bool, index: usize) -> bool {
        let hovered = rect.contains(self.pointer);
        let keyboard = self.ui_focus == index;
        let fill = if selected {
            Color::from_rgba(60, 104, 105, 255)
        } else if hovered || keyboard {
            Color::from_rgba(53, 72, 80, 255)
        } else {
            Color::from_rgba(30, 44, 52, 255)
        };
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
        if keyboard {
            draw_rectangle_lines(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                1.,
                Color::from_rgba(141, 196, 187, 255),
            );
        }
        let label = fit(label, (rect.w - 8.).max(0.), 14);
        let width = measure_text(&label, None, 14, 1.).width;
        draw_text(
            &label,
            rect.x + (rect.w - width) * 0.5,
            rect.y + rect.h * 0.5 + 5.,
            14.,
            WHITE,
        );
        !self.ui_block
            && ((self.ui_click && hovered)
                || self.ui_taps[..self.ui_tap_len]
                    .iter()
                    .any(|p| rect.contains(*p))
                || keyboard && self.ui_accept)
    }
    /// Draw after the game's HUD. F2, the browser Controls button or controller
    /// Home opens this editor. Tab/arrows and Enter or the D-pad/A navigate it.
    pub fn draw_settings(&mut self) {
        if !self.open {
            if self.paused {
                crate::set_default_camera();
                let label = "Paused / Pause action to resume / F2 controls";
                let w = screen_width();
                let h = screen_height();
                let size = if w < 520. { 12 } else { 16 };
                let width = measure_text(label, None, size, 1.).width;
                draw_rectangle(0., h * 0.5 - 24., w, 48., Color::new(0.02, 0.04, 0.05, 0.9));
                draw_text(label, (w - width) * 0.5, h * 0.5 + 6., size as f32, WHITE);
            }
            return;
        }
        crate::set_default_camera();
        crate::ui::warm_up_font_sizes(&[14, 16, 20]);
        let w = screen_width();
        let h = screen_height();
        let width = (w - 24.).clamp(280., 760.);
        let height = (h - 24.).clamp(300., 620.);
        let x = (w - width) * 0.5;
        let y = (h - height) * 0.5;
        let narrow = width < 530.;
        draw_rectangle(0., 0., w, h, Color::new(0.02, 0.04, 0.05, 0.88));
        draw_rectangle(x, y, width, height, Color::from_rgba(18, 29, 37, 255));
        draw_rectangle_lines(x, y, width, height, 1., Color::from_rgba(70, 101, 108, 255));
        draw_text("CONTROLS", x + 18., y + 31., 20., WHITE);
        let per_page = (((height - 190.) / 43.).floor() as usize).clamp(1, 8);
        let count = if self.devices_tab {
            8
        } else {
            self.actions.len()
        };
        let pages = count.div_ceil(per_page).max(1);
        self.page = self.page.min(pages - 1);
        let start = self.page * per_page;
        let end = (start + per_page).min(count);
        let footer = 3 + (end - start) * 2;
        let mut focusable = vec![0, 1, 2];
        for (row, i) in (start..end).enumerate() {
            focusable.push(3 + row * 2);
            if self.devices_tab || self.actions[i].kind != ActionKind::Delta {
                focusable.push(4 + row * 2);
            }
        }
        focusable.extend([footer, footer + 1, footer + 2]);
        let current = focusable
            .iter()
            .position(|&n| n == self.ui_focus)
            .unwrap_or(0);
        self.ui_focus = focusable[if self.ui_down {
            (current + 1) % focusable.len()
        } else if self.ui_up {
            (current + focusable.len() - 1) % focusable.len()
        } else {
            current
        }];
        if self.ui_button(
            Rect::new(x + width - 94., y + 13., 76., 27.),
            "Done",
            false,
            0,
        ) {
            self.set_controls_open(false);
            return;
        }
        if self.ui_button(
            Rect::new(x + 18., y + 47., 92., 28.),
            "Bindings",
            !self.devices_tab,
            1,
        ) {
            self.devices_tab = false;
            self.page = 0;
            self.capture = None;
        }
        if self.ui_button(
            Rect::new(x + 120., y + 47., 92., 28.),
            "Devices",
            self.devices_tab,
            2,
        ) {
            self.devices_tab = true;
            self.page = 0;
            self.capture = None;
        }
        let hint = if self.capture.is_some() {
            if narrow {
                "Press a key/button; move axis right/down."
            } else {
                "Press a key/button, or move an axis in the positive direction. Escape cancels."
            }
        } else {
            "Changes save on this device. F2 / Esc closes."
        };
        draw_text(
            &fit(hint, width - 36., 14),
            x + 18.,
            y + 99.,
            14.,
            Color::from_rgba(161, 185, 189, 255),
        );
        let mut settings = self.settings;
        if self.devices_tab {
            for (row, i) in (start..end).enumerate() {
                let yy = y + 116. + row as f32 * 43.;
                let (label, value) = match i {
                    0 => ("Mouse look", format!("{:.2}", settings.mouse_sensitivity)),
                    1 => ("Touch look", format!("{:.4}", settings.touch_sensitivity)),
                    2 => ("Pad look X", format!("{:.2} rad/s", settings.pad_look.x)),
                    3 => ("Pad look Y", format!("{:.2} rad/s", settings.pad_look.y)),
                    4 => ("Dead zone", format!("{:.2}", settings.pad_deadzone)),
                    5 => ("Stick curve", format!("{:.2}", settings.pad_curve)),
                    6 => (
                        "Invert Y",
                        if settings.invert_y {
                            "On".into()
                        } else {
                            "Off".into()
                        },
                    ),
                    _ => (
                        "Controller",
                        format!(
                            "{} {}",
                            settings.gamepad + 1,
                            if gamepads()[settings.gamepad as usize].connected {
                                "connected"
                            } else {
                                "offline"
                            }
                        ),
                    ),
                };
                draw_text(&fit(label, width - 192., 14), x + 18., yy + 12., 14., WHITE);
                draw_text(
                    &fit(&value, width - 192., 14),
                    x + 18.,
                    yy + 30.,
                    14.,
                    Color::from_rgba(161, 185, 189, 255),
                );
                let minus = self.ui_button(
                    Rect::new(x + width - 158., yy, 62., 31.),
                    "-",
                    false,
                    3 + row * 2,
                );
                let plus = self.ui_button(
                    Rect::new(x + width - 86., yy, 62., 31.),
                    "+",
                    false,
                    4 + row * 2,
                );
                if minus || plus {
                    let d = if plus { 1. } else { -1. };
                    match i {
                        0 => {
                            settings.mouse_sensitivity = (settings.mouse_sensitivity
                                * if plus { 1.15 } else { 1. / 1.15 })
                            .clamp(0.01, 20.)
                        }
                        1 => {
                            settings.touch_sensitivity = (settings.touch_sensitivity
                                * if plus { 1.15 } else { 1. / 1.15 })
                            .clamp(0.00005, 0.1)
                        }
                        2 => settings.pad_look.x = (settings.pad_look.x + d * 0.25).clamp(0., 20.),
                        3 => settings.pad_look.y = (settings.pad_look.y + d * 0.25).clamp(0., 20.),
                        4 => {
                            settings.pad_deadzone =
                                (settings.pad_deadzone + d * 0.02).clamp(0., 0.95)
                        }
                        5 => settings.pad_curve = (settings.pad_curve + d * 0.25).clamp(0.25, 4.),
                        6 => settings.invert_y = !settings.invert_y,
                        _ => {
                            settings.gamepad = if plus {
                                (settings.gamepad + 1) % 4
                            } else {
                                (settings.gamepad + 3) % 4
                            }
                        }
                    }
                }
            }
        } else {
            let key_x = if narrow {
                x + width - 190.
            } else {
                x + width - 340.
            };
            let pad_x = if narrow {
                x + width - 96.
            } else {
                x + width - 174.
            };
            let bw = if narrow { 88. } else { 156. };
            for (row, i) in (start..end).enumerate() {
                let yy = y + 116. + row as f32 * 43.;
                let info = &self.actions[i];
                let name = info.name.clone();
                let kind = info.kind;
                let label = fit(&info.label, key_x - x - 28., if narrow { 14 } else { 16 });
                draw_text(
                    &label,
                    x + 18.,
                    yy + 20.,
                    if narrow { 14. } else { 16. },
                    WHITE,
                );
                let keyboard = self
                    .map
                    .bindings(info.id)
                    .unwrap()
                    .iter()
                    .filter(|b| {
                        matches!(
                            b.code,
                            InputCode::Key(_)
                                | InputCode::Mouse(_)
                                | InputCode::MouseDelta(_)
                                | InputCode::Wheel(_)
                        )
                    })
                    .take(2)
                    .map(|b| code_label(b.code))
                    .collect::<Vec<_>>()
                    .join(" / ");
                let pad = self
                    .map
                    .bindings(info.id)
                    .unwrap()
                    .iter()
                    .find(|b| matches!(b.code, InputCode::PadButton(_) | InputCode::PadAxis(_)))
                    .map_or_else(|| "Bind pad".into(), |b| code_label(b.code));
                if self.ui_button(
                    Rect::new(key_x, yy, bw, 31.),
                    if keyboard.is_empty() {
                        "Bind key"
                    } else {
                        &keyboard
                    },
                    self.capture
                        .is_some_and(|c| c.action == i && c.device == CaptureDevice::KeyboardMouse),
                    3 + row * 2,
                ) {
                    let _ = self.begin_capture(&name, CaptureDevice::KeyboardMouse);
                }
                if kind != ActionKind::Delta
                    && self.ui_button(
                        Rect::new(pad_x, yy, bw, 31.),
                        &pad,
                        self.capture
                            .is_some_and(|c| c.action == i && c.device == CaptureDevice::Gamepad),
                        4 + row * 2,
                    )
                {
                    let _ = self.begin_capture(&name, CaptureDevice::Gamepad);
                }
            }
        }
        if settings != self.settings {
            self.set_settings(settings)
                .expect("bounded editor settings");
            self.save();
        }
        let yy = y + height - 61.;
        if self.ui_button(Rect::new(x + 18., yy, 48., 26.), "<", false, footer) {
            self.page = self.page.saturating_sub(1);
            self.ui_focus = 0;
            self.capture = None;
        }
        draw_text(
            &format!("{} / {pages}", self.page + 1),
            x + 76.,
            yy + 18.,
            14.,
            WHITE,
        );
        if self.ui_button(Rect::new(x + 132., yy, 48., 26.), ">", false, footer + 1) {
            self.page = (self.page + 1).min(pages - 1);
            self.ui_focus = 0;
            self.capture = None;
        }
        if self.ui_button(
            Rect::new(x + width - 88., yy, 70., 26.),
            "Reset",
            false,
            footer + 2,
        ) {
            self.reset_defaults();
            self.capture = None;
        }
        let message = match self.status {
            ControlsStatus::Defaults => "Default bindings",
            ControlsStatus::Transient => "Session controls; persistence disabled",
            ControlsStatus::Saved => "Saved on this device",
            ControlsStatus::Loaded { unknown: 0 } => "Loaded saved controls",
            ControlsStatus::Loaded { .. } => "Loaded controls; retired actions ignored",
            ControlsStatus::Invalid(_) => "Invalid file; defaults remain available",
            ControlsStatus::Storage(_) => "Storage unavailable; session changes only",
        };
        draw_text(
            &fit(message, width - 36., 14),
            x + 18.,
            y + height - 12.,
            14.,
            Color::from_rgba(161, 185, 189, 255),
        );
    }
}
