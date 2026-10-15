//! Validated, compact gameplay event payloads inside the reliable session channel.
use super::{GameEvent, MAX_PLAYERS};
use ferravex_math::Vec3;
use ferravex_net::ProtocolError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetworkEvent {
    /// Authority tick after the event's simulation step.
    pub tick: u32,
    pub event: GameEvent,
}
impl NetworkEvent {
    pub fn encode(self) -> Result<Vec<u8>, ProtocolError> {
        let mut out = Vec::with_capacity(32);
        out.extend_from_slice(&self.tick.to_le_bytes());
        match self.event {
            GameEvent::Shot {
                player,
                origin,
                end,
            } => {
                validate_id(player)?;
                validate_position(origin)?;
                validate_position(end)?;
                out.extend_from_slice(&[1, player]);
                for coordinate in origin.to_array().into_iter().chain(end.to_array()) {
                    out.extend_from_slice(&coordinate.to_le_bytes());
                }
            }
            GameEvent::Hit {
                attacker,
                victim,
                damage,
                headshot,
            } => {
                validate_id(attacker)?;
                validate_id(victim)?;
                if damage > 100 || attacker == victim {
                    return Err(ProtocolError::Invalid);
                }
                out.extend_from_slice(&[2, attacker, victim, damage, u8::from(headshot)]);
            }
            GameEvent::Killed { attacker, victim } => {
                validate_id(attacker)?;
                validate_id(victim)?;
                if attacker == victim {
                    return Err(ProtocolError::Invalid);
                }
                out.extend_from_slice(&[3, attacker, victim]);
            }
            GameEvent::Respawned { player } => {
                validate_id(player)?;
                out.extend_from_slice(&[4, player]);
            }
        }
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 6 {
            return Err(ProtocolError::Truncated);
        }
        let tick = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let event = match (bytes[4], bytes.len()) {
            (1, 30) => {
                let values: [f32; 6] = std::array::from_fn(|i| {
                    f32::from_le_bytes(bytes[6 + i * 4..10 + i * 4].try_into().unwrap())
                });
                GameEvent::Shot {
                    player: bytes[5],
                    origin: Vec3::new(values[0], values[1], values[2]),
                    end: Vec3::new(values[3], values[4], values[5]),
                }
            }
            (2, 9) if bytes[8] <= 1 => GameEvent::Hit {
                attacker: bytes[5],
                victim: bytes[6],
                damage: bytes[7],
                headshot: bytes[8] != 0,
            },
            (3, 7) => GameEvent::Killed {
                attacker: bytes[5],
                victim: bytes[6],
            },
            (4, 6) => GameEvent::Respawned { player: bytes[5] },
            _ => return Err(ProtocolError::Invalid),
        };
        let result = Self { tick, event };
        // Centralize the semantic checks so all accepted payloads are canonical.
        result.encode()?;
        Ok(result)
    }
}
fn validate_id(id: u8) -> Result<(), ProtocolError> {
    if id as usize >= MAX_PLAYERS {
        Err(ProtocolError::Invalid)
    } else {
        Ok(())
    }
}
fn validate_position(position: Vec3) -> Result<(), ProtocolError> {
    if !position.is_finite() || position.to_array().iter().any(|v| v.abs() > 8192.) {
        Err(ProtocolError::Invalid)
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_game_event_roundtrips_and_malformed_payloads_fail() {
        for event in [
            GameEvent::Shot {
                player: 0,
                origin: Vec3::Y,
                end: Vec3::new(1., 2., 3.),
            },
            GameEvent::Hit {
                attacker: 0,
                victim: 1,
                damage: 34,
                headshot: false,
            },
            GameEvent::Killed {
                attacker: 0,
                victim: 1,
            },
            GameEvent::Respawned { player: 1 },
        ] {
            let event = NetworkEvent {
                tick: u32::MAX,
                event,
            };
            let bytes = event.encode().unwrap();
            assert_eq!(NetworkEvent::decode(&bytes).unwrap(), event);
            for n in 0..bytes.len() {
                assert!(NetworkEvent::decode(&bytes[..n]).is_err());
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(NetworkEvent::decode(&trailing).is_err());
        }
        assert!(
            NetworkEvent {
                tick: 0,
                event: GameEvent::Shot {
                    player: 0,
                    origin: Vec3::new(f32::NAN, 0., 0.),
                    end: Vec3::ZERO
                }
            }
            .encode()
            .is_err()
        );
        assert!(NetworkEvent::decode(&[0, 0, 0, 0, 2, 0, 1, 34, 2]).is_err());
    }
}
