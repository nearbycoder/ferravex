//! Authored schedules; all times are simulation seconds from shift opening.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    West,
    East,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Service {
    Local,
    Mail,
    Freight,
}

impl Service {
    pub(crate) fn speed(self) -> f32 {
        match self {
            Self::Local => 0.90,
            Self::Mail => 1.12,
            Self::Freight => 0.68,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Mail => "Mail",
            Self::Freight => "Freight",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Departure {
    pub name: &'static str,
    pub side: Side,
    /// A/B are west north/south, C/D east north/south.
    pub destination: usize,
    pub release: u32,
    pub due: u32,
    pub service: Service,
}

const fn train(
    name: &'static str,
    side: Side,
    destination: usize,
    release: u32,
    due: u32,
    service: Service,
) -> Departure {
    Departure {
        name,
        side,
        destination,
        release,
        due,
        service,
    }
}

use Service::{Freight, Local, Mail};
use Side::{East, West};

const ORIENTATION: [Departure; 4] = [
    train("W1", West, 0, 0, 26, Local),
    train("E1", East, 2, 8, 38, Local),
    train("W2", West, 3, 17, 55, Local),
    train("E2", East, 1, 27, 72, Local),
];
const TWO_BANKS: [Departure; 5] = [
    train("E3", East, 1, 0, 27, Local),
    train("W3", West, 2, 4, 32, Mail),
    train("E4", East, 3, 10, 42, Freight),
    train("W4", West, 0, 16, 53, Local),
    train("E5", East, 0, 25, 68, Mail),
];
const HARVEST: [Departure; 6] = [
    train("W5", West, 3, 0, 30, Freight),
    train("E6", East, 1, 5, 38, Freight),
    train("W6", West, 0, 10, 45, Mail),
    train("E7", East, 2, 15, 52, Local),
    train("W7", West, 2, 22, 64, Freight),
    train("E8", East, 0, 28, 78, Local),
];
const INSPECTION: [Departure; 7] = [
    train("M1", West, 2, 0, 22, Mail),
    train("L1", East, 3, 3, 30, Local),
    train("F1", West, 1, 8, 42, Freight),
    train("M2", East, 0, 14, 55, Mail),
    train("L2", West, 3, 20, 64, Local),
    train("F2", East, 2, 27, 70, Freight),
    train("M3", West, 2, 34, 82, Mail),
];
const COAST: [Departure; 8] = [
    train("C1", West, 3, 0, 24, Mail),
    train("C2", East, 0, 3, 32, Local),
    train("C3", West, 1, 7, 39, Freight),
    train("C4", East, 2, 11, 45, Mail),
    train("C5", East, 1, 16, 55, Local),
    train("C6", West, 2, 21, 64, Freight),
    train("C7", West, 0, 27, 72, Mail),
    train("C8", East, 3, 33, 80, Local),
];
const RAIN: [Departure; 9] = [
    train("R1", East, 1, 0, 27, Mail),
    train("R2", West, 0, 3, 35, Local),
    train("R3", West, 3, 7, 45, Freight),
    train("R4", East, 2, 10, 49, Mail),
    train("R5", West, 2, 15, 62, Local),
    train("R6", East, 0, 21, 73, Freight),
    train("R7", East, 3, 27, 81, Mail),
    train("R8", West, 1, 33, 88, Local),
    train("R9", East, 0, 40, 100, Mail),
];
const SNOW: [Departure; 10] = [
    train("S1", West, 2, 0, 28, Mail),
    train("S2", East, 1, 3, 39, Freight),
    train("S3", East, 2, 7, 45, Local),
    train("S4", West, 1, 12, 53, Mail),
    train("S5", East, 0, 17, 64, Local),
    train("S6", West, 3, 22, 76, Freight),
    train("S7", West, 0, 28, 82, Mail),
    train("S8", East, 3, 33, 90, Local),
    train("S9", West, 2, 39, 104, Mail),
    train("S0", East, 1, 45, 118, Freight),
];
const ROYAL: [Departure; 12] = [
    train("K1", West, 2, 0, 23, Mail),
    train("K2", East, 1, 3, 32, Mail),
    train("K3", West, 0, 6, 37, Local),
    train("K4", East, 3, 9, 44, Freight),
    train("K5", West, 3, 13, 56, Mail),
    train("K6", East, 2, 18, 60, Local),
    train("K7", East, 0, 23, 71, Freight),
    train("K8", West, 1, 28, 79, Local),
    train("K9", East, 1, 33, 91, Mail),
    train("KA", West, 2, 38, 103, Freight),
    train("KB", West, 0, 44, 109, Mail),
    train("KC", East, 3, 50, 118, Mail),
];

pub(crate) struct Shift {
    pub name: &'static str,
    pub story: &'static str,
    pub tip: &'static str,
    pub trains: &'static [Departure],
    pub closing: u32,
    pub closure: Option<(u32, u32)>,
    pub speed: f32,
    pub bridge_y: f32,
    pub west_y: f32,
    pub east_y: f32,
}

pub(crate) const SHIFTS: [Shift; 8] = [
    Shift {
        name: "The apprentice's bell",
        story: "Route four trains to their marked platforms.",
        tip: "West points serve A/B; east points serve C/D.",
        trains: &ORIENTATION,
        closing: 125,
        closure: None,
        speed: 1.0,
        bridge_y: 315.0,
        west_y: 315.0,
        east_y: 315.0,
    },
    Shift {
        name: "Two banks, one bridge",
        story: "Opposing trains share one bridge.",
        tip: "Green admits one train, then resets to red.",
        trains: &TWO_BANKS,
        closing: 150,
        closure: None,
        speed: 1.0,
        bridge_y: 282.0,
        west_y: 330.0,
        east_y: 300.0,
    },
    Shift {
        name: "Harvest wagons",
        story: "Freight is slower than mail and local trains.",
        tip: "Leave space behind slow trains.",
        trains: &HARVEST,
        closing: 180,
        closure: None,
        speed: 1.0,
        bridge_y: 350.0,
        west_y: 305.0,
        east_y: 315.0,
    },
    Shift {
        name: "The bridge inspector",
        story: "Bridge closed from 18 to 34 seconds.",
        tip: "Use near-side platforms during the closure.",
        trains: &INSPECTION,
        closing: 200,
        closure: Some((18, 34)),
        speed: 1.0,
        bridge_y: 255.0,
        west_y: 320.0,
        east_y: 350.0,
    },
    Shift {
        name: "The seaside special",
        story: "Deliver eight trains.",
        tip: "Gold: no delay. Silver: up to 45 seconds total delay.",
        trains: &COAST,
        closing: 220,
        closure: None,
        speed: 1.05,
        bridge_y: 340.0,
        west_y: 280.0,
        east_y: 315.0,
    },
    Shift {
        name: "Rain on the wires",
        story: "Rain: 85% speed. Bridge closed from 26 to 43 seconds.",
        tip: "The timetable shows each train's due time.",
        trains: &RAIN,
        closing: 260,
        closure: Some((26, 43)),
        speed: 0.85,
        bridge_y: 295.0,
        west_y: 350.0,
        east_y: 280.0,
    },
    Shift {
        name: "Winter through service",
        story: "Snow slows ten incoming trains.",
        tip: "Three retries for the whole campaign.",
        trains: &SNOW,
        closing: 320,
        closure: None,
        speed: 0.78,
        bridge_y: 260.0,
        west_y: 330.0,
        east_y: 350.0,
    },
    Shift {
        name: "The royal timetable",
        story: "Deliver twelve trains. Bridge closed from 40 to 57 seconds.",
        tip: "Final shift. Keep the bridge clear.",
        trains: &ROYAL,
        closing: 340,
        closure: Some((40, 57)),
        speed: 0.95,
        bridge_y: 315.0,
        west_y: 280.0,
        east_y: 350.0,
    },
];
