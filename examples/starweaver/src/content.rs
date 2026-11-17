//! Authored star chart, finite deposits and fabrication recipes.

pub(crate) const SECTORS: usize = 9;
pub(crate) const DEADLINE: u16 = 52;
pub(crate) const RESOURCE_NAMES: [&str; 3] = ["Alloy", "Crystal", "Circuit"];
pub(crate) const COMPONENT_NAMES: [&str; 3] = ["Beacon spindle", "Phase loom", "Heart engine"];
pub(crate) const RELIC_NAMES: [&str; 3] = ["Navigator's key", "Singing lens", "Sentinel core"];
pub(crate) const RELIC_SECTORS: [usize; 3] = [3, 4, 7];
/// Credits paid for one alloy, crystal or circuit at a station.
pub(crate) const SELL_PRICE: [u16; 3] = [8, 12, 16];

pub(crate) struct Sector {
    pub name: &'static str,
    pub kind: &'static str,
    pub x: f32,
    pub y: f32,
    pub color: [f32; 3],
    pub supplies: [u16; 3],
    pub arrival_damage: u16,
    pub extraction_damage: u16,
    pub station: bool,
}
pub(crate) const CHART: [Sector; SECTORS] = [
    Sector {
        name: "Loom Dock",
        kind: "Home gate",
        x: 88.,
        y: 332.,
        color: [0.37, 0.78, 0.91],
        supplies: [0, 0, 0],
        arrival_damage: 0,
        extraction_damage: 0,
        station: true,
    },
    Sector {
        name: "Cinder Belt",
        kind: "Alloy field",
        x: 206.,
        y: 218.,
        color: [0.88, 0.48, 0.24],
        supplies: [8, 1, 1],
        arrival_damage: 0,
        extraction_damage: 0,
        station: false,
    },
    Sector {
        name: "Glass Comet",
        kind: "Crystal field",
        x: 196.,
        y: 454.,
        color: [0.44, 0.85, 0.94],
        supplies: [1, 7, 0],
        arrival_damage: 0,
        extraction_damage: 0,
        station: false,
    },
    Sector {
        name: "Wayfarer",
        kind: "Wreck",
        x: 340.,
        y: 177.,
        color: [0.63, 0.69, 0.82],
        supplies: [5, 0, 5],
        arrival_damage: 6,
        extraction_damage: 0,
        station: false,
    },
    Sector {
        name: "Hollow Choir",
        kind: "Lens vault",
        x: 360.,
        y: 326.,
        color: [0.72, 0.47, 0.91],
        supplies: [0, 7, 2],
        arrival_damage: 10,
        extraction_damage: 2,
        station: false,
    },
    Sector {
        name: "Verdant Archive",
        kind: "Trade station",
        x: 340.,
        y: 474.,
        color: [0.42, 0.78, 0.51],
        supplies: [2, 5, 2],
        arrival_damage: 0,
        extraction_damage: 0,
        station: true,
    },
    Sector {
        name: "Copper Reef",
        kind: "Privateer outpost",
        x: 493.,
        y: 220.,
        color: [0.87, 0.62, 0.29],
        supplies: [5, 2, 6],
        arrival_damage: 8,
        extraction_damage: 1,
        station: false,
    },
    Sector {
        name: "Silent Crown",
        kind: "Core vault",
        x: 523.,
        y: 352.,
        color: [0.92, 0.43, 0.52],
        supplies: [5, 5, 5],
        arrival_damage: 14,
        extraction_damage: 2,
        station: false,
    },
    Sector {
        name: "Far Beacon",
        kind: "Crew rescue",
        x: 510.,
        y: 482.,
        color: [0.94, 0.83, 0.43],
        supplies: [1, 3, 2],
        arrival_damage: 0,
        extraction_damage: 0,
        station: false,
    },
];
pub(crate) const LINKS: [(usize, usize); 13] = [
    (0, 1),
    (0, 2),
    (1, 3),
    (1, 4),
    (2, 4),
    (2, 5),
    (3, 6),
    (4, 6),
    (4, 7),
    (5, 7),
    (5, 8),
    (6, 7),
    (7, 8),
];
pub(crate) struct Recipe {
    pub name: &'static str,
    pub cost: [u16; 3],
}
pub(crate) const RECIPES: [Recipe; 6] = [
    Recipe {
        name: "Beacon spindle",
        cost: [3, 2, 1],
    },
    Recipe {
        name: "Phase loom",
        cost: [2, 3, 1],
    },
    Recipe {
        name: "Heart engine",
        cost: [3, 2, 3],
    },
    Recipe {
        name: "Radiant shield",
        cost: [2, 0, 1],
    },
    Recipe {
        name: "Cargo cradle",
        cost: [3, 0, 1],
    },
    Recipe {
        name: "Deep scanner",
        cost: [1, 2, 0],
    },
];
