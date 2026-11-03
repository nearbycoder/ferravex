pub(crate) const COLS: usize = 11;
pub(crate) const ROWS: usize = 8;
pub(crate) const CELLS: usize = COLS * ROWS;
pub(crate) const BASE: usize = 6 * COLS + 1;

pub(crate) struct Village {
    pub x: usize,
    pub y: usize,
    pub people: u8,
    pub name: &'static str,
}
pub(crate) struct Mission {
    pub name: &'static str,
    pub story: &'static str,
    pub tip: &'static str,
    pub rows: [&'static str; ROWS],
    pub villages: &'static [Village],
    pub fires: &'static [(usize, usize)],
    pub wind: [(i8, i8); 4],
    pub limit: u8,
}
const W: (i8, i8) = (-1, 0);
const N: (i8, i8) = (0, -1);
const S: (i8, i8) = (0, 1);
const fn village(x: usize, y: usize, people: u8, name: &'static str) -> Village {
    Village { x, y, people, name }
}
pub(crate) const MISSIONS: [Mission; 6] = [
    Mission {
        name: "Pine Creek",
        story: "Evacuate both hamlets.",
        tip: "Roads stop fire. Check the forecast before ending a turn.",
        rows: [
            "ff..fff.fff",
            ".=..ff..fff",
            ".=..fff.fff",
            ".=..fff.fff",
            "===========",
            ".=....f.fff",
            ".===..f.fff",
            "...~~~~....",
        ],
        villages: &[village(3, 2, 3, "Pine"), village(5, 5, 3, "Creek")],
        fires: &[(8, 2), (9, 5)],
        wind: [W, W, S, W],
        limit: 18,
    },
    Mission {
        name: "Orchard Wind",
        story: "Evacuate eight residents.",
        tip: "Refill at water or base. Rangers cut breaks for 1 AP.",
        rows: [
            "..ff..fffff",
            ".=..f.fffff",
            ".=...ffffff",
            ".=.f..fffff",
            "======.====",
            ".=...ff.fff",
            ".===..f.fff",
            ".~~~~...fff",
        ],
        villages: &[village(4, 2, 4, "Cider"), village(6, 5, 4, "Hill")],
        fires: &[(9, 1), (9, 5)],
        wind: [W, S, W, N],
        limit: 18,
    },
    Mission {
        name: "Ridge Road",
        story: "Reach three settlements across the ridge.",
        tip: "Rescue capacity: 6. Rock and water stop fire.",
        rows: [
            "..ff.f.ffff",
            ".=..f..ffff",
            ".=##..f#fff",
            ".=...f..fff",
            "===========",
            ".=##...#fff",
            ".===....fff",
            "~~~...bbfff",
        ],
        villages: &[
            village(3, 1, 3, "Cairn"),
            village(5, 3, 3, "Ridge"),
            village(7, 6, 3, "East"),
        ],
        fires: &[(9, 1), (9, 3), (5, 0)],
        wind: [S, W, W, N],
        limit: 20,
    },
    Mission {
        name: "Ember Ferry",
        story: "Evacuate twelve residents across the bridge.",
        tip: "Cross at row 5. Return loaded crews to base.",
        rows: [
            "..fff~.ffff",
            ".=...~fffff",
            ".=...~.ffff",
            ".=fff~.ffff",
            "===========",
            ".=...~.ffff",
            ".===.~...ff",
            "....~~fffff",
        ],
        villages: &[
            village(2, 1, 4, "Ferry"),
            village(6, 2, 4, "Bank"),
            village(8, 6, 4, "Marsh"),
        ],
        fires: &[(8, 0), (9, 3), (7, 7)],
        wind: [S, W, N, W],
        limit: 22,
    },
    Mission {
        name: "Red Sky",
        story: "Protect three settlements from four fires.",
        tip: "Air drop: 12 credits; wets five tiles.",
        rows: [
            ".fff.ffffff",
            ".=..f..ffff",
            ".=..##.ffff",
            ".=.....ffff",
            "======.====",
            ".=..#...fff",
            ".===.....ff",
            "~~~~..bbfff",
        ],
        villages: &[
            village(3, 1, 4, "School"),
            village(6, 3, 4, "Mill"),
            village(8, 6, 4, "Depot"),
        ],
        fires: &[(8, 0), (9, 2), (9, 6), (4, 0)],
        wind: [W, N, W, S],
        limit: 22,
    },
    Mission {
        name: "The Dawn Line",
        story: "Evacuate four villages in changing winds.",
        tip: "Protect occupied villages first.",
        rows: [
            ".fff.f.ffff",
            ".=..f...fff",
            ".=....f.fff",
            ".=..#ff.fff",
            "===========",
            ".=..#....ff",
            ".===..~~fff",
            "~~~...bbfff",
        ],
        villages: &[
            village(2, 1, 3, "Gate"),
            village(5, 2, 3, "Larch"),
            village(7, 5, 3, "Foundry"),
            village(8, 1, 3, "Beacon"),
        ],
        fires: &[(9, 0), (9, 3), (5, 0), (9, 7)],
        wind: [W, N, W, S],
        limit: 24,
    },
];
