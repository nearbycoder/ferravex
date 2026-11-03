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
- **Click a card** to inspect it, then click **PLAY** to use it. Choice-menu
  buttons accept immediately. **END TURN** matches X.
- **X/E** ends the combat turn, skips a card reward, or leaves forge/shop without
  buying. Skipping a reward is intentional deck management.
- **C/Q** inspects the permanent deck. Arrows turn its pages; C, Space or X returns.
- **H** opens the paginated field guide, **Esc/P** pauses, **F2** changes bindings,
  **M** toggles sound and **R** starts a fresh whole expedition.

Planning has no time limit. The enemy acts only when you end a turn. Each fight
allows forty turns; ending the fortieth without winning loses the run. The
shared host provides the title, pause, ending, retry and local records. Scores
and completed-run records can be saved locally; unfinished expeditions are not
saved. Autoplay uses legal controls and does not replace personal records.

## Combat

Begin with 80 HP, 30 coins and ten cards: three Gilt Strikes, three Velvet Guards,
one Riposte, one Night Ink, one Insight and one Hammerfall. Choose a starter
relic before the first encounter. Each turn starts with three energy and five
cards, modified by relics. The hand holds at most seven cards.

Unplayed cards enter the discard pile at turn end. An empty draw pile reshuffles
the discard pile using Ferravex's `RewardRng`. Cards marked **Exhaust** stay out
for the rest of that battle. The full permanent deck returns for the next fight;
Wounds are removed. The deck inspector shows permanent cards, including upgrades.

The gold intent tells you what the opponent will do. Block absorbs attack damage
and expires when your next turn begins. Opponent armor reduces **each hit** after
strength and status modifiers, so three small hits struggle against heavy armor.
Strength adds damage to every hit. Weak multiplies attack damage by three
quarters; vulnerable then multiplies by three halves. Each division rounds down.
The remaining duration is shown beside the fighter's other statuses.

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
rustfmt --edition 2024 --check examples/gilded-deck/src/lib.rs examples/gilded-deck/src/main.rs examples/gilded-deck/src/content.rs examples/gilded-deck/src/model.rs examples/gilded-deck/src/view.rs
```

Eighteen behavioral tests and one executed rustdoc example pass. They cover a
full legal-input replay, complete legal wins with all three starter relics,
cost rejection, held input, shuffling/exhaust, exact damage/status timing,
poison kills, thorns/death ordering, reward/camp/shop accounting, permanent
upgrades, Wound capacity/cleanup, turn limits, deck inspection and pointer bounds.
The standard pilot wins at tick **7824**, with **73 HP**, **22 cards**, **3 relics**,
**268 coins** and **20940 points**. It never writes campaign progress, health,
cards or enemy state outside ordinary controls.

The permanent deck is capped at 28 cards, the hand at seven, and all battle piles
together at forty cards. Excess Wounds are refused at that bound. Status stacks,
strength, thorns, energy and block have explicit caps. There are nine encounters
and at most forty turns per encounter, so farming cannot keep a run alive forever.

Parent's native frame-1000 review found the elite portrait, hand, selected-card
text, status panel and intent readable. Suggested further captures: frame120
for the opening hand, frame3000 for mid-run play, frame7000 for the Regent and
frame7900 for the ending. Browser staging and combined integration belong to the
parent. Automated rules tests and a graphical smoke review do not establish
human balance, physical mobile/gamepad support, audible sound quality or
cross-platform determinism.
