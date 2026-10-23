//! Session extensions. Legacy input/full snapshots remain available within v4.
use super::*;
pub const MAX_CLOCK_US: u64 = 1 << 53;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimedInput {
    pub input: InputFrame,
    pub shot_tick: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Client {
    Inputs {
        session: u64,
        inputs: Vec<TimedInput>,
    },
    SnapshotAck {
        session: u64,
        tick: Option<u32>,
    },
    Ping {
        session: u64,
        nonce: u32,
        client_us: u64,
    },
    Command {
        session: u64,
        event: reliable::EventPacket,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Server {
    SnapshotDelta {
        session: u64,
        delta: delta::Delta,
    },
    Pong {
        session: u64,
        nonce: u32,
        client_us: u64,
        receive_us: u64,
        send_us: u64,
        tick: u32,
    },
    CommandAck {
        session: u64,
        sequence: u64,
    },
}
fn session(out: &mut Encoder<'_>, id: u64) -> Result<(), EncodeError> {
    valid_session(id)?;
    put_u64(out, id);
    Ok(())
}
fn put_tick(out: &mut Encoder<'_>, tick: Option<u32>) {
    out.push(tick.is_some() as u8);
    put_u32(out, tick.unwrap_or(0));
}
fn read_tick(r: &mut Reader<'_>) -> Result<Option<u32>, ProtocolError> {
    let present = r.boolean()?;
    let tick = r.u32()?;
    if !present && tick != 0 {
        return Err(ProtocolError::Invalid);
    }
    Ok(present.then_some(tick))
}
fn read_session(r: &mut Reader<'_>) -> Result<u64, ProtocolError> {
    let id = r.u64()?;
    if id == 0 {
        Err(ProtocolError::Invalid)
    } else {
        Ok(id)
    }
}
fn payload(r: &mut Reader<'_>, max: usize) -> Result<Vec<u8>, ProtocolError> {
    let n = r.u16()? as usize;
    if n > max {
        return Err(ProtocolError::TooLarge);
    }
    let end = r.pos.checked_add(n).ok_or(ProtocolError::Invalid)?;
    let bytes = r.data.get(r.pos..end).ok_or(ProtocolError::Truncated)?;
    r.pos = end;
    Ok(bytes.to_vec())
}
#[cfg(test)]
pub(super) fn encode_client(p: &Client) -> Result<Vec<u8>, ProtocolError> {
    encode_allocated(|out| write_client(p, out))
}
pub(super) fn write_client(p: &Client, out: &mut Encoder<'_>) -> Result<(), EncodeError> {
    if let Client::Command { session, event } = p {
        return BorrowedEvent::Command {
            session: *session,
            event,
        }
        .write(out);
    }
    out.header(match p {
        Client::Inputs { .. } => 7,
        Client::SnapshotAck { .. } => 8,
        Client::Ping { .. } => 9,
        Client::Command { .. } => unreachable!("command encoded above"),
    });
    match p {
        Client::Inputs {
            session: id,
            inputs,
        } => {
            session(out, *id)?;
            valid_len("inputs", inputs.len(), 1, MAX_INPUTS_PER_PACKET)?;
            out.push(inputs.len() as u8);
            for (index, t) in inputs.iter().enumerate() {
                validate_input(&t.input, index)?;
                put_input(out, &t.input);
                put_tick(out, t.shot_tick);
            }
        }
        Client::SnapshotAck { session: id, tick } => {
            session(out, *id)?;
            put_tick(out, *tick);
        }
        Client::Ping {
            session: id,
            nonce,
            client_us,
        } => {
            session(out, *id)?;
            valid_field(*client_us <= MAX_CLOCK_US, "client_us", None)?;
            put_u32(out, *nonce);
            put_u64(out, *client_us);
        }
        Client::Command { .. } => unreachable!("command encoded above"),
    }
    Ok(())
}
pub(super) fn decode_client(r: &mut Reader<'_>, kind: u8) -> Result<Client, ProtocolError> {
    let session = read_session(r)?;
    Ok(match kind {
        7 => {
            let n = r.u8()? as usize;
            if !(1..=MAX_INPUTS_PER_PACKET).contains(&n) {
                return Err(ProtocolError::Invalid);
            }
            let mut inputs = Vec::with_capacity(n);
            for _ in 0..n {
                inputs.push(TimedInput {
                    input: r.input()?,
                    shot_tick: read_tick(r)?,
                });
            }
            Client::Inputs { session, inputs }
        }
        8 => Client::SnapshotAck {
            session,
            tick: read_tick(r)?,
        },
        9 => {
            let nonce = r.u32()?;
            let client_us = r.u64()?;
            if client_us > MAX_CLOCK_US {
                return Err(ProtocolError::Invalid);
            }
            Client::Ping {
                session,
                nonce,
                client_us,
            }
        }
        10 => {
            let sequence = r.u64()?;
            let payload = payload(r, reliable::EVENT_BYTES)?;
            if payload.is_empty() {
                return Err(ProtocolError::Invalid);
            }
            Client::Command {
                session,
                event: reliable::EventPacket { sequence, payload },
            }
        }
        _ => return Err(ProtocolError::Kind),
    })
}
#[cfg(test)]
pub(super) fn encode_server(p: &Server) -> Result<Vec<u8>, ProtocolError> {
    encode_allocated(|out| write_server(p, out))
}
pub(super) fn write_server(p: &Server, out: &mut Encoder<'_>) -> Result<(), EncodeError> {
    out.header(match p {
        Server::SnapshotDelta { .. } => 11,
        Server::Pong { .. } => 12,
        Server::CommandAck { .. } => 13,
    });
    match p {
        Server::SnapshotDelta { session: id, delta } => {
            session(out, *id)?;
            valid_field(
                sequence_newer(delta.tick, delta.base_tick),
                "delta.tick",
                None,
            )?;
            valid_len("delta.bytes", delta.bytes.len(), 3, MAX_PACKET_BYTES - 24)?;
            put_u32(out, delta.base_tick);
            put_u32(out, delta.tick);
            put_u16(out, delta.bytes.len() as u16);
            out.extend_from_slice(&delta.bytes);
        }
        Server::Pong {
            session: id,
            nonce,
            client_us,
            receive_us,
            send_us,
            tick,
        } => {
            session(out, *id)?;
            valid_field(*client_us <= MAX_CLOCK_US, "client_us", None)?;
            valid_field(*receive_us <= MAX_CLOCK_US, "receive_us", None)?;
            valid_field(*send_us <= MAX_CLOCK_US, "send_us", None)?;
            valid_field(send_us >= receive_us, "pong.send_order", None)?;
            valid_field(
                send_us - receive_us <= 5_000_000,
                "pong.processing_us",
                None,
            )?;
            put_u32(out, *nonce);
            for value in [client_us, receive_us, send_us] {
                put_u64(out, *value);
            }
            put_u32(out, *tick);
        }
        Server::CommandAck {
            session: id,
            sequence,
        } => {
            session(out, *id)?;
            put_u64(out, *sequence);
        }
    }
    Ok(())
}
fn validate_pong(client: u64, receive: u64, send: u64) -> Result<(), ProtocolError> {
    if client > MAX_CLOCK_US
        || receive > MAX_CLOCK_US
        || send > MAX_CLOCK_US
        || send < receive
        || send - receive > 5_000_000
    {
        Err(ProtocolError::Invalid)
    } else {
        Ok(())
    }
}
pub(super) fn decode_server(r: &mut Reader<'_>, kind: u8) -> Result<Server, ProtocolError> {
    let session = read_session(r)?;
    Ok(match kind {
        11 => {
            let base_tick = r.u32()?;
            let tick = r.u32()?;
            let bytes = payload(r, MAX_PACKET_BYTES - 24)?;
            if !sequence_newer(tick, base_tick) || bytes.len() < 3 {
                return Err(ProtocolError::Invalid);
            }
            Server::SnapshotDelta {
                session,
                delta: delta::Delta {
                    base_tick,
                    tick,
                    bytes,
                },
            }
        }
        12 => {
            let nonce = r.u32()?;
            let client_us = r.u64()?;
            let receive_us = r.u64()?;
            let send_us = r.u64()?;
            let tick = r.u32()?;
            validate_pong(client_us, receive_us, send_us)?;
            Server::Pong {
                session,
                nonce,
                client_us,
                receive_us,
                send_us,
                tick,
            }
        }
        13 => Server::CommandAck {
            session,
            sequence: r.u64()?,
        },
        _ => return Err(ProtocolError::Kind),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_roundtrips_reject_truncation_versions_trailing_and_semantics() {
        let clients = [
            Client::Inputs {
                session: 1,
                inputs: vec![TimedInput {
                    input: InputFrame::default(),
                    shot_tick: Some(u32::MAX),
                }],
            },
            Client::SnapshotAck {
                session: 1,
                tick: None,
            },
            Client::Ping {
                session: 1,
                nonce: 7,
                client_us: 100,
            },
            Client::Command {
                session: 1,
                event: reliable::EventPacket {
                    sequence: 2,
                    payload: vec![1, 3],
                },
            },
        ];
        for p in clients {
            let p = ClientPacket::Extension(p);
            let b = super::super::encode_client(&p).unwrap();
            assert_eq!(super::super::decode_client(&b).unwrap(), p);
            for n in 0..b.len() {
                assert!(super::super::decode_client(&b[..n]).is_err());
            }
            let mut bad = b;
            bad.push(0);
            assert!(super::super::decode_client(&bad).is_err());
        }
        let servers = [
            Server::SnapshotDelta {
                session: 2,
                delta: delta::Delta::between(u32::MAX, &[0; 20], 0, &[0; 20]).unwrap(),
            },
            Server::Pong {
                session: 1,
                nonce: 3,
                client_us: 100,
                receive_us: 1000,
                send_us: 1001,
                tick: 0,
            },
            Server::CommandAck {
                session: 1,
                sequence: 3,
            },
        ];
        for p in servers {
            let p = ServerPacket::Extension(p);
            let b = super::super::encode_server(&p).unwrap();
            assert_eq!(super::super::decode_server(&b).unwrap(), p);
            for n in 0..b.len() {
                assert!(super::super::decode_server(&b[..n]).is_err());
            }
        }
        assert!(
            encode_client(&Client::Ping {
                session: 0,
                nonce: 0,
                client_us: 0
            })
            .is_err()
        );
        assert!(
            encode_server(&Server::Pong {
                session: 1,
                nonce: 0,
                client_us: 0,
                receive_us: 2,
                send_us: 1,
                tick: 0
            })
            .is_err()
        );
        let mut bad = super::super::encode_client(&ClientPacket::Extension(Client::SnapshotAck {
            session: 1,
            tick: None,
        }))
        .unwrap();
        bad[15] = 1;
        assert!(super::super::decode_client(&bad).is_err());
        bad[4] = 4;
        assert_eq!(
            super::super::decode_client(&bad),
            Err(ProtocolError::Version)
        );
    }
}
