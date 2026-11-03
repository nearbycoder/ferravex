# Mooncourt

Win the Earthrise Cup across six lunar courts. Mooncourt is a finite singles
racket tournament: place shots into open space, move to meet the incoming ball,
and decide when to spend stamina on an attacking smash. An airborne ball, its
ground shadow and its landing marker are separate cues. Returns depend on
position, height and timing; the rivals move and strike under the same physical
court and ball rules.

## Play

```sh
cargo run -p mooncourt --offline --locked -j 2
```

The shared host provides the title, pause, paginated guide, remapping, sound,
retry and completed-run records. Enter starts the host; Space enters the first
court after its opponent briefing. The model does not save unfinished matches.

| Control | Action |
| --- | --- |
| Arrows / WASD | Move across your half of the court |
| 1 / 2 / 3 | Aim left / center / right on the rival's half |
| Click the three target buttons | Choose the same aim lanes |
| Space / Enter | Drive, serve, enter court or confirm retry |
| X / E | High recovery lob |
| C / Q | Fast stamina-consuming smash |
| 1 / 2 / 3 between matches | Train footwork / endurance / racket reach |
| Click a training card | Purchase the corresponding free training level |
| Space between matches | Skip training and proceed |
| H | Field guide; arrows turn pages |
| Esc / P | Pause |
| R | Restart the entire tournament |
| M / F2 | Sound / control bindings |

Movement uses held keys. Shots use fresh press edges; holding a shot button does
not repeatedly swing. Mouse input selects targets and menu cards; movement and
swings use the keyboard by default.

## Tournament and rivals

Each match is best of three games: the first athlete to win two games advances.
Games use 0, 15, 30, 40, deuce and advantage. Win at least four points with a
two-point lead. Service changes after each game. This is a compact house format:
there are no sets, second serves, doubles alleys or diagonal-service-box faults.
A serve starts from the baseline and may target any of the three lanes.

| Court | Rival | Style |
| --- | --- | --- |
| Tycho School | Pip | Patient, slower baseline play; introductory timing |
| Glasshouse | Vega | Attacks the corner furthest from you |
| Basalt Garden | Orin | Fast baseline coverage and alternating deep drives |
| Crescent Terrace | Nix | High moonballs that demand height-aware timing |
| Mare Arena | Sol | Quick smashes and shorter angled returns |
| Earthrise Center | Regent Io | Fast reactions and a mixture of all shot types |

The courts have distinct palettes, briefings and gravity values. Rivals have
bounded movement speeds, acceleration, reaction delays and stamina. Their shot
choices depend on their authored style and the current rally; they do not
receive scripted misses or an autoplay handicap.

Winning a match offers one free training level. Footwork adds 15 movement units
per second, endurance adds 5 stamina per second, and racket training adds 4 units
of reach. Each track caps at three levels. A 2–0 win earns three medals; a 2–1
win earns two. The sixth victory awards the Cup and freezes the tournament on
its ending screen.

A lost match offers a wildcard retry. Three wildcards are available across the
whole tournament; a fourth match loss ends the run. Retrying preserves earlier
victories and training, resets that match and stamina, and rolls its score back
to the opening total. Lost attempts cannot farm campaign score.

## Ball, timing and stamina

The court is a real two-dimensional movement space, with a separate ballistic
ball height. Shots cross a net whose height is checked at the actual crossing.
The first bounce must land inside the opposing singles court. A second bounce,
net fault or out ball awards the point. A ball that passes beyond the receiving
area after its legal bounce also awards the point to its last hitter.

The colored landing ring shows where the flight will first touch the ground.
The small pale shadow shows the current ground position. Move to the landing
region, then step toward the net to intercept the ball earlier and higher. A
gold ring around your athlete and **SWING NOW!** in the sidebar mean the ball is
within racket reach. The swing window lasts ten ticks; its cooldown lasts twenty.
The racket cannot strike a high ball, a distant ball or your own outgoing shot.

| Shot | Stamina cost after contact | Flight time | Use |
| --- | --- | --- | --- |
| Drive | 7 | 1.25 seconds | Reliable placement and ordinary rallies |
| Lob | 4 | 2.35 seconds | Gain recovery time and invite an overhead reply |
| Smash | 26 | 0.86 seconds | Attack open space from a sufficiently high ball |

Each attempted swing also costs 2 stamina. A smash requires at least 26 remaining
stamina and ball height of at least 14; otherwise the return becomes a drive.
A low attacking shot near the net can catch its tape. Serves use a safe
1.6-second drive. Stamina recovers at 13 per second before training, plus 30
between points; movement slows to 80% below 12 stamina.

Each point won earns 50 plus twice the rally length, capped at 20 strokes for
that bonus. A game earns 200. A match earns 800 plus 100 per medal. Scores,
medals, return count and longest rally appear in the ending.

## Strategy

Choose a lane away from the rival before you swing. Returning from the baseline
usually produces a low drive; moving a little toward the net gives a higher,
faster attacking opportunity. Recover toward the middle after a wide shot so
you can reach either reply. Use lobs when out of position, and do not chase the
ball's airborne drawing instead of its ground shadow. Sol's short angles require
forward movement as well as lateral coverage. Io is beatable through the same
placement and stamina choices used against earlier rivals.

## Validation and implementation

`model.rs` owns fixed 60 Hz rules, `content.rs` owns the six opponents, and
`view.rs` draws the court and interface without changing the game. The rules
reuse Ferravex math and `ferravex_play::motion::approach_vec2`. The shared Arcade
host owns platform input, lifecycle, sound, records and window fitting. There
are no external libraries, assets or services.

```sh
cargo test -p mooncourt --offline --locked -j 2
cargo clippy -p mooncourt --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p mooncourt --offline --locked -j 2 -- --headless
```

The suite executes 13 behavioral tests and one public API doctest. It covers
movement and serving boundaries, real racket reach and height, press-only swings,
shot trajectories and costs, fallback shots, net/out/bounce decisions, deuce and
service changes, training, wildcard rollback and whole-run loss. A separate
complete tournament test starts at `Game::new`, consumes only ordinary pilot
inputs, visits all six matches and verifies terminal invariance and an identical
same-host replay.

The final legal pilot wins at tick 19,689 (about 5 minutes 28 seconds of
simulation), with all 18 medals, all three wildcards, 11,712 points, 66 returns
and a longest rally of 13. This is automated completion evidence, not a human
skill estimate or balance study. The parent inspected native frame 600 and
reported readable court, silhouettes, scoreboard and point feedback. Browser,
combined build and further visual checks belong to the parent. Device audio,
physical mobile/controller play and cross-platform bitwise determinism have not
been verified by this implementation task.
