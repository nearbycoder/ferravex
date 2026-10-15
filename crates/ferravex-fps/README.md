# ferravex-fps

Original fixed-tick shooter simulation and native UDP client/server. Dependencies are local `ferravex-math`, `ferravex-net` and `ferravex-physics`; the authoritative server has no registry dependencies. Movement, hits, ammunition, reloads, damage and respawns remain server-owned.

## Authority rewind

`Match::step(&[(player_id, InputFrame)]) -> Vec<GameEvent>` retains its existing behavior: movement first, then shots against current target poses.

`Match::step_with_rewind(inputs, &[ShotTime { player, tick }]) -> RewindStep` advances exactly the same simulation but uses recorded target poses for the specified shooters. `RewindStep.events` contains the ordinary game events; `rejected_shots` identifies invalid time claims. Invalid times suppress firing without spending ammunition while movement and weapon timers continue. The first request for each player wins; only the first `MAX_PLAYERS` requests are inspected. Shooters without an explicit request retain current-world firing.

`Match::rewind_hit_query(attacker, shot_tick, yaw, pitch) -> Result<ShotTrace, RewindError>` performs a read-only query without spending ammunition or applying damage. It validates the attacker, aim and requested tick. `ShotTrace` contains `origin`, `end`, `victim` and `headshot`.

History records immutable authority poses at tick boundaries, retaining at most 17 frames: current plus `MAX_REWIND_TICKS = 16` previous ticks, or 250 ms at 64 Hz. Future, expired and unavailable ticks fail explicitly, including across `u32` tick wrap. Target poses include alive state and a life generation; old poses cannot hit a respawned player or a newly connected player reusing the same ID. Shooter origin and arena occlusion always use the present authority state. The implementation does not rewind or mutate the live physics world.

This is **an explicit authority API**, not automatic UDP latency compensation. The current `InputFrame` wire schema does not contain a shot timestamp, and `GameServer::tick` continues calling `Match::step`. A transport integration must associate authenticated input with a server-clock tick and an appropriate latency policy before calling `step_with_rewind`. The 250 ms cap alone is not a complete anti-cheat policy. There is no sub-tick interpolation, dynamic-world rewind, shooter rewind, or historical projectile simulation. Avoid mutating player identity/life directly through the public player vector; use spawn/remove and the normal respawn path so life tracking remains valid.

## Reliable game events

Protocol version 3 carries authority `Shot`, `Hit`, `Killed` and `Respawned` events over an ordered reliable channel inside each UDP session. `NetworkEvent { tick, event }` provides validated compact payload encode/decode; tick is the authority tick after the event's step.

The server queues events per peer, transmits at most eight per peer per tick, and retries unacknowledged events after 100 ms. Clients acknowledge individual sequences, deduplicate repeats, buffer reordering and deliver events exactly once in sequence order. Every event and ACK is bound to the current session cookie and peer endpoint. Reconnecting resets sequence state and rejects delayed packets or ACKs from the previous session.

Applications must consume events regularly:

```rust,ignore
client.poll()?;
for message in client.drain_events() {
    match message.event {
        GameEvent::Hit { attacker, victim, .. } => { /* confirmed feedback */ }
        GameEvent::Shot { origin, end, .. } => { /* remote tracer */ }
        _ => {}
    }
}
```

The client delivery queue is capped at 512 events and its reordering window at 256. A full application queue stops acknowledging new data. The server caps each peer at 256 pending events and disconnects overflowing peers rather than dropping guaranteed events or growing without bound. `GameClient::pending_event_count`, `GameServer::pending_event_count` and `GameServer::slow_event_disconnects` expose this state. Delivery is guaranteed only during a live session whose application continues draining and whose packets eventually get through; reconnect does not replay prior-session events.

Applications drain this channel to drive hit confirmation, damage feedback and effects from authoritative events.

Snapshots remain full, independently paced, unreliable snapshots. The delta module in `ferravex-net` is a tested standalone building block and is **not integrated** into this session. Reliable events do not change prediction/reconciliation or make snapshots reliable. UDP remains unencrypted development/LAN transport; no new cryptographic/authentication claim is made.

## Verification

```sh
cargo test -p ferravex-fps -p ferravex-net
cargo clippy -p ferravex-fps -p ferravex-net --all-targets -- -D warnings
cargo check --target wasm32-unknown-unknown
cargo tree -p ferravex-fps
```

Rewind tests cover a moving victim receiving historical-hit damage, invalid shot times, current-wall occlusion, old-life/slot protection, tick wrap and query immutability. Real UDP tests intentionally drop, reorder and duplicate authority events, exercise reconnect/stale-session rejection, and verify backpressure and bounded slow-peer disconnection. Existing movement, authority, input-loss, replay and prediction tests remain in place.


Movement now uses the shared first-party upright capsule motor in
`ferravex-physics::character`, including slope/step policy, coyote time and buffered
jump presses. Holding jump does not repeatedly jump after landing. Native authority,
client prediction and offline practice call the same fixed 64 Hz path. Protocol 3
includes jump timers and held state so reconciliation can replay them exactly.
The arena is static; moving-platform pose/support replication remains future work
for multiplayer. `try_move_player` reports query/validation failures without
committing motion; the compatibility `move_player` discards that diagnostic.
