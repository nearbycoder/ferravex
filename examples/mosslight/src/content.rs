//! Authored expedition layouts. Every floor is checked for reachable objectives.

pub(crate) const COLS: usize = 15;
pub(crate) const ROWS: usize = 11;
pub(crate) const CELLS: usize = COLS * ROWS;

pub(crate) struct Floor {
    pub name: &'static str,
    pub hint: &'static str,
    pub map: [&'static str; ROWS],
}

pub(crate) const FLOORS: [Floor; 6] = [
    Floor {
        name: "The Lantern Stair",
        hint: "Red tiles attack after your next action.",
        map: [
            "###############",
            "#@...#....$...#",
            "#.##.#.###.#..#",
            "#....#..s..#K.#",
            "#.##....#..#..#",
            "#..s..#.#.....#",
            "#.#.###.###.#.#",
            "#.!...#...#.#.#",
            "#.###.#.$...#.#",
            "#.....O.....D>#",
            "###############",
        ],
    },
    Floor {
        name: "The Glasshouse",
        hint: "Walls block spore beams.",
        map: [
            "###############",
            "#@..s...#....$#",
            "#.#.###.#.##..#",
            "#.#...#...f...#",
            "#...^.#.###.#.#",
            "###.#...#K..#.#",
            "#...#.f.#.#...#",
            "#.#.###...#.#.#",
            "#.!.....g.#...#",
            "#$...O.....D>.#",
            "###############",
        ],
    },
    Floor {
        name: "The Thorn Library",
        hint: "Knights rest after attacking. Guard blocks 3 damage.",
        map: [
            "###############",
            "#@....#...$...#",
            "#.###.#.###.#.#",
            "#...#...#...#.#",
            "#g#.#.#.#.f.#.#",
            "#.#...#K#...#.#",
            "#.#.#.#.#.#.#.#",
            "#...#...g.#...#",
            "#.###.###.#.^.#",
            "#.!..O..$..D>.#",
            "###############",
        ],
    },
    Floor {
        name: "The Flooded Choir",
        hint: "Flasks heal 12 HP and use one turn.",
        map: [
            "###############",
            "#@..#...$...f.#",
            "#.#.#.#####.#.#",
            "#.#...#...#...#",
            "#.###.#.#.###.#",
            "#...s...#...K.#",
            "#.#.###.###.#.#",
            "#f#...#..g..#.#",
            "#...#.#.^.#...#",
            "#.!...O...$D>.#",
            "###############",
        ],
    },
    Floor {
        name: "The Amber Vault",
        hint: "The key opens gold seals and stairs.",
        map: [
            "###############",
            "#@....#.$....$#",
            "#.###.#.###.#.#",
            "#...g...#...#.#",
            "#.#.###.#.f...#",
            "#.#...#K#.#.#.#",
            "#f###.#.#.#.#.#",
            "#...#...g...#.#",
            "#.^.#.###.^.#.#",
            "#.!...O.$..D>.#",
            "###############",
        ],
    },
    Floor {
        name: "The Root King's Hall",
        hint: "Dodge the boss's cross and ring attacks.",
        map: [
            "###############",
            "#@...#...#...$#",
            "#.##.#.#.#.#..#",
            "#....s.#...#..#",
            "#.#.##...##.#.#",
            "#...#..B..#...#",
            "#.#...^.^...#.#",
            "#...#.....#...#",
            "#.!..#K#...f..#",
            "#$...O.....D>.#",
            "###############",
        ],
    },
];
