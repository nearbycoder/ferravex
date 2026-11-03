//! Six authored, finite investigations. Scene records are independently verified facts.

pub(crate) struct Place {
    pub name: &'static str,
    pub title: &'static str,
    pub evidence: &'static str,
}
pub(crate) struct Suspect {
    pub name: &'static str,
    pub role: &'static str,
    pub statement: &'static str,
}
pub(crate) struct Case {
    pub title: &'static str,
    pub district: &'static str,
    pub opening: &'static str,
    pub places: [Place; 3],
    pub suspects: [Suspect; 3],
    pub methods: [&'static str; 3],
    pub motives: [&'static str; 3],
    pub culprit: usize,
    pub method: usize,
    pub motive: usize,
    pub confession: &'static str,
}

pub(crate) const CASES: [Case; 6] = [
    Case {
        title: "The Last Carbon",
        district: "OLD PRESS / MONDAY",
        opening: "The Gazette's wage ledger vanished from a locked safe at 8:10.",
        places: [
            Place {
                name: "Safe room",
                title: "The fresh key mould",
                evidence: "The safe used a copied key. Fresh wax carries Vera's fingerprint. No forced latch.",
            },
            Place {
                name: "Print floor",
                title: "Ada's continuous shift",
                evidence: "Ada was filmed repairing rollers from 8:00-8:25, covering the 8:10 theft.",
            },
            Place {
                name: "Loading gate",
                title: "Leon outside the gate",
                evidence: "Leon was filmed unloading outside from 8:02-8:20. Payroll was due for audit.",
            },
        ],
        suspects: [
            Suspect {
                name: "Ada Flint",
                role: "Compositor",
                statement: "I was repairing rollers. The operators can confirm it.",
            },
            Suspect {
                name: "Leon Reed",
                role: "Night courier",
                statement: "I was unloading outside until 8:20.",
            },
            Suspect {
                name: "Vera Quill",
                role: "Bookkeeper",
                statement: "I never touched the key wax. There was no copied key.",
            },
        ],
        methods: ["Forced latch", "Copied key", "Loose skylight"],
        motives: ["Resell old books", "Settle a debt", "Hide payroll fraud"],
        culprit: 2,
        method: 1,
        motive: 2,
        confession: "Vera copied the key to hide payroll fraud.",
    },
    Case {
        title: "The Silent Bell",
        district: "BELL MUSEUM / TUESDAY",
        opening: "A silver handbell disappeared during a seven-minute power cut.",
        places: [
            Place {
                name: "Display plinth",
                title: "The hatch print",
                evidence: "Orrin's fresh print is inside the service hatch, over the pre-blackout seal. The bell fits through it.",
            },
            Place {
                name: "Music balcony",
                title: "Leda's recorded recital",
                evidence: "Leda was filmed at the piano for the full seven-minute blackout.",
            },
            Place {
                name: "Security booth",
                title: "Hugo on the monitor",
                evidence: "Hugo stayed in the sealed booth throughout. Orrin has an unpaid bill for his sister's care.",
            },
        ],
        suspects: [
            Suspect {
                name: "Orrin Pike",
                role: "Repairer",
                statement: "I never touched the hatch after sealing. The glass must have broken.",
            },
            Suspect {
                name: "Leda Vale",
                role: "Pianist",
                statement: "I played throughout the blackout. The audience filmed it.",
            },
            Suspect {
                name: "Hugo Bell",
                role: "Night guard",
                statement: "I stayed in the booth. Orrin needed money for his sister's care.",
            },
        ],
        methods: [
            "Lift service hatch",
            "Break display glass",
            "Swap sealed crate",
        ],
        motives: [
            "Destroy the museum",
            "Pay sister's care",
            "Win a music prize",
        ],
        culprit: 0,
        method: 0,
        motive: 1,
        confession: "Orrin used the service hatch to pay his sister's care bill.",
    },
    Case {
        title: "The Blue Lantern",
        district: "NORTH LIGHT / WEDNESDAY",
        opening: "North Light went dark just as an unregistered barge crossed the shoals.",
        places: [
            Place {
                name: "Lantern room",
                title: "Salt in the governor",
                evidence: "Salt jammed the regulator. Mira's sealed flask was full minutes before; now empty, with matching salt. Lens intact.",
            },
            Place {
                name: "Keeper's stair",
                title: "Keeper under the beam",
                evidence: "Sera was filmed trapped under a beam from 10:55-11:15.",
            },
            Place {
                name: "Cliff path",
                title: "Tomas on the headland",
                evidence: "Tomas was photographed on the far headland at 11:04. Mira's barge carried undeclared cargo.",
            },
        ],
        suspects: [
            Suspect {
                name: "Sera Dune",
                role: "Lighthouse keeper",
                statement: "I was trapped under the beam.",
            },
            Suspect {
                name: "Mira Sable",
                role: "Barge owner",
                statement: "My flask stayed closed. A cracked lens caused the blackout.",
            },
            Suspect {
                name: "Tomas Wren",
                role: "Surveyor",
                statement: "I was photographing the far headland. Mira asked about hidden routes.",
            },
        ],
        methods: ["Cut power cable", "Crack the lens", "Salt in regulator"],
        motives: ["Hide a smuggling run", "Win a repair grant", "Stop a storm"],
        culprit: 1,
        method: 2,
        motive: 0,
        confession: "Mira salted the regulator to conceal smuggling.",
    },
    Case {
        title: "The Borrowed Voice",
        district: "CIVIC RADIO / THURSDAY",
        opening: "A forged broadcast sent relief trucks to the wrong depot.",
        places: [
            Place {
                name: "Playback desk",
                title: "The spliced cylinder",
                evidence: "The broadcast spliced two archived recordings. Both original cylinders are under the playback desk.",
            },
            Place {
                name: "Transmission roof",
                title: "Ivo on the mast",
                evidence: "Ivo stayed clipped to the mast throughout the broadcast. The roof hatch stayed locked.",
            },
            Place {
                name: "Live music room",
                title: "Cleo's public rehearsal",
                evidence: "Cleo was filmed conducting throughout. Truck manifests show diverted relief supplies.",
            },
        ],
        suspects: [
            Suspect {
                name: "Ivo Rusk",
                role: "Antenna mechanic",
                statement: "I stayed clipped to the mast. I heard the recording start.",
            },
            Suspect {
                name: "Cleo March",
                role: "Conductor",
                statement: "I never left the children's performance. The relief trucks were diverted.",
            },
            Suspect {
                name: "Nola Finch",
                role: "Sound archivist",
                statement: "Neither cylinder left the archive. The councillor spoke live.",
            },
        ],
        methods: [
            "Live telephone call",
            "Spliced wax recording",
            "Hidden loudspeaker",
        ],
        motives: [
            "Sell rare recordings",
            "Sabotage a concert",
            "Expose diverted aid",
        ],
        culprit: 2,
        method: 1,
        motive: 2,
        confession: "Nola spliced recordings to expose diverted relief supplies.",
    },
    Case {
        title: "The Winter Orchard",
        district: "SEED CONSERVATORY / FRIDAY",
        opening: "A rare winter pear was swapped before a land valuation.",
        places: [
            Place {
                name: "Sample bench",
                title: "A broken packet seal",
                evidence: "The lab confirms swapped seed packets: broken seals and reversed contents. No grafting, dye or disease.",
            },
            Place {
                name: "Glass nursery",
                title: "Ruth's sealed ward",
                evidence: "Ruth remained in sealed quarantine with two inspectors during the swap.",
            },
            Place {
                name: "Auction foyer",
                title: "Felix at the auction",
                evidence: "Felix was filmed in the foyer throughout. Confirming the rare pear would condemn Elian's family orchard.",
            },
        ],
        suspects: [
            Suspect {
                name: "Elian Moss",
                role: "Orchard gardener",
                statement: "I never swapped packets. The pear must have been grafted.",
            },
            Suspect {
                name: "Ruth Alder",
                role: "Botanist",
                statement: "I stayed in quarantine with the inspectors.",
            },
            Suspect {
                name: "Felix Hart",
                role: "Auction clerk",
                statement: "I stayed at the auction desk. Elian wanted to save his orchard.",
            },
        ],
        methods: ["Grafted branch", "Painted fruit", "Swapped seed packets"],
        motives: [
            "Save condemned orchard",
            "Win a botany medal",
            "Raise auction fees",
        ],
        culprit: 0,
        method: 2,
        motive: 0,
        confession: "Elian swapped packets to save his family orchard.",
    },
    Case {
        title: "The Glass Mayor",
        district: "COUNCIL ARCHIVE / SATURDAY",
        opening: "Your evidence case vanished before the public hearing.",
        places: [
            Place {
                name: "Dispatch table",
                title: "The swapped dispatch",
                evidence: "The camera shows Bram swapping the tagged evidence case for an empty one before sealing. Vault undamaged.",
            },
            Place {
                name: "Public gallery",
                title: "Aster before the crowd",
                evidence: "Aster stayed at the public hearing throughout the theft; continuous footage confirms it.",
            },
            Place {
                name: "Furnace room",
                title: "Pell's intercepted parcel",
                evidence: "Pell received the sealed case with two witnesses. Its records prove Bram's fraudulent land seizures.",
            },
        ],
        suspects: [
            Suspect {
                name: "Aster Vale",
                role: "Mayor",
                statement: "I stayed at the public hearing.",
            },
            Suspect {
                name: "Bram Voss",
                role: "Deputy registrar",
                statement: "I never moved or swapped the cases. Someone forced the vault.",
            },
            Suspect {
                name: "Pell Rowan",
                role: "Fire warden",
                statement: "I received the sealed case with two witnesses. Bram's office owns the grey seal.",
            },
        ],
        methods: [
            "Swapped dispatch case",
            "Cut vault hinges",
            "Tunnel under dais",
        ],
        motives: [
            "Protect the fire warden",
            "Erase land seizures",
            "Avoid an election",
        ],
        culprit: 1,
        method: 0,
        motive: 1,
        confession: "Bram swapped cases to destroy proof of his land seizures.",
    },
];
