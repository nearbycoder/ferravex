# Glowtide

A tiny lantern spirit, an ocean full of glowing creatures, and one tide to
survive. Glowtide is a first-party bullet-heaven roguelite: your weapons fire
automatically while you steer, dash, collect motes and choose a build.

## Your first tide

Choose **Dive in**, select Lumi, then start a ten-minute tide. **Quick Tide**
compresses the director's schedule into five minutes. Stay near fallen creatures
long enough to collect their motes: they grant experience, and each level offers
new weapons, passives or upgrades. The world waits while you choose.

Eight weapons have different rhythms: Wisp Bolt, Halo Orbit, Bubble Mortar,
Tide Lash, Chain Spark, Lantern Aura, Shell Boomerang and Star Skipper. Carry up
to six weapons and six passives. Raise a weapon to level seven, own its paired
passive, then open a chest to evolve it. Each card shows its level as pips and
what the pick adds as icon chips read straight from the rules (a sword and
+30%, a plus and +1, a clock and -20%); a gold star-and-chest tag marks a pick
that makes an evolution ready. Reroll and Banish show their remaining uses and
key caps. Boss attacks have visible windups; save a dash for the opening.
Defeat the final boss to see dawn. In-run callouts are a word or three, and a
gold arrow at the screen edge points to any treasure chest out of view.

A run earns **Glimmer**, even if you lose. Spend it at the Shrine on permanent
bonuses or unlock Pip, Mira and Bramble, each with a different starting weapon
and strengths; both screens show those bonuses as icon chips (heart HP, sword
damage, wind speed, shield armor) and prices as a gem and a number. Progress is
stored on this device using a versioned save. Quick and full tides keep
separate survival, kill and level records. Results show a title, your time,
level, kills and Glimmer as icon tiles (a crown marks a new best) and damage by
weapon.

## Controls

| Action | Keyboard / mouse | Touch / gamepad |
| --- | --- | --- |
| Move | WASD or arrows | Drag the play field / left stick |
| Dash | Space or Shift | Dash button / A |
| Choose an upgrade | Click or 1–4 | Tap / focus and confirm |
| Reroll | E | Card control / Y |
| Banish | Q, then choose a card | Card control / X |
| Pause | Escape | Pause button / Start |
| Restart | R | Pause menu / Back |

Open **Controls** (F2) to inspect or remap bindings. Menus support keyboard,
mouse, touch and controller focus. On a phone, use fullscreen for more room.
Reduced motion and reduced flash independently limit presentation effects.
The shared [player guide](../../docs/PLAYER_GUIDE.md) explains audio, preferences,
backups and browser recovery.

## Run and inspect

```sh
cargo run -p glowtide --release --offline --locked -j 2
cargo test -p glowtide --offline --locked -j 2
cargo run -p glowtide --offline --locked -j 2 -- --autoplay --smoke-frames 600 --screenshot /tmp/glowtide.png
```

For all browser hosts, build with `python3 -B scripts/build-web.py release` and
serve with `python3 -B scripts/serve-web.py`; open `/glowtide/` in that catalog.
The public site exposes it at `/examples/glowtide/`.

## Implementation

- `sim/` owns seeded fixed-tick rules, bounded pools, spatial queries, weapons,
  waves, bosses, loot and upgrade choices. It has no platform I/O or rendering.
- `run.rs` connects the rules to `GameSession`: input edges are consumed only on
  executed ticks; terminal ticks stop catch-up; pause discards elapsed time.
- `world.rs`, `gfx.rs`, `hud.rs` and `menus.rs` paint the ocean and crisp UI;
  `chips.rs` derives every card, character and Shrine chip from the rules
  (`weapon_stats` deltas, passive modifiers, character stats, Shrine bonuses),
  with tests that keep them equal.
  Post-processing blooms the world before HUD text is drawn. Modern text,
  eased cards, roll counters and iris transitions use the shared engine.
- `fx.rs` and `sfx.rs` consume accepted simulation events. Restart clears effects
  and audio; preference changes suppress disabled effects immediately.
- `meta.rs` owns schema-versioned local progress, checked purchases and records.

Tests cover seeded runs, render cadence and pause equivalence, zero-tick edges,
terminal catch-up, damage and loot, boss telegraphs, evolution, choices,
revival, persistence, responsive layout and a 300-plus-enemy allocation budget.
These checks do not establish physical touch/gamepad behavior, audible sound,
or frame rates on other devices. See [platform limits](../../docs/PLATFORMS.md).
