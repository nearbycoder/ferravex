//! Authored menus and kitchen layout. No wall clocks, assets or platform state.
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
pub(crate) const GRID_Y: f32 = 222.;
pub(crate) const TILE: f32 = 52.;
pub(crate) const INGREDIENTS: [&str; 4] = ["Tomato", "Greens", "Mushroom", "Rice"];
pub(crate) const SHORT_INGREDIENTS: [&str; 4] = ["Tomato", "Greens", "Fungi", "Rice"];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Recipe {
    pub name: &'static str,
    pub mask: u8,
    pub cook: u32,
    pub value: u32,
    pub note: &'static str,
}
pub(crate) const RECIPES: [Recipe; 6] = [
    Recipe {
        name: "Garden Salad",
        mask: 3,
        cook: 0,
        value: 18,
        note: "A cool bowl for the neighborhood. No burner needed.",
    },
    Recipe {
        name: "Forest Soup",
        mask: 5,
        cook: 180,
        value: 22,
        note: "Tomatoes and mushrooms simmer into a rich red soup.",
    },
    Recipe {
        name: "Tomato Rice",
        mask: 9,
        cook: 240,
        value: 24,
        note: "A bright, hearty favorite for the market crowd.",
    },
    Recipe {
        name: "Herb Pilaf",
        mask: 10,
        cook: 210,
        value: 24,
        note: "Finely sliced greens scent a pot of fluffy rice.",
    },
    Recipe {
        name: "Fungi Risotto",
        mask: 12,
        cook: 300,
        value: 26,
        note: "Slow cooking brings out the forest's deeper flavor.",
    },
    Recipe {
        name: "House Feast",
        mask: 15,
        cook: 360,
        value: 36,
        note: "Every pantry joins the final banquet: four ingredients, one big pot.",
    },
];
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shift {
    pub name: &'static str,
    pub story: &'static str,
    pub menu: &'static [usize],
    pub interval: u32,
    pub patience: u32,
    pub quota: u8,
}
pub(crate) const SHIFTS: [Shift; 7] = [
    Shift {
        name: "The First Lunch",
        story: "Serve fresh salads.",
        menu: &[0, 0, 0, 0],
        interval: 1200,
        patience: 3600,
        quota: 3,
    },
    Shift {
        name: "Rain on the Windows",
        story: "Prepare salads and soup.",
        menu: &[1, 0, 1, 0, 1],
        interval: 1260,
        patience: 3600,
        quota: 4,
    },
    Shift {
        name: "Market Day",
        story: "Use both burners for tomato rice.",
        menu: &[2, 0, 1, 2, 1],
        interval: 1200,
        patience: 3420,
        quota: 4,
    },
    Shift {
        name: "The Garden Club",
        story: "Herb pilaf needs greens and rice.",
        menu: &[3, 2, 0, 3, 1, 3],
        interval: 1140,
        patience: 3300,
        quota: 5,
    },
    Shift {
        name: "After the Theater",
        story: "Risotto takes longer to cook.",
        menu: &[4, 1, 3, 4, 2, 0, 4],
        interval: 1080,
        patience: 3300,
        quota: 6,
    },
    Shift {
        name: "The Critic's Table",
        story: "Serve the mixed menu with minimal waste.",
        menu: &[2, 4, 1, 0, 3, 4, 2],
        interval: 1020,
        patience: 3180,
        quota: 6,
    },
    Shift {
        name: "The Lantern Banquet",
        story: "Prepare the four-ingredient House Feast.",
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
pub(crate) const UPGRADE_HELP: [&str; 3] = [
    "Chop in 1.0s, then 0.7s.",
    "Walk faster between stations.",
    "Longer grace before burning.",
];
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
