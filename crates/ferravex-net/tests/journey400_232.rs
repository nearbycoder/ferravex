use ferravex_net::MAX_PACKET_BYTES;
use ferravex_net::local_link::{Impairment, Limits, LinkError, LocalLink};
use std::time::Duration;
fn us(t: u64) -> Duration {
    Duration::from_micros(t)
}
fn pair(limits: Limits, impairment: Impairment) -> LocalLink {
    LocalLink::new(1, &[10, 20, 30], limits, impairment).unwrap()
}
#[test]
fn exact_fifo_ties_routing_and_initialized_prefix_tails() {
    let mut link = pair(Limits::default(), Impairment::default());
    let a = link.endpoint(10).unwrap();
    let b = link.endpoint(20).unwrap();
    let c = link.endpoint(30).unwrap();
    for bytes in [b"one".as_slice(), b"two", b"three"] {
        link.send(a, b, bytes, us(0)).unwrap();
    }
    link.send(b, c, b"other", us(0)).unwrap();
    let mut output = [0xAA; MAX_PACKET_BYTES];
    // Remove an unrelated last slot first, exercising FIFO despite swap_remove.
    assert_eq!(
        link.receive(c, us(0), &mut output)
            .unwrap()
            .unwrap()
            .ordinal,
        4
    );
    for (ordinal, bytes) in [b"one".as_slice(), b"two", b"three"].iter().enumerate() {
        output.fill(0xAA);
        let d = link.receive(b, us(0), &mut output).unwrap().unwrap();
        assert_eq!(d.ordinal, ordinal as u64 + 1);
        assert_eq!(d.from, a);
        assert_eq!(d.to, b);
        assert_eq!(&output[..d.len], *bytes);
        assert!(output[d.len..].iter().all(|b| *b == 0xAA));
    }
    output.fill(0xAA);
    assert!(link.receive(b, us(0), &mut output).unwrap().is_none());
    assert!(output.iter().all(|b| *b == 0xAA));
    assert_eq!((link.queued_packets(), link.queued_bytes()), (0, 0));
}
#[test]
fn capacity_preflight_refusal_does_not_advance_time_or_impairment() {
    let mut link = pair(
        Limits {
            packets: 2,
            bytes: 4,
        },
        Impairment {
            loss_every: Some(2),
            ..Default::default()
        },
    );
    let a = link.endpoint(10).unwrap();
    let b = link.endpoint(20).unwrap();
    link.send(a, b, b"123", us(0)).unwrap();
    assert_eq!(link.send(a, b, b"xx", us(10)), Err(LinkError::ByteCapacity));
    link.send(a, b, b"4", us(0)).unwrap(); // Failed future send did not advance time/ordinal.
    assert_eq!(
        link.send(a, b, b"z", us(10)),
        Err(LinkError::PacketCapacity)
    );
    let mut short = [0xAA; 2];
    assert_eq!(
        link.receive(b, us(10), &mut short),
        Err(LinkError::OutputTooSmall { required: 3 })
    );
    assert_eq!(short, [0xAA; 2]);
    assert_eq!((link.queued_packets(), link.queued_bytes()), (2, 4));
    let mut out = [0xAA; 4];
    let d = link.receive(b, us(0), &mut out).unwrap().unwrap();
    assert_eq!(d.ordinal, 1);
    assert!(!d.lost);
    out.fill(0xAA);
    let d = link.receive(b, us(0), &mut out).unwrap().unwrap();
    assert_eq!(d.ordinal, 2);
    assert!(d.lost);
    assert_eq!(out, [0xAA; 4]);
    link.send(a, b, b"z", us(0)).unwrap();
    assert_eq!(
        link.receive(b, us(0), &mut out).unwrap().unwrap().ordinal,
        3
    );
}
#[test]
fn deterministic_delay_loss_and_reorder_are_finite_post_admission_work() {
    let impairment = Impairment {
        delay: us(10),
        loss_every: Some(3),
        reorder_every: Some(2),
        reorder_delay: us(20),
    };
    let run = || {
        let mut link = pair(Limits::default(), impairment);
        let a = link.endpoint(10).unwrap();
        let b = link.endpoint(20).unwrap();
        for n in 1..=6 {
            link.send(a, b, &[n], us(0)).unwrap();
        }
        let mut out = [0xAA; 4];
        let mut trace = Vec::new();
        assert!(link.receive(b, us(9), &mut out).unwrap().is_none());
        for t in [10, 30] {
            for _ in 0..6 {
                if let Some(d) = link.receive(b, us(t), &mut out).unwrap() {
                    trace.push((d.ordinal, d.lost, d.due_at));
                }
            }
        }
        assert_eq!(link.queued_packets(), 0);
        trace
    };
    let expected = vec![
        (1, false, us(10)),
        (3, true, us(10)),
        (5, false, us(10)),
        (2, false, us(30)),
        (4, false, us(30)),
        (6, true, us(30)),
    ];
    assert_eq!(run(), expected);
    assert_eq!(run(), expected);
}
#[test]
fn reset_disposal_cross_router_and_peer_generations_cancel_only_owned_traffic() {
    let mut link = pair(Limits::default(), Impairment::default());
    let mut other = LocalLink::new(2, &[10, 20], Limits::default(), Impairment::default()).unwrap();
    let a = link.endpoint(10).unwrap();
    let b = link.endpoint(20).unwrap();
    let c = link.endpoint(30).unwrap();
    assert_eq!(other.send(a, b, b"x", us(0)), Err(LinkError::StaleEndpoint));
    link.send(a, b, b"up", us(0)).unwrap();
    link.send(b, a, b"down", us(0)).unwrap();
    link.send(b, c, b"keep", us(0)).unwrap();
    let fresh = link.reset_endpoint(a, us(1)).unwrap();
    assert_ne!(a, fresh);
    assert_eq!(fresh.generation(), 2);
    assert_eq!((fresh.link_id(), fresh.peer_id()), (1, 10));
    assert_eq!(
        link.send(a, b, b"old", us(2)),
        Err(LinkError::StaleEndpoint)
    );
    assert_eq!((link.queued_packets(), link.queued_bytes()), (1, 4));
    link.send(fresh, b, b"new", us(1)).unwrap();
    link.dispose(fresh, us(1)).unwrap();
    assert_eq!(link.endpoint(10), Err(LinkError::UnknownPeer));
    assert_eq!(link.dispose(fresh, us(1)), Err(LinkError::StaleEndpoint));
    link.reset(us(1)).unwrap();
    assert_eq!(link.queued_bytes(), 0);
    assert_eq!(
        link.receive(c, us(1), &mut [0; 8]),
        Err(LinkError::StaleEndpoint)
    );
    assert_eq!(link.endpoint(10), Err(LinkError::UnknownPeer));
    let b = link.endpoint(20).unwrap();
    let c = link.endpoint(30).unwrap();
    link.send(b, c, b"reset", us(1)).unwrap();
    assert_eq!(
        link.receive(c, us(1), &mut [0; 8])
            .unwrap()
            .unwrap()
            .ordinal,
        1
    );
}
#[test]
fn all_validation_empty_full_maximum_time_and_output_errors_are_transactional() {
    let new = |id, peers: &[u64], limits, impairment| {
        LocalLink::new(id, peers, limits, impairment).err().unwrap()
    };
    assert_eq!(
        new(0, &[1, 2], Limits::default(), Impairment::default()),
        LinkError::InvalidIdentity
    );
    for ids in [vec![1], vec![1, 1], vec![0; 65]] {
        assert_eq!(
            new(1, &ids, Limits::default(), Impairment::default()),
            LinkError::InvalidIdentity
        );
    }
    for limits in [
        Limits {
            packets: 4097,
            bytes: 0,
        },
        Limits {
            packets: 1,
            bytes: 4096 * MAX_PACKET_BYTES + 1,
        },
    ] {
        assert_eq!(
            new(1, &[1, 2], limits, Impairment::default()),
            LinkError::InvalidLimits
        );
    }
    for imp in [
        Impairment {
            loss_every: Some(0),
            ..Default::default()
        },
        Impairment {
            reorder_every: Some(0),
            ..Default::default()
        },
        Impairment {
            delay: Duration::from_nanos(1),
            ..Default::default()
        },
        Impairment {
            reorder_delay: Duration::from_secs(61),
            ..Default::default()
        },
    ] {
        assert_eq!(
            new(1, &[1, 2], Limits::default(), imp),
            LinkError::InvalidImpairment
        );
    }
    let mut link = pair(Limits::default(), Impairment::default());
    let a = link.endpoint(10).unwrap();
    let b = link.endpoint(20).unwrap();
    assert_eq!(link.endpoint(99), Err(LinkError::UnknownPeer));
    assert_eq!(link.send(a, a, b"x", us(10)), Err(LinkError::SameEndpoint));
    assert_eq!(link.send(a, b, b"", us(10)), Err(LinkError::EmptyPacket));
    assert_eq!(
        link.send(a, b, &[0; MAX_PACKET_BYTES + 1], us(10)),
        Err(LinkError::PacketTooLarge)
    );
    for now in [Duration::from_nanos(1), us((1 << 53) + 1)] {
        assert_eq!(link.send(a, b, b"x", now), Err(LinkError::InvalidTime));
        let mut output = [0xAA; 4];
        assert_eq!(
            link.receive(b, now, &mut output),
            Err(LinkError::InvalidTime)
        );
        assert_eq!(output, [0xAA; 4]);
        assert_eq!(link.reset(now), Err(LinkError::InvalidTime));
    }
    link.send(a, b, &[7; MAX_PACKET_BYTES], us(0)).unwrap();
    let mut output = [0xAA; MAX_PACKET_BYTES + 1];
    assert!(link.receive(b, us(1), &mut output).unwrap().is_some());
    assert_eq!(output[MAX_PACKET_BYTES], 0xAA);
    assert_eq!(link.send(a, b, b"x", us(0)), Err(LinkError::BackwardsTime));
    assert_eq!(link.reset_endpoint(a, us(0)), Err(LinkError::BackwardsTime));
    assert_eq!(link.dispose(a, us(0)), Err(LinkError::BackwardsTime));
    assert_eq!(
        link.receive(b, us(0), &mut output),
        Err(LinkError::BackwardsTime)
    );
    assert_eq!(link.reset(us(0)), Err(LinkError::BackwardsTime));
    link.send(a, b, b"x", us(1 << 53)).unwrap();
    assert!(link.receive(b, us(1 << 53), &mut output).unwrap().is_some());
    let mut delayed = pair(
        Limits::default(),
        Impairment {
            delay: us(1),
            ..Default::default()
        },
    );
    assert_eq!(
        delayed.send(a, b, b"x", us(1 << 53)),
        Err(LinkError::DeliveryTimeOverflow)
    );
    delayed.send(a, b, b"x", us(0)).unwrap();
    for limits in [
        Limits {
            packets: 0,
            bytes: 1,
        },
        Limits {
            packets: 1,
            bytes: 0,
        },
    ] {
        let mut zero = pair(limits, Impairment::default());
        assert!(zero.send(a, b, b"x", us(0)).is_err());
        assert_eq!(zero.queued_packets(), 0);
    }
}

#[test]
fn maximum_configured_roster_and_queue_full_loss_are_still_bounded() {
    let peers: Vec<_> = (0..64).collect();
    let mut link = LocalLink::new(
        9,
        &peers,
        Limits {
            packets: 4096,
            bytes: 4096 * MAX_PACKET_BYTES,
        },
        Impairment {
            loss_every: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    let a = link.endpoint(0).unwrap();
    let b = link.endpoint(63).unwrap();
    for _ in 0..4096 {
        link.send(a, b, b"x", us(0)).unwrap();
    }
    assert_eq!(
        link.send(a, b, b"also lost", us(0)),
        Err(LinkError::PacketCapacity)
    );
    assert_eq!(link.queued_bytes(), 4096);
    // Loss needs explicit bounded host pumping; it cannot bypass full admission.
    let mut output = [0xAA; 1];
    for ordinal in 1..=4096 {
        let d = link.receive(b, us(0), &mut output).unwrap().unwrap();
        assert!(d.lost);
        assert_eq!(d.ordinal, ordinal);
        assert_eq!(output, [0xAA]);
    }
    assert_eq!((link.queued_packets(), link.queued_bytes()), (0, 0));
}

#[test]
fn original_datagrams_are_copied_and_delayed_routes_do_not_block_other_peers() {
    let mut link = pair(
        Limits::default(),
        Impairment {
            reorder_every: Some(2),
            reorder_delay: us(20),
            ..Default::default()
        },
    );
    let a = link.endpoint(10).unwrap();
    let b = link.endpoint(20).unwrap();
    let c = link.endpoint(30).unwrap();
    let mut input = *b"original";
    link.send(a, b, &input, us(0)).unwrap();
    input.fill(0);
    link.send(a, b, b"delayed", us(0)).unwrap();
    link.send(a, c, b"immediate", us(0)).unwrap();
    let mut output = [0xAA; 16];
    let d = link.receive(b, us(0), &mut output).unwrap().unwrap();
    assert_eq!(&output[..d.len], b"original");
    output.fill(0xAA);
    assert!(link.receive(b, us(0), &mut output).unwrap().is_none());
    assert_eq!(output, [0xAA; 16]);
    let d = link.receive(c, us(0), &mut output).unwrap().unwrap();
    assert_eq!(&output[..d.len], b"immediate");
    let d = link.receive(b, us(20), &mut output).unwrap().unwrap();
    assert_eq!(&output[..d.len], b"delayed");
    assert_eq!((d.admitted_at, d.due_at), (us(0), us(20)));
    assert_eq!((link.queued_packets(), link.queued_bytes()), (0, 0));
}

#[test]
fn short_loss_output_and_stale_lifecycle_errors_leave_time_and_traffic_untouched() {
    let mut link = pair(
        Limits::default(),
        Impairment {
            loss_every: Some(1),
            ..Default::default()
        },
    );
    let a = link.endpoint(10).unwrap();
    let b = link.endpoint(20).unwrap();
    link.send(a, b, b"lost", us(0)).unwrap();
    let fresh = link
        .reset_endpoint(link.endpoint(30).unwrap(), us(0))
        .unwrap();
    let old = fresh; // Keep a handle from before the next reset.
    let fresh = link.reset_endpoint(fresh, us(0)).unwrap();
    assert_eq!(
        link.reset_endpoint(old, us(50)),
        Err(LinkError::StaleEndpoint)
    );
    assert_eq!(link.dispose(old, us(50)), Err(LinkError::StaleEndpoint));
    let mut output = [0xAA; 3];
    assert_eq!(
        link.receive(old, us(50), &mut output),
        Err(LinkError::StaleEndpoint)
    );
    assert_eq!(
        link.receive(b, us(50), &mut output),
        Err(LinkError::OutputTooSmall { required: 4 })
    );
    assert_eq!(output, [0xAA; 3]);
    assert_eq!((link.queued_packets(), link.queued_bytes()), (1, 4));
    let mut output = [0xAA; 4];
    let d = link.receive(b, us(0), &mut output).unwrap().unwrap();
    assert!(d.lost);
    assert_eq!(output, [0xAA; 4]);
    // Empty receive is successful and does advance the shared watermark.
    assert!(link.receive(fresh, us(50), &mut output).unwrap().is_none());
    assert_eq!(
        link.send(a, b, b"earlier", us(49)),
        Err(LinkError::BackwardsTime)
    );
}
