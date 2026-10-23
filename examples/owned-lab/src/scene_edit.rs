//! Checked edits preserve the authored object indices used by existing browser saves.
use super::*;
use ferravex_engine::play::scene::ScenePathError;

/// A rejected edit leaves all scene transforms and both collision worlds unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneEditError {
    /// Disconnect before changing authoritative geometry.
    Connected,
    /// At most one edit per authored object is accepted in a batch.
    Capacity,
    /// Ground (index zero) is fixed; this index is absent or is the ground.
    InvalidObject(usize),
    /// An object appeared twice in the same batch.
    DuplicateObject(usize),
    /// Coordinates must be finite and strictly inside (-100, 100).
    InvalidPosition(usize),
    /// A render/collision handle no longer matches the authored scene.
    MissingGeometry(usize),
    /// Checked path lookup failed.
    Path(ScenePathError),
}

impl std::fmt::Display for SceneEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => f.write_str("disconnect the peer before editing geometry"),
            Self::Capacity => f.write_str("too many scene edits"),
            Self::InvalidObject(i) => write!(f, "object {i} is fixed or missing"),
            Self::DuplicateObject(i) => write!(f, "object {i} occurs twice"),
            Self::InvalidPosition(i) => {
                write!(f, "object {i} needs finite coordinates inside (-100, 100)")
            }
            Self::MissingGeometry(i) => {
                write!(f, "object {i} has missing scene or collision geometry")
            }
            Self::Path(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SceneEditError {}

impl Lab {
    /// Apply a bounded group of position edits atomically. All three representations
    /// (checked scene, FPS solids, dynamic-world terrain) are validated before mutation.
    /// Object indices and browser scene-v1 save rows are unchanged.
    ///
    /// ```
    /// use owned_lab::{Lab, SceneEditError};
    /// let mut lab = Lab::new_interactive();
    /// let before = lab.object(1);
    /// assert_eq!(lab.edit_positions(&[(1, [1., 2., 3.]), (0, [0.; 3])]),
    ///            Err(SceneEditError::InvalidObject(0)));
    /// assert_eq!(lab.object(1), before);
    /// lab.edit_positions(&[(1, [-20., 3., 0.])]).unwrap();
    /// assert_eq!(lab.object(1).unwrap()[0], -20.);
    /// ```
    pub fn edit_positions(&mut self, edits: &[(usize, [f32; 3])]) -> Result<(), SceneEditError> {
        if self.peer.mode != 0 {
            return Err(SceneEditError::Connected);
        }
        if edits.len() > self.objects.len() {
            return Err(SceneEditError::Capacity);
        }
        for (slot, &(index, position)) in edits.iter().enumerate() {
            if index == 0 || index >= self.objects.len() {
                return Err(SceneEditError::InvalidObject(index));
            }
            if edits[..slot].iter().any(|(prior, _)| *prior == index) {
                return Err(SceneEditError::DuplicateObject(index));
            }
            if !position.iter().all(|v| v.is_finite() && v.abs() < 100.) {
                return Err(SceneEditError::InvalidPosition(index));
            }
            let object = &self.objects[index];
            self.scene
                .node(object.node)
                .map_err(|_| SceneEditError::MissingGeometry(index))?;
            if let Some(solid) = object.solid {
                let body = self
                    .terrain
                    .get(solid)
                    .and_then(|handle| self.physics.body(*handle));
                if self.game.arena.solids.get(solid).is_none() || body.is_none() {
                    return Err(SceneEditError::MissingGeometry(index));
                }
            }
        }
        for &(index, position) in edits {
            let object = &self.objects[index];
            let mut transform = self.scene.node(object.node).unwrap().local();
            transform.translation = position;
            self.scene
                .set_local(object.node, transform)
                .expect("validated scene transform");
            if let Some(solid) = object.solid {
                self.physics
                    .body_mut(self.terrain[solid])
                    .unwrap()
                    .set_position(position)
                    .expect("validated finite terrain position");
                self.game.arena.solids[solid].center = position.into();
            }
        }
        Ok(())
    }

    /// Compatibility single-object edit. See [`Self::edit_positions`] for checked errors.
    pub fn edit(&mut self, index: usize, x: f32, y: f32, z: f32) -> bool {
        self.edit_positions(&[(index, [x, y, z])]).is_ok()
    }
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
    pub fn object(&self, index: usize) -> Option<[f32; 6]> {
        let local = self.scene.node(self.objects.get(index)?.node).ok()?.local();
        let [x, y, z] = local.translation;
        let [sx, sy, sz] = local.scale;
        Some([x, y, z, sx, sy, sz])
    }

    /// Resolve literal scene path components to the existing editor index.
    /// Names are stable for this authored scene, including the fixed ground.
    ///
    /// ```
    /// let lab = owned_lab::Lab::new_interactive();
    /// assert_eq!(lab.object_by_path(&["courtyard", "object-1"]).unwrap(), 1);
    /// assert!(lab.object_by_path(&["courtyard", "absent"]).is_err());
    /// ```
    pub fn object_by_path(&self, path: &[&str]) -> Result<usize, SceneEditError> {
        let node = self
            .scene
            .resolve_path(None, path)
            .map_err(SceneEditError::Path)?;
        self.objects
            .iter()
            .position(|object| object.node == node)
            .ok_or(SceneEditError::InvalidObject(usize::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Geometry = (Vec<[f32; 6]>, Vec<[f32; 3]>, Vec<[f32; 3]>);
    fn geometry(lab: &Lab) -> Geometry {
        (
            (0..lab.object_count())
                .map(|i| lab.object(i).unwrap())
                .collect(),
            lab.game
                .arena
                .solids
                .iter()
                .map(|s| s.center.to_array())
                .collect(),
            lab.terrain
                .iter()
                .map(|handle| lab.physics.body(*handle).unwrap().position())
                .collect(),
        )
    }
    #[test]
    fn checked_batches_reject_bad_rows_without_changing_any_geometry() {
        let mut lab = Lab::new_interactive();
        let before = geometry(&lab);
        for (edits, error) in [
            (
                vec![(1, [1.; 3]), (0, [2.; 3])],
                SceneEditError::InvalidObject(0),
            ),
            (
                vec![(1, [1.; 3]), (2, [f32::NAN; 3])],
                SceneEditError::InvalidPosition(2),
            ),
            (
                vec![(1, [1.; 3]), (2, [100.; 3])],
                SceneEditError::InvalidPosition(2),
            ),
            (
                vec![(1, [1.; 3]), (1, [2.; 3])],
                SceneEditError::DuplicateObject(1),
            ),
            (
                vec![(usize::MAX, [1.; 3])],
                SceneEditError::InvalidObject(usize::MAX),
            ),
        ] {
            assert_eq!(lab.edit_positions(&edits), Err(error));
            assert_eq!(geometry(&lab), before);
        }
        lab.peer.set_mode(1);
        assert_eq!(
            lab.edit_positions(&[(1, [1.; 3])]),
            Err(SceneEditError::Connected)
        );
        assert!(
            !lab.edit(1, 1., 1., 1.),
            "legacy ABI also enforces peer safety"
        );
        assert_eq!(geometry(&lab), before);
        lab.peer.set_mode(0);
        lab.physics.remove(lab.terrain[1]).unwrap();
        assert_eq!(
            lab.edit_positions(&[(1, [1.; 3]), (2, [2.; 3])]),
            Err(SceneEditError::MissingGeometry(2))
        );
        assert_eq!(lab.object(1), Some(before.0[1]));
        assert_eq!(lab.game.arena.solids[0].center.to_array(), before.1[0]);
    }
    #[test]
    fn checked_paths_and_committed_edits_share_render_and_collision_transforms() {
        let mut lab = Lab::new_interactive();
        assert_eq!(lab.object_by_path(&["courtyard", "object-1"]), Ok(1));
        assert!(matches!(
            lab.object_by_path(&["absent"]),
            Err(SceneEditError::Path(_))
        ));
        lab.edit_positions(&[(1, [-20., 3., 2.]), (2, [20., 3., -2.])])
            .unwrap();
        lab.update_interactive(0., InputFrame::default(), false, 2, 1.);
        for (index, position) in [(1, [-20., 3., 2.]), (2, [20., 3., -2.])] {
            assert_eq!(&lab.object(index).unwrap()[..3], &position);
            assert_eq!(lab.game.arena.solids[index - 1].center.to_array(), position);
            assert_eq!(
                lab.physics.body(lab.terrain[index - 1]).unwrap().position(),
                position
            );
            assert_eq!(
                lab.items[index]
                    .transform
                    .transform_point3(Vec3::ZERO)
                    .to_array(),
                position
            );
        }
    }
}
