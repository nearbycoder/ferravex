# ferravex-fps

First-party fixed-tick shooter authority, prediction and tactical sessions. All
dependencies are local Ferravex crates (`math`, `physics`, `net`, `play`). The
headless server has no renderer, window or registry dependency. Native UDP and
Strike's browser datagram adapter run the same Rust session code.

## Play locally

```sh
# Short tactical rounds, two players, native UDP and browser WebSocket gateway.
cargo run --release -p ferravex-fps --bin ferravex-server -- \
  --bind 127.0.0.1:4000 --websocket 127.0.0.1:4001 --quick --players 2

# Native client (run twice, or combine one native and one browser client).
cargo run --release -p strike -- --connect 127.0.0.1:4000 --play
```

For browsers, open the local Strike example, select **Multiplayer**, enter
`ws://127.0.0.1:4001/ferravex`, and Connect. The same example works in two tabs.
Start normal Play and click the canvas to use game controls. Open the multiplayer
panel to buy or issue objective commands. Controls are remappable in F2:

| Key | Action |
|---|---|
| WASD / mouse | Move / look |
| Left mouse / R / Space | Fire / reload / jump |
| B / N | Buy rifle / sniper |
| V / K | Buy armor / defuse kit |
| Hold E | Plant at the nearest Dockyard site, defuse, or pick up a dropped bomb |
| G | Drop bomb |
| Tab / F3 | Roster / diagnostics |

The optional DOM panel provides accessible buy and objective buttons. Its Plant
and Defuse buttons start a stationary interaction; move or Cancel to interrupt.
The touch E control and keyboard E cancel on release, pause, or settings/focus
loss. The authority also cancels remote interactions after twelve ticks without
fresh controls. Purchases and interactions are validated on the server.

`--tactical` selects the regular 10-second buy / 115-second round configuration,
800 starting credits and first-to-13 wins. `--quick` selects 1-second buy,
30-second rounds, 10-second bomb, 1-second plant, 2-second defuse (1 with a kit),
1-second intermission, 4000 credits and first-to-3. `--rounds N` overrides the win
threshold; `--players N` selects a 2–16 player lobby. Even IDs attack; odd IDs
defend. The roster locks when the lobby fills. A missing team forfeits the match;
disconnected roster slots cannot resurrect and block elimination. Native
`GameServer::restart_tactical` lets the authority start another match. Without
`--tactical` or `--quick`, the server keeps its original practice/deathmatch rules.

## Authority, clock and rewind

The authority advances at 64 Hz, at most one coalesced input per peer per tick.
Redundant input frames recover loss; recovered quick fire retains the original
shot aim/time while movement coalesces to the newest sample. Input floods cannot
advance simulation faster. Prediction runs the same capsule motor and weapon
settings, then replays unacknowledged controls after authority snapshots.

Protocol v4 exchanges correlated monotonic clock probes and timestamps shots
with the tick used to present remote actors. Snapshots carry per-slot life epochs; rendering uses the latest roster and never
blends a respawn/reused slot with its old life. The client keeps a 32-snapshot
interpolation history, unwraps tick counters, and chooses a jitter-adjusted
6–12 tick presentation delay. RTT and jitter are exposed for diagnostics. Missing
clock observations use the existing snapshot interpolation fallback.

`Match::step_with_rewind(inputs, &[ShotTime { player, tick }])` applies recorded
target poses to explicitly timed shooters. Future, expired and unavailable claims
suppress firing without spending ammo; movement and weapon timers still advance.
A rejected claim disarms held fire until fresh input arrives, so a future claim
cannot become valid on a later tick by itself. The server retains current plus
16 historical ticks (250 ms), including alive state/life generations. Shooter
origin and static occlusion use the present world; old lives and removed actors
cannot be hit. There is no dynamic-world, projectile, shooter or sub-tick rewind.
The fixed window alone is not a complete anti-cheat policy.

`Match::step` and ordinary practice retain their existing behavior. Custom games
can implement `CombatRules` and use `step_with_rules` / `WeaponSettings` without
duplicating movement, collision, rewind or hit processing. The tactical adapter
integrates economy, armor, teams, weapon timing, interruption, planting/defusing,
round reset and forfeit with that same path. Practice checkpoints/replay remain
separate from tactical state; recording network/tactical matches is not implemented.

## Replication and reliable commands

Snapshots normally arrive every three authority ticks. Exact snapshot ACKs select
an acknowledged byte baseline; sparse deltas are used only when smaller than a
full packet. Both ends retain at most 32 baseline packets. Unknown ACKs are
rejected, missing baselines request a full frame, and session/tick/header/numeric
validation occurs before accepting reconstructed state. Dense changes and roster
changes use full snapshots. Positions are still `f32`; the standalone centimeter
quantizer is not used on this wire.

Authority Shot/Hit/Killed/Respawned events, tactical views, lobby state and command
results share the ordered reliable server channel. Buy/plant/defuse/cancel/drop/
pickup commands have a separate reliable client channel. Individual ACKs,
100 ms retries, deduplication, sequence ordering and reconnect resets apply in
both directions. The server validates client intent; no client position, health,
damage, price or interaction duration is trusted.

Pending reliable traffic is capped at 256 items per sender, reordering at 256,
application gameplay delivery at 512, pending prediction inputs at 128, and poll
work at 128 datagrams. Slow consumers stop receiving ACK credit and overflowing
peers disconnect. Commands reserve result capacity before execution. Delivery
requires a live session, continued consumption and eventual packet delivery;
reconnect does not replay an old session's events. Drain accepted gameplay events
for hit feedback, never invent confirmations from client prediction:

```rust,ignore
client.poll()?;
for message in client.drain_events() {
    // Present message.event; the authority already applied its damage.
}
client.queue_action(ferravex_play::rounds::Action::BuyArmor)?;
```

## Browser transport and limits

The browser runs the same `GameClient`; first-party C exports carry at most 1200
bytes per datagram and 128 queued packets each way. `ferravex_mp_time` supplies
monotonic microseconds, with no unsupported Wasm OS-clock calls. The application
owns WebSocket connection creation; browser buffer pressure is capped at 64 KiB.
The Rust adapter compacts identical pending datagrams in both directions, so
reliable retries cannot crowd out new traffic when background tabs render at
one frame per second. Throttling still increases latency; it is not suitable
for active competitive play.
The Rust browser adapter can also host an authority over a supplied byte link
(one remote peer); Strike's visible connection menu currently joins native servers.
Courtyard retains its separate first-party two-peer WebRTC example.

The optional native gateway gives every WebSocket connection its own UDP endpoint
at the authority. It implements bounded binary RFC 6455 handshake/framing with
fragmentation, masking, ping/pong and close validation. There are at most 16
connections, 4 KiB handshake headers, 8 KiB incoming buffers, 1200-byte messages,
32 fragments/message and 64 KiB / 256 pending output frames per connection.
Handshake and idle deadlines close stale connections. Browser origins default to
local port 8087; add exact origins with repeatable `--origin` arguments when
serving elsewhere. Origin checks do not authenticate a player.

Native UDP and the plain `ws://` gateway are development/LAN transports. They do
not implement accounts, native encryption/TLS, hosted matchmaking, NAT traversal,
congestion control or Internet abuse mitigation. `wss://` requires the deployment
host's TLS termination. TCP can stall newer snapshots behind retransmitted data.
Both peers must run protocol v4 and the same map/rules; custom arenas must be
provided to both. Pistol/rifle/sniper have distinct authority timing/damage, but
Strike currently uses the existing rifle mesh as their presentation proxy. Dockyard
shows its two default objective areas with ground paint and renders the device
when dropped or planted; a custom server map needs corresponding client visuals.

## Local verification

```sh
cargo test -p ferravex-fps -p ferravex-net -p ferravex-play --all-features --offline --locked
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
python3 scripts/build-web.py
node web/tests/multiplayer.mjs --native
```

Tests cover real UDP clock/delta recovery, timed damage, invalid-claim disarming,
reordered/duplicated reliable purchases, plant/defuse/reset/forfeit, TCP WebSocket
upgrade/forwarding and malformed frames, actual Wasm authority/client loss/jitter/
reorder/duplicate traffic, a one-Hz throttled client with duplicated retries,
and two Wasm clients connected to the native authority.
Apple/Windows runtime testing requires those hosts; iOS testing is excluded here.
