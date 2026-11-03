//! Six authored floor plans and bounded guard patrols.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Patrol {
    pub points: &'static [(u8, u8)],
    pub period: u16,
}
pub(crate) struct Heist {
    pub name: &'static str,
    pub object: &'static str,
    pub briefing: &'static str,
    pub rows: [&'static str; 10],
    pub patrols: &'static [Patrol],
    pub seconds: u32,
}
const LEFT: &[(u8, u8)] = &[(3, 3), (5, 3), (5, 6), (3, 6)];
const RIGHT: &[(u8, u8)] = &[(10, 3), (12, 3), (12, 6), (10, 6)];
const LEFT_REV: &[(u8, u8)] = &[(5, 6), (5, 3), (3, 3), (3, 6)];
const RIGHT_REV: &[(u8, u8)] = &[(12, 6), (12, 3), (10, 3), (10, 6)];
pub(crate) const HEISTS: [Heist; 6] = [
    Heist {
        name: "The Velvet Foyer",
        object: "The donor's ledger",
        briefing: "Collect both records. Shadow corridors avoid the central guards.",
        rows: [
            "################",
            "#:K::::=::::::L#",
            "#:....##.......#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.$...#....$..#",
            "#E:::::#::::::L#",
            "################",
        ],
        patrols: &[
            Patrol {
                points: LEFT,
                period: 38,
            },
            Patrol {
                points: RIGHT,
                period: 42,
            },
        ],
        seconds: 90,
    },
    Heist {
        name: "The Glass Conservatory",
        object: "The seed archive",
        briefing: "Use the green console to disable red beams. Collect both cases.",
        rows: [
            "################",
            "#E:T:::#::::::L#",
            "#:..$..#..$....#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:###.##.###...#",
            "#:K::::=:::!::L#",
            "################",
        ],
        patrols: &[
            Patrol {
                points: LEFT_REV,
                period: 32,
            },
            Patrol {
                points: RIGHT,
                period: 35,
            },
        ],
        seconds: 100,
    },
    Heist {
        name: "The Opera Vault",
        object: "The silenced score",
        briefing: "H cabinets hide you while crouching. Coins distract guards.",
        rows: [
            "################",
            "#L::!:::=::::K:#",
            "#.....H.##.H..:#",
            "#.......#.....:#",
            "#.......=.....:#",
            "#.......#.....:#",
            "#.......#.....:#",
            "#..#.$..#..$#.:#",
            "#L::::::#:::T:E#",
            "################",
        ],
        patrols: &[
            Patrol {
                points: LEFT,
                period: 28,
            },
            Patrol {
                points: RIGHT_REV,
                period: 30,
            },
        ],
        seconds: 105,
    },
    Heist {
        name: "The Night Train",
        object: "The workers' bonds",
        briefing: "Collect three packets. Use the brass pass to open doors.",
        rows: [
            "################",
            "#E:T:::#L:::::L#",
            "#:.$...#..$....#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.....=.......#",
            "#:.....#.......#",
            "#:.H..##.......#",
            "#:K::::=:::!::L#",
            "################",
        ],
        patrols: &[
            Patrol {
                points: LEFT_REV,
                period: 25,
            },
            Patrol {
                points: RIGHT,
                period: 27,
            },
            Patrol {
                points: RIGHT_REV,
                period: 38,
            },
        ],
        seconds: 110,
    },
    Heist {
        name: "The Clockwork Bank",
        object: "The stolen pensions",
        briefing: "Collect three rolls. Beams trigger alarms without a guard nearby.",
        rows: [
            "################",
            "#L!::::L#:::T:E#",
            "#....$H.#..$H.:#",
            "#.......#.....:#",
            "#.......#.....:#",
            "#.......=.....:#",
            "#.......#.....:#",
            "#...###.##.###:#",
            "#L::!:::=::::K:#",
            "################",
        ],
        patrols: &[
            Patrol {
                points: LEFT,
                period: 23,
            },
            Patrol {
                points: RIGHT_REV,
                period: 25,
            },
            Patrol {
                points: RIGHT,
                period: 34,
            },
        ],
        seconds: 115,
    },
    Heist {
        name: "The Museum of Midnight",
        object: "The city's true history",
        briefing: "Collect three records and return to the west exit.",
        rows: [
            "################",
            "#:K::::=:::!::L#",
            "#:.H..##.H.....#",
            "#:.....#.......#",
            "#:.....=.......#",
            "#:.....#.......#",
            "#:.....#.......#",
            "#:.$...#..$....#",
            "#E:T:::#L::::!L#",
            "################",
        ],
        patrols: &[
            Patrol {
                points: LEFT,
                period: 24,
            },
            Patrol {
                points: LEFT_REV,
                period: 31,
            },
            Patrol {
                points: RIGHT,
                period: 23,
            },
            Patrol {
                points: RIGHT_REV,
                period: 33,
            },
        ],
        seconds: 120,
    },
];
