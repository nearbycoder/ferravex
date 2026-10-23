use super::{
    AnimationError, AnimationEvent, EventBuffer, EventTrack, LoopMode, QuatTrack, Sample, Timeline,
    Vec3Track,
};
use crate::scene::Transform;
use ferravex_math::Quat;
use std::sync::Arc;

pub const MAX_ANIMATION_JOINTS: usize = 256;
pub const MAX_ANIMATION_CLIPS: usize = 128;
pub const MAX_TRACK_KEYS: usize = 65536;
#[derive(Clone, Debug)]
pub struct JointTrack {
    pub joint: u16,
    pub translation: Option<Vec3Track>,
    pub rotation: Option<QuatTrack>,
    pub scale: Option<Vec3Track>,
}
#[derive(Debug)]
struct ClipData {
    joints: usize,
    duration: f32,
    mode: LoopMode,
    tracks: Vec<JointTrack>,
    events: EventTrack,
}
/// Immutable, cheaply cloned clip. Untracked properties retain the bank's bind
/// pose. Parent indices/hierarchy belong to the scene Skeleton, not the clip.
#[derive(Clone, Debug)]
pub struct AnimationClip(Arc<ClipData>);
impl AnimationClip {
    pub fn new(
        joints: usize,
        duration: f32,
        mode: LoopMode,
        tracks: Vec<JointTrack>,
        events: Vec<AnimationEvent>,
    ) -> Result<Self, AnimationError> {
        if joints == 0 || joints > MAX_ANIMATION_JOINTS || tracks.len() > joints {
            return Err(AnimationError::JointCount);
        }
        let events = EventTrack::new(duration, events)?;
        let mut seen = [false; MAX_ANIMATION_JOINTS];
        let mut keys = 0;
        for track in &tracks {
            let i = track.joint as usize;
            if i >= joints || seen[i] {
                return Err(AnimationError::InvalidClip);
            }
            seen[i] = true;
            if track.translation.is_none() && track.rotation.is_none() && track.scale.is_none() {
                return Err(AnimationError::InvalidClip);
            }
            for t in [&track.translation, &track.scale].into_iter().flatten() {
                keys += t.keys.len();
                if t.duration() > duration {
                    return Err(AnimationError::InvalidClip);
                }
            }
            if let Some(t) = &track.rotation {
                keys += t.keys.len();
                if t.duration() > duration {
                    return Err(AnimationError::InvalidClip);
                }
            }
        }
        if keys > MAX_TRACK_KEYS {
            return Err(AnimationError::Capacity);
        }
        Ok(Self(Arc::new(ClipData {
            joints,
            duration,
            mode,
            tracks,
            events,
        })))
    }
    pub fn joint_count(&self) -> usize {
        self.0.joints
    }
    pub fn duration(&self) -> f32 {
        self.0.duration
    }
    pub fn mode(&self) -> LoopMode {
        self.0.mode
    }
    pub fn events(&self) -> &EventTrack {
        &self.0.events
    }
    /// Allocation-free sampling. Input errors leave output unchanged.
    pub fn sample_into(
        &self,
        time: f32,
        bind: &[Transform],
        out: &mut [Transform],
    ) -> Result<(), AnimationError> {
        if bind.len() != self.0.joints || out.len() != bind.len() {
            return Err(AnimationError::JointCount);
        }
        let time = super::playback_time(time, self.0.duration, self.0.mode)?;
        if !bind.iter().all(|t| t.valid()) {
            return Err(AnimationError::InvalidPose);
        }
        self.sample_valid(time, bind, out);
        Ok(())
    }
    fn sample_valid(&self, time: f32, bind: &[Transform], out: &mut [Transform]) {
        out.copy_from_slice(bind);
        for t in out.iter_mut() {
            t.rotation = <[f32; 4] as Sample>::blend(t.rotation, t.rotation, 0.);
        }
        for track in &self.0.tracks {
            let t = &mut out[track.joint as usize];
            // Clip sampling clamps each property independently. A short property
            // track holds its last key until the whole clip loops.
            if let Some(v) = &track.translation {
                t.translation = v
                    .sample(time, LoopMode::Clamp)
                    .expect("validated clip time");
            }
            if let Some(v) = &track.rotation {
                t.rotation = v
                    .sample(time, LoopMode::Clamp)
                    .expect("validated clip time");
            }
            if let Some(v) = &track.scale {
                t.scale = v
                    .sample(time, LoopMode::Clamp)
                    .expect("validated clip time");
            }
        }
    }
}
#[derive(Debug)]
struct BankData {
    bind: Vec<Transform>,
    clips: Vec<AnimationClip>,
}
/// Shared immutable authoring data. Every Animator allocates only its own pose
/// scratch buffers once; hundreds of actors can reuse this bank's tracks.
#[derive(Clone, Debug)]
pub struct AnimationBank(Arc<BankData>);
impl AnimationBank {
    pub fn new(
        mut bind: Vec<Transform>,
        clips: Vec<AnimationClip>,
    ) -> Result<Self, AnimationError> {
        if bind.is_empty()
            || bind.len() > MAX_ANIMATION_JOINTS
            || clips.is_empty()
            || clips.len() > MAX_ANIMATION_CLIPS
        {
            return Err(AnimationError::Capacity);
        }
        if !bind.iter().all(|t| t.valid()) {
            return Err(AnimationError::InvalidPose);
        }
        if clips.iter().any(|c| c.joint_count() != bind.len()) {
            return Err(AnimationError::JointCount);
        }
        for t in &mut bind {
            t.rotation = <[f32; 4] as Sample>::blend(t.rotation, t.rotation, 0.);
        }
        Ok(Self(Arc::new(BankData { bind, clips })))
    }
    pub fn bind_pose(&self) -> &[Transform] {
        &self.0.bind
    }
    pub fn clips(&self) -> &[AnimationClip] {
        &self.0.clips
    }
}
#[derive(Clone, Debug)]
pub struct JointMask {
    weights: Vec<f32>,
}
impl JointMask {
    pub fn new(weights: Vec<f32>) -> Result<Self, AnimationError> {
        if weights.is_empty() || weights.len() > MAX_ANIMATION_JOINTS {
            return Err(AnimationError::JointCount);
        }
        if weights
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.).contains(v))
        {
            return Err(AnimationError::InvalidWeight);
        }
        Ok(Self { weights })
    }
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }
}
#[derive(Clone, Copy, Debug)]
pub enum LayerMode {
    Override,
    Additive,
}
fn blended(a: Transform, b: Transform, weight: f32) -> Transform {
    if weight == 0. {
        return a;
    }
    Transform {
        translation: <[f32; 3] as Sample>::blend(a.translation, b.translation, weight),
        rotation: <[f32; 4] as Sample>::blend(a.rotation, b.rotation, weight),
        scale: <[f32; 3] as Sample>::blend(a.scale, b.scale, weight),
    }
}
fn additive(base: Transform, layer: Transform, reference: Transform, w: f32) -> Transform {
    if w == 0. {
        return base;
    }
    let offset = |a: [f32; 3], b: [f32; 3], r: [f32; 3]| {
        std::array::from_fn(|i| {
            (f64::from(a[i]) + (f64::from(b[i]) - f64::from(r[i])) * f64::from(w)) as f32
        })
    };
    let q =
        |t: Transform| Quat::from_array(<[f32; 4] as Sample>::blend(t.rotation, t.rotation, 0.));
    let delta = q(reference).conjugate() * q(layer);
    Transform {
        translation: offset(base.translation, layer.translation, reference.translation),
        rotation: (q(base) * Quat::IDENTITY.slerp(delta, w))
            .normalize()
            .to_array(),
        // Additive scale is a difference from reference, so zero reference scale
        // is well-defined. It deliberately does not divide by reference scale.
        scale: offset(base.scale, layer.scale, reference.scale),
    }
}
fn validate_layer(
    base: &[Transform],
    layer: &[Transform],
    weight: f32,
    mask: Option<&JointMask>,
) -> Result<(), AnimationError> {
    if base.is_empty()
        || base.len() > MAX_ANIMATION_JOINTS
        || base.len() != layer.len()
        || mask.is_some_and(|m| m.weights.len() != base.len())
    {
        return Err(AnimationError::JointCount);
    }
    if !weight.is_finite() || !(0.0..=1.).contains(&weight) {
        return Err(AnimationError::InvalidWeight);
    }
    if !base.iter().chain(layer).all(|t| t.valid()) {
        return Err(AnimationError::InvalidPose);
    }
    Ok(())
}
/// Blend local TRS values using shortest-arc normalized quaternion interpolation.
/// Allocation-free; malformed lengths, poses or weights leave output unchanged.
/// A joint with zero effective weight retains its exact base transform, including
/// its quaternion magnitude. All inputs are validated even for excluded joints.
///
/// ```
/// use ferravex_play::{animation::{blend_pose_into, JointMask}, scene::Transform};
/// let base = [Transform::default(); 2];
/// let layer = [Transform { translation: [8., 0., 0.], ..base[0] }; 2];
/// let mask = JointMask::new(vec![0., 0.5])?;
/// let mut out = base;
/// blend_pose_into(&base, &layer, 0.5, Some(&mask), &mut out)?;
/// assert_eq!(out[0], base[0]);
/// assert_eq!(out[1].translation[0], 2.);
/// # Ok::<(), ferravex_play::animation::AnimationError>(())
/// ```
pub fn blend_pose_into(
    base: &[Transform],
    layer: &[Transform],
    weight: f32,
    mask: Option<&JointMask>,
    out: &mut [Transform],
) -> Result<(), AnimationError> {
    validate_layer(base, layer, weight, mask)?;
    if out.len() != base.len() {
        return Err(AnimationError::JointCount);
    }
    for (i, target) in out.iter_mut().enumerate() {
        *target = blended(
            base[i],
            layer[i],
            weight * mask.map_or(1., |m| m.weights[i]),
        );
    }
    Ok(())
}
/// Add a pose relative to an authored reference. Numeric overflow is rejected
/// before writing any output. Zero effective weight preserves the exact base
/// transform; excluded joints still require valid inputs and matching lengths.
/// Allocation-free. See [`Animator::apply_layer`] for a reusable in-place layer
/// stack using the bank's bind pose as its reference.
///
/// ```
/// use ferravex_play::{animation::{additive_pose_into, AnimationError}, scene::Transform};
/// let base = [Transform { translation: [2., 0., 0.], ..Transform::default() }];
/// let layer = [Transform { translation: [8., 0., 0.], ..base[0] }];
/// let reference = [Transform { translation: [4., 0., 0.], ..base[0] }];
/// let mut out = base;
/// additive_pose_into(&base, &layer, &reference, 0.5, None, &mut out)?;
/// assert_eq!(out[0].translation[0], 4.);
/// let previous = out;
/// assert_eq!(additive_pose_into(&base, &layer, &[], 1., None, &mut out),
///            Err(AnimationError::JointCount));
/// assert_eq!(out, previous);
/// # Ok::<(), AnimationError>(())
/// ```
pub fn additive_pose_into(
    base: &[Transform],
    layer: &[Transform],
    reference: &[Transform],
    weight: f32,
    mask: Option<&JointMask>,
    out: &mut [Transform],
) -> Result<(), AnimationError> {
    validate_layer(base, layer, weight, mask)?;
    if reference.len() != base.len() || out.len() != base.len() {
        return Err(AnimationError::JointCount);
    }
    if !reference.iter().all(|t| t.valid()) {
        return Err(AnimationError::InvalidPose);
    }
    let at = |i: usize| {
        additive(
            base[i],
            layer[i],
            reference[i],
            weight * mask.map_or(1., |m| m.weights[i]),
        )
    };
    if (0..base.len()).any(|i| !at(i).valid()) {
        return Err(AnimationError::InvalidPose);
    }
    for (i, target) in out.iter_mut().enumerate() {
        *target = at(i);
    }
    Ok(())
}
#[derive(Clone, Debug)]
struct Playing {
    clip: usize,
    clock: Timeline,
}
#[derive(Clone, Debug)]
struct Transition {
    source: Option<Playing>,
    elapsed: f32,
    duration: f32,
}
/// Reusable local-pose player. Crossfades keep the outgoing clip moving. An
/// interrupted crossfade snapshots the current base blend before layers, so
/// reapplying aim/recoil cannot bake the same layer into the source twice. If the
/// fade has not advanced yet, retargeting keeps the original outgoing clip moving.
/// Events come only from the incoming clip, avoiding duplicate footstep/weapon
/// events during transitions. Layers are visual and do not emit extra events.
/// No frame, play/seek/transition or layer operation allocates after construction.
#[derive(Clone, Debug)]
pub struct Animator {
    bank: AnimationBank,
    active: Playing,
    transition: Option<Transition>,
    pose: Vec<Transform>,
    base: Vec<Transform>,
    target: Vec<Transform>,
    source: Vec<Transform>,
}
impl Animator {
    pub fn new(bank: &AnimationBank, clip: usize) -> Result<Self, AnimationError> {
        let playing = Self::playing(bank, clip)?;
        let mut pose = bank.0.bind.clone();
        bank.0.clips[clip].sample_valid(0., &bank.0.bind, &mut pose);
        Ok(Self {
            bank: bank.clone(),
            active: playing,
            transition: None,
            target: pose.clone(),
            source: pose.clone(),
            base: pose.clone(),
            pose,
        })
    }
    fn playing(bank: &AnimationBank, clip: usize) -> Result<Playing, AnimationError> {
        let data = bank.0.clips.get(clip).ok_or(AnimationError::InvalidClip)?;
        Ok(Playing {
            clip,
            clock: Timeline::new(data.duration(), data.mode())?,
        })
    }
    pub fn pose(&self) -> &[Transform] {
        &self.pose
    }
    pub fn base_pose(&self) -> &[Transform] {
        &self.base
    }
    pub fn clip(&self) -> usize {
        self.active.clip
    }
    pub fn timeline(&self) -> &Timeline {
        &self.active.clock
    }
    pub fn transitioning(&self) -> bool {
        self.transition.is_some()
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.active.clock.set_paused(paused);
        if let Some(Transition {
            source: Some(source),
            ..
        }) = &mut self.transition
        {
            source.clock.set_paused(paused);
        }
    }
    pub fn set_speed(&mut self, speed: f32) -> Result<(), AnimationError> {
        self.active.clock.set_speed(speed)?;
        if let Some(Transition {
            source: Some(source),
            ..
        }) = &mut self.transition
        {
            source.clock.set_speed(speed)?;
        }
        Ok(())
    }
    /// Restart the requested clip and cancel any transition. For per-frame state
    /// selection use transition(), which keeps an already selected clip running.
    pub fn play(&mut self, clip: usize) -> Result<(), AnimationError> {
        let mut next = Self::playing(&self.bank, clip)?;
        next.clock.set_speed(self.active.clock.speed())?;
        next.clock.set_paused(self.active.clock.paused());
        self.active = next;
        self.transition = None;
        self.evaluate();
        Ok(())
    }
    /// Select a clip with a fade lasting `seconds` in `0..=60`. Positive durations
    /// leave the current pose/layers intact until the next advance. Retargeting a
    /// progressing fade uses the unlayered base as a frozen source; retargeting
    /// before any fade progress preserves its original moving source.
    ///
    /// Selecting the incoming clip again with a positive duration is a no-op.
    /// Zero duration completes an existing fade immediately at the current
    /// playback time, clearing layers without restarting the clock or replaying
    /// start events. With no fade, selecting the current clip remains a no-op.
    /// Selecting a different clip with zero duration instead calls [`Self::play`].
    /// Invalid clip indices or durations leave all state unchanged.
    ///
    /// ```
    /// use ferravex_play::{animation::*, scene::Transform};
    /// let clip = AnimationClip::new(1, 1., LoopMode::Repeat, vec![], vec![])?;
    /// let bank = AnimationBank::new(vec![Transform::default()], vec![clip.clone(), clip])?;
    /// let mut player = Animator::new(&bank, 0)?;
    /// let mut events = EventBuffer::<8>::default();
    /// player.transition(1, 0.5)?;
    /// player.advance(0.125, &mut events)?;
    /// assert!(player.transitioning());
    /// player.transition(1, 0.)?;
    /// assert!(!player.transitioning());
    /// assert_eq!(player.timeline().time(), 0.125);
    /// # Ok::<(), AnimationError>(())
    /// ```
    pub fn transition(&mut self, clip: usize, seconds: f32) -> Result<(), AnimationError> {
        if !seconds.is_finite() || !(0.0..=60.).contains(&seconds) {
            return Err(AnimationError::InvalidTime);
        }
        if clip >= self.bank.0.clips.len() {
            return Err(AnimationError::InvalidClip);
        }
        if clip == self.active.clip {
            if seconds == 0. && self.transition.is_some() {
                self.transition = None;
                self.evaluate();
            }
            return Ok(());
        }
        if seconds == 0. {
            return self.play(clip);
        }
        let mut next = Self::playing(&self.bank, clip)?;
        next.clock.set_speed(self.active.clock.speed())?;
        next.clock.set_paused(self.active.clock.paused());
        let source = match &self.transition {
            None => Some(self.active.clone()),
            // No contribution from the discarded target exists yet. Preserve
            // outgoing motion instead of needlessly freezing the current pose.
            Some(t) if t.elapsed == 0. => t.source.clone(),
            Some(_) => None,
        };
        if source.is_none() {
            self.source.copy_from_slice(&self.base);
        }
        self.active = next;
        self.transition = Some(Transition {
            source,
            elapsed: 0.,
            duration: seconds,
        });
        Ok(())
    }
    pub fn seek(&mut self, time: f32) -> Result<(), AnimationError> {
        self.active.clock.seek(time)?;
        self.transition = None;
        self.evaluate();
        Ok(())
    }
    fn evaluate(&mut self) {
        let active = &self.bank.0.clips[self.active.clip];
        active.sample_valid(
            self.active.clock.time(),
            &self.bank.0.bind,
            &mut self.target,
        );
        if let Some(t) = &self.transition {
            if let Some(source) = &t.source {
                self.bank.0.clips[source.clip].sample_valid(
                    source.clock.time(),
                    &self.bank.0.bind,
                    &mut self.source,
                );
            }
            let weight = (t.elapsed / t.duration).clamp(0., 1.);
            for i in 0..self.pose.len() {
                self.pose[i] = blended(self.source[i], self.target[i], weight);
            }
        } else {
            self.pose.copy_from_slice(&self.target);
        }
        self.base.copy_from_slice(&self.pose);
    }
    pub fn advance<const N: usize>(
        &mut self,
        dt: f32,
        events: &mut EventBuffer<N>,
    ) -> Result<(), AnimationError> {
        let clip = &self.bank.0.clips[self.active.clip];
        self.active
            .clock
            .advance_tagged(dt, clip.events(), self.active.clip as u16, events)?;
        if let Some(t) = &mut self.transition {
            if let Some(source) = &mut t.source {
                source.clock.advance(dt)?;
            }
            if !self.active.clock.paused() && self.active.clock.speed() > 0. {
                t.elapsed = (t.elapsed + dt).min(t.duration);
            }
            if t.elapsed >= t.duration {
                self.transition = None;
            }
        }
        self.evaluate();
        Ok(())
    }
    /// Apply one visual layer until the next advance/play/seek or immediate
    /// transition. Repeat to stack masked overrides or additive recoil/aim over
    /// the current base pose.
    /// Invalid clips, times, weights, mask lengths or overflowing additive results
    /// leave the visible pose, base pose and playback/transition state unchanged.
    /// Zero effective weight preserves the joint exactly. No allocation occurs.
    ///
    /// ```
    /// use ferravex_play::{animation::*, scene::Transform};
    /// let clip = AnimationClip::new(2, 1., LoopMode::Clamp, vec![], vec![])?;
    /// let bank = AnimationBank::new(vec![Transform::default(); 2], vec![clip])?;
    /// let mut player = Animator::new(&bank, 0)?;
    /// let mask = JointMask::new(vec![1.])?; // wrong skeleton size
    /// let previous = player.pose().to_vec();
    /// assert_eq!(player.apply_layer(0, 0., 1., Some(&mask), LayerMode::Additive),
    ///            Err(AnimationError::JointCount));
    /// assert_eq!(player.pose(), previous);
    /// # Ok::<(), AnimationError>(())
    /// ```
    pub fn apply_layer(
        &mut self,
        clip: usize,
        time: f32,
        weight: f32,
        mask: Option<&JointMask>,
        mode: LayerMode,
    ) -> Result<(), AnimationError> {
        if clip >= self.bank.0.clips.len() {
            return Err(AnimationError::InvalidClip);
        }
        validate_layer(&self.pose, &self.pose, weight, mask)?;
        self.bank.0.clips[clip].sample_into(time, &self.bank.0.bind, &mut self.target)?;
        // Stage each result in the existing scratch buffer. Even failure on the
        // last joint cannot change the current layer stack or the fade source.
        for (i, (base, target)) in self.pose.iter().zip(&mut self.target).enumerate() {
            let w = weight * mask.map_or(1., |m| m.weights[i]);
            let result = match mode {
                LayerMode::Override => blended(*base, *target, w),
                LayerMode::Additive => additive(*base, *target, self.bank.0.bind[i], w),
            };
            if !result.valid() {
                return Err(AnimationError::InvalidPose);
            }
            *target = result;
        }
        self.pose.copy_from_slice(&self.target);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Interpolation, Keyframe, Track};

    fn transform(x: f32) -> Transform {
        Transform {
            translation: [x, x * 0.5, -x],
            rotation: Quat::from_rotation_y(x * 0.1).to_array(),
            scale: [1. + x * 0.1; 3],
        }
    }

    fn track<T: Sample>(a: T, b: T) -> Track<T> {
        Track::new(
            vec![
                Keyframe { time: 0., value: a },
                Keyframe { time: 1., value: b },
            ],
            Interpolation::Linear,
        )
        .unwrap()
    }

    fn clip(from: &[Transform], to: &[Transform]) -> AnimationClip {
        AnimationClip::new(
            from.len(),
            1.,
            LoopMode::Clamp,
            from.iter()
                .zip(to)
                .enumerate()
                .map(|(joint, (a, b))| JointTrack {
                    joint: joint as u16,
                    translation: Some(track(a.translation, b.translation)),
                    rotation: Some(track(a.rotation, b.rotation)),
                    scale: Some(track(a.scale, b.scale)),
                })
                .collect(),
            vec![
                AnimationEvent { time: 0., id: 10 },
                AnimationEvent { time: 0.25, id: 11 },
            ],
        )
        .unwrap()
    }

    fn bank() -> AnimationBank {
        AnimationBank::new(
            vec![transform(0.), transform(1.)],
            [0., 10., 20.]
                .into_iter()
                .map(|x| {
                    clip(
                        &[transform(x), transform(1.)],
                        &[transform(x + 2.), transform(1.)],
                    )
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn retargets_before_fade_progress_keep_the_original_clip_moving() {
        for hold in 0..3 {
            let mut actual = Animator::new(&bank(), 0).unwrap();
            let mut events = EventBuffer::<8>::default();
            actual.advance(0.125, &mut events).unwrap();
            let mut expected = actual.clone();
            actual.set_paused(hold == 1);
            actual.set_speed(if hold == 2 { 0. } else { 1. }).unwrap();
            for target in [1, 2, 1, 2] {
                actual.transition(target, 0.5).unwrap();
                actual
                    .advance(if hold == 0 { 0. } else { 0.125 }, &mut events)
                    .unwrap();
                assert!(events.hits().is_empty());
            }
            actual.set_paused(false);
            actual.set_speed(1.).unwrap();
            expected.transition(2, 0.5).unwrap();
            let mut expected_events = EventBuffer::<8>::default();
            expected.advance(0.125, &mut expected_events).unwrap();
            actual.advance(0.125, &mut events).unwrap();
            assert_eq!(actual.pose(), expected.pose(), "hold mode {hold}");
            assert_eq!(events.hits(), expected_events.hits());
            assert_eq!(events.hits()[0].clip, 2);
        }
    }

    #[test]
    fn repeated_interruptions_preserve_trs_without_baking_masked_layers() {
        let bank = bank();
        let mut player = Animator::new(&bank, 0).unwrap();
        let mut events = EventBuffer::<8>::default();
        let mask = JointMask::new(vec![1., 0.]).unwrap();
        player.transition(1, 0.5).unwrap();
        player.advance(0.125, &mut events).unwrap();
        for target in [2, 0, 1].into_iter().cycle().take(24) {
            let base = player.base_pose().to_vec();
            player
                .apply_layer(2, 0.5, 0.2, Some(&mask), LayerMode::Additive)
                .unwrap();
            let visible = player.pose().to_vec();
            player.transition((target + 1) % 3, 0.5).unwrap();
            player.transition(target, 0.5).unwrap();
            assert_eq!(player.pose(), visible);
            player.advance(0., &mut events).unwrap();
            assert_eq!(player.base_pose(), base);
            player
                .apply_layer(2, 0.5, 0.2, Some(&mask), LayerMode::Additive)
                .unwrap();
            assert_eq!(player.pose(), visible);
            player.advance(0.0625, &mut events).unwrap();
            let target_x = target as f32 * 10. + 0.125;
            let expected_x = base[0].translation[0] * 0.875 + target_x * 0.125;
            assert!((player.base_pose()[0].translation[0] - expected_x).abs() < 1e-5);
            assert!(
                events
                    .hits()
                    .iter()
                    .all(|event| event.clip == target as u16)
            );
        }
        player.advance(0.5, &mut events).unwrap();
        assert!(!player.transitioning());
        let mut expected = [Transform::default(); 2];
        bank.clips()[player.clip()]
            .sample_into(player.timeline().time(), bank.bind_pose(), &mut expected)
            .unwrap();
        assert_eq!(player.pose(), expected);
    }

    #[test]
    fn zero_duration_finishes_the_selected_fade_without_restarting_its_clock() {
        let bank = bank();
        let mut player = Animator::new(&bank, 0).unwrap();
        let mut events = EventBuffer::<8>::default();
        player.transition(1, 1.).unwrap();
        player.advance(0.125, &mut events).unwrap();
        let previous = player.pose().to_vec();
        player.transition(1, 0.25).unwrap();
        assert_eq!(player.pose(), previous);
        player
            .apply_layer(2, 0., 0.5, None, LayerMode::Additive)
            .unwrap();
        player.set_speed(2.).unwrap();
        player.set_paused(true);
        player.transition(1, 0.).unwrap();
        assert!(!player.transitioning());
        assert_eq!(player.timeline().time(), 0.125);
        assert_eq!(player.timeline().speed(), 2.);
        assert!(player.timeline().paused());
        let mut expected = [Transform::default(); 2];
        bank.clips()[1]
            .sample_into(0.125, bank.bind_pose(), &mut expected)
            .unwrap();
        assert_eq!(player.pose(), expected);
        assert_eq!(player.base_pose(), expected);
        player.set_paused(false);
        player.advance(0.0625, &mut events).unwrap();
        assert_eq!(events.hits().len(), 1);
        assert_eq!(events.hits()[0].id, 11);
        assert_eq!(events.hits()[0].clip, 1);
    }

    #[test]
    fn zero_duration_to_a_new_clip_clears_frozen_source_and_layers() {
        let bank = bank();
        let mut player = Animator::new(&bank, 0).unwrap();
        let mut events = EventBuffer::<8>::default();
        player.transition(1, 1.).unwrap();
        player.advance(0.125, &mut events).unwrap();
        player.transition(2, 1.).unwrap();
        player
            .apply_layer(1, 0., 1., None, LayerMode::Additive)
            .unwrap();
        player.set_speed(2.).unwrap();
        player.set_paused(true);
        player.transition(0, 0.).unwrap();
        assert!(!player.transitioning());
        assert_eq!(player.pose(), Animator::new(&bank, 0).unwrap().pose());
        assert_eq!(player.base_pose(), player.pose());
        assert_eq!(player.timeline().time(), 0.);
        assert_eq!(player.timeline().speed(), 2.);
        assert!(player.timeline().paused());
        player.set_paused(false);
        player.advance(0.0625, &mut events).unwrap();
        assert_eq!(events.hits().len(), 1);
        assert_eq!(events.hits()[0].id, 10);
        assert_eq!(events.hits()[0].clip, 0);
        let previous = player.pose().to_vec();
        player.transition(0, 0.).unwrap();
        assert_eq!(player.pose(), previous);
        assert_eq!(player.timeline().time(), 0.125);
    }

    #[test]
    fn zero_weight_and_masked_out_joints_preserve_exact_base_transforms() {
        let base = [Transform {
            rotation: [1., 2., 3., 4.],
            ..transform(2.)
        }; 2];
        let layer = [transform(10.); 2];
        let reference = [transform(0.); 2];
        let mask = JointMask::new(vec![0., 0.5]).unwrap();
        let mut out = [Transform::default(); 2];
        blend_pose_into(&base, &layer, 0., None, &mut out).unwrap();
        assert_eq!(out, base);
        additive_pose_into(&base, &layer, &reference, 0., None, &mut out).unwrap();
        assert_eq!(out, base);
        blend_pose_into(&base, &layer, 0.5, Some(&mask), &mut out).unwrap();
        assert_eq!(out[0], base[0]);
        assert_eq!(out[1].translation[0], 4.);
        additive_pose_into(&base, &layer, &reference, 0.5, Some(&mask), &mut out).unwrap();
        assert_eq!(out[0], base[0]);
        assert_eq!(out[1].translation[0], 4.5);
        assert!((Quat::from_array(out[1].rotation).length() - 1.).abs() < 1e-6);
    }

    #[test]
    fn mismatched_joint_counts_never_write_partial_output() {
        let base = [transform(0.); 2];
        let layer = [transform(10.); 2];
        let mask = JointMask::new(vec![1.]).unwrap();
        for (base, layer, mask, output_len) in [
            (&base[..], &layer[..1], None, 2),
            (&base[..], &layer[..], Some(&mask), 2),
            (&base[..], &layer[..], None, 1),
            (&base[..0], &layer[..0], None, 2),
        ] {
            let mut out = [transform(99.); 2];
            let previous = out;
            assert_eq!(
                blend_pose_into(base, layer, 1., mask, &mut out[..output_len]),
                Err(AnimationError::JointCount)
            );
            assert_eq!(out, previous);
            assert_eq!(
                additive_pose_into(base, layer, base, 1., mask, &mut out[..output_len]),
                Err(AnimationError::JointCount)
            );
            assert_eq!(out, previous);
        }
        let mut out = [transform(99.); 2];
        let previous = out;
        assert_eq!(
            additive_pose_into(&base, &layer, &base[..1], 1., None, &mut out),
            Err(AnimationError::JointCount)
        );
        assert_eq!(out, previous);
    }

    #[test]
    fn invalid_poses_and_weights_are_rejected_even_when_masked_out() {
        let base = [transform(0.); 2];
        let mask = JointMask::new(vec![0.; 2]).unwrap();
        let mut out = [transform(99.); 2];
        let previous = out;
        for weight in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert_eq!(
                blend_pose_into(&base, &base, weight, Some(&mask), &mut out),
                Err(AnimationError::InvalidWeight)
            );
            assert_eq!(
                additive_pose_into(&base, &base, &base, weight, Some(&mask), &mut out),
                Err(AnimationError::InvalidWeight)
            );
            assert_eq!(out, previous);
        }
        for invalid in [
            Transform {
                translation: [f32::NAN; 3],
                ..transform(0.)
            },
            Transform {
                rotation: [0.; 4],
                ..transform(0.)
            },
            Transform {
                scale: [f32::INFINITY; 3],
                ..transform(0.)
            },
        ] {
            let bad = [base[0], invalid];
            for (base, layer) in [(&base, &bad), (&bad, &base)] {
                assert_eq!(
                    blend_pose_into(base, layer, 0., Some(&mask), &mut out),
                    Err(AnimationError::InvalidPose)
                );
                assert_eq!(
                    additive_pose_into(base, layer, base, 0., Some(&mask), &mut out),
                    Err(AnimationError::InvalidPose)
                );
                assert_eq!(out, previous);
            }
            assert_eq!(
                additive_pose_into(&base, &base, &bad, 0., Some(&mask), &mut out),
                Err(AnimationError::InvalidPose)
            );
            assert_eq!(out, previous);
        }
    }

    #[test]
    fn additive_overflow_on_a_later_joint_is_transactional_and_mask_sensitive() {
        for scale in [false, true] {
            let mut huge = Transform::default();
            let mut negative = huge;
            if scale {
                huge.scale = [f32::MAX; 3];
                negative.scale = [-f32::MAX; 3];
            } else {
                huge.translation = [f32::MAX; 3];
                negative.translation = [-f32::MAX; 3];
            }
            let base = [transform(1.), huge];
            let layer = [transform(2.), huge];
            let reference = [transform(0.), negative];
            let mut out = [transform(99.); 2];
            let previous = out;
            assert_eq!(
                additive_pose_into(&base, &layer, &reference, 1., None, &mut out),
                Err(AnimationError::InvalidPose)
            );
            assert_eq!(out, previous);
            let mask = JointMask::new(vec![1., 0.]).unwrap();
            additive_pose_into(&base, &layer, &reference, 1., Some(&mask), &mut out).unwrap();
            assert_eq!(out[0].translation[0], 3.);
            assert_eq!(out[1], base[1]);
        }
    }

    #[test]
    fn failed_layer_application_preserves_stack_base_and_fade_continuation() {
        let huge = Transform {
            translation: [f32::MAX; 3],
            ..Transform::default()
        };
        let poses = [
            [transform(0.), huge],
            [transform(2.), huge],
            [transform(4.), huge],
            [transform(1.), Transform::default()],
        ];
        let bank = AnimationBank::new(
            vec![Transform::default(); 2],
            poses.iter().map(|p| clip(p, p)).collect(),
        )
        .unwrap();
        let mut player = Animator::new(&bank, 0).unwrap();
        let mut events = EventBuffer::<8>::default();
        player.transition(1, 1.).unwrap();
        player.advance(0.125, &mut events).unwrap();
        player.transition(2, 1.).unwrap();
        player
            .apply_layer(3, 0., 0.5, None, LayerMode::Additive)
            .unwrap();
        let mut expected = player.clone();
        let mask = JointMask::new(vec![1.]).unwrap();
        for mode in [LayerMode::Override, LayerMode::Additive] {
            for (clip, time, weight, mask, error) in [
                (99, 0., 1., None, AnimationError::InvalidClip),
                (1, f32::NAN, 1., None, AnimationError::InvalidTime),
                (1, 0., f32::NAN, None, AnimationError::InvalidWeight),
                (1, 0., 1., Some(&mask), AnimationError::JointCount),
            ] {
                assert_eq!(
                    player.apply_layer(clip, time, weight, mask, mode),
                    Err(error)
                );
                assert_eq!(player.pose(), expected.pose());
                assert_eq!(player.base_pose(), expected.base_pose());
            }
        }
        assert_eq!(
            player.apply_layer(1, 0., 1., None, LayerMode::Additive),
            Err(AnimationError::InvalidPose)
        );
        assert_eq!(player.pose(), expected.pose());
        assert_eq!(player.base_pose(), expected.base_pose());
        for dt in [0., 0.125, 1.] {
            let mut expected_events = EventBuffer::<8>::default();
            player.advance(dt, &mut events).unwrap();
            expected.advance(dt, &mut expected_events).unwrap();
            assert_eq!(player.pose(), expected.pose());
            assert_eq!(player.base_pose(), expected.base_pose());
            assert_eq!(events.hits(), expected_events.hits());
        }
    }
}
