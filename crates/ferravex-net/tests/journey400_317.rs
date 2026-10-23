//! Task 317: frozen v5 bytes, atomic caller buffers and borrowed reliable envelopes.
use ferravex_net::*;
use ferravex_net::{delta::Delta, reliable::EventPacket};

fn event(len: usize) -> EventPacket {
    EventPacket {
        sequence: u64::MAX,
        payload: (0..len).map(|i| i as u8).collect(),
    }
}
fn frame(sequence: u32) -> InputFrame {
    InputFrame {
        sequence,
        move_x: -1.,
        move_z: 1.,
        yaw: std::f32::consts::PI,
        pitch: -MAX_PITCH,
        buttons: BUTTON_JUMP | BUTTON_FIRE | BUTTON_RELOAD,
    }
}
fn player(id: u8) -> PlayerSnapshot {
    PlayerSnapshot {
        id,
        position: [-4096., 0., 4096.],
        velocity: [-256., 0., 256.],
        yaw: -std::f32::consts::PI,
        pitch: MAX_PITCH,
        health: 100,
        ammo: 30,
        grounded: true,
        jump_coyote: 0.5,
        jump_buffer: 0.5,
        jump_held: true,
        fire_cooldown: 4096,
        reload_ticks: 4096,
        respawn_ticks: 4096,
    }
}
fn corpus() -> (Vec<ClientPacket>, Vec<ServerPacket>) {
    let mut clients = vec![
        ClientPacket::Hello { nonce: 0 },
        ClientPacket::Hello { nonce: u64::MAX },
        ClientPacket::EventAck {
            session: 1,
            sequence: u64::MAX,
        },
        ClientPacket::Input {
            session: u64::MAX,
            inputs: vec![InputFrame::default()],
        },
        ClientPacket::Input {
            session: 1,
            inputs: (0..6).map(frame).collect(),
        },
        ClientPacket::Extension(ClientExtension::Inputs {
            session: 1,
            inputs: vec![TimedInput::default()],
        }),
        ClientPacket::Extension(ClientExtension::Inputs {
            session: u64::MAX,
            inputs: (0..6)
                .map(|i| TimedInput {
                    input: frame(u32::MAX - i),
                    shot_tick: [None, Some(0), Some(u32::MAX)][i as usize % 3],
                })
                .collect(),
        }),
    ];
    for tick in [None, Some(0), Some(u32::MAX)] {
        clients.push(ClientPacket::Extension(ClientExtension::SnapshotAck {
            session: 1,
            tick,
        }));
    }
    for client_us in [0, 1 << 53] {
        clients.push(ClientPacket::Extension(ClientExtension::Ping {
            session: 1,
            nonce: u32::MAX,
            client_us,
        }));
    }
    for len in [1, 900] {
        clients.push(ClientPacket::Extension(ClientExtension::Command {
            session: 1,
            event: event(len),
        }));
    }
    let mut servers = vec![
        ServerPacket::Welcome {
            nonce: 0,
            session: 1,
            player_id: 0,
            tick: 0,
        },
        ServerPacket::Welcome {
            nonce: u64::MAX,
            session: u64::MAX,
            player_id: 15,
            tick: u32::MAX,
        },
        ServerPacket::Snapshot(Snapshot {
            session: 1,
            ..Snapshot::default()
        }),
        ServerPacket::Snapshot(Snapshot {
            session: u64::MAX,
            tick: u32::MAX,
            ack_sequence: u32::MAX,
            ack_valid: true,
            epochs: [u32::MAX; 16],
            players: (0..16).map(player).collect(),
        }),
        ServerPacket::Snapshot(Snapshot {
            session: 1,
            ack_sequence: u32::MAX,
            ack_valid: false,
            epochs: {
                let mut e = [0; 16];
                e[15] = 9;
                e
            },
            players: vec![player(15)],
            ..Snapshot::default()
        }),
    ];
    for len in [0, 1, 900] {
        servers.push(ServerPacket::ReliableEvent {
            session: 1,
            event: event(len),
        });
    }
    // The envelope historically accepts opaque/noncanonical inner deltas, even
    // malformed inner tags. Do not strengthen outer validation using applied_len.
    for bytes in [
        vec![0, 0, 0],
        vec![4, 0, 1, 0, 0, 7, 1, 0, 7, 2, 0, 7, 3, 0, 7],
        vec![255, 255, 255],
        vec![3; 1176],
    ] {
        servers.push(ServerPacket::Extension(ServerExtension::SnapshotDelta {
            session: 1,
            delta: Delta {
                base_tick: u32::MAX,
                tick: 0,
                bytes,
            },
        }));
    }
    for (client_us, receive_us, send_us) in [(0, 0, 0), (1 << 53, (1 << 53) - 5_000_000, 1 << 53)] {
        servers.push(ServerPacket::Extension(ServerExtension::Pong {
            session: u64::MAX,
            nonce: u32::MAX,
            client_us,
            receive_us,
            send_us,
            tick: u32::MAX,
        }));
    }
    servers.push(ServerPacket::Extension(ServerExtension::CommandAck {
        session: 1,
        sequence: u64::MAX,
    }));
    (clients, servers)
}

// Captured from the untouched task-316 root (SHA-256 7d09a4e5...) and
// original wire_v4 codec before the first encoding edit, using this exact corpus.
// These are wire-byte fixtures, not values generated by the new encoder.
const GOLDEN: &[&str] = &[
    "4656525805010000000000000000",
    "465652580501ffffffffffffffff",
    "4656525805060100000000000000ffffffffffffffff",
    "465652580502ffffffffffffffff01000000000000000000000000000000000000000000",
    "46565258050201000000000000000600000000000080bf0000803fdb0f49406666c6bf0701000000000080bf0000803fdb0f49406666c6bf0702000000000080bf0000803fdb0f49406666c6bf0703000000000080bf0000803fdb0f49406666c6bf0704000000000080bf0000803fdb0f49406666c6bf0705000000000080bf0000803fdb0f49406666c6bf07",
    "4656525805070100000000000000010000000000000000000000000000000000000000000000000000",
    "465652580507ffffffffffffffff06ffffffff000080bf0000803fdb0f49406666c6bf070000000000feffffff000080bf0000803fdb0f49406666c6bf070100000000fdffffff000080bf0000803fdb0f49406666c6bf0701fffffffffcffffff000080bf0000803fdb0f49406666c6bf070000000000fbffffff000080bf0000803fdb0f49406666c6bf070100000000faffffff000080bf0000803fdb0f49406666c6bf0701ffffffff",
    "46565258050801000000000000000000000000",
    "46565258050801000000000000000100000000",
    "465652580508010000000000000001ffffffff",
    "4656525805090100000000000000ffffffff0000000000000000",
    "4656525805090100000000000000ffffffff0000000000002000",
    "46565258050a0100000000000000ffffffffffffffff010000",
    "46565258050a0100000000000000ffffffffffffffff8403000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283",
    "465652580503000000000000000001000000000000000000000000",
    "465652580503ffffffffffffffffffffffffffffffff0fffffffff",
    "46565258050401000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "465652580504ffffffffffffffffffffffffffffffff01ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1000000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001001000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001002000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001003000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001004000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001005000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001006000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001007000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001008000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f0100100010001009000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f010010001000100a000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f010010001000100b000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f010010001000100c000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f010010001000100d000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f010010001000100e000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f010010001000100f000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f01001000100010",
    "465652580504010000000000000000000000ffffffff0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000009000000010f000080c50000000000008045000080c30000000000008043db0f49c06666c63f641e010000003f0000003f01001000100010",
    "4656525805050100000000000000ffffffffffffffff0000",
    "4656525805050100000000000000ffffffffffffffff010000",
    "4656525805050100000000000000ffffffffffffffff8403000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283",
    "46565258050b0100000000000000ffffffff000000000300000000",
    "46565258050b0100000000000000ffffffff000000000f00040001000007010007020007030007",
    "46565258050b0100000000000000ffffffff000000000300ffffff",
    "46565258050b0100000000000000ffffffff000000009804030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303",
    "46565258050cffffffffffffffffffffffff000000000000000000000000000000000000000000000000ffffffff",
    "46565258050cffffffffffffffffffffffff0000000000002000c0b4b3ffffff1f000000000000002000ffffffff",
    "46565258050d0100000000000000ffffffffffffffff",
];
fn unhex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b| match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                _ => panic!("invalid fixture"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}
fn buffers(
    expected: &[u8],
    length: Result<usize, EncodeError>,
    encode: impl Fn(&mut [u8]) -> Result<usize, EncodeError>,
    every_short: bool,
) {
    let n = expected.len();
    assert_eq!(length, Ok(n));
    let mut output = [0xa5; MAX_PACKET_BYTES + 3];
    // A full backing allocation does not make an undersized slice acceptable.
    for capacity in (0..n).filter(|c| every_short || *c == 0 || *c == n - 1) {
        assert_eq!(
            encode(&mut output[..capacity]),
            Err(EncodeError::Capacity {
                required: n,
                available: capacity
            })
        );
        assert_eq!(output, [0xa5; MAX_PACKET_BYTES + 3]);
    }
    assert_eq!(encode(&mut output[..n]), Ok(n));
    assert_eq!(&output[..n], expected);
    assert!(output[n..].iter().all(|b| *b == 0xa5));
    output.fill(0xa5);
    assert_eq!(encode(&mut output), Ok(n));
    assert_eq!(&output[..n], expected);
    assert!(output[n..].iter().all(|b| *b == 0xa5));
}
#[test]
fn every_tag_and_edge_matches_frozen_bytes_and_every_short_buffer_is_atomic() {
    let (clients, servers) = corpus();
    assert_eq!(clients.len() + servers.len(), GOLDEN.len());
    for (packet, hex) in clients.iter().zip(GOLDEN.iter()) {
        let expected = unhex(hex);
        assert_eq!(encode_client(packet).unwrap(), expected);
        assert_eq!(decode_client(&expected).unwrap(), *packet);
        buffers(
            &expected,
            encoded_client_len(packet),
            |out| encode_client_into(packet, out),
            true,
        );
    }
    let mut saw_limit = false;
    for (packet, hex) in servers.iter().zip(&GOLDEN[clients.len()..]) {
        let expected = unhex(hex);
        saw_limit |= expected.len() == MAX_PACKET_BYTES;
        assert_eq!(encode_server(packet).unwrap(), expected);
        assert_eq!(decode_server(&expected).unwrap(), *packet);
        buffers(
            &expected,
            encoded_server_len(packet),
            |out| encode_server_into(packet, out),
            true,
        );
    }
    assert!(saw_limit);
}

fn generated() -> (Vec<ClientPacket>, Vec<ServerPacket>) {
    let (mut clients, mut servers) = (Vec::new(), Vec::new());
    let mut seed = 0x3170_0317_u32;
    for i in 0..128_u32 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let session = u64::from(seed) + 1;
        let inputs: Vec<_> = (0..i % 6 + 1)
            .map(|j| InputFrame {
                sequence: seed.wrapping_add(j),
                move_x: (i as i32 % 17 - 8) as f32 / 8.,
                move_z: -0.,
                yaw: (j as f32 - 3.) / 2.,
                pitch: if i % 2 == 0 { f32::from_bits(1) } else { -1. },
                buttons: i as u8 % 8,
            })
            .collect();
        clients.extend([
            ClientPacket::Hello {
                nonce: u64::from(seed) << 32,
            },
            ClientPacket::EventAck {
                session,
                sequence: u64::from(seed),
            },
            ClientPacket::Input {
                session,
                inputs: inputs.clone(),
            },
            ClientPacket::Extension(ClientExtension::Inputs {
                session,
                inputs: inputs
                    .into_iter()
                    .map(|input| TimedInput {
                        input,
                        shot_tick: (i % 2 == 0).then_some(seed),
                    })
                    .collect(),
            }),
            ClientPacket::Extension(ClientExtension::SnapshotAck {
                session,
                tick: (i % 2 == 0).then_some(seed),
            }),
            ClientPacket::Extension(ClientExtension::Ping {
                session,
                nonce: seed,
                client_us: u64::from(seed),
            }),
            ClientPacket::Extension(ClientExtension::Command {
                session,
                event: event(i as usize % 31 + 1),
            }),
        ]);
        let players: Vec<_> = (0..i % 17)
            .map(|id| PlayerSnapshot {
                position: [i as f32, -0., f32::from_bits(1)],
                ..player(id as u8)
            })
            .collect();
        let mut epochs = [0; 16];
        for p in &players {
            epochs[p.id as usize] = seed;
        }
        servers.extend([
            ServerPacket::Welcome {
                nonce: u64::from(seed),
                session,
                player_id: i as u8 % 16,
                tick: seed,
            },
            ServerPacket::Snapshot(Snapshot {
                session,
                tick: seed,
                ack_sequence: i,
                ack_valid: i % 2 == 0,
                epochs,
                players,
            }),
            ServerPacket::ReliableEvent {
                session,
                event: event(i as usize % 37),
            },
            ServerPacket::Extension(ServerExtension::SnapshotDelta {
                session,
                delta: Delta {
                    base_tick: seed,
                    tick: seed.wrapping_add(1),
                    bytes: vec![i as u8; i as usize % 41 + 3],
                },
            }),
            ServerPacket::Extension(ServerExtension::Pong {
                session,
                nonce: i,
                client_us: u64::from(seed),
                receive_us: u64::from(seed),
                send_us: u64::from(seed) + u64::from(i),
                tick: seed,
            }),
            ServerPacket::Extension(ServerExtension::CommandAck {
                session,
                sequence: u64::from(seed),
            }),
        ]);
    }
    (clients, servers)
}
fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for b in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
        hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    hash
}
#[test]
fn bounded_generated_packets_match_original_fingerprint_decode_and_encode_into() {
    let (clients, servers) = generated();
    assert_eq!(clients.len() + servers.len(), 1664);
    let mut hash = 0xcbf29ce484222325;
    for packet in &clients {
        let bytes = encode_client(packet).unwrap();
        hash = hash_bytes(hash, &bytes);
        assert_eq!(decode_client(&bytes).unwrap(), *packet);
        buffers(
            &bytes,
            encoded_client_len(packet),
            |out| encode_client_into(packet, out),
            false,
        );
    }
    for packet in &servers {
        let bytes = encode_server(packet).unwrap();
        hash = hash_bytes(hash, &bytes);
        assert_eq!(decode_server(&bytes).unwrap(), *packet);
        buffers(
            &bytes,
            encoded_server_len(packet),
            |out| encode_server_into(packet, out),
            false,
        );
    }
    // FNV-1a over each LE u64 wire length followed by exact wire bytes;
    // captured before edits, supplements the explicit per-tag golden bytes.
    assert_eq!(hash, 0xc611372dd16d7e5f);
}

fn field(name: &'static str, index: Option<usize>) -> EncodeError {
    EncodeError::InvalidField { field: name, index }
}
fn length(name: &'static str, len: usize, min: usize, max: usize) -> EncodeError {
    EncodeError::Length {
        field: name,
        len,
        min,
        max,
    }
}
// Kept as data so a one-off frozen-codec oracle can also verify all historical
// rejections, without copying an alternate encoder into the production/test API.
type Rejections<P> = Vec<(P, EncodeError)>;
fn invalid_corpus() -> (Rejections<ClientPacket>, Rejections<ServerPacket>) {
    let (clients, servers) = corpus();
    let (mut bad_clients, mut bad_servers) = (Vec::new(), Vec::new());
    for mut p in clients {
        match &mut p {
            ClientPacket::Hello { .. } => continue,
            ClientPacket::EventAck { session, .. } | ClientPacket::Input { session, .. } => {
                *session = 0
            }
            ClientPacket::Extension(p) => match p {
                ClientExtension::Inputs { session, .. }
                | ClientExtension::SnapshotAck { session, .. }
                | ClientExtension::Ping { session, .. }
                | ClientExtension::Command { session, .. } => *session = 0,
            },
        }
        bad_clients.push((p, field("session", None)));
    }
    for mut p in servers {
        match &mut p {
            ServerPacket::Welcome { session, .. } | ServerPacket::ReliableEvent { session, .. } => {
                *session = 0
            }
            ServerPacket::Snapshot(s) => s.session = 0,
            ServerPacket::Extension(p) => match p {
                ServerExtension::SnapshotDelta { session, .. }
                | ServerExtension::Pong { session, .. }
                | ServerExtension::CommandAck { session, .. } => *session = 0,
            },
        }
        bad_servers.push((p, field("session", None)));
    }
    // Session precedes even length/numeric failures.
    bad_clients.push((
        ClientPacket::Input {
            session: 0,
            inputs: vec![],
        },
        field("session", None),
    ));
    bad_servers.push((
        ServerPacket::Welcome {
            nonce: 0,
            session: 0,
            player_id: 16,
            tick: 0,
        },
        field("session", None),
    ));
    bad_servers.push((
        ServerPacket::Welcome {
            nonce: 0,
            session: 1,
            player_id: 16,
            tick: 0,
        },
        field("player_id", None),
    ));
    for count in [0, 7] {
        let inputs = vec![
            InputFrame {
                move_x: f32::NAN,
                ..InputFrame::default()
            };
            count
        ];
        bad_clients.push((
            ClientPacket::Input {
                session: 1,
                inputs: inputs.clone(),
            },
            length("inputs", count, 1, 6),
        ));
        bad_clients.push((
            ClientPacket::Extension(ClientExtension::Inputs {
                session: 1,
                inputs: inputs
                    .into_iter()
                    .map(|input| TimedInput {
                        input,
                        shot_tick: None,
                    })
                    .collect(),
            }),
            length("inputs", count, 1, 6),
        ));
    }
    // Input fields in predicate order, including nonfinite and numeric bounds.
    for (name, bad) in [
        (
            "inputs.move_x",
            InputFrame {
                move_x: f32::NAN,
                ..frame(1)
            },
        ),
        (
            "inputs.move_x",
            InputFrame {
                move_x: 1.0001,
                ..frame(1)
            },
        ),
        (
            "inputs.move_z",
            InputFrame {
                move_z: f32::INFINITY,
                ..frame(1)
            },
        ),
        (
            "inputs.move_z",
            InputFrame {
                move_z: -1.0001,
                ..frame(1)
            },
        ),
        (
            "inputs.yaw",
            InputFrame {
                yaw: f32::NEG_INFINITY,
                ..frame(1)
            },
        ),
        (
            "inputs.yaw",
            InputFrame {
                yaw: 3.15,
                ..frame(1)
            },
        ),
        (
            "inputs.pitch",
            InputFrame {
                pitch: f32::NAN,
                ..frame(1)
            },
        ),
        (
            "inputs.pitch",
            InputFrame {
                pitch: -1.56,
                ..frame(1)
            },
        ),
        (
            "inputs.buttons",
            InputFrame {
                buttons: 8,
                ..frame(1)
            },
        ),
        (
            "inputs.move_x",
            InputFrame {
                move_x: f32::NAN,
                move_z: f32::NAN,
                yaw: f32::NAN,
                pitch: f32::NAN,
                buttons: 255,
                ..frame(1)
            },
        ),
    ] {
        let inputs = vec![frame(0), bad];
        bad_clients.push((
            ClientPacket::Input {
                session: 1,
                inputs: inputs.clone(),
            },
            field(name, Some(1)),
        ));
        bad_clients.push((
            ClientPacket::Extension(ClientExtension::Inputs {
                session: 1,
                inputs: inputs
                    .into_iter()
                    .map(|input| TimedInput {
                        input,
                        shot_tick: Some(u32::MAX),
                    })
                    .collect(),
            }),
            field(name, Some(1)),
        ));
    }
    bad_clients.push((
        ClientPacket::Extension(ClientExtension::Ping {
            session: 1,
            nonce: 0,
            client_us: (1 << 53) + 1,
        }),
        field("client_us", None),
    ));
    for count in [0, 901] {
        bad_clients.push((
            ClientPacket::Extension(ClientExtension::Command {
                session: 1,
                event: event(count),
            }),
            length("event.payload", count, 1, 900),
        ));
    }
    bad_servers.push((
        ServerPacket::ReliableEvent {
            session: 1,
            event: event(901),
        },
        length("event.payload", 901, 0, 900),
    ));
    for (base_tick, tick) in [(0, 0), (10, 9), (0, 1 << 31), (u32::MAX, (1 << 31) - 1)] {
        bad_servers.push((
            ServerPacket::Extension(ServerExtension::SnapshotDelta {
                session: 1,
                delta: Delta {
                    base_tick,
                    tick,
                    bytes: vec![],
                },
            }),
            field("delta.tick", None),
        ));
    }
    for count in [0, 1, 2, 1177, 1200] {
        bad_servers.push((
            ServerPacket::Extension(ServerExtension::SnapshotDelta {
                session: 1,
                delta: Delta {
                    base_tick: 0,
                    tick: 1,
                    bytes: vec![0; count],
                },
            }),
            length("delta.bytes", count, 3, 1176),
        ));
    }
    for (client_us, receive_us, send_us, name) in [
        ((1 << 53) + 1, (1 << 53) + 1, (1 << 53) + 1, "client_us"),
        (0, (1 << 53) + 1, (1 << 53) + 1, "receive_us"),
        (0, 0, (1 << 53) + 1, "send_us"),
        (0, 100, 99, "pong.send_order"),
        (0, 100, 5_000_101, "pong.processing_us"),
    ] {
        bad_servers.push((
            ServerPacket::Extension(ServerExtension::Pong {
                session: 1,
                nonce: 0,
                client_us,
                receive_us,
                send_us,
                tick: 0,
            }),
            field(name, None),
        ));
    }
    let snap = |players| Snapshot {
        session: 1,
        players,
        ..Snapshot::default()
    };
    bad_servers.push((
        ServerPacket::Snapshot(snap(vec![player(255); 17])),
        length("players", 17, 0, 16),
    ));
    for (name, bad) in [
        ("players.id", player(16)),
        (
            "players.position.x",
            PlayerSnapshot {
                position: [f32::NAN, 0., 0.],
                ..player(15)
            },
        ),
        (
            "players.position.y",
            PlayerSnapshot {
                position: [0., f32::INFINITY, 0.],
                ..player(15)
            },
        ),
        (
            "players.position.z",
            PlayerSnapshot {
                position: [0., 0., -4096.5],
                ..player(15)
            },
        ),
        (
            "players.velocity.x",
            PlayerSnapshot {
                velocity: [256.5, 0., 0.],
                ..player(15)
            },
        ),
        (
            "players.velocity.y",
            PlayerSnapshot {
                velocity: [0., f32::NAN, 0.],
                ..player(15)
            },
        ),
        (
            "players.velocity.z",
            PlayerSnapshot {
                velocity: [0., 0., f32::NEG_INFINITY],
                ..player(15)
            },
        ),
        (
            "players.yaw",
            PlayerSnapshot {
                yaw: 3.15,
                ..player(15)
            },
        ),
        (
            "players.pitch",
            PlayerSnapshot {
                pitch: f32::INFINITY,
                ..player(15)
            },
        ),
        (
            "players.jump_coyote",
            PlayerSnapshot {
                jump_coyote: -0.001,
                ..player(15)
            },
        ),
        (
            "players.jump_coyote",
            PlayerSnapshot {
                jump_coyote: f32::NAN,
                ..player(15)
            },
        ),
        (
            "players.jump_buffer",
            PlayerSnapshot {
                jump_buffer: 0.501,
                ..player(15)
            },
        ),
        (
            "players.jump_buffer",
            PlayerSnapshot {
                jump_buffer: f32::INFINITY,
                ..player(15)
            },
        ),
        (
            "players.health",
            PlayerSnapshot {
                health: 101,
                ..player(15)
            },
        ),
        (
            "players.ammo",
            PlayerSnapshot {
                ammo: 31,
                ..player(15)
            },
        ),
        (
            "players.fire_cooldown",
            PlayerSnapshot {
                fire_cooldown: 4097,
                ..player(15)
            },
        ),
        (
            "players.reload_ticks",
            PlayerSnapshot {
                reload_ticks: 4097,
                ..player(15)
            },
        ),
        (
            "players.respawn_ticks",
            PlayerSnapshot {
                respawn_ticks: 4097,
                ..player(15)
            },
        ),
    ] {
        let mut players: Vec<_> = (0..16).map(player).collect();
        players[15] = bad;
        bad_servers.push((ServerPacket::Snapshot(snap(players)), field(name, Some(15))));
    }
    let mut players: Vec<_> = (0..16).map(player).collect();
    players[15] = PlayerSnapshot {
        id: 0,
        health: 101,
        ..player(15)
    };
    bad_servers.push((
        ServerPacket::Snapshot(snap(players.clone())),
        field("players.health", Some(15)),
    ));
    players[15].health = 100;
    bad_servers.push((
        ServerPacket::Snapshot(snap(players)),
        field("players.duplicate_id", Some(15)),
    ));
    let mut orphan = snap(vec![player(0)]);
    orphan.epochs[15] = 1;
    bad_servers.push((
        ServerPacket::Snapshot(orphan.clone()),
        field("epochs.orphan", Some(15)),
    ));
    orphan.epochs[1] = 1;
    bad_servers.push((
        ServerPacket::Snapshot(orphan),
        field("epochs.orphan", Some(1)),
    ));
    (bad_clients, bad_servers)
}
#[test]
fn all_field_errors_have_stable_precedence_and_late_failures_never_write() {
    let (clients, servers) = invalid_corpus();
    for (packet, expected) in clients {
        assert_eq!(encoded_client_len(&packet), Err(expected));
        assert_eq!(encode_client(&packet), Err(ProtocolError::Invalid));
        let mut out = [0xa5; MAX_PACKET_BYTES + 3];
        for n in [0, 1, MAX_PACKET_BYTES + 3] {
            assert_eq!(encode_client_into(&packet, &mut out[..n]), Err(expected));
            assert_eq!(out, [0xa5; MAX_PACKET_BYTES + 3]);
        }
    }
    for (packet, expected) in servers {
        assert_eq!(encoded_server_len(&packet), Err(expected));
        assert_eq!(encode_server(&packet), Err(ProtocolError::Invalid));
        let mut out = [0xa5; MAX_PACKET_BYTES + 3];
        for n in [0, 1, MAX_PACKET_BYTES + 3] {
            assert_eq!(encode_server_into(&packet, &mut out[..n]), Err(expected));
            assert_eq!(out, [0xa5; MAX_PACKET_BYTES + 3]);
        }
    }
}

fn envelope(command: bool, session: u64, event: &EventPacket) -> BorrowedEvent<'_> {
    if command {
        BorrowedEvent::Command { session, event }
    } else {
        BorrowedEvent::ReliableEvent { session, event }
    }
}
#[test]
fn borrowed_bridge_is_exact_for_both_families_including_all_errors() {
    let mut output = [0xa5; MAX_PACKET_BYTES + 3];
    for command in [false, true] {
        for len in [0, 1, 899, 900, 901] {
            let event = event(len);
            for session in [0, 1, u64::MAX] {
                let borrowed = envelope(command, session, &event);
                let valid = session != 0 && len <= 900 && (!command || len > 0);
                let expected_error = if session == 0 {
                    field("session", None)
                } else {
                    length("event.payload", len, usize::from(command), 900)
                };
                let (length, allocated) = if command {
                    let p = ClientPacket::Extension(ClientExtension::Command {
                        session,
                        event: event.clone(),
                    });
                    (encoded_client_len(&p), encode_client(&p))
                } else {
                    let p = ServerPacket::ReliableEvent {
                        session,
                        event: event.clone(),
                    };
                    (encoded_server_len(&p), encode_server(&p))
                };
                assert_eq!(borrowed.encoded_len(), length);
                if valid {
                    let bytes = allocated.unwrap();
                    assert_eq!(bytes.len(), len + 24);
                    buffers(
                        &bytes,
                        borrowed.encoded_len(),
                        |out| borrowed.encode_into(out),
                        true,
                    );
                } else {
                    assert_eq!(allocated, Err(ProtocolError::Invalid));
                    assert_eq!(length, Err(expected_error));
                    for n in [0, MAX_PACKET_BYTES + 3] {
                        assert_eq!(borrowed.encode_into(&mut output[..n]), Err(expected_error));
                        assert_eq!(output, [0xa5; MAX_PACKET_BYTES + 3]);
                    }
                }
            }
        }
    }
}
fn decode_event(command: bool, wire: &[u8], expected_session: u64) -> EventPacket {
    if command {
        let ClientPacket::Extension(ClientExtension::Command { session, event }) =
            decode_client(wire).unwrap()
        else {
            panic!("wrong family")
        };
        assert_eq!(session, expected_session);
        event
    } else {
        let ServerPacket::ReliableEvent { session, event } = decode_server(wire).unwrap() else {
            panic!("wrong family")
        };
        assert_eq!(session, expected_session);
        event
    }
}
fn ack_bytes(command: bool, sequence: u64, out: &mut [u8]) -> usize {
    if command {
        encode_server_into(
            &ServerPacket::Extension(ServerExtension::CommandAck {
                session: 7,
                sequence,
            }),
            out,
        )
        .unwrap()
    } else {
        encode_client_into(
            &ClientPacket::EventAck {
                session: 7,
                sequence,
            },
            out,
        )
        .unwrap()
    }
}
fn apply_ack(command: bool, wire: &[u8], sender: &mut reliable::EventSender) {
    let sequence = if command {
        let ServerPacket::Extension(ServerExtension::CommandAck {
            session: 7,
            sequence,
        }) = decode_server(wire).unwrap()
        else {
            panic!("wrong ACK family/session")
        };
        sequence
    } else {
        let ClientPacket::EventAck {
            session: 7,
            sequence,
        } = decode_client(wire).unwrap()
        else {
            panic!("wrong ACK family/session")
        };
        sequence
    };
    assert!(sender.acknowledge(sequence));
}
#[test]
fn supplied_time_send_due_encode_decode_admit_drain_ack_for_events_and_commands() {
    use reliable::{EVENT_LIMIT, Error, EventReceiver, EventSender};
    for command in [false, true] {
        let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
        for payload in [b"first".as_slice(), b"second", b"third"] {
            sender.queue(payload).unwrap();
        }
        let mut wire = [0; MAX_PACKET_BYTES];
        // These are explicit local acceptance outcomes, no socket/clock/driver.
        for (now, short) in [(1000, false), (1001, true)] {
            let report = sender.send_due(now, 3, 16, |event| {
                let n = envelope(command, 7, event).encode_into(&mut wire).unwrap();
                let result = if short {
                    Ok(n - 1)
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
                };
                matches!(result, Ok(sent) if sent == n)
            });
            assert_eq!((report.sent, report.remaining_due), (0, 3));
        }
        let mut acks = [[0; 22]; 3];
        let mut ack_count = 0;
        assert_eq!(
            sender
                .send_due(1002, 3, 16, |event| {
                    let n = envelope(command, 7, event).encode_into(&mut wire).unwrap();
                    if event.sequence == 0 {
                        return true;
                    } // Complete local acceptance, lost.
                    let packet = decode_event(command, &wire[..n], 7);
                    if packet.sequence == 1 {
                        assert_eq!(receiver.admit(&packet, 0), Err(Error::Full)); // No ACK.
                    } else {
                        receiver.admit(&packet, EVENT_LIMIT).unwrap();
                        assert_eq!(
                            ack_bytes(command, packet.sequence, &mut acks[ack_count]),
                            22
                        );
                        ack_count += 1;
                    }
                    true
                })
                .sent,
            3
        );
        assert_eq!(ack_count, 1);
        apply_ack(command, &acks[0], &mut sender); // Sequence two only.
        assert!(!sender.acknowledge(99));
        assert_eq!(sender.pending(), 2);
        assert_eq!(
            receiver.drain_ready(3, 16, |_, _| panic!("gap")).delivered,
            0
        );
        assert_eq!(
            sender.send_due(1101, 3, 16, |_| panic!("too early")).sent,
            0
        );
        ack_count = 0;
        assert_eq!(
            sender
                .send_due(1102, 2, 11, |event| {
                    let n = envelope(command, 7, event).encode_into(&mut wire).unwrap();
                    let packet = decode_event(command, &wire[..n], 7);
                    if packet.sequence == 0 {
                        receiver.admit(&packet, EVENT_LIMIT).unwrap();
                        ack_bytes(command, packet.sequence, &mut acks[ack_count]);
                        ack_count += 1;
                    } else {
                        assert_eq!(receiver.admit(&packet, 1), Err(Error::Full)); // Retained zero+two.
                    }
                    true
                })
                .sent,
            2
        );
        assert_eq!(ack_count, 1);
        apply_ack(command, &acks[0], &mut sender);
        assert_eq!(sender.pending(), 1);
        assert_eq!(receiver.drain_ready(1, 5, |_, _| false).delivered, 0);
        let mut delivered = Vec::new();
        assert_eq!(
            receiver
                .drain_ready(1, 5, |seq, bytes| {
                    delivered.push((seq, bytes.to_vec()));
                    true
                })
                .delivered,
            1
        );
        assert_eq!(sender.send_due(1201, 1, 6, |_| panic!("too early")).sent, 0);
        ack_count = 0;
        sender.send_due(1202, 1, 6, |event| {
            let n = envelope(command, 7, event).encode_into(&mut wire).unwrap();
            let packet = decode_event(command, &wire[..n], 7);
            receiver.admit(&packet, EVENT_LIMIT).unwrap();
            ack_bytes(command, packet.sequence, &mut acks[ack_count]);
            ack_count += 1;
            true
        });
        assert_eq!(ack_count, 1);
        apply_ack(command, &acks[0], &mut sender);
        assert_eq!(sender.pending(), 0);
        // Already ACKed data drains when capacity reopens without new input.
        assert_eq!(
            receiver
                .drain_ready(2, 11, |seq, bytes| {
                    delivered.push((seq, bytes.to_vec()));
                    true
                })
                .delivered,
            2
        );
        assert_eq!(
            delivered,
            [
                (0, b"first".to_vec()),
                (1, b"second".to_vec()),
                (2, b"third".to_vec())
            ]
        );
    }
}

#[test]
fn reused_buffer_has_no_codec_state_and_preserves_tail_across_failures_and_new_sessions() {
    let mut output = [0xa5; MAX_PACKET_BYTES];
    let full = ServerPacket::Extension(ServerExtension::SnapshotDelta {
        session: 7,
        delta: Delta {
            base_tick: 0,
            tick: 1,
            bytes: vec![3; 1176],
        },
    });
    assert_eq!(encode_server_into(&full, &mut output), Ok(MAX_PACKET_BYTES));
    let old = output;
    let bad = ClientPacket::Input {
        session: 0,
        inputs: vec![],
    };
    assert_eq!(
        encode_client_into(&bad, &mut output),
        Err(field("session", None))
    );
    assert_eq!(output, old);
    let hello = ClientPacket::Hello { nonce: 1 };
    assert_eq!(encode_client_into(&hello, &mut output), Ok(14));
    assert_eq!(&output[14..], &old[14..]); // Old bytes persist; always send only returned prefix.
    assert_eq!(decode_client(&output[..14]).unwrap(), hello);
    let p = ServerPacket::Snapshot(Snapshot {
        session: 8,
        ..Snapshot::default()
    });
    assert_eq!(encode_server_into(&p, &mut output), Ok(88));
    assert_eq!(decode_server(&output[..88]).unwrap(), p);
    assert_eq!(&output[88..], &old[88..]);
}

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
struct CountAlloc;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
    }
}
// SAFETY: All allocation pointers/layouts pass unchanged to System. Const TLS
// cells count this test thread only and do not allocate or retain pointers.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: caller supplies GlobalAlloc's required valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: caller supplies GlobalAlloc's required valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: caller supplies an original System pointer and valid new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: caller supplies the original System pointer and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

#[test]
fn warmed_caller_buffers_and_borrowed_send_due_allocate_nothing_with_scope_control() {
    // Packet creation, expected-byte creation, and queue storage are explicitly
    // outside the measured encoding work. Decoder/admit intentionally allocate
    // in the separate end-to-end recipe, so are not hidden in a zero claim.
    let (clients, servers) = corpus();
    let (bad_clients, bad_servers) = invalid_corpus();
    let expected_clients: Vec<_> = clients.iter().map(|p| encode_client(p).unwrap()).collect();
    let expected_servers: Vec<_> = servers.iter().map(|p| encode_server(p).unwrap()).collect();
    let mut sender = reliable::EventSender::default();
    for _ in 0..reliable::EVENT_LIMIT {
        sender.queue(&[7; 900]).unwrap();
    }
    let mut pointers = [0; reliable::EVENT_LIMIT];
    let mut wire = [0; MAX_PACKET_BYTES];
    sender.send_due(
        0,
        reliable::EVENT_LIMIT,
        900 * reliable::EVENT_LIMIT,
        |event| {
            pointers[event.sequence as usize] = event.payload.as_ptr() as usize;
            let borrowed = envelope(false, 7, event);
            assert_eq!(borrowed.encode_into(&mut wire), borrowed.encoded_len());
            true
        },
    );
    for (p, bytes) in clients.iter().zip(&expected_clients) {
        assert_eq!(encode_client_into(p, &mut wire), Ok(bytes.len()));
    }
    for (p, bytes) in servers.iter().zip(&expected_servers) {
        assert_eq!(encode_server_into(p, &mut wire), Ok(bytes.len()));
    }
    // A positive control counts actual allocating wrapper calls. Returned Vecs
    // are held through the observation so the allocator cannot be elided.
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|t| t.set(true));
    let allocated = encode_client(&clients[0]).unwrap();
    let decoded = decode_client(&expected_clients[3]).unwrap();
    TRACK.with(|t| t.set(false));
    assert!(ALLOCATIONS.with(Cell::get) >= 2); // wrapper Vec + decoder input Vec
    assert_eq!(allocated, expected_clients[0]);
    assert_eq!(decoded, clients[3]);
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|t| t.set(true));
    for round in 1..=32 {
        for (p, bytes) in clients.iter().zip(&expected_clients) {
            assert_eq!(encoded_client_len(p), Ok(bytes.len()));
            assert_eq!(
                encode_client_into(p, &mut wire[..bytes.len() - 1]),
                Err(EncodeError::Capacity {
                    required: bytes.len(),
                    available: bytes.len() - 1
                })
            );
            assert_eq!(encode_client_into(p, &mut wire), Ok(bytes.len()));
            assert_eq!(&wire[..bytes.len()], bytes);
        }
        for (p, bytes) in servers.iter().zip(&expected_servers) {
            assert_eq!(encoded_server_len(p), Ok(bytes.len()));
            assert_eq!(
                encode_server_into(p, &mut wire[..bytes.len() - 1]),
                Err(EncodeError::Capacity {
                    required: bytes.len(),
                    available: bytes.len() - 1
                })
            );
            assert_eq!(encode_server_into(p, &mut wire), Ok(bytes.len()));
            assert_eq!(&wire[..bytes.len()], bytes);
        }
        for (p, err) in &bad_clients {
            assert_eq!(encoded_client_len(p), Err(*err));
            assert_eq!(encode_client_into(p, &mut wire), Err(*err));
        }
        for (p, err) in &bad_servers {
            assert_eq!(encoded_server_len(p), Err(*err));
            assert_eq!(encode_server_into(p, &mut wire), Err(*err));
        }
        let now = round * 100;
        assert_eq!(
            sender
                .send_due(now, 0, 900 * reliable::EVENT_LIMIT, |_| panic!(
                    "zero budget"
                ))
                .sent,
            0
        );
        assert_eq!(
            sender
                .send_due(now, reliable::EVENT_LIMIT, 899, |_| panic!(
                    "short payload budget"
                ))
                .sent,
            0
        );
        assert_eq!(
            sender
                .send_due(
                    now,
                    reliable::EVENT_LIMIT,
                    900 * reliable::EVENT_LIMIT,
                    |event| {
                        assert_eq!(
                            envelope(true, 7, event).encode_into(&mut wire[..923]),
                            Err(EncodeError::Capacity {
                                required: 924,
                                available: 923
                            })
                        );
                        false
                    }
                )
                .sent,
            0
        );
        let report = sender.send_due(
            now,
            reliable::EVENT_LIMIT,
            900 * reliable::EVENT_LIMIT,
            |event| {
                assert_eq!(
                    event.payload.as_ptr() as usize,
                    pointers[event.sequence as usize]
                );
                for command in [false, true] {
                    let borrowed = envelope(command, 7, event);
                    assert_eq!(borrowed.encoded_len(), Ok(924));
                    assert_eq!(borrowed.encode_into(&mut wire), Ok(924));
                }
                true
            },
        );
        assert_eq!(
            (report.sent, report.remaining_due),
            (reliable::EVENT_LIMIT, 0)
        );
    }
    TRACK.with(|t| t.set(false));
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "alloc/realloc/alloc_zeroed calls on measured native test thread"
    );
    assert_eq!(sender.pending(), reliable::EVENT_LIMIT); // No fabricated ACKs.
}
