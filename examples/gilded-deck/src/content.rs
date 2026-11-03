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
    pub fn description(self) -> String {
        match self.kind {
            Kind::Strike => format!("Deal {} damage.", self.amount(7, 10)),
            Kind::Guard => format!("Gain {} block.", self.amount(6, 9)),
            Kind::Riposte => format!(
                "Gain {} block. Deal {} damage. Apply 1 weak.",
                self.amount(5, 7),
                self.amount(4, 6)
            ),
            Kind::Venom => format!(
                "Apply {} poison. Poison bypasses armor and block.",
                self.amount(4, 6)
            ),
            Kind::Insight => format!("Draw {} cards. Exhaust.", self.amount(2, 3)),
            Kind::Crush => format!("Deal {} damage. Apply 2 vulnerable.", self.amount(16, 21)),
            Kind::Siphon => format!(
                "Deal {} damage. Heal {} HP. Exhaust.",
                self.amount(5, 8),
                self.amount(3, 5)
            ),
            Kind::Flurry => format!(
                "Deal {} damage three times. Strength applies to each hit.",
                self.amount(3, 4)
            ),
            Kind::Fortify => format!(
                "Gain {} block and {} thorns for this battle.",
                self.amount(14, 19),
                self.amount(2, 3)
            ),
            Kind::Spark => format!("Gain {} energy this turn. Exhaust.", self.amount(1, 2)),
            Kind::Focus => format!(
                "Gain {} strength for this battle. Exhaust.",
                self.amount(2, 3)
            ),
            Kind::Gambit => format!(
                "Pay {} HP. Deal {} damage.",
                self.amount(3, 2),
                self.amount(9, 13)
            ),
            Kind::Ward => format!(
                "Gain {} block. Remove your poison, weak and vulnerable.",
                self.amount(5, 8)
            ),
            Kind::Reversal => format!(
                "Deal your current block plus {} damage. Spend all block.",
                self.amount(4, 8)
            ),
            Kind::Purify => format!("Heal {} HP. Exhaust. No energy cost.", self.amount(4, 7)),
            Kind::Finale => format!(
                "Deal {} plus 3 per exhausted card damage.",
                self.amount(24, 31)
            ),
            Kind::Wound => "Unplayable. Discard at turn end. Removed after this battle.".into(),
        }
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
    pub story: &'static str,
    pub hp: i16,
    pub armor: i16,
    pub elite: bool,
    pub intents: &'static [Intent],
}
pub(crate) const ENCOUNTERS: [Encounter; 9] = [
    Encounter {
        name: "Tollkeeper",
        place: "The Copper Gate",
        story: "First battle.",
        hp: 32,
        armor: 0,
        elite: false,
        intents: &[hit(6, 1), Intent { block: 6, ..EMPTY }, hit(9, 1)],
    },
    Encounter {
        name: "Silk Duelist",
        place: "The Scarlet Stair",
        story: "Defeat the courtier.",
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
        story: "Armor reduces each hit.",
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
        story: "Wound cards clog your deck.",
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
        story: "Enemy damage increases each cycle.",
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
        story: "Debt causes poison.",
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
        story: "Use heavy attacks or poison against armor.",
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
        story: "Expect shields and false cards.",
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
        story: "Boss gains 3 strength once below half health.",
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
pub(crate) const RELIC_TEXT: [&str; 6] = [
    "+1 strength at the start of every battle.",
    "Draw one extra card each turn, up to seven.",
    "Heal 6 extra HP after every victory.",
    "Gain 16 maximum HP and heal 16 immediately.",
    "Start every turn with 4 energy instead of 3.",
    "Start every battle with 3 thorns.",
];
