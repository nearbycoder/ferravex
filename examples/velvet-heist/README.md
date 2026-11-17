# Velvet Heist

A complete six-heist stealth campaign about returning a city's stolen history.
Infiltrate velvet galleries, steal the required records, and carry them back to
the extraction stair before the shutters close. The optional artworks belong to
the people too, but reaching them means leaving the safer edge corridors.

## The six assignments

| Heist | Required records | Guards | Deadline | New pressure |
| --- | --- | --- | --- | --- |
| The Velvet Foyer | 2 ledger volumes | 2 | 90 seconds | Brass pass, locked passage, shadow routes |
| The Glass Conservatory | 2 seed cases | 2 | 100 seconds | Console and security beam; entry at the north stair |
| The Opera Vault | 2 score folios | 2 | 105 seconds | Concealment cabinets, two locked passages; entry from the east |
| The Night Train | 3 bond packets | 3 | 110 seconds | A second east-side patrol and three separate objectives |
| The Clockwork Bank | 3 pension rolls | 3 | 115 seconds | Two beams, overlapping patrols, west-side vaults |
| The Museum of Midnight | 3 history records | 4 | 120 seconds | Four watchers, two beams and the final broadcast |

The authored 16-by-10 floor plans contain **15 required records, 12 optional
artworks, six brass passes, ten security doors, five consoles and seven beams**.
Every floor has two optional artworks. The orientations, wall openings,
concealment furniture, patrol directions, patrol periods and objective positions
vary. Completing the final museum extraction reaches a scored ending. A capture
is recoverable only while one of the three career-wide favors remains.

## Controls

- **Arrows/WASD:** move continuously while held. Facing changes with the direction
  you press. Walls and closed doors stop movement.
- **Space/Enter:** interact with an object underfoot or one cardinal tile away.
  A door you face takes priority, followed by the current tile and the adjacent
  tiles in north, east, south, west order. A new interaction requires a press;
  holding the key cannot repeatedly award loot.
- **X/E:** toggle crouching. Crouching moves quietly and uses shadows for cover;
  standing moves twice as fast.
- **C/Q:** throw one coin toward the marked target, or three tiles ahead when
  no target is marked. Throws stop at walls and require a clear target within
  five tiles.
- **Left click on the map:** mark a coin target. Moving clears the mark and uses
  your facing again. Clicking the briefing, debrief or capture screen confirms
  its displayed action.
- **Sidebar buttons:** the X (posture), C (coins) and Space (action) buttons are
  clickable twins of those keys and use exactly the same rules.
- **Esc/P:** pause. **H:** open the field guide. **F2:** remap controls.
  **M:** toggle sound. **R:** restart the entire career.

The shared host fits the logical canvas to the window and maps pointer input
back into its coordinates. Keyboard and pointer are the intended controls;
mobile and physical gamepad play have not been verified.

## Read the screen

The play screen uses sprites, icons, meters and numbers instead of sentences.

- **Header:** six progress pips and the heist's name. The objective row shows
  each required record as a ghost that fills in with a tick when packed, an
  arrow to the extraction door (it lights and runs once every record is
  packed), then a plus and the two optional artworks.
- **Strip above the map:** a clock ring and the time left (red and pulsing in
  the last 20 seconds); a bell and the alarm meter, whose faint segment keeps
  the heist's peak alarm, with quarter marks toward lockdown; and the medal
  forecast. The first medal is always lit, any alarm breaks the other two, and
  the third shows a small painting until both artworks are packed.
- **Sidebar:** a large eye shows **HIDDEN** (eye with a slash, violet),
  **EXPOSED** (amber) or **SEEN** (red, pulsing). One head per guard follows,
  marked `!` while that guard watches you and `?` while it searches. Below are
  three buttons: X shows the standing and crouching thief with a switch (sound
  arcs mark the loud posture); C shows the coins left and a reticle that lights
  when a target is marked; Space shows what it would act on now and its reward
  (`+250` record, `+175` artwork, the extraction total, an unlocked door or
  crossed-out beams), with a progress ring while working. The kit tiles show
  the brass pass (padlock until found) and the beam grid (red until rewired),
  and the hearts are the career's three recovery favors.
- **Cards:** the briefing card names the heist and its target, counts the
  floor's time, records, guards, artworks, doors and beams, and draws the
  floor's lesson as a picture equation (for example console, arrow, crossed-out
  beam). The debrief card shows the medals (a missed medal shows a slash for
  the alarm or a painting for the artworks) and the haul; the capture card
  shows the cause (guard, bell or hourglass) and which heart a retry spends.
- **Results:** records saved, medals of 18 and favors kept, then each heist's
  medals. A loss shows heists cleared instead.

## Read the floor plan

The extraction stair is a mint door with chevrons; the brass pass is a gold
key; required records are white ledger pages with a red seal; optional
artworks are gilt paintings; the beam console is a teal screen; cabinets are
violet wardrobes. Locked doors carry a padlock until the pass is found and
glint afterward. Live security beams are red and flicker. Violet floor tiles
and edge corridors are shadows. The thief wears violet; guards wear dark coats
and carry a facing marker.

Guard sight is tinted on the floor. Faint tiles are each guard's normal
standing vision, clipped by solid walls and closed doors; bright tiles are where
that guard would see you in your current posture, so crouching visibly shrinks
the danger and clears the shadows. Searching guards tint their sight red.
Standing guards see up to six Manhattan tiles in a forward 90-degree cone.
Crouching reduces that range to four. A crouching thief in shadow or a cabinet
is concealed unless immediately adjacent and in the guard's cone; inside a
cabinet only the thief's eyes show. Sharing a guard's tile is always capture.

The object Space would act on glows under a floating Space key cap. Each
standing footstep draws an amber diamond of its two-tile hearing reach, and a
landing coin draws its six-tile reach. Hovering a tile previews a throw: a
dotted arc and reticle with a faint `?` over every guard it would lure, or a
red cross when the tile is out of reach or out of sight.

Standing movement takes nine ticks per tile; crouching takes eighteen, at 60
ticks per second. Nearby guards hear a standing footstep within two tiles and
investigate it. Crouched footsteps are silent. Patrol movement periods range
from 23 to 42 ticks, depending on the assignment; searching guards move every
18 ticks. Guards follow real walkable paths, including doors you have opened.

A sighting makes a guard search the last seen tile. A thrown coin attracts
guards within six tiles to its landing position, including through nearby walls.
They search for three seconds without a fresh clue, turning at the target before
resuming their patrol. Each heist begins with three coins; invalid or blocked
throws spend none.

## Feedback

Every accepted event is felt where it happens: rings where work starts; a
sparkle, burst and `+250`/`+175` where loot is packed; a key or open-padlock
popup at the pass and doors; mint bursts along every beam when a console is
rewired; a gold ring and coin where a coin lands; `?` over each guard that
heard it; `!` and a short shake when a guard spots you; a bell `+45%` and shake
on a live beam; and broken-medal sparks in the strip at the first alarm. While
a guard watches, the screen edges glow red with the alarm level. The clock
rings each of the last ten seconds. Extraction pops the heist's points and
sparkles each earned medal on the debrief card, with confetti scaled by the
medals. Refused commands give a small red ring and at most an icon: a crossed
coin target, an empty coin, a padlock at a door without the pass, or the
records still missing at the exit.

## Take the records and get out

The brass pass opens every security door in its heist and is not consumed.
Interactions continue automatically after a valid Space press, while guards and
the deadline continue advancing:

| Work | Duration |
| --- | --- |
| Take brass pass | 12 ticks / 0.2 seconds |
| Open door | 30 ticks / 0.5 seconds |
| Pack optional artwork | 30 ticks / 0.5 seconds |
| Lift required record | 45 ticks / 0.75 seconds |
| Rewire console | 90 ticks / 1.5 seconds |

Moving cancels unfinished work, with no partial reward. A console disables every
beam for the remainder of that heist. Crossing a live beam adds 45 percent alarm;
you can choose a risky crossing, but repeated crossings can trigger lockdown.

While visible, standing adds 1.8 percent alarm per tick and crouching adds 0.7
percent. After one full second unseen, alarm decays by 0.2 percent per tick.
Reaching 100 percent, sharing a tile with a guard, or running out of time fails
the heist. The highest alarm reached remains part of the medal calculation even
if the current alarm later falls to zero.

Extraction works from the exit tile or beside it, only after every required
record is packed. Optional artwork is not required. A clean extraction with
both artworks earns three medals; a clean extraction without both earns two;
any alarmed extraction earns one. A record is worth 250 points and artwork
175. Extraction adds 1,000 points, 200 per medal and three per full second
remaining. No reward comes from repeated invalid interaction, waiting or
repeatedly visiting a collected display.

After failure, Space spends a recovery favor and restores the same heist's
opening score, floor plan, guards, items, coins and time. Previously extracted
records and completed-heist medals remain safe. Three favors permit three local
retries; the fourth failure ends the career. R always begins a completely new
career. The shared host saves completed-run records and personal bests when
storage is available; unfinished careers are not saved.

## Strategy

Crouch before leaving the stair. Inspect the long route along the violet edges
before taking a fast central shortcut. Find the pass, rewire the console, and
notice where opening a second door creates a new guard route as well as a route
for you. Start long work only when the nearby cone has turned away.

For optional artwork, mark a coin landing beyond the patrol, throw, and approach
from the other side. A cabinet can let a search finish without spending another
coin. If alarm rises, break the wall-occluded sightline first; stopping in open
floor while crouched is not concealment. Time bonuses reward fast routes, but
one clean optional artwork can be worth more than shaving a few seconds.

## Run and verification

From the repository root:

```sh
cargo run -p velvet-heist --offline --locked -j 2
cargo run -p velvet-heist --offline --locked -j 2 -- --headless
cargo test -p velvet-heist --offline --locked -j 2
cargo clippy -p velvet-heist --all-targets --offline --locked -j 2 -- -D warnings
```

The legal headless pilot completes all six heists at tick **10,604**, with **15
records, 12,960 points, nine medals and all three favors retained**. It favors
quiet routes, opens doors through normal work commands, and waits for adjacent
patrols. It does not take every optional artwork or achieve a perfect stealth
rank. There is no bot-specific invulnerability, state injection or modified
rules.

Twenty-three behavioral tests cover the complete legal career and same-host
replay, authored geometry, inert briefings, movement speed/collision, cones and
wall occlusion, shadow cover, interaction duration and cancellation, pass/door
rules, beam disabling and alarm, footsteps, finite coins and investigation,
suspicion cooling, guard pathing/capture, retry rollback, extraction medals,
pointer bounds and terminal invariance. Presentation tests check that the
sidebar buttons mirror X, C and Space exactly, that the Space preview matches
every accepted press over a full career, that each tick records only its own
events, header progress from heist 1 to 6, short HUD goals, and the effects
requested for packing a record, throwing a coin, a refusal and a sighting. The
public model doctest executes without a renderer. Strict all-target Clippy and
scoped formatting pass.

Suggested native captures are frame 500 (foyer), 3000 (opera), 5200 (train),
9000 (final museum), and 10700 (ending):

```sh
cargo run -p velvet-heist --offline --locked -j 2 -- --autoplay --smoke-frames 500 --screenshot target/velvet-heist.png
```

The parent owns final combined native/WebAssembly builds, browser captures and
catalog registration. Automated model solutions and one native visual review
are not broad human balance testing, physical audio verification or evidence of
cross-platform determinism.
