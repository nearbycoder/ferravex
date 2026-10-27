//! Immediate-mode screen overlays drawn between the 3D scene and the HUD:
//! speed lines, the waypoint marker, glows and rising tip pops. Pure drawing;
//! every input comes from accepted simulation state or the presentation owner.
//!
//! Colors deliberately avoid the HUD's exact cream/amber/panel inks so that
//! overlays can never be mistaken for HUD copy.
use super::presentation::{FlightPresentation, GOLD, POP_LIFE};
use super::*;

const SPEED_LINE: Color = Color::new(0.96, 0.98, 1.0, 1.0);
const MARKER: Color = Color::new(1.0, 0.82, 0.40, 1.0);
const SHADOW: Color = Color::new(0.05, 0.10, 0.11, 0.7);

/// World-to-screen projection for one frame's camera.
pub(super) struct Projector {
    view_projection: Mat4,
    size: Vec2,
}

impl Projector {
    pub(super) fn new(camera: &Camera3D, w: f32, h: f32) -> Self {
        Self {
            view_projection: camera.matrix(w, h),
            size: vec2(w, h),
        }
    }

    /// Screen position and whether the point is in front of the camera.
    pub(super) fn project(&self, point: Vec3) -> Option<(Vec2, bool)> {
        let clip = self
            .view_projection
            .mul_vec4(vec4(point.x, point.y, point.z, 1.));
        if !clip.w.is_finite() || clip.w.abs() < 1e-4 {
            return None;
        }
        let ndc = vec2(clip.x / clip.w, clip.y / clip.w);
        let screen = vec2(
            (ndc.x + 1.) * 0.5 * self.size.x,
            (1. - ndc.y) * 0.5 * self.size.y,
        );
        screen.is_finite().then_some((screen, clip.w > 0.))
    }
}

/// Radial streaks at the screen edges while flying fast. Zero under reduced
/// motion because the presentation owner never raises `speed_lines` then.
pub(super) fn speed_lines(presentation: &FlightPresentation, w: f32, h: f32) {
    let strength = presentation.speed_lines;
    if strength < 0.02 {
        return;
    }
    let center = vec2(w * 0.5, h * 0.42);
    let reach = vec2(w, h).length() * 0.5;
    for i in 0..22 {
        let angle = i as f32 * 2.399_963 + 0.3;
        let phase = (presentation.clock * 1.9 + i as f32 * 0.137).fract();
        let start = reach * (0.55 + phase * 0.45);
        let length = reach * (0.10 + 0.08 * ((i * 7) % 5) as f32 / 4.);
        let direction = vec2(angle.cos(), angle.sin());
        let a = center + direction * start;
        let b = center + direction * (start + length);
        let alpha = strength * 0.42 * (1. - phase);
        draw_line(
            a.x,
            a.y,
            b.x,
            b.y,
            1.5 + phase * 1.5,
            Color::new(SPEED_LINE.r, SPEED_LINE.g, SPEED_LINE.b, alpha),
        );
    }
}

/// Gold edge glow after a bullseye or delivery; zero under reduced flash.
pub(super) fn gate_glow(presentation: &FlightPresentation, w: f32, h: f32) {
    let glow = presentation.gate_glow;
    if glow <= 0. {
        return;
    }
    for (inset, alpha) in [(2., 0.55), (8., 0.25)] {
        draw_rectangle_lines(
            inset,
            inset,
            (w - inset * 2.).max(0.),
            (h - inset * 2.).max(0.),
            5.,
            Color::new(GOLD.r, GOLD.g, GOLD.b, glow * alpha),
        );
    }
}

fn shadowed(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        text,
        x + 2.,
        y + 2.,
        size,
        Color::new(SHADOW.r, SHADOW.g, SHADOW.b, SHADOW.a * color.a),
    );
    draw_text(text, x, y, size, color);
}

fn centered(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let width = text.chars().count() as f32 * size * 0.75;
    shadowed(text, x - width * 0.5, y, size, color);
}

/// Corner brackets around the current target, or an edge arrow when it is
/// off-screen or behind. Hidden rings are never marked: they are a secret.
pub(super) fn waypoint(projector: &Projector, flight: &Flight, w: f32, h: f32) {
    let Some(target) = ROUTE.get(flight.next).copied() else {
        return;
    };
    let Some((point, in_front)) = projector.project(target) else {
        return;
    };
    let distance = flight.pos.distance(target);
    // Keep clear of the top badges/navigation and the telemetry footer.
    let top = 150_f32.min(h * 0.3);
    let bottom = (h - 110.).max(top + 40.);
    let margin = 36.;
    let on_screen =
        in_front && (margin..=w - margin).contains(&point.x) && (top..=bottom).contains(&point.y);
    if on_screen {
        let size = (1400. / distance.max(1.)).clamp(14., 46.);
        let arm = size * 0.45;
        let color = Color::new(MARKER.r, MARKER.g, MARKER.b, 0.9);
        for (sx, sy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            let corner = point + vec2(sx * size, sy * size);
            draw_line(
                corner.x,
                corner.y,
                corner.x - sx * arm,
                corner.y,
                2.5,
                color,
            );
            draw_line(
                corner.x,
                corner.y,
                corner.x,
                corner.y - sy * arm,
                2.5,
                color,
            );
        }
        let label = if flight.next == GATES {
            format!("POST {distance:.0}M")
        } else {
            format!("{distance:.0}M")
        };
        centered(&label, point.x, point.y + size + 18., 12., color);
        return;
    }
    // Off-screen: an arrow on an inset ellipse, pointing toward the target.
    let center = vec2(w * 0.5, (top + bottom) * 0.5);
    let mut direction = point - center;
    if !in_front {
        direction = -direction;
    }
    let Some(direction) = direction.try_normalize() else {
        return;
    };
    let radius = vec2((w * 0.5 - margin).max(20.), ((bottom - top) * 0.5).max(20.));
    let tip = center + direction * radius;
    let side = vec2(-direction.y, direction.x);
    let color = Color::new(MARKER.r, MARKER.g, MARKER.b, 0.95);
    draw_triangle(
        tip + direction * 4. + vec2(2., 2.),
        tip - direction * 18. + side * 12. + vec2(2., 2.),
        tip - direction * 18. - side * 12. + vec2(2., 2.),
        SHADOW,
    );
    draw_triangle(
        tip + direction * 4.,
        tip - direction * 18. + side * 12.,
        tip - direction * 18. - side * 12.,
        color,
    );
}

/// Tip pops rise from the ring they were earned at, fading out. Under reduced
/// motion they stay put but remain readable.
pub(super) fn pops(projector: &Projector, presentation: &FlightPresentation) {
    for pop in presentation.pops.iter().flatten() {
        let Some((point, true)) = projector.project(pop.at) else {
            continue;
        };
        let t = pop.age / POP_LIFE;
        let alpha = ((1. - t) / 0.35).clamp(0., 1.);
        let grow = if presentation.motion > 0. {
            1. + 0.35 * (1. - (t * 6.).min(1.))
        } else {
            1.
        };
        let color = Color::new(pop.color.r, pop.color.g, pop.color.b, alpha);
        centered(
            &pop.text,
            point.x,
            point.y - presentation.pop_rise(pop),
            22. * grow,
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_maps_the_view_centre_and_flags_points_behind_the_camera() {
        let camera = Camera3D {
            position: Vec3::ZERO,
            target: Vec3::Z,
            up: Vec3::Y,
            fovy: 1.,
            ..Default::default()
        };
        let projector = Projector::new(&camera, 800., 600.);
        let (ahead, front) = projector.project(Vec3::Z * 50.).unwrap();
        assert!(front);
        assert!((ahead - vec2(400., 300.)).length() < 0.01);
        let (_, front) = projector.project(-Vec3::Z * 50.).unwrap();
        assert!(!front);
        let (up, _) = projector.project(vec3(0., 10., 50.)).unwrap();
        assert!(up.y < 300., "screen y grows downward");
    }
}
