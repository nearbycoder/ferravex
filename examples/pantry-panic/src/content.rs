//! Authored menus and kitchen layout. No wall clocks, assets or platform state.
use ferravex_arcade::Vec2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub x: i32,
    pub y: i32,
}
impl Cell {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}
pub(crate) const GRID_X: f32 = 36.;
pub(crate) const GRID_Y: f32 = 230.;
pub(crate) const TILE: f32 = 52.;
/// The dining room above the kitchen: the arrival door, then three tables.
pub(crate) const DINING_TOP: f32 = 118.;
pub(crate) const DINING_BOTTOM: f32 = 222.;
pub(crate) const TABLE_X: f32 = 94.;
pub(crate) const TABLE_STEP: f32 = 182.;
pub(crate) const TABLE_W: f32 = 176.;
/// Recipe-book thumbnails across the top of the side panel (centers).
pub(crate) const THUMB_X: f32 = 726.;
pub(crate) const THUMB_STEP: f32 = 34.;
pub(crate) const THUMB_Y: f32 = 150.;
/// The table (order slot) under `p`, matching the drawn dining room.
pub(crate) fn table_at(p: Vec2) -> Option<usize> {
    (0..3).find(|&i| {
        let x = TABLE_X + i as f32 * TABLE_STEP;
        p.x >= x && p.x < x + TABLE_W && p.y >= DINING_TOP && p.y < DINING_BOTTOM
    })
}
/// The recipe thumbnail under `p`.
pub(crate) fn thumb_at(p: Vec2) -> Option<usize> {
    (0..RECIPES.len()).find(|&i| {
        let x = THUMB_X + i as f32 * THUMB_STEP;
        (p.x - x).abs() < THUMB_STEP * 0.5 && (p.y - THUMB_Y).abs() < 17.
    })
}
/// The between-service upgrade card under `p`.
pub(crate) fn upgrade_at(p: Vec2) -> Option<usize> {
    (0..3).find(|&i| {
        p.x >= 86. + i as f32 * 270. && p.x < 338. + i as f32 * 270. && p.y >= 346. && p.y <= 442.
    })
}
/// The menu card's start/continue button.
pub(crate) fn menu_start(p: Vec2) -> bool {
    p.x >= 290. && p.x <= 670. && p.y >= 484. && p.y <= 536.
}
/// The kitchen cell under `p` (possibly outside the room).
pub(crate) fn cell_at(p: Vec2) -> Cell {
    Cell::new(
        ((p.x - GRID_X) / TILE).floor() as i32,
        ((p.y - GRID_Y) / TILE).floor() as i32,
    )
}
/// The station drawn on `cell`.
pub(crate) fn station_on(cell: Cell) -> Option<Station> {
    STATIONS.iter().find(|s| s.1 == cell).map(|s| s.0)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Recipe {
    pub name: &'static str,
    pub mask: u8,
    pub cook: u32,
    pub value: u32,
}
pub(crate) const RECIPES: [Recipe; 6] = [
    Recipe {
        name: "Garden Salad",
        mask: 3,
        cook: 0,
        value: 18,
    },
    Recipe {
        name: "Forest Soup",
        mask: 5,
        cook: 180,
        value: 22,
    },
    Recipe {
        name: "Tomato Rice",
        mask: 9,
        cook: 240,
        value: 24,
    },
    Recipe {
        name: "Herb Pilaf",
        mask: 10,
        cook: 210,
        value: 24,
    },
    Recipe {
        name: "Fungi Risotto",
        mask: 12,
        cook: 300,
        value: 26,
    },
    Recipe {
        name: "House Feast",
        mask: 15,
        cook: 360,
        value: 36,
    },
];
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shift {
    pub name: &'static str,
    pub menu: &'static [usize],
    pub interval: u32,
    pub patience: u32,
    pub quota: u8,
}
pub(crate) const SHIFTS: [Shift; 7] = [
    Shift {
        name: "The First Lunch",
        menu: &[0, 0, 0, 0],
        interval: 1200,
        patience: 3600,
        quota: 3,
    },
    Shift {
        name: "Rain on the Windows",
        menu: &[1, 0, 1, 0, 1],
        interval: 1260,
        patience: 3600,
        quota: 4,
    },
    Shift {
        name: "Market Day",
        menu: &[2, 0, 1, 2, 1],
        interval: 1200,
        patience: 3420,
        quota: 4,
    },
    Shift {
        name: "The Garden Club",
        menu: &[3, 2, 0, 3, 1, 3],
        interval: 1140,
        patience: 3300,
        quota: 5,
    },
    Shift {
        name: "After the Theater",
        menu: &[4, 1, 3, 4, 2, 0, 4],
        interval: 1080,
        patience: 3300,
        quota: 6,
    },
    Shift {
        name: "The Critic's Table",
        menu: &[2, 4, 1, 0, 3, 4, 2],
        interval: 1020,
        patience: 3180,
        quota: 6,
    },
    Shift {
        name: "The Lantern Banquet",
        menu: &[5, 0, 4, 5, 3, 2, 1, 5],
        interval: 1380,
        patience: 4200,
        quota: 7,
    },
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Station {
    Pantry(usize),
    Chop,
    Mix,
    Burner(usize),
    Plate,
    Serve,
    Bin,
}
pub(crate) const STATIONS: [(Station, Cell, Cell); 11] = [
    (Station::Pantry(0), Cell::new(1, 0), Cell::new(1, 1)),
    (Station::Pantry(1), Cell::new(3, 0), Cell::new(3, 1)),
    (Station::Pantry(2), Cell::new(5, 0), Cell::new(5, 1)),
    (Station::Pantry(3), Cell::new(7, 0), Cell::new(7, 1)),
    (Station::Chop, Cell::new(0, 2), Cell::new(1, 2)),
    (Station::Mix, Cell::new(0, 4), Cell::new(1, 4)),
    (Station::Burner(0), Cell::new(3, 5), Cell::new(3, 4)),
    (Station::Burner(1), Cell::new(5, 5), Cell::new(5, 4)),
    (Station::Plate, Cell::new(10, 4), Cell::new(9, 4)),
    (Station::Serve, Cell::new(10, 1), Cell::new(9, 1)),
    (Station::Bin, Cell::new(8, 5), Cell::new(8, 4)),
];
pub(crate) const UPGRADE_NAMES: [&str; 3] = ["Sharp Knife", "Quick Shoes", "Safe Burners"];
pub(crate) fn upgrade_cost(kind: usize, rank: u8) -> u32 {
    [45, 50, 35][kind] + u32::from(rank) * 30
}
pub(crate) fn access(station: Station) -> Cell {
    STATIONS
        .iter()
        .find(|s| s.0 == station)
        .expect("known station")
        .2
}
pub(crate) fn recipe_for(mask: u8) -> Option<usize> {
    RECIPES.iter().position(|r| r.mask == mask)
}
