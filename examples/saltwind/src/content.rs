//! Authored islands, market specialties and the six-season charter story.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Port {
    pub name: &'static str,
    pub position: [f32; 2],
    pub prices: [u32; 3],
}

pub(crate) const GOODS: [&str; 3] = ["Grain", "Cloth", "Spice"];
pub(crate) const PORTS: [Port; 6] = [
    Port {
        name: "Haven",
        position: [118., 377.],
        prices: [14, 34, 60],
    },
    Port {
        name: "Brinewatch",
        position: [216., 215.],
        prices: [35, 15, 48],
    },
    Port {
        name: "Glass Quay",
        position: [410., 196.],
        prices: [38, 36, 22],
    },
    Port {
        name: "Cedar Reach",
        position: [537., 339.],
        prices: [18, 42, 50],
    },
    Port {
        name: "Saffron Key",
        position: [428., 480.],
        prices: [42, 20, 18],
    },
    Port {
        name: "Gullspire",
        position: [212., 493.],
        prices: [20, 46, 38],
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct Charter {
    pub name: &'static str,
    pub destination: usize,
    pub good: usize,
    pub amount: u32,
    pub reward: u32,
}

pub(crate) const CHARTERS: [Charter; 6] = [
    Charter {
        name: "Bread for the Beacon",
        destination: 1,
        good: 0,
        amount: 3,
        reward: 170,
    },
    Charter {
        name: "Windows to the World",
        destination: 2,
        good: 1,
        amount: 3,
        reward: 180,
    },
    Charter {
        name: "A Feast in the Hills",
        destination: 3,
        good: 2,
        amount: 4,
        reward: 230,
    },
    Charter {
        name: "After the Monsoon",
        destination: 4,
        good: 0,
        amount: 4,
        reward: 230,
    },
    Charter {
        name: "The Winter Stores",
        destination: 5,
        good: 2,
        amount: 5,
        reward: 290,
    },
    Charter {
        name: "The Homeward Cargo",
        destination: 0,
        good: 0,
        amount: 5,
        reward: 280,
    },
];

/// Headline of each week's forecast; its price effect lives in the model.
pub(crate) const FORECASTS: [&str; 6] = [
    "Fair winds",
    "Festival",
    "Monsoon",
    "Harvest",
    "Cold nights",
    "Autumn fair",
];
