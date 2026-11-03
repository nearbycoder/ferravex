//! Authored petitions and the consequences that reconnect their stories.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Choice {
    pub label: &'static str,
    pub delta: [i16; 4],
    pub legacy: usize,
    pub flag: u16,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Petition {
    pub title: &'static str,
    pub speaker: usize,
    pub body: &'static str,
    pub choices: [Choice; 3],
}
pub(crate) const LEVEE: u16 = 1;
pub(crate) const CHARTER: u16 = 2;
pub(crate) const SANCTUARY: u16 = 4;
pub(crate) const LEDGER: u16 = 8;
pub(crate) const COURTS: u16 = 16;
pub(crate) const PEACE: u16 = 32;
pub(crate) const SCHOOLS: u16 = 64;
const fn c(label: &'static str, delta: [i16; 4], legacy: usize, flag: u16) -> Choice {
    Choice {
        label,
        delta,
        legacy,
        flag,
    }
}
// Resource order: treasury, commons, guard, chapel. Legacy: charter, iron, trade, faith.
pub(crate) const PETITIONS: [Petition; 30] = [
    Petition {
        title: "The river's warning",
        speaker: 1,
        body: "The levee needs repairs. Choose how to fund them.",
        choices: [
            c(
                "Repair the levee with paid village crews",
                [-14, 12, -3, 2],
                0,
                LEVEE,
            ),
            c(
                "Sell riverside toll rights to the guild",
                [13, -8, 2, -2],
                2,
                0,
            ),
            c("Call a chapel labor tithe", [-4, -7, 2, 13], 3, 0),
        ],
    },
    Petition {
        title: "The salt charter",
        speaker: 0,
        body: "Choose who controls the harbor.",
        choices: [
            c(
                "Grant the charter under public inspection",
                [10, 3, -8, -2],
                2,
                CHARTER,
            ),
            c("Keep the harbor under royal command", [-8, -2, 14, 2], 1, 0),
            c(
                "Give fishing families the harbor stalls",
                [-10, 13, -3, 4],
                0,
                0,
            ),
        ],
    },
    Petition {
        title: "Bells at the north gate",
        speaker: 3,
        body: "Decide whether to admit refugees.",
        choices: [
            c(
                "Open the orchard and fund a refuge",
                [-13, 11, -4, 8],
                3,
                SANCTUARY,
            ),
            c(
                "Admit only workers sponsored by a guild",
                [9, -3, 2, -7],
                2,
                0,
            ),
            c(
                "Close the gate and provision the watch",
                [-5, -10, 15, 1],
                1,
                0,
            ),
        ],
    },
    Petition {
        title: "A book with missing pages",
        speaker: 0,
        body: "Missing tax revenue: publish the accounts or protect donors?",
        choices: [
            c(
                "Publish the ledger and protect witnesses",
                [7, 12, -6, -5],
                0,
                LEDGER,
            ),
            c("Accept a discreet noble repayment", [15, -8, 5, -4], 2, 0),
            c("Let the chapel audit the accounts", [-4, -3, 2, 13], 3, 0),
        ],
    },
    Petition {
        title: "Winter muster",
        speaker: 2,
        body: "Fund the frontier guard or protect the harvest?",
        choices: [
            c(
                "Pay a small professional winter guard",
                [-13, 2, 15, -2],
                1,
                0,
            ),
            c(
                "Train village watches after the harvest",
                [-5, 10, -4, 5],
                0,
                0,
            ),
            c(
                "Hire guild escorts only for trade roads",
                [8, -6, 7, -5],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "The orchard petition",
        speaker: 1,
        body: "Recruit local apprentices for bridge work.",
        choices: [
            c(
                "Fund a village apprenticeship season",
                [-10, 14, -2, 1],
                0,
                0,
            ),
            c(
                "Reserve apprentices for the armories",
                [-8, -3, 14, 3],
                1,
                0,
            ),
            c(
                "Let guilds sell apprenticeship places",
                [12, -8, 2, 1],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "The river takes its due",
        speaker: 1,
        body: "The levee has failed. Allocate flood relief.",
        choices: [
            c(
                "Buy grain and cancel the valley's taxes",
                [-20, 17, -3, 5],
                0,
                0,
            ),
            c(
                "Requisition grain under armed escort",
                [4, -15, 12, -3],
                1,
                0,
            ),
            c("Pledge abbey land for relief credit", [9, 4, -4, -13], 2, 0),
        ],
    },
    Petition {
        title: "A stolen loaf, a broken hand",
        speaker: 3,
        body: "Decide whether to replace unequal courts.",
        choices: [
            c(
                "Create equal courts and pay local judges",
                [-11, 14, -5, 3],
                0,
                COURTS,
            ),
            c(
                "Uphold the law and make the baker pay alms",
                [4, -8, 11, 3],
                1,
                0,
            ),
            c(
                "Transfer mercy cases to chapel tribunals",
                [-6, 2, -3, 13],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "Ink for the villages",
        speaker: 3,
        body: "Choose what village schools teach.",
        choices: [
            c(
                "Open free schools with a mixed curriculum",
                [-13, 10, -2, 5],
                0,
                SCHOOLS,
            ),
            c(
                "Train clerks in merchant counting houses",
                [10, 2, -3, -6],
                2,
                0,
            ),
            c(
                "Fund scripture schools in the abbeys",
                [-9, 4, -3, 14],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "The first crown feast",
        speaker: 0,
        body: "Choose who receives the royal feast.",
        choices: [
            c(
                "Seat village delegates beside the nobles",
                [-9, 13, -4, 4],
                0,
                0,
            ),
            c(
                "Honor the captains with banners and pay",
                [-11, -3, 16, 3],
                1,
                0,
            ),
            c(
                "Auction royal favors to fund the feast",
                [16, -6, -2, -4],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "Letters under a white flag",
        speaker: 2,
        body: "Negotiate a border treaty.",
        choices: [
            c(
                "Sign the treaty and establish joint patrols",
                [-8, 12, -7, 8],
                0,
                PEACE,
            ),
            c(
                "Take the hills while the regent is weak",
                [-12, -7, 19, -1],
                1,
                0,
            ),
            c(
                "Purchase a five-year mining concession",
                [14, -5, 1, -6],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "A fever in the abbey",
        speaker: 3,
        body: "Close a shrine to contain the fever?",
        choices: [
            c(
                "Fund healers and close crowded markets",
                [-15, 13, 3, 3],
                0,
                0,
            ),
            c(
                "Guard the roads and quarantine the abbey",
                [-6, -6, 13, 3],
                1,
                0,
            ),
            c(
                "Sell licensed stalls beside a smaller shrine",
                [12, -7, -2, 5],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "The convoy that never came",
        speaker: 0,
        body: "Settle the captains' cargo dispute.",
        choices: [
            c(
                "Subsidize a public convoy for all ports",
                [-14, 12, 2, -1],
                0,
                0,
            ),
            c(
                "Give the navy first claim on every cargo",
                [3, -9, 14, -2],
                1,
                0,
            ),
            c("Auction a single convoy monopoly", [15, -7, -4, 1], 2, 0),
        ],
    },
    Petition {
        title: "The abbey's empty fields",
        speaker: 3,
        body: "Let tenants buy chapel land?",
        choices: [
            c(
                "Back tenant purchases with a hospice grant",
                [-12, 15, -3, 2],
                0,
                0,
            ),
            c(
                "Preserve the abbey estates and lower tithes",
                [3, -4, -2, 12],
                3,
                0,
            ),
            c(
                "Sell the estates to pay the frontier guard",
                [11, -10, 10, -7],
                1,
                0,
            ),
        ],
    },
    Petition {
        title: "The vanished witness",
        speaker: 0,
        body: "Decide what to do with leaked tax accounts.",
        choices: [
            c(
                "Fund an inquiry and shelter the informant",
                [-12, 13, -4, 4],
                0,
                0,
            ),
            c(
                "Sell the papers back under a royal fine",
                [16, -9, 3, -4],
                2,
                0,
            ),
            c(
                "Let the guard seize the household vault",
                [7, -8, 12, -5],
                1,
                0,
            ),
        ],
    },
    Petition {
        title: "The armorer's new furnace",
        speaker: 2,
        body: "Use the new furnace for tools or armor?",
        choices: [
            c(
                "Order ploughs and move the furnace upriver",
                [-13, 15, 1, -1],
                0,
                0,
            ),
            c(
                "Commission armor for a professional guard",
                [-11, -4, 17, 2],
                1,
                0,
            ),
            c(
                "Sell the furnace license to exporters",
                [15, -6, 1, -4],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "The apprentice's petition",
        speaker: 1,
        body: "An apprentice has found duplicate taxes.",
        choices: [
            c(
                "Hear her evidence and refund the villages",
                [-13, 16, -4, 3],
                0,
                0,
            ),
            c(
                "Hire her to improve the royal tax office",
                [13, -5, 4, -4],
                2,
                0,
            ),
            c(
                "Send her to the abbey as a royal scholar",
                [-6, 3, -2, 12],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "The winter bread line",
        speaker: 1,
        body: "Allocate scarce grain after a poor harvest.",
        choices: [
            c(
                "Buy emergency flour and cap bread prices",
                [-16, 17, -3, 4],
                0,
                0,
            ),
            c(
                "Guard the carts and allow market prices",
                [10, -12, 12, -3],
                1,
                0,
            ),
            c(
                "Open chapel stores for a season of tithes",
                [3, 8, -3, -6],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "A captain before the bench",
        speaker: 2,
        body: "A captain seized a village cart. Decide the response.",
        choices: [
            c(
                "Pay restitution and dismiss the captain",
                [-10, 13, -9, 6],
                0,
                0,
            ),
            c(
                "Affirm the seizure and reward his service",
                [7, -13, 16, -4],
                1,
                0,
            ),
            c(
                "Let Sorel arbitrate a private settlement",
                [-6, 3, -3, 12],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "A chair for the succession",
        speaker: 0,
        body: "Choose the heir's education.",
        choices: [
            c(
                "Send the heir on a circuit of open councils",
                [-9, 14, -4, 3],
                0,
                0,
            ),
            c(
                "Send the heir to command the frontier",
                [-10, -3, 16, 2],
                1,
                0,
            ),
            c(
                "Have the heir study law in the abbey",
                [-7, 3, -3, 13],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "A courier with no seal",
        speaker: 2,
        body: "Border fires require a response.",
        choices: [
            c(
                "Fund a border truce with neutral witnesses",
                [-17, 10, -3, 10],
                0,
                PEACE,
            ),
            c(
                "Fortify the hills and call the banners",
                [-14, -8, 21, -2],
                1,
                0,
            ),
            c(
                "Pay merchants to keep both armies supplied",
                [15, -8, 4, -7],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "The silver that cracks",
        speaker: 0,
        body: "Replace the kingdom's devalued coins.",
        choices: [
            c(
                "Replace clipped coins at the crown's cost",
                [-15, 13, 4, 3],
                0,
                0,
            ),
            c(
                "Levy a minting fee on every exchange",
                [16, -9, 2, -3],
                2,
                0,
            ),
            c("Back the currency with chapel plate", [8, 4, -3, -8], 3, 0),
        ],
    },
    Petition {
        title: "A home for old soldiers",
        speaker: 2,
        body: "Choose support for retired soldiers.",
        choices: [
            c(
                "Build veterans' cottages on crown land",
                [-14, 10, 10, -2],
                1,
                0,
            ),
            c(
                "Place the veterans in chapel hospices",
                [-7, 3, -3, 13],
                3,
                0,
            ),
            c(
                "Issue tradable pensions funded by guilds",
                [10, -3, 5, -7],
                2,
                0,
            ),
        ],
    },
    Petition {
        title: "The uncensored chronicle",
        speaker: 3,
        body: "Allow public access to the council archive?",
        choices: [
            c(
                "Open the archive and publish every decree",
                [-9, 13, -5, 6],
                0,
                0,
            ),
            c(
                "Commission an official book of victories",
                [4, -7, 13, 1],
                1,
                0,
            ),
            c(
                "Let abbey scholars preserve sealed copies",
                [-5, 1, -2, 13],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "The returning orchard child",
        speaker: 1,
        body: "Choose terms for a new bridge crew.",
        choices: [
            c(
                "Give all bridge workers equal town rights",
                [-12, 14, -4, 6],
                0,
                SANCTUARY,
            ),
            c(
                "Offer a temporary contract under guard",
                [9, -5, 9, -5],
                2,
                0,
            ),
            c("Use the royal engineers instead", [-9, -4, 15, 3], 1, 0),
        ],
    },
    Petition {
        title: "A market without masters",
        speaker: 0,
        body: "Allow trade outside the guilds?",
        choices: [
            c(
                "Open licenses and protect small merchants",
                [8, 10, -7, -4],
                2,
                0,
            ),
            c(
                "Renew guild privileges for a royal loan",
                [17, -10, 2, -5],
                2,
                0,
            ),
            c(
                "Create town cooperatives with public funds",
                [-12, 15, -4, 4],
                0,
                0,
            ),
        ],
    },
    Petition {
        title: "Bells for two faiths",
        speaker: 3,
        body: "Settle the dispute over a new prayer hall.",
        choices: [
            c(
                "Guarantee both halls under one civic law",
                [-10, 13, -3, 4],
                0,
                0,
            ),
            c(
                "Give Sorel oversight of all public worship",
                [4, -8, 2, 14],
                3,
                0,
            ),
            c(
                "Let the guard license gatherings by permit",
                [7, -9, 14, -6],
                1,
                0,
            ),
        ],
    },
    Petition {
        title: "The last great flood",
        speaker: 1,
        body: "Heavy rain threatens the valley. Allocate defenses.",
        choices: [
            c(
                "Spend the reserve on boats and shelter",
                [-20, 18, -2, 6],
                0,
                0,
            ),
            c(
                "Defend the stores and evacuate by district",
                [-6, -11, 16, 1],
                1,
                0,
            ),
            c("Buy relief on chapel-backed credit", [9, 5, -5, -11], 3, 0),
        ],
    },
    Petition {
        title: "The heir's difficult question",
        speaker: 2,
        body: "Choose limits on the next monarch.",
        choices: [
            c(
                "Bind the crown to a representative council",
                [-11, 15, -8, 7],
                0,
                0,
            ),
            c(
                "Bind the heir to the guard's sworn code",
                [-8, -6, 17, 3],
                1,
                0,
            ),
            c(
                "Bind the succession to sacred obligations",
                [-7, 3, -4, 15],
                3,
                0,
            ),
        ],
    },
    Petition {
        title: "The crown on the empty chair",
        speaker: 0,
        body: "Make the final investment of your reign.",
        choices: [
            c(
                "Endow a permanent citizens' assembly",
                [-12, 16, -5, 5],
                0,
                0,
            ),
            c("Endow a disciplined royal service", [-10, -3, 17, 3], 1, 0),
            c("Endow the hospice and public trust", [-9, 6, -3, 15], 3, 0),
        ],
    },
];

pub(crate) fn petition(turn: usize, flags: u16) -> Petition {
    let mut p = PETITIONS[turn.min(29)];
    match turn {
        5 if flags & SANCTUARY != 0 => {
            p.title = "Anja's first arch";
            p.body = "Refugee masons offer a bridge at half price.";
            p.choices[0] = c(
                "Grant citizenship and commission the arch",
                [-5, 15, -3, 4],
                0,
                SANCTUARY,
            );
        }
        6 if flags & LEVEE != 0 => {
            p.title = "The gates of Crown's Reach";
            p.body = "The repaired levee holds. Choose flood management.";
            p.choices[0] = c(
                "Open the gates across the royal meadow",
                [-5, 15, -4, 5],
                0,
                0,
            );
            p.choices[2] = c(
                "Charge emergency passage on the dry road",
                [16, -8, 2, -4],
                2,
                0,
            );
        }
        12 if flags & CHARTER != 0 => {
            p.title = "The Salt Charter comes home";
            p.body = "The chartered convoy returns with cheap salt.";
            p.choices[0] = c("Release cheap salt into every market", [5, 13, -4, 1], 2, 0);
            p.choices[2] = c(
                "Sell exclusive salt rights to the houses",
                [20, -10, -3, 1],
                2,
                0,
            );
        }
        14 if flags & LEDGER != 0 => {
            p.title = "The Open Ledger's witness";
            p.body = "Protect the clerk who exposed tax theft?";
            p.choices[0] = c(
                "Protect the clerk and recover the stolen tax",
                [9, 14, -8, -3],
                0,
                0,
            );
        }
        16 if flags & SCHOOLS != 0 => {
            p.title = "The Lantern Schools speak";
            p.body = "School pupils have found duplicate taxes.";
            p.choices[0] = c(
                "Accept the audit and make pupils inspectors",
                [4, 15, -6, 2],
                0,
                0,
            );
        }
        18 if flags & COURTS != 0 => {
            p.title = "The Common Bench tries a captain";
            p.body = "The captain faces the common court.";
            p.choices[0] = c(
                "Uphold the verdict and make restitution",
                [-4, 16, -8, 7],
                0,
                0,
            );
        }
        20 if flags & PEACE != 0 => {
            p.title = "The White Flag's second winter";
            p.body = "The peace treaty allows joint fire crews.";
            p.choices[0] = c(
                "Send joint rescue crews across the border",
                [-6, 13, 3, 5],
                0,
                PEACE,
            );
        }
        24 if flags & SANCTUARY != 0 => {
            p.title = "The Orchard Oath repaid";
            p.body = "Refugee masons offer a free public bridge.";
            p.choices[0] = c(
                "Accept the public bridge and keep it toll-free",
                [8, 14, -5, 3],
                0,
                0,
            );
        }
        27 if flags & LEVEE != 0 => {
            p.title = "The levee's final test";
            p.body = "Upgrade the existing levee before heavy rain.";
            p.choices[0] = c(
                "Renew the sluices and reward the crews",
                [-8, 17, -3, 4],
                0,
                0,
            );
        }
        _ => {}
    }
    p
}
