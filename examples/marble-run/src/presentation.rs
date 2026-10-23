//! Camera and decoration state only. Preference changes never touch trial ticks.
use crate::{course_composition, follow_input, follow_settings, level::Level};
use ferravex_engine::camera::{
    CameraComposer, FramingBounds, FramingSettings, NoObstructions, Pose3, Rig3,
};
use ferravex_render::{preferences, prelude::*};

pub(crate) struct Presentation {
    pub camera: Rig3,
    composition: CameraComposer<1>,
    bounds: FramingBounds,
    reduced_motion: bool,
    decoration_time: f32,
}

impl Presentation {
    pub fn new(level: &Level) -> Self {
        let mut min = level.spawn.min(level.finish);
        let mut max = level.spawn.max(level.finish);
        for p in &level.platforms {
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    for z in [-0.5, 0.5] {
                        let corner = p.at(p.size * vec3(x, y, z));
                        min = min.min(corner);
                        max = max.max(corner);
                    }
                }
            }
        }
        for &p in &level.gems {
            min = min.min(p - Vec3::splat(0.6));
            max = max.max(p + Vec3::splat(0.6));
        }
        // Include the authored finish arch and a little clearance above the ball.
        min = min.min(level.finish + vec3(-2.5, -0.6, -1.));
        max = max.max(level.finish + vec3(2.5, 3., 1.));
        Self {
            camera: Rig3::new(follow_settings()).unwrap(),
            composition: course_composition(level),
            bounds: FramingBounds { min, max },
            reduced_motion: false,
            decoration_time: 0.,
        }
    }

    /// Must run before events and sampling, including paused/zero-tick frames.
    pub fn begin_frame(&mut self) {
        let policy = preferences::feedback_policy();
        if policy.reduced_motion() {
            self.decoration_time = 0.;
            self.composition.clear();
            self.camera.shake.clear();
            if !self.reduced_motion {
                self.camera.cut();
            }
        }
        self.reduced_motion = policy.reduced_motion();
        // This game has no flash, hit-stop, particle or FeedbackController channel.
        // Notices and the next-checkpoint marker are steady in both flash modes.
    }

    pub fn recover(&mut self, position: Vec3) {
        self.composition.clear();
        let mut input = follow_input(position);
        if !self.reduced_motion
            && let ferravex_engine::camera::Mode3::Follow {
                ref mut eye_offset, ..
            } = input.mode
        {
            // Relocate immediately near the safe anchor, then settle with Rig3's
            // damping. Never sweep the camera through the intervening course.
            *eye_offset += vec3(0., 1.2, 0.8);
        }
        self.camera.snap(input, &NoObstructions).unwrap();
    }

    pub fn update(&mut self, position: Vec3, velocity: Vec3, dt: f32) -> Pose3 {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        let mut input = follow_input(position);
        if self.reduced_motion {
            // Essential tracking remains, without anticipation or settling.
            self.camera.snap(input, &NoObstructions).unwrap().pose
        } else {
            self.decoration_time = (self.decoration_time + dt) % (20. * std::f32::consts::PI);
            input.target.velocity = velocity;
            let input = self.composition.update(input, Vec3::ZERO, dt).unwrap();
            self.camera.update(dt, input, &NoObstructions).unwrap().pose
        }
    }

    pub fn gem_pose(&self, position: Vec3, index: usize) -> (Vec3, Quat) {
        if self.reduced_motion {
            (position + vec3(0., 0.15, 0.), Quat::IDENTITY)
        } else {
            (
                position
                    + vec3(
                        0.,
                        0.15 + (self.decoration_time * 2. + index as f32).sin() * 0.1,
                        0.,
                    ),
                Quat::from_rotation_y(self.decoration_time * 1.3),
            )
        }
    }

    pub fn overview(&self, width: f32, height: f32) -> Pose3 {
        let aspect = if width.is_finite() && height.is_finite() && width > 0. && height > 0. {
            (width / height).clamp(0.1, 10.)
        } else {
            1.
        };
        FramingSettings {
            aspect_ratio: aspect,
            fov_y: 48_f32.to_radians(),
            margin: vec2(0.08, 0.12),
            near: 0.1,
            ..Default::default()
        }
        .frame(
            &[self.bounds],
            Quat::from_rotation_y(0.6_f32.atan2(0.8))
                * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4),
        )
        .expect("bounded authored course framing")
        .pose
    }

    pub fn far_plane(&self, view: Pose3) -> f32 {
        (view.position.distance(view.target) + (self.bounds.max - self.bounds.min).length() + 20.)
            .max(220.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        courses::COURSES,
        sim::{Controls, Run},
        trial::Trial,
    };
    use ferravex_engine::feedback::FeedbackPolicy;
    use std::sync::Arc;

    #[test]
    fn recovery_settles_near_anchor_freezes_on_pause_and_retry_clears_history() {
        preferences::set_feedback_policy(FeedbackPolicy::default());
        let mut run = Run::new();
        let mut presentation = Presentation::new(&run.level);
        presentation.begin_frame();
        for _ in 0..120 {
            presentation.update(run.level.finish, vec3(10., 0., 0.), 1. / 60.);
        }
        run.checkpoint = 1;
        run.recover();
        presentation.recover(run.position());
        let goal = run.position() + vec3(8., 12., 14.);
        let first = presentation.camera.pose().unwrap();
        assert!(first.position.distance(goal) > 1.);
        assert!(first.position.distance(goal) < 1.5);
        assert!(
            first
                .target
                .abs_diff_eq(run.position() + vec3(0., -0.2, -5.), 1e-5)
        );
        assert_eq!(presentation.update(run.position(), Vec3::ZERO, 0.), first);
        let mut previous_distance = first.position.distance(goal);
        for _ in 0..120 {
            let next = presentation.update(run.position(), Vec3::ZERO, 1. / 60.);
            let distance = next.position.distance(goal);
            assert!(distance <= previous_distance + 1e-5);
            previous_distance = distance;
        }
        assert!(previous_distance < 0.001);
        let mut retry = Presentation::new(&run.level);
        retry.begin_frame();
        let reset = retry.update(run.level.spawn, Vec3::ZERO, 0.);
        assert!(
            reset
                .position
                .abs_diff_eq(run.level.spawn + vec3(8., 12., 14.), 1e-5)
        );
    }

    #[test]
    fn preferences_apply_on_paused_zero_tick_frames_without_reviving_effects() {
        for motion in [false, true] {
            for flash in [false, true] {
                preferences::set_feedback_policy(FeedbackPolicy::default());
                let run = Run::new();
                let mut presentation = Presentation::new(&run.level);
                presentation.begin_frame();
                presentation.update(run.position(), vec3(5., 0., 0.), 0.2);
                presentation.recover(run.position());
                let prior = presentation.camera.pose().unwrap();
                preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
                presentation.begin_frame();
                let view = presentation.update(run.position(), Vec3::ZERO, 0.);
                let gem = presentation.gem_pose(run.level.gems[0], 0);
                if motion {
                    assert_eq!(presentation.decoration_time, 0.);
                    assert_eq!(gem.1, Quat::IDENTITY);
                    assert!(
                        view.position
                            .abs_diff_eq(run.position() + vec3(8., 12., 14.), 1e-5)
                    );
                    preferences::set_feedback_policy(FeedbackPolicy::default());
                    presentation.begin_frame();
                    assert_eq!(presentation.update(run.position(), Vec3::ZERO, 0.), view);
                    assert_eq!(presentation.decoration_time, 0.);
                } else {
                    assert_eq!(view, prior, "flash policy must not change motion");
                    assert!(presentation.decoration_time > 0.);
                }
            }
        }
        preferences::set_feedback_policy(FeedbackPolicy::default());
    }

    #[test]
    fn reduced_presentation_keeps_countdown_physics_hazards_and_timer_identical() {
        let level = Arc::new(Level::parse(COURSES[2].source).unwrap());
        let mut normal = Run::with_level(level.clone());
        let mut reduced = Run::with_level(level.clone());
        let mut a = Trial::new(false);
        let mut b = Trial::new(false);
        let mut presentation = Presentation::new(&level);
        preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
        for _ in 0..300 {
            presentation.begin_frame();
            for (trial, run) in [(&mut a, &mut normal), (&mut b, &mut reduced)] {
                let frame = trial.advance(1. / 60., false);
                for _ in 0..frame.steps {
                    trial.step(run, Controls::default());
                }
            }
            presentation.update(b.pose(&reduced).0, Vec3::ZERO, 1. / 60.);
        }
        assert_eq!(normal.position(), reduced.position());
        assert_eq!(normal.elapsed, reduced.elapsed);
        assert!(reduced.elapsed > 1.9);
        assert_eq!(a.session.tick(), b.session.tick());
        assert_eq!(
            normal.physics.bodies[normal.sweepers[0]].rotation(),
            reduced.physics.bodies[reduced.sweepers[0]].rotation()
        );
        assert_ne!(
            *reduced.physics.bodies[reduced.sweepers[0]].rotation(),
            Quat::IDENTITY
        );
        preferences::set_feedback_policy(FeedbackPolicy::default());
    }

    #[test]
    fn overview_fits_all_four_courses_in_portrait_landscape_and_short_views() {
        for course in &COURSES {
            let level = Level::parse(course.source).unwrap();
            let presentation = Presentation::new(&level);
            for (w, h) in [(320., 640.), (320., 152.), (1280., 720.)] {
                let view = presentation.overview(w, h);
                let forward = (view.target - view.position).normalize();
                let right = forward.cross(view.up).normalize();
                let up = right.cross(forward);
                let tan = (view.fov_y * 0.5).tan();
                for x in [presentation.bounds.min.x, presentation.bounds.max.x] {
                    for y in [presentation.bounds.min.y, presentation.bounds.max.y] {
                        for z in [presentation.bounds.min.z, presentation.bounds.max.z] {
                            let relative = vec3(x, y, z) - view.position;
                            let depth = relative.dot(forward);
                            assert!(depth > 0.1 && depth < presentation.far_plane(view));
                            assert!(relative.dot(right).abs() <= depth * tan * (w / h) * 0.841);
                            assert!(relative.dot(up).abs() <= depth * tan * 0.761);
                        }
                    }
                }
            }
        }
    }
}
