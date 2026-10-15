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
    Transform {
        translation: <[f32; 3] as Sample>::blend(a.translation, b.translation, weight),
        rotation: <[f32; 4] as Sample>::blend(a.rotation, b.rotation, weight),
        scale: <[f32; 3] as Sample>::blend(a.scale, b.scale, weight),
    }
}
fn additive(base: Transform, layer: Transform, reference: Transform, w: f32) -> Transform {
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
/// before writing any output. See LayerMode/Animator::apply_layer for a reusable
/// in-place layer stack using the bank's bind pose as its reference.
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
/// reapplying aim/recoil cannot bake the same layer into the source twice.
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
    pub fn transition(&mut self, clip: usize, seconds: f32) -> Result<(), AnimationError> {
        if !seconds.is_finite() || !(0.0..=60.).contains(&seconds) {
            return Err(AnimationError::InvalidTime);
        }
        if clip >= self.bank.0.clips.len() {
            return Err(AnimationError::InvalidClip);
        }
        if clip == self.active.clip {
            return Ok(());
        }
        if seconds == 0. {
            return self.play(clip);
        }
        let mut next = Self::playing(&self.bank, clip)?;
        next.clock.set_speed(self.active.clock.speed())?;
        next.clock.set_paused(self.active.clock.paused());
        let source = if self.transition.is_some() {
            self.source.copy_from_slice(&self.base);
            None
        } else {
            Some(self.active.clone())
        };
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
    /// Apply one visual layer until the next advance/play/seek. Repeat to stack
    /// masked overrides or additive recoil/aim over the current base pose.
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
        let at = |i: usize, base: Transform, layer: Transform| {
            let w = weight * mask.map_or(1., |m| m.weights[i]);
            match mode {
                LayerMode::Override => blended(base, layer, w),
                LayerMode::Additive => additive(base, layer, self.bank.0.bind[i], w),
            }
        };
        if (0..self.pose.len()).any(|i| !at(i, self.pose[i], self.target[i]).valid()) {
            return Err(AnimationError::InvalidPose);
        }
        // target is scratch, so the operation can commit only after validation.
        for i in 0..self.pose.len() {
            self.pose[i] = at(i, self.pose[i], self.target[i]);
        }
        Ok(())
    }
}
