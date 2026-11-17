# Gilded Deck

A complete deck-building expedition through a palace that owns its citizens'
names. Defeat nine captors, recover their ledgers, choose cards and relics, and
break the Regent's final contract. Health and deck decisions persist between
battles. A defeated liberator starts a new run; there is no mid-battle rescue.

The campaign contains **nine authored encounters, sixteen playable card types,
one temporary Wound, six relics, eight three-card reward offerings and eight
camp decisions**. The two vault guardians are elites; the ninth opponent is the
final boss. Rules use integer combat values and deterministic seeded shuffles.
Different card, relic and camp choices change the run; the authored route and
initial shuffle seeds stay fixed.

## Play and controls

From the repository root:

```sh
cargo run -p gilded-deck --offline --locked -j 2
cargo run -p gilded-deck --offline --locked -j 2 -- --headless
```

The graphical executable needs a graphics display. The parent-owned browser
host runs the same rules through WebAssembly.

- **Arrows/WASD** select a card or menu choice. Held directions do not repeat.
- **1–5** select that hand position. In choice menus these keys accept the
  numbered choice immediately. Cards six and seven remain reachable with arrows.
- **Space/Enter** plays the selected card or accepts the selected menu choice.
- **Click a card** to lift it (the same as its number key), then click the
  lifted card again to play it (the same as Space). Choice cards accept
  immediately. The **End turn**, **Skip** and forge **Back** buttons match X.
- **X/E** ends the combat turn, skips a card reward, or leaves forge/shop without
  buying. Skipping a reward is intentional deck management.
- **C/Q** or the deck button (top left) inspects the permanent deck. Arrows or the
  side arrows turn its pages; C, Space, X or the close button returns.
- **H** opens the paginated field guide, **Esc/P** pauses, **F2** changes bindings,
  **M** toggles sound and **R** starts a fresh whole expedition.

Planning has no time limit. The enemy acts only when you end a turn. Each fight
allows forty turns; ending the fortieth without winning loses the run. The
shared host provides the title, pause, ending, retry and local records. Scores
and completed-run records can be saved locally; unfinished expeditions are not
saved. Autoplay uses legal controls and does not replace personal records.

## Reading the table

The game speaks in icons; words are kept for names. One icon and one color mean
the same thing everywhere: on cards, status badges, the enemy's intent and the
popups that rise when something happens.

| Icon | Meaning |
| --- | --- |
| Sword | Damage. On a card in battle it shows what would land after strength, weak, vulnerable and armor: green when boosted, red when reduced. `x3` hits three times |
| Shield | Block (on the left end of a health bar). A squared steel plate is permanent armor |
| Drop | Poison |
| Arrow down / up | Weak / strength |
| Target | Vulnerable |
| Leaf | Thorns |
| Heart | Health; a green `+` heals, a red `-` is paid |
| Bolt | Energy (the gold orb holds this turn's energy; pips count it out) |
| Card stack | Draw, the draw and discard piles, and the deck button |
| Flame | Exhaust: a flame-marked card burns until the next battle; the flame pile counts them |
| Lock / cross | An unplayable card / a Wound shuffled into the discard pile |
| Struck-through icons | Removed: Clean Slate's afflictions, Mirror Edge's block |

The header shows nine progress pips (the current encounter glows; between
battles the next captor is already the goal), the place name, a five-word goal,
owned relics (hover for their names), the purse and the turn hourglass, which
turns red from turn 30 of the 40 allowed. The enemy's next move floats above its
head; skulls mark elites and a crown the Regent.

Hovering or selecting a playable card previews it with the real rules: the
captor's health bar shows a pulsing chunk that would be lost, a gold reticle (or a
blue halo on the Liberator) marks where it lands, block and status badges show
their new values, and the energy pips it would spend blink. A skull beside the
captor's bar means the card is lethal. Hovering **End turn** previews the enemy's
move on your health bar; a faint chunk stays visible whenever the next move
would cost health, and a red skull warns when ending the turn would be fatal.
When nothing playable is left, the **End turn** button glows.

Accepted events are felt where they happen: cards are dealt face down from the
draw pile and played cards sweep toward their target; hits burst and pop their
damage between the fighters (big hits shake the table), block, healing, statuses,
poison and thorns rise as icon popups, exhausted cards flare at the flame pile, a
new battle announces its captor, and refused commands ring red with the missing
thing ("Need 2" with a bolt, a coin price, "Full", "Locked").

## Combat

Begin with 80 HP, 30 coins and ten cards: three Gilt Strikes, three Velvet Guards,
one Riposte, one Night Ink, one Insight and one Hammerfall. Choose a starter
relic before the first encounter. Each turn starts with three energy and five
cards, modified by relics. The hand holds at most seven cards.

Unplayed cards enter the discard pile at turn end. An empty draw pile reshuffles
the discard pile using Ferravex's `RewardRng`. Flame-marked (**Exhaust**) cards
stay out for the rest of that battle. The full permanent deck returns for the next fight;
Wounds are removed. The deck inspector shows permanent cards, including upgrades.

The intent above the opponent tells you what it will do. Block absorbs attack damage
and expires when your next turn begins. Opponent armor reduces **each hit** after
strength and status modifiers, so three small hits struggle against heavy armor.
Strength adds damage to every hit. Weak multiplies attack damage by three
quarters; vulnerable then multiplies by three halves. Each division rounds down.
The remaining duration is the number on the fighter's status badge.

Poison ignores block and armor. Enemy poison ticks before its intent; your poison
ticks after its intent, including poison just applied by that intent. A tick
deals the current stack count, then removes one stack. An enemy killed by poison
does not execute its intent. Thorns damage an attacker after a hit costs you HP;
fully blocked hits do not trigger it. If a fatal hit also kills the opponent
through thorns, the player's death takes priority.

## Cards

The second number in a value pair is the permanent forged upgrade. Upgrading
keeps the energy cost. Damage values below are before strength, weak, vulnerable,
armor and block.

| Card | Energy | Effect, normal / gilded |
| --- | --- | --- |
| Gilt Strike | 1 | Deal 7 / 10 damage |
| Velvet Guard | 1 | Gain 6 / 9 block |
| Riposte | 1 | Gain 5 / 7 block, deal 4 / 6 damage, apply 1 weak |
| Night Ink | 1 | Apply 4 / 6 poison |
| Insight | 0 | Draw 2 / 3 cards; exhaust |
| Hammerfall | 2 | Deal 16 / 21 damage, then apply 2 vulnerable |
| Blood Tithe | 1 | Deal 5 / 8 damage, heal 3 / 5 HP; exhaust |
| Three Knives | 1 | Deal 3 / 4 damage three times |
| Briar Wall | 2 | Gain 14 / 19 block and 2 / 3 thorns for the battle |
| Gold Spark | 0 | Gain 1 / 2 energy this turn; exhaust |
| Sharpen | 1 | Gain 2 / 3 strength for the battle; exhaust |
| Red Wager | 0 | Pay 3 / 2 HP and deal 9 / 13 damage; cannot pay your last HP |
| Clean Slate | 1 | Gain 5 / 8 block; remove your poison, weak and vulnerable |
| Mirror Edge | 1 | Deal current block plus 4 / 8 damage, then spend all block |
| Mercy | 0 | Heal 4 / 7 HP; exhaust |
| Grand Finale | 3 | Deal 24 / 31 damage plus 3 per already exhausted card |
| Wound | Unplayable | Temporary dead draw; discarded at turn end and removed after battle |

## Captors and relics

| Encounter | HP / armor | Distinct threat |
| --- | --- | --- |
| Tollkeeper, Copper Gate | 32 / 0 | Six-damage toll, guard, then nine-damage strike |
| Silk Duelist, Scarlet Stair | 42 / 0 | Double hits, weak and a twelve-damage lunge |
| Brass Colossus, First Vault | 64 / 2 | Elite armor, block and vulnerable before heavy blows |
| Ink Witch, Black Library | 55 / 0 | Wounds, weak and poison |
| Twin Masks, Mirror Salon | 64 / 0 | Three hits and accumulating strength |
| Tax Collector, Second Vault | 82 / 1 | Elite poison, armor and twenty-damage collections |
| Gilded Sentinel, Crown Bridge | 88 / 3 | Heavy armor, block and vulnerable |
| Mirror Baron, Last Ballroom | 96 / 0 | Wounds, large shields, triple hits and growing strength |
| The Regent, Gilded Throne | 136 / 2 | Double hits, Wounds, block and strength; gains 3 strength once below half HP |

Starter relics are **Gilt Needle** (+1 starting strength), **Scholar Seal**
(draw one extra card each turn) and **Mercy Bell** (heal six extra HP after a
victory). Each elite offers **Iron Heart** (+16 maximum HP and immediate healing),
**Amber Clock** (four starting energy per turn) or **Thorn Brooch** (three starting
thorns). An already owned relic cannot be claimed again.

Each victory grants four HP and `22 + 5 × encounter_index` coins, with indices
zero through eight. Before moving on, choose one card from the three authored
rewards or skip it. After an elite, also choose a relic.

Each camp allows one action. Rest heals eighteen HP for free. Forge upgrades one
card permanently. A shop purchase either adds its displayed card for 35 coins,
heals 28 HP for 25 coins, or removes one ungilded Gilt Strike for 20 coins. A
rejected purchase spends nothing. Browsing the shop or forge does not spend the
action; successful rest, upgrade or purchase does. Depart can be chosen without
taking an action.

Battle score is `1000 + 150 × encounter_index + max(600 - 35 × turns, 0)`.
Finishing the final battle also awards thirty points per remaining HP after
victory healing. Card plays and rejected inputs award no points.

## Strategy

Read the intent before spending energy. A guard can be more valuable than a
small strike against a heavy attack; a non-attacking enemy gives time to play
Sharpen or build poison. Hammerfall's vulnerable helps later attacks, so order
matters. Three Knives benefits three times from strength but also pays armor
three times. Night Ink bypasses armor entirely.

Exhausting cheap support cards both improves the current turn and increases
Grand Finale's damage. Mirror Edge can cash in spare block, but spends the
defense you may need for the impending attack. Hold Clean Slate for the Witch's
poison or the Regent's weak. Skip redundant rewards when a smaller deck would
reach your best cards more often. A camp rest preserves coins; a tonic heals
more but spends both coins and the camp action.

## Validation and bounds

```sh
cargo test -p gilded-deck --offline --locked -j 2
cargo clippy -p gilded-deck --all-targets --offline --locked -j 2 -- -D warnings
rustfmt --edition 2024 --check examples/gilded-deck/src/*.rs
```

Twenty-eight behavioral tests and one executed rustdoc example pass. They cover
a full legal-input replay, complete legal wins with all three starter relics,
cost rejection, held input, shuffling/exhaust, exact damage/status timing,
poison kills, thorns/death ordering, reward/camp/shop accounting, permanent
upgrades, Wound capacity/cleanup, turn limits, deck inspection, pointer bounds,
clicks that mirror their keys (lifted card, End turn, Skip, forge Back and Gild,
choice cards, deck button and pages), the per-update presentation events,
progress pips, effect requests, the header word budget and one-line guide
bullets.
The standard pilot wins at tick **7824**, with **73 HP**, **22 cards**, **3 relics**,
**268 coins** and **20940 points**. It never writes campaign progress, health,
cards or enemy state outside ordinary controls.

The permanent deck is capped at 28 cards, the hand at seven, and all battle piles
together at forty cards. Excess Wounds are refused at that bound. Status stacks,
strength, thorns, energy and block have explicit caps. There are nine encounters
and at most forty turns per encounter, so farming cannot keep a run alive forever.

Presentation never feeds the rules: each update clears and refills a bounded
list of accepted events (at most 32) that only drawing and effects read, and
previews run the real rules on a copy. Smoke captures (autoplay frames 60 and
400 for battle, 262 for a reward, 282 to 330 for camp and shop, `FAST=4` frame
228 for the forge and 380 for an elite relic, `FAST=8` frame 905 for the Regent,
`FAST=40` frame 260 for the ending) were reviewed; hover states cannot be
captured because the smoke pointer stays off-canvas. Browser staging and combined
integration belong to the parent. Automated rules tests and a graphical smoke
review do not establish human balance, physical mobile/gamepad support, audible
sound quality or cross-platform determinism.
