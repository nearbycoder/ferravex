//! Six authored field permits and a morphological identification atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Species {
    pub name: &'static str,
    pub clue: &'static str,
    pub fact: &'static str,
    pub value: u32,
}
pub(crate) const SPECIES: [Species; 6] = [
    Species {
        name: "AMMONITE",
        clue: "Coiled chambers. Ribbed outer shell.",
        fact: "A chambered marine shell. Each chamber records a stage of growth.",
        value: 35,
    },
    Species {
        name: "FERN FROND",
        clue: "Paired leaflets. Central stem impression.",
        fact: "A branching plant impression preserved in fine river silt.",
        value: 30,
    },
    Species {
        name: "TRILOBITE",
        clue: "Three body lobes. Segmented armor.",
        fact: "A jointed seafloor animal with a distinctive three-lobed body.",
        value: 45,
    },
    Species {
        name: "PREDATOR TOOTH",
        clue: "Serrated cutting edge. Hollow root.",
        fact: "A hunting animal's tooth, built for gripping and cutting.",
        value: 50,
    },
    Species {
        name: "PTEROSAUR WING",
        clue: "Hollow bone wall. Elongated fourth finger.",
        fact: "A lightweight flying reptile wing, supported by a long finger.",
        value: 55,
    },
    Species {
        name: "DAWN GIANT",
        clue: "Spongy interior. Huge weight-bearing joint.",
        fact: "Part of the Dawn Giant, a long-necked herbivore found across the basin.",
        value: 65,
    },
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Find {
    pub x: usize,
    pub y: usize,
    pub kind: usize,
    pub depth: u8,
    pub fragile: u8,
}
const fn find(x: usize, y: usize, kind: usize, depth: u8, fragile: u8) -> Find {
    Find {
        x,
        y,
        kind,
        depth,
        fragile,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    pub name: &'static str,
    pub piece: &'static str,
    pub story: &'static str,
    pub strata: &'static str,
    pub daylight: u16,
    pub rock: u8,
    pub color: [u8; 3],
    pub finds: [Find; 4],
}
pub(crate) const SITES: [Site; 6] = [
    Site {
        name: "RIVERBEND",
        piece: "SKULL",
        story: "Identify the skull and one other specimen.",
        strata: "Soft silt over river sand",
        daylight: 54,
        rock: 0,
        color: [160, 113, 67],
        finds: [
            find(2, 2, 5, 3, 22),
            find(5, 1, 0, 2, 15),
            find(1, 4, 1, 3, 20),
            find(6, 4, 2, 4, 18),
        ],
    },
    Site {
        name: "AMBER HOLLOW",
        piece: "NECK",
        story: "Find the neck vertebra. Brush fragile plant fossils gently.",
        strata: "Amber shale and clay",
        daylight: 52,
        rock: 1,
        color: [173, 119, 42],
        finds: [
            find(5, 3, 5, 5, 24),
            find(1, 1, 1, 3, 24),
            find(3, 4, 3, 4, 12),
            find(6, 1, 0, 4, 16),
        ],
    },
    Site {
        name: "SALT CLIFF",
        piece: "RIB CAGE",
        story: "Find the rib cage and one other specimen.",
        strata: "Marine limestone bands",
        daylight: 50,
        rock: 2,
        color: [165, 142, 112],
        finds: [
            find(2, 4, 5, 6, 25),
            find(5, 1, 2, 4, 20),
            find(1, 1, 0, 4, 15),
            find(6, 3, 4, 5, 26),
        ],
    },
    Site {
        name: "BASALT STEP",
        piece: "PELVIS",
        story: "Drill volcanic rock; switch tools before reaching bone.",
        strata: "Basalt cap above ash",
        daylight: 48,
        rock: 3,
        color: [91, 93, 99],
        finds: [
            find(6, 2, 5, 8, 22),
            find(2, 1, 3, 5, 12),
            find(1, 4, 2, 6, 20),
            find(4, 4, 1, 5, 25),
        ],
    },
    Site {
        name: "WINDGLASS",
        piece: "LEGS",
        story: "Find the leg bones. Work within the short daylight limit.",
        strata: "Wind-packed sandstone",
        daylight: 46,
        rock: 2,
        color: [176, 140, 91],
        finds: [
            find(4, 4, 5, 8, 26),
            find(1, 1, 4, 5, 30),
            find(6, 1, 0, 6, 15),
            find(2, 3, 3, 6, 16),
        ],
    },
    Site {
        name: "DAWN QUARRY",
        piece: "TAIL",
        story: "Find the tail and complete the skeleton.",
        strata: "Layered granite and marl",
        daylight: 48,
        rock: 3,
        color: [156, 106, 83],
        finds: [
            find(5, 4, 5, 9, 28),
            find(2, 1, 4, 6, 28),
            find(6, 1, 2, 7, 22),
            find(1, 4, 1, 6, 24),
        ],
    },
];
