//! Runtime presentation settings: load once, persist only explicit changes.
use crate::{
    audio,
    lighting::Quality,
    storage::{LoadStatus, PlatformStore, Slot, StorageError},
};
pub use ferravex_play::persistence::Preferences;
pub(crate) struct Session {
    slot: Slot<Preferences>,
    current: Preferences,
    error: Option<StorageError>,
}
impl Session {
    pub fn new(owner: &str, transient: bool) -> Self {
        let backend = if transient {
            PlatformStore::transient()
        } else {
            PlatformStore::new()
        };
        let slot = Slot::with_backend(owner, "preferences", Preferences::default(), backend)
            .expect("validated preferences owner");
        let current = *slot.value();
        let error = if let LoadStatus::Rejected(e) = slot.status() {
            Some(e)
        } else {
            None
        };
        let session = Self {
            slot,
            current,
            error,
        };
        if !transient {
            apply(current);
        }
        session
    }
    pub fn current(&self) -> Preferences {
        self.current
    }
    pub fn error(&self) -> Option<StorageError> {
        self.error
    }
    pub fn set(&mut self, next: Preferences) -> Result<(), StorageError> {
        if !next.valid() {
            return Err(StorageError::Invalid(
                ferravex_play::persistence::SaveError::InvalidData,
            ));
        }
        if next == self.current {
            return self.error.map_or(Ok(()), Err);
        }
        self.current = next;
        apply(next);
        self.save()
    }
    pub fn save(&mut self) -> Result<(), StorageError> {
        let result = self.slot.commit(self.current);
        self.error = result.err();
        result
    }
    pub fn reset(&mut self) -> Result<(), StorageError> {
        self.current = Preferences::default();
        apply(self.current);
        let result = self.slot.reset(self.current);
        self.error = result.err();
        result
    }
    /// External sound buttons may change the mixer between frames. Detect that
    /// change in memory and write once; failed writes are not retried per frame.
    pub fn sync_audio(&mut self) {
        let next = Preferences {
            master: audio::master_gain(),
            muted: audio::muted(),
            buses: audio::bus_gains(),
            ..self.current
        };
        if next != self.current {
            self.current = next;
            let _ = self.save();
        }
    }
}
pub(crate) fn apply(p: Preferences) {
    crate::edit(|s| s.presentation = p);
    crate::set_fidelity(p.resolution, p.shadows, p.lights);
    crate::set_look(crate::Look::from_index(p.look as u32).expect("validated look"));
    audio::set_master_gain(p.master);
    audio::set_muted(p.muted);
    for (bus, gain) in [
        audio::Bus::Effects,
        audio::Bus::Ambience,
        audio::Bus::Music,
        audio::Bus::Ui,
    ]
    .into_iter()
    .zip(p.buses)
    {
        audio::set_bus_gain(bus, gain);
    }
    crate::set_quality_preference(p.quality.map(|q| match q {
        0 => Quality::Low,
        1 => Quality::Medium,
        _ => Quality::High,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_changes_persist_only_once_and_explicit_quality_overrides_win() {
        crate::edit(|s| *s = crate::State::default());
        crate::audio::reset();
        let mut session = Session::new("preferences-test", true);
        let p = Preferences {
            master: 0.25,
            buses: [0.2, 0.3, 0.4, 0.5],
            muted: true,
            quality: Some(2),
            look: 1,
            ..Default::default()
        };
        session.set(p).unwrap();
        assert_eq!(audio::master_gain(), 0.25);
        assert_eq!(audio::bus_gains(), p.buses);
        assert!(audio::muted());
        assert_eq!(crate::quality(), Quality::High);
        let stamp = session.slot.generation();
        for _ in 0..1000 {
            session.sync_audio();
        }
        assert_eq!(session.slot.generation(), stamp);
        crate::set_quality(Quality::Low);
        assert_eq!(crate::quality(), Quality::High);
        crate::quality_override(Quality::Medium);
        session
            .set(Preferences {
                quality: Some(0),
                ..p
            })
            .unwrap();
        assert_eq!(crate::quality(), Quality::Medium);
        audio::set_muted(false);
        session.sync_audio();
        assert!(!session.current().muted);
        assert_eq!(session.slot.generation(), stamp + 2);
        let previous = session.current();
        assert!(
            session
                .set(Preferences {
                    master: f32::NAN,
                    ..previous
                })
                .is_err()
        );
        assert_eq!(session.current(), previous);
        session.reset().unwrap();
        assert_eq!(session.current(), Preferences::default());
        assert_eq!(crate::quality(), Quality::Medium);
    }
}
