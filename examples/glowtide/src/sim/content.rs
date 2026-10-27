//! Authored data: weapons, passives, foes, characters, tides and their copy.
//!
//! Everything here is plain `'static` data, so the simulation never allocates
//! to describe content and presentation can borrow names/flavor freely.

/// Number of distinct weapons, and therefore damage-breakdown rows.
pub const WEAPONS: usize = 8;
/// Number of distinct passives.
pub const PASSIVES: usize = 10;
/// Weapon slots a lantern spirit can carry at once.
pub const WEAPON_SLOTS: usize = 6;
/// Passive slots a lantern spirit can carry at once.
pub const PASSIVE_SLOTS: usize = 6;
/// Highest weapon level before evolution.
pub const WEAPON_MAX: u8 = 7;
/// Highest passive level.
pub const PASSIVE_MAX: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum WeaponId {
    WispBolt,
    HaloOrbit,
    BubbleMortar,
    TideLash,
    ChainSpark,
    LanternAura,
    ShellBoomerang,
    StarSkipper,
}

impl WeaponId {
    pub const ALL: [WeaponId; WEAPONS] = [
        WeaponId::WispBolt,
        WeaponId::HaloOrbit,
        WeaponId::BubbleMortar,
        WeaponId::TideLash,
        WeaponId::ChainSpark,
        WeaponId::LanternAura,
        WeaponId::ShellBoomerang,
        WeaponId::StarSkipper,
    ];
    pub fn info(self) -> &'static WeaponInfo {
        &WEAPON_INFO[self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PassiveId {
    MoonPearl,
    TidalSpiral,
    PressureValve,
    StingingCells,
    EchoShell,
    KelpTea,
    SwiftFins,
    LuckyBarnacle,
    MagnetAnemone,
    ThickShell,
}

impl PassiveId {
    pub const ALL: [PassiveId; PASSIVES] = [
        PassiveId::MoonPearl,
        PassiveId::TidalSpiral,
        PassiveId::PressureValve,
        PassiveId::StingingCells,
        PassiveId::EchoShell,
        PassiveId::KelpTea,
        PassiveId::SwiftFins,
        PassiveId::LuckyBarnacle,
        PassiveId::MagnetAnemone,
        PassiveId::ThickShell,
    ];
    pub fn info(self) -> &'static PassiveInfo {
        &PASSIVE_INFO[self as usize]
    }
}

/// Linear RGB in `0..=1`, kept renderer-independent.
pub type Rgb = [f32; 3];

pub struct WeaponInfo {
    pub name: &'static str,
    pub evolved: &'static str,
    /// Passive required (owned at any level) to evolve a max-level weapon.
    pub pair: PassiveId,
    pub color: Rgb,
    pub summary: &'static str,
    /// Description of each level-up; index = level being reached (2..=7).
    pub levels: [&'static str; WEAPON_MAX as usize + 1],
    pub flavor: &'static str,
    pub evolved_summary: &'static str,
}

pub struct PassiveInfo {
    pub name: &'static str,
    pub color: Rgb,
    pub summary: &'static str,
    pub flavor: &'static str,
}

pub const WEAPON_INFO: [WeaponInfo; WEAPONS] = [
    WeaponInfo {
        name: "Wisp Bolt",
        evolved: "Lunar Lance",
        pair: PassiveId::MoonPearl,
        color: [0.55, 0.95, 1.0],
        summary: "Homing wisps seek the nearest foe.",
        levels: [
            "",
            "Homing wisps seek the nearest foe.",
            "+1 wisp per volley.",
            "+30% damage.",
            "Wisps pierce one more foe.",
            "+1 wisp per volley.",
            "Volleys come 20% faster.",
            "+1 wisp and +40% damage.",
        ],
        flavor: "Tiny, polite and extremely pointy.",
        evolved_summary: "Piercing moonbeam lances. Nothing is safe. Nothing.",
    },
    WeaponInfo {
        name: "Halo Orbit",
        evolved: "Sunring",
        pair: PassiveId::TidalSpiral,
        color: [1.0, 0.86, 0.45],
        summary: "Glow orbs circle you, bonking all they touch.",
        levels: [
            "",
            "Glow orbs circle you, bonking all they touch.",
            "+1 orb.",
            "Orbs spin 25% faster and hit harder.",
            "+1 orb and wider orbit.",
            "Orbs linger 1s longer.",
            "+1 orb.",
            "+40% damage.",
        ],
        flavor: "Personal space, enforced.",
        evolved_summary: "A permanent blazing halo. Very holy. Slightly hot.",
    },
    WeaponInfo {
        name: "Bubble Mortar",
        evolved: "Depth Charge",
        pair: PassiveId::PressureValve,
        color: [0.6, 0.75, 1.0],
        summary: "Lobs bubbles that pop in a splashy blast.",
        levels: [
            "",
            "Lobs bubbles that pop in a splashy blast.",
            "+1 bubble.",
            "+25% blast size.",
            "+40% damage.",
            "+1 bubble.",
            "Lobs 20% more often.",
            "+1 bubble and bigger pops.",
        ],
        flavor: "Fizzy. Furious. Fun at parties.",
        evolved_summary: "Each pop leaves a whirlpool that slurps foes in.",
    },
    WeaponInfo {
        name: "Tide Lash",
        evolved: "Man-o'-War",
        pair: PassiveId::StingingCells,
        color: [0.75, 1.0, 0.7],
        summary: "A stinging tentacle sweep at the nearest foe.",
        levels: [
            "",
            "A stinging tentacle sweep at the nearest foe.",
            "Also lashes the other way.",
            "+35% damage.",
            "+20% reach.",
            "Lashes 20% more often.",
            "+35% damage.",
            "Huge sweep and +30% damage.",
        ],
        flavor: "Slap first. Apologize never.",
        evolved_summary: "A full-circle sting that heals you on every hit.",
    },
    WeaponInfo {
        name: "Chain Spark",
        evolved: "Thunder Eel",
        pair: PassiveId::EchoShell,
        color: [0.7, 0.85, 1.0],
        summary: "Lightning jumps between nearby foes.",
        levels: [
            "",
            "Lightning jumps between nearby foes.",
            "+2 jumps.",
            "+30% damage.",
            "+1 extra spark.",
            "+2 jumps.",
            "Sparks 20% more often.",
            "+1 spark and +40% damage.",
        ],
        flavor: "Shockingly good at networking.",
        evolved_summary: "Twin storms of chained lightning that stun.",
    },
    WeaponInfo {
        name: "Lantern Aura",
        evolved: "Bloomlight",
        pair: PassiveId::KelpTea,
        color: [1.0, 0.7, 0.45],
        summary: "A warm glow that singes and slows foes nearby.",
        levels: [
            "",
            "A warm glow that singes and slows foes nearby.",
            "+20% radius.",
            "+30% damage.",
            "Singes 20% more often.",
            "+20% radius.",
            "+40% damage.",
            "+25% radius and stronger slow.",
        ],
        flavor: "Smells faintly of toasted kelp.",
        evolved_summary: "A huge blooming glow. Kills restore a little light.",
    },
    WeaponInfo {
        name: "Shell Boomerang",
        evolved: "Nautilus",
        pair: PassiveId::SwiftFins,
        color: [1.0, 0.6, 0.65],
        summary: "Throws a shell that comes back. Usually.",
        levels: [
            "",
            "Throws a shell that comes back. Usually.",
            "+1 shell.",
            "+30% damage.",
            "Flies 25% farther.",
            "+1 shell.",
            "+40% damage.",
            "+1 shell and bigger shells.",
        ],
        flavor: "Return policy: guaranteed.",
        evolved_summary: "Spiralling nautilus shells, three at a time.",
    },
    WeaponInfo {
        name: "Star Skipper",
        evolved: "Constellation",
        pair: PassiveId::LuckyBarnacle,
        color: [1.0, 0.95, 0.55],
        summary: "Spinning starfish ricochet off the screen edges.",
        levels: [
            "",
            "Spinning starfish ricochet off the screen edges.",
            "+1 starfish.",
            "Skips 1s longer.",
            "+35% damage.",
            "+1 starfish.",
            "Faster, bigger stars.",
            "+1 starfish and +40% damage.",
        ],
        flavor: "Has never once looked where it was going.",
        evolved_summary: "Every bounce sheds a shower of stardust.",
    },
];

pub const PASSIVE_INFO: [PassiveInfo; PASSIVES] = [
    PassiveInfo {
        name: "Moon Pearl",
        color: [0.85, 0.9, 1.0],
        summary: "Weapons recharge 8% faster.",
        flavor: "Found inside a very confused oyster.",
    },
    PassiveInfo {
        name: "Tidal Spiral",
        color: [0.5, 0.9, 0.95],
        summary: "Attacks are 10% bigger.",
        flavor: "Everything is bigger in the deep.",
    },
    PassiveInfo {
        name: "Pressure Valve",
        color: [0.65, 0.7, 1.0],
        summary: "+10% damage.",
        flavor: "Do not open. Seriously.",
    },
    PassiveInfo {
        name: "Stinging Cells",
        color: [0.7, 1.0, 0.6],
        summary: "+6% critical chance, +15% crit damage.",
        flavor: "Spicy on purpose.",
    },
    PassiveInfo {
        name: "Echo Shell",
        color: [0.75, 0.75, 1.0],
        summary: "Odd levels: +1 projectile for every weapon.",
        flavor: "Says everything twice. Says everything twice.",
    },
    PassiveInfo {
        name: "Kelp Tea",
        color: [0.6, 0.95, 0.5],
        summary: "Recover 0.3 HP per second, +8 max HP.",
        flavor: "Steeped for exactly one tide.",
    },
    PassiveInfo {
        name: "Swift Fins",
        color: [0.55, 1.0, 0.9],
        summary: "+8% speed, dash recharges 8% faster.",
        flavor: "Zoom zoom, but wetter.",
    },
    PassiveInfo {
        name: "Lucky Barnacle",
        color: [1.0, 0.9, 0.5],
        summary: "+10% luck: crits, Glimmer and bonus cards.",
        flavor: "Stuck to you. Emotionally, too.",
    },
    PassiveInfo {
        name: "Magnet Anemone",
        color: [1.0, 0.6, 0.85],
        summary: "Pull motes from 30% farther away.",
        flavor: "Very clingy. Great listener.",
    },
    PassiveInfo {
        name: "Thick Shell",
        color: [0.95, 0.75, 0.55],
        summary: "+1 armor and +15 max HP.",
        flavor: "Rated for crushing depths and crushing defeats.",
    },
];

/// Foe archetypes. Bosses share the enemy table for spatial queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FoeKind {
    Driftlet,
    Mothling,
    Grumpfish,
    Sneel,
    Urchin,
    Crabbo,
    MotherBrine,
    Gulpjaw,
}

pub const FOE_KINDS: usize = 8;

impl FoeKind {
    pub fn info(self) -> &'static FoeInfo {
        &FOE_INFO[self as usize]
    }
    pub fn is_boss(self) -> bool {
        matches!(self, FoeKind::MotherBrine | FoeKind::Gulpjaw)
    }
}

pub struct FoeInfo {
    pub name: &'static str,
    pub hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub radius: f32,
    pub xp: u32,
    /// 0 = full knockback, 1 = immovable.
    pub sturdiness: f32,
    pub color: Rgb,
}

pub const FOE_INFO: [FoeInfo; FOE_KINDS] = [
    FoeInfo {
        name: "Driftlet",
        hp: 9.,
        speed: 60.,
        damage: 6.,
        radius: 10.,
        xp: 1,
        sturdiness: 0.,
        color: [0.45, 0.85, 1.0],
    },
    FoeInfo {
        name: "Mothling",
        hp: 6.,
        speed: 92.,
        damage: 5.,
        radius: 8.,
        xp: 1,
        sturdiness: 0.,
        color: [1.0, 0.75, 0.4],
    },
    FoeInfo {
        name: "Grumpfish",
        hp: 34.,
        speed: 44.,
        damage: 10.,
        radius: 14.,
        xp: 3,
        sturdiness: 0.3,
        color: [1.0, 0.62, 0.35],
    },
    FoeInfo {
        name: "Sneel",
        hp: 24.,
        speed: 66.,
        damage: 12.,
        radius: 10.,
        xp: 3,
        sturdiness: 0.2,
        color: [0.55, 1.0, 0.6],
    },
    FoeInfo {
        name: "Grumbling Urchin",
        hp: 42.,
        speed: 38.,
        damage: 8.,
        radius: 13.,
        xp: 4,
        sturdiness: 0.5,
        color: [0.85, 0.5, 1.0],
    },
    FoeInfo {
        name: "Sir Pinchington",
        hp: 130.,
        speed: 30.,
        damage: 16.,
        radius: 19.,
        xp: 8,
        sturdiness: 0.8,
        color: [1.0, 0.45, 0.45],
    },
    FoeInfo {
        name: "Mother Brine",
        hp: 2600.,
        speed: 46.,
        damage: 22.,
        radius: 52.,
        xp: 0,
        sturdiness: 1.,
        color: [1.0, 0.5, 0.85],
    },
    FoeInfo {
        name: "Old Gulpjaw",
        hp: 9000.,
        speed: 58.,
        damage: 30.,
        radius: 66.,
        xp: 0,
        sturdiness: 1.,
        color: [0.5, 0.95, 1.0],
    },
];

/// Elite titles; the foe's own name follows.
pub const ELITE_NAMES: [&str; 12] = [
    "Gerald, the Unreasonably Large",
    "Big Barbara",
    "Kevin the Unkind",
    "Sir Reginald Bubbles",
    "Auntie Ink",
    "The Notorious S.N.A.C.K.",
    "Lord Moistington",
    "Brenda Who Bites",
    "Captain Sulk",
    "Professor Gloom",
    "Dave",
    "The Last Crouton",
];

pub struct Character {
    pub name: &'static str,
    pub title: &'static str,
    pub weapon: WeaponId,
    pub max_hp: f32,
    pub speed: f32,
    pub might: f32,
    pub area: f32,
    pub cost: u64,
    pub color: Rgb,
    pub quirk: &'static str,
    pub flavor: &'static str,
}

pub const CHARACTERS: [Character; 4] = [
    Character {
        name: "Lumi",
        title: "the Lantern",
        weapon: WeaponId::WispBolt,
        max_hp: 100.,
        speed: 1.,
        might: 1.,
        area: 1.,
        cost: 0,
        color: [1.0, 0.85, 0.5],
        quirk: "Balanced. Brave. Very small.",
        flavor: "A candle flame who wandered off the cake.",
    },
    Character {
        name: "Pip",
        title: "the Pufferling",
        weapon: WeaponId::BubbleMortar,
        max_hp: 120.,
        speed: 0.94,
        might: 1.1,
        area: 1.,
        cost: 120,
        color: [0.65, 0.8, 1.0],
        quirk: "+20 HP, +10% damage, a bit slow.",
        flavor: "Inflates when nervous. Always nervous.",
    },
    Character {
        name: "Mira",
        title: "the Moon Jelly",
        weapon: WeaponId::HaloOrbit,
        max_hp: 90.,
        speed: 1.06,
        might: 1.,
        area: 1.15,
        cost: 260,
        color: [1.0, 0.65, 0.95],
        quirk: "+15% area and quick, -10 HP.",
        flavor: "Ninety-five percent water, five percent attitude.",
    },
    Character {
        name: "Bramble",
        title: "the Hermit Crab",
        weapon: WeaponId::ShellBoomerang,
        max_hp: 150.,
        speed: 0.9,
        might: 1.,
        area: 1.,
        cost: 420,
        color: [1.0, 0.55, 0.45],
        quirk: "+50 HP and +1 armor. Slow.",
        flavor: "Has moved house 41 times. Still unpacking.",
    },
];

/// Tide phases (biome/palette shifts) in logical minutes of a full tide.
pub struct Tide {
    pub name: &'static str,
    pub start: f32,
    /// Deep background, floor accent, light tint.
    pub deep: Rgb,
    pub floor: Rgb,
    pub light: Rgb,
}

pub const TIDES: [Tide; 5] = [
    Tide {
        name: "Dusk Shallows",
        start: 0.,
        deep: [0.02, 0.07, 0.11],
        floor: [0.05, 0.22, 0.27],
        light: [0.45, 0.95, 1.0],
    },
    Tide {
        name: "Moonlit Kelp",
        start: 2.,
        deep: [0.03, 0.05, 0.13],
        floor: [0.12, 0.14, 0.34],
        light: [0.62, 0.68, 1.0],
    },
    Tide {
        name: "Midnight Bloom",
        start: 4.,
        deep: [0.07, 0.03, 0.11],
        floor: [0.28, 0.08, 0.3],
        light: [1.0, 0.55, 0.9],
    },
    Tide {
        name: "The Abyssal Drift",
        start: 6.,
        deep: [0.015, 0.02, 0.05],
        floor: [0.06, 0.08, 0.2],
        light: [0.5, 0.55, 1.0],
    },
    Tide {
        name: "Dawnbreak Current",
        start: 8.,
        deep: [0.06, 0.05, 0.08],
        floor: [0.32, 0.2, 0.15],
        light: [1.0, 0.8, 0.5],
    },
];

/// Permanent upgrades bought with Glimmer at the Shrine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ShrineId {
    Might,
    Vitality,
    Swiftness,
    Reach,
    Reroll,
    Banish,
    Greed,
    SecondWind,
}

pub const SHRINE_ITEMS: usize = 8;

pub struct ShrineItem {
    pub name: &'static str,
    pub summary: &'static str,
    pub max: u8,
    pub base_cost: u64,
}

pub const SHRINE: [ShrineItem; SHRINE_ITEMS] = [
    ShrineItem {
        name: "Brine Might",
        summary: "+5% damage per rank.",
        max: 5,
        base_cost: 40,
    },
    ShrineItem {
        name: "Sturdy Glow",
        summary: "+10 max HP per rank.",
        max: 5,
        base_cost: 35,
    },
    ShrineItem {
        name: "Current Rider",
        summary: "+4% move speed per rank.",
        max: 3,
        base_cost: 50,
    },
    ShrineItem {
        name: "Long Tentacles",
        summary: "+15% pickup reach per rank.",
        max: 3,
        base_cost: 30,
    },
    ShrineItem {
        name: "Second Opinion",
        summary: "+1 reroll per run.",
        max: 3,
        base_cost: 60,
    },
    ShrineItem {
        name: "Not Today",
        summary: "+1 banish per run.",
        max: 2,
        base_cost: 70,
    },
    ShrineItem {
        name: "Shiny Habits",
        summary: "+10% Glimmer per rank.",
        max: 5,
        base_cost: 45,
    },
    ShrineItem {
        name: "Second Wind",
        summary: "Revive once per run at half HP.",
        max: 1,
        base_cost: 300,
    },
];

impl ShrineItem {
    /// Cost of buying the next rank after `rank` ranks.
    pub fn cost(&self, rank: u8) -> u64 {
        self.base_cost + self.base_cost * u64::from(rank) * 3 / 2
    }
}

/// Run summaries award one title; the first matching rule wins.
pub const TITLES: [(&str, &str); 12] = [
    (
        "Dawnbringer",
        "Outlasted Old Gulpjaw. The sea owes you one.",
    ),
    (
        "Leviathan Tickler",
        "Got Gulpjaw to half health. It giggled.",
    ),
    (
        "Professional Bubble Enthusiast",
        "Most damage from bubbles.",
    ),
    ("Certified Halo Haver", "Your orbit did the talking."),
    ("Unlicensed Electrician", "Chain Spark carried the run."),
    ("Dash Addict", "Dashed more than 60 times. Cardio!"),
    ("Mote Hoover", "Vacuumed up a truly silly amount of glow."),
    ("Untouchable Glimmer", "Took barely a scratch."),
    ("Snack-Sized Hero", "Snuffed out before the second tide."),
    ("Matriarch Melter", "Dissolved Mother Brine into sea foam."),
    ("Tide Tourist", "Saw the sights. Some of them bit back."),
    ("Small Light, Big Night", "Shone bravely in the deep."),
];
