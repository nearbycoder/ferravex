# Crown & Council

A complete, turn-based kingdom strategy game for the Ferravex framework. Rule the river kingdom through **30 individually authored petitions in three acts**, choose between competing public obligations, and leave one of four political legacies. There is no reading timer or random event shuffle. Early promises change nine later petitions, including their choices, costs and consequences.

The game uses illuminated parchment, four procedural council portraits, visible resource forecasts and a permanent chronicle for the current reign. It needs no external artwork, libraries or services.

## Play

```sh
cargo run -p crown-council --offline --locked -j 2
```

Begin with the shared title screen and the first act's briefing. Each petition has three decisions. A choice applies its resource changes and opens the next decision immediately. Previous choices and actual resource changes remain available in History. A new briefing introduces each ten-petition act.

| Control | Action |
| --- | --- |
| 1, 2, 3 | Accept that council decision immediately |
| Arrows / WASD, then Enter / Space | Select and accept a decision or project |
| Click a decision/project card | Accept the clicked choice |
| Enter / Space | Continue act briefings; return from History |
| X / E | Open or close investments |
| 1–4 in investments | Commission that project |
| C / Q | Open or close the chronicle |
| Arrows in the chronicle | Previous/next accepted decree |
| H | Shared paginated field guide |
| Esc / P | Shared pause menu |
| R | Restart the entire reign, including all resources, policies and projects |
| M | Shared sound toggle |

The council advances only when a decision is accepted. Reading records or inspecting projects spends no council. A held movement key or held primary button does not repeat a decree. The first briefing also explains the controls and resource rules.

## Four pillars

Treasury, Commons, Guard and Chapel each start at **60** and are capped at **100**. Reaching **zero in any one** ends the reign immediately, respectively through bankruptcy, revolt, mutiny or schism. The other three pillars cannot compensate for a fallen one.

Every decree pays **2 Treasury upkeep**. The card's forecast already includes this cost; Coin means Treasury, Folk means Commons, and Faith means Chapel. These are the decree's resource changes, before any project completing that council. Positive gains above 100 are discarded. Decisions are allowed to exhaust a reserve, so read the forecast before accepting one.

The decree is applied and checked for collapse before projects mature. A future payout cannot rescue a reserve reduced to zero by the current decision. The chronicle records actual before/after resources, including capped changes and completed projects.

## Authored reign

| Council | Act I: Roots of a reign | Council | Act II: The price of promises | Council | Act III: What the crown leaves |
| --- | --- | --- | --- | --- | --- |
| 1 | The river's warning | 11 | Letters under a white flag | 21 | A courier with no seal |
| 2 | The salt charter | 12 | A fever in the abbey | 22 | The silver that cracks |
| 3 | Bells at the north gate | 13 | The convoy that never came | 23 | A home for old soldiers |
| 4 | A book with missing pages | 14 | The abbey's empty fields | 24 | The uncensored chronicle |
| 5 | Winter muster | 15 | The vanished witness | 25 | The returning orchard child |
| 6 | The orchard petition | 16 | The armorer's new furnace | 26 | A market without masters |
| 7 | The river takes its due | 17 | The apprentice's petition | 27 | Bells for two faiths |
| 8 | A stolen loaf, a broken hand | 18 | The winter bread line | 28 | The last great flood |
| 9 | Ink for the villages | 19 | A captain before the bench | 29 | The heir's difficult question |
| 10 | The first crown feast | 20 | A chair for the succession | 30 | The crown on the empty chair |

Those are the baseline titles. Seven early promises create nine conditional versions:

- **Crown's Reach:** paid levee repairs change both floods, at councils 7 and 28.
- **The Salt Charter:** the harbor settlement changes the returning convoy at council 13.
- **The Orchard Oath:** sanctuary changes Anja's apprenticeship at council 6 and her returning bridge guild at council 25.
- **The Open Ledger:** publishing tax records changes the endangered witness at council 15.
- **The Common Bench:** equal courts change the captain's trial at council 19.
- **The Lantern Schools:** public education changes the apprentice's audit at council 17.
- **The White Flag Compact:** early peace changes the border emergency at council 21.

Every petition has three authored choices and a distinct consequence for each choice. The chronicle retains the actual variant encountered, rather than reconstructing history from your latest policies.

## Investments

Each work can be commissioned once. Costs are immediate, and the countdown advances only after accepted council decisions. Multiple works can be active. The purchase must leave at least one Treasury, and there must be enough councils left for completion. A completed or unaffordable card accepts no purchase and grants no score.

| Project | Treasury cost | Councils to finish | Completion reward: Treasury / Commons / Guard / Chapel |
| --- | --- | --- | --- |
| The Winter Granary | 18 | 3 | +2 / +18 / +4 / 0 |
| The Public Archive | 16 | 4 | +4 / +10 / 0 / +8 |
| The Muster House | 16 | 3 | +3 / +2 / +18 / 0 |
| The River Exchange | 20 | 5 | +30 / +4 / −2 / −2 |

Completed projects and their resource changes remain in History. The River Exchange returns the most coin, but the demands it places on guard and chapel are real.

## Judgment and replay

Each choice displays a **Charter**, **Iron**, **Commerce** or **Concordat** policy tag. Survive all thirty councils and your most frequent policy determines the settlement:

- **The Charter Crown:** a crown constrained by public promises and representation.
- **The Iron Regency:** a disciplined state governed by its oath of service.
- **The Merchant Commonwealth:** a kingdom joined by contracts and independent trading houses.
- **The Concordat Crown:** a crown bound to a sacred public trust and its obligations.

Ties favor Charter, then Iron, then Commerce, then Concordat. The last decree cannot erase your earlier record. The result screen shows the score and completed projects.

Grades reward the kingdom left behind: **S** requires every reserve at least 40 and three completed works; **A** requires 25 and two works; **B** requires 15; other successful reigns earn **C**, and a collapsed reign earns **F**. Badges are **Oathkeeper** for five named promises, **Builder** for two completed works, and **Four Pillars** for every final reserve at least 40. Badges are awarded only to successful reigns.

The score adds 100 plus the lowest remaining reserve for each accepted council, 60 for each completed project, and three times the sum of final reserves for surviving the reign. It cannot be farmed by reopening menus or attempting invalid purchases. The shared host provides completed-run best records and immediate retry. It does not save an unfinished reign or a permanent collection of ending badges.

## Strategy

Keep a cash reserve before accepting a generous decree: visible upkeep makes an uninterrupted sequence of public spending unsustainable. Invest early enough for a work to mature before the corresponding crisis. Avoid buying several projects so aggressively that their future rewards are needed to survive the very next choice.

Read both the forecast and the policy tag. Trade income can fund public commitments, and professional service can stabilize the guard without handing it every dispute. A reserve near 100 is often a source of bargaining power; another gain to an already full pillar is wasted. Early institutions produce better later options, but the investment still needs to be paid today.

## Headless and verification

```sh
cargo run -p crown-council --offline --locked -j 2 -- --headless
cargo test -p crown-council --offline --locked -j 2
cargo clippy -p crown-council --all-targets --offline --locked -j 2 -- -D warnings
```

The headless adviser uses only the same `Input` controls as a player. It balances the visible reserves, commissions affordable projects, advances through decisions and advances each act. It does not inject resources, event flags, progress or a terminal outcome. The verified default run completes all 30 councils and all four projects at tick **3285**, earning **The Merchant Commonwealth**, **grade S**, **5943 points**, and the **Builder** and **Four Pillars** badges.

Eleven unit tests cover legal full-reign play and maximum-grade and Oathkeeper attainability, four materially different successful policy settlements, exact costs and bounded resources, actual later consequence-chain variants reached by legal play, project purchase rejection and delayed payouts, invalid/held controls, genuine legal-policy collapse, terminal invariance, clean restart, chronicle navigation, content uniqueness and measured text bounds. One executable rustdoc example starts a reign with ordinary inputs and completes it through the adviser.

`model::Game` is independent of drawing, wall clocks and platform I/O. Its public interface is `new`, `step`, `decisions`, `resources`, `outcome`, `ending`, `score`, `pilot`, `grade`, `badges` and `summary`, plus `Default`. `Ending::name` supplies the human-readable settlement. The `ArcadeGame` implementation supplies the shared host contract. Storage is bounded to 30 decree records, four works, four resource values, four policy counts and seven promise bits; badge results contain at most three entries.

The native model tests and headless run establish the recorded behavior on the tested host. The parent integration reviewed native captures of both the sealed decree and a petition; this is not a claim of a full human playthrough, audio-device coverage, browser/device coverage or cross-platform determinism. Browser packaging and collection-wide integration are handled outside this example. There is no multiplayer, mid-reign save, procedural story generator or external-service requirement.
