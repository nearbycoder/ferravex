//! Six authored operations in the liberation of the Copper Coast.

pub(crate) const COLS: usize = 11;
pub(crate) const ROWS: usize = 8;
pub(crate) const CELLS: usize = COLS * ROWS;
pub(crate) const MAX_ENEMIES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Objective {
    Eliminate,
    Rescue,
    Capture,
    Hold,
    Sabotage,
    Liberate,
}

pub(crate) struct Mission {
    pub name: &'static str,
    pub story: &'static str,
    pub order: &'static str,
    pub tip: &'static str,
    pub map: [&'static str; ROWS],
    pub objective: Objective,
    pub limit: u16,
}
// # solid wall; c low cover; 1/2/3 squad; g gunner; b bruiser;
// t sentry; W warden; o/p objectives; e extraction; r/s reinforcements.
pub(crate) const MISSIONS: [Mission; 6] = [
    Mission {
        name: "Breakwater",
        story: "Eliminate the patrol.",
        order: "Clear four patrols from the breakwater.",
        tip: "Red aim lines show the next enemy shots.",
        map: [
            "###########",
            "#.....g...#",
            "#..c..c.t.#",
            "#....#....#",
            "#1.c.#.g..#",
            "#2...c....#",
            "#3....b...#",
            "###########",
        ],
        objective: Objective::Eliminate,
        limit: 20,
    },
    Mission {
        name: "The Captive Signal",
        story: "Rescue the operator.",
        order: "Reach the operator [O], then escort her to [E].",
        tip: "Start escort beside O for 2 AP. Carry the operator to E.",
        map: [
            "###########",
            "#e...#..ot#",
            "#..c.#.c..#",
            "#1...#g...#",
            "#2.c...c..#",
            "#3...#..g.#",
            "#....b....#",
            "###########",
        ],
        objective: Objective::Rescue,
        limit: 24,
    },
    Mission {
        name: "Relay Nine",
        story: "Capture the relay.",
        order: "Tune [O]; hold within one tile for 2 enemy turns.",
        tip: "Capture: 2 AP. Keep an ally nearby and enemies away.",
        map: [
            "###########",
            "#..g..c.t.#",
            "#..#...#..#",
            "#1.#.o.#g.#",
            "#2.c...c..#",
            "#3.#...#..#",
            "#..b...g..#",
            "###########",
        ],
        objective: Objective::Capture,
        limit: 24,
    },
    Mission {
        name: "Bridge of Sparks",
        story: "Defend the signal cabin.",
        order: "Power [O]; hold 4 turns and survive through turn 6.",
        tip: "Reinforcements arrive on turns 3 and 5.",
        map: [
            "###########",
            "#.....r..t#",
            "#..c.#.c..#",
            "#1...o..g.#",
            "#2.c.#....#",
            "#3...#.c..#",
            "#..b....s.#",
            "###########",
        ],
        objective: Objective::Hold,
        limit: 24,
    },
    Mission {
        name: "Foundry Warden",
        story: "Disable both furnaces.",
        order: "Disable both [O/P] furnaces and clear the patrol.",
        tip: "Disable each furnace from an adjacent tile for 2 AP.",
        map: [
            "###########",
            "#....o..t.#",
            "#..c.#c...#",
            "#1...#..g.#",
            "#2.c...c..#",
            "#3...#.b..#",
            "#....p..t.#",
            "###########",
        ],
        objective: Objective::Sabotage,
        limit: 26,
    },
    Mission {
        name: "The Last Engine",
        story: "Defeat the boss.",
        order: "Defeat the Warden and guard; activate [O].",
        tip: "Boss: 18 hull, 4 damage. Dodge its marked shots.",
        map: [
            "###########",
            "#....g..t.#",
            "#..c.#c...#",
            "#1...#..W.#",
            "#2.c..o...#",
            "#3...#.b..#",
            "#....g..t.#",
            "###########",
        ],
        objective: Objective::Liberate,
        limit: 28,
    },
];
