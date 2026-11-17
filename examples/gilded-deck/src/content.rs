//! Authored cards, opponents, routes and rewards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Strike,
    Guard,
    Riposte,
    Venom,
    Insight,
    Crush,
    Siphon,
    Flurry,
    Fortify,
    Spark,
    Focus,
    Gambit,
    Ward,
    Reversal,
    Purify,
    Finale,
    Wound,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Card {
    pub kind: Kind,
    pub upgraded: bool,
}
impl Card {
    pub const fn new(kind: Kind) -> Self {
        Self {
            kind,
            upgraded: false,
        }
    }
    pub fn name(self) -> &'static str {
        match self.kind {
            Kind::Strike => "Gilt Strike",
            Kind::Guard => "Velvet Guard",
            Kind::Riposte => "Riposte",
            Kind::Venom => "Night Ink",
            Kind::Insight => "Insight",
            Kind::Crush => "Hammerfall",
            Kind::Siphon => "Blood Tithe",
            Kind::Flurry => "Three Knives",
            Kind::Fortify => "Briar Wall",
            Kind::Spark => "Gold Spark",
            Kind::Focus => "Sharpen",
            Kind::Gambit => "Red Wager",
            Kind::Ward => "Clean Slate",
            Kind::Reversal => "Mirror Edge",
            Kind::Purify => "Mercy",
            Kind::Finale => "Grand Finale",
            Kind::Wound => "Wound",
        }
    }
    pub fn cost(self) -> u8 {
        match self.kind {
            Kind::Insight | Kind::Spark | Kind::Gambit | Kind::Purify => 0,
            Kind::Crush | Kind::Fortify => 2,
            Kind::Finale => 3,
            Kind::Wound => 9,
            _ => 1,
        }
    }
    pub fn exhausts(self) -> bool {
        matches!(
            self.kind,
            Kind::Insight | Kind::Siphon | Kind::Spark | Kind::Focus | Kind::Purify
        )
    }
    pub fn amount(self, normal: i16, improved: i16) -> i16 {
        if self.upgraded { improved } else { normal }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Intent {
    pub damage: i16,
    pub hits: u8,
    pub block: i16,
    pub strength: i16,
    pub weak: u8,
    pub vulnerable: u8,
    pub poison: u8,
    pub wound: bool,
}
const EMPTY: Intent = Intent {
    damage: 0,
    hits: 0,
    block: 0,
    strength: 0,
    weak: 0,
    vulnerable: 0,
    poison: 0,
    wound: false,
};
const fn hit(damage: i16, hits: u8) -> Intent {
    Intent {
        damage,
        hits,
        ..EMPTY
    }
}
pub(crate) struct Encounter {
    pub name: &'static str,
    pub place: &'static str,
    pub hp: i16,
    pub armor: i16,
    pub elite: bool,
    pub intents: &'static [Intent],
}
pub(crate) const ENCOUNTERS: [Encounter; 9] = [
    Encounter {
        name: "Tollkeeper",
        place: "The Copper Gate",
        hp: 32,
        armor: 0,
        elite: false,
        intents: &[hit(6, 1), Intent { block: 6, ..EMPTY }, hit(9, 1)],
    },
    Encounter {
        name: "Silk Duelist",
        place: "The Scarlet Stair",
        hp: 42,
        armor: 0,
        elite: false,
        intents: &[
            hit(4, 2),
            Intent {
                damage: 5,
                hits: 1,
                weak: 1,
                ..EMPTY
            },
            hit(12, 1),
        ],
    },
    Encounter {
        name: "Brass Colossus",
        place: "The First Vault",
        hp: 64,
        armor: 2,
        elite: true,
        intents: &[
            Intent { block: 10, ..EMPTY },
            hit(14, 1),
            Intent {
                damage: 17,
                hits: 1,
                vulnerable: 1,
                ..EMPTY
            },
        ],
    },
    Encounter {
        name: "Ink Witch",
        place: "The Black Library",
        hp: 55,
        armor: 0,
        elite: false,
        intents: &[
            Intent {
                wound: true,
                weak: 1,
                ..EMPTY
            },
            hit(10, 1),
            Intent {
                damage: 6,
                hits: 1,
                poison: 3,
                ..EMPTY
            },
        ],
    },
    Encounter {
        name: "Twin Masks",
        place: "The Mirror Salon",
        hp: 64,
        armor: 0,
        elite: false,
        intents: &[
            hit(4, 3),
            Intent {
                block: 8,
                strength: 2,
                ..EMPTY
            },
            hit(11, 1),
        ],
    },
    Encounter {
        name: "Tax Collector",
        place: "The Second Vault",
        hp: 82,
        armor: 1,
        elite: true,
        intents: &[
            hit(12, 1),
            Intent {
                poison: 3,
                block: 6,
                ..EMPTY
            },
            hit(20, 1),
            Intent {
                block: 12,
                strength: 2,
                ..EMPTY
            },
        ],
    },
    Encounter {
        name: "Gilded Sentinel",
        place: "The Crown Bridge",
        hp: 88,
        armor: 3,
        elite: false,
        intents: &[
            hit(16, 1),
            Intent {
                block: 12,
                vulnerable: 1,
                ..EMPTY
            },
            hit(22, 1),
        ],
    },
    Encounter {
        name: "Mirror Baron",
        place: "The Last Ballroom",
        hp: 96,
        armor: 0,
        elite: false,
        intents: &[
            Intent {
                block: 14,
                wound: true,
                ..EMPTY
            },
            hit(7, 3),
            Intent {
                strength: 3,
                weak: 1,
                ..EMPTY
            },
        ],
    },
    Encounter {
        name: "The Regent",
        place: "The Gilded Throne",
        hp: 136,
        armor: 2,
        elite: false,
        intents: &[
            Intent {
                damage: 10,
                hits: 2,
                wound: true,
                ..EMPTY
            },
            Intent {
                block: 16,
                strength: 2,
                ..EMPTY
            },
            Intent {
                damage: 26,
                hits: 1,
                weak: 1,
                ..EMPTY
            },
        ],
    },
];
pub(crate) const REWARDS: [[Kind; 3]; 8] = [
    [Kind::Focus, Kind::Ward, Kind::Flurry],
    [Kind::Venom, Kind::Siphon, Kind::Fortify],
    [Kind::Spark, Kind::Finale, Kind::Purify],
    [Kind::Crush, Kind::Ward, Kind::Insight],
    [Kind::Fortify, Kind::Reversal, Kind::Flurry],
    [Kind::Finale, Kind::Spark, Kind::Siphon],
    [Kind::Purify, Kind::Venom, Kind::Gambit],
    [Kind::Ward, Kind::Crush, Kind::Finale],
];
pub(crate) const RELIC_NAMES: [&str; 6] = [
    "Gilt Needle",
    "Scholar Seal",
    "Mercy Bell",
    "Iron Heart",
    "Amber Clock",
    "Thorn Brooch",
];
