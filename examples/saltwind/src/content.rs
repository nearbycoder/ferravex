//! Authored islands, market specialties and the six-season charter story.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Port {
    pub name: &'static str,
    pub position: [f32; 2],
    pub prices: [u32; 3],
    pub specialty: &'static str,
}

pub(crate) const GOODS: [&str; 3] = ["Grain", "Cloth", "Spice"];
pub(crate) const PORTS: [Port; 6] = [
    Port {
        name: "Haven",
        position: [118., 377.],
        prices: [14, 34, 60],
        specialty: "Grain coast",
    },
    Port {
        name: "Brinewatch",
        position: [216., 215.],
        prices: [35, 15, 48],
        specialty: "Weavers' harbor",
    },
    Port {
        name: "Glass Quay",
        position: [410., 196.],
        prices: [38, 36, 22],
        specialty: "Spice exchange",
    },
    Port {
        name: "Cedar Reach",
        position: [537., 339.],
        prices: [18, 42, 50],
        specialty: "Grain terraces",
    },
    Port {
        name: "Saffron Key",
        position: [428., 480.],
        prices: [42, 20, 18],
        specialty: "Spice gardens",
    },
    Port {
        name: "Gullspire",
        position: [212., 493.],
        prices: [20, 46, 38],
        specialty: "Grain storehouses",
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct Charter {
    pub name: &'static str,
    pub destination: usize,
    pub good: usize,
    pub amount: u32,
    pub reward: u32,
    pub arrival: &'static str,
}

pub(crate) const CHARTERS: [Charter; 6] = [
    Charter {
        name: "Bread for the Beacon",
        destination: 1,
        good: 0,
        amount: 3,
        reward: 170,
        arrival: "Delivery complete.",
    },
    Charter {
        name: "Windows to the World",
        destination: 2,
        good: 1,
        amount: 3,
        reward: 180,
        arrival: "Delivery complete.",
    },
    Charter {
        name: "A Feast in the Hills",
        destination: 3,
        good: 2,
        amount: 4,
        reward: 230,
        arrival: "Delivery complete.",
    },
    Charter {
        name: "After the Monsoon",
        destination: 4,
        good: 0,
        amount: 4,
        reward: 230,
        arrival: "Delivery complete.",
    },
    Charter {
        name: "The Winter Stores",
        destination: 5,
        good: 2,
        amount: 5,
        reward: 290,
        arrival: "Delivery complete.",
    },
    Charter {
        name: "The Homeward Cargo",
        destination: 0,
        good: 0,
        amount: 5,
        reward: 280,
        arrival: "Delivery complete.",
    },
];

pub(crate) const NEWS: [&str; 6] = [
    "Fair spring winds. Grain and cloth markets are quiet.",
    "Festival week: cloth fetches higher prices across the islands.",
    "The monsoon turns north. Spice prices are rising.",
    "A rich harvest lowers grain prices. Beware open-sea squalls.",
    "Cold nights lift grain demand. The debt deadline approaches.",
    "The autumn fair needs cloth. Bring the ship safely home.",
];
