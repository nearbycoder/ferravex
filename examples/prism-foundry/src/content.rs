//! Authored chambers. Solutions are reference dial settings used only by the legal pilot/tests.
use crate::model::{Kind, Piece};
use Kind::{Emitter, Filter, Mirror, Receiver, Splitter, Wall};

pub(crate) const RED: u8 = 1;
pub(crate) const GREEN: u8 = 2;
pub(crate) const BLUE: u8 = 4;
pub(crate) const WHITE: u8 = 7;
const fn p(x: u8, y: u8, kind: Kind, start: u8, solution: u8) -> Piece {
    Piece {
        x,
        y,
        kind,
        start,
        solution,
    }
}
const fn source(x: u8, y: u8, direction: u8, color: u8) -> Piece {
    p(x, y, Emitter { direction, color }, 0, 0)
}
const fn goal(x: u8, y: u8, color: u8) -> Piece {
    p(x, y, Receiver(color), 0, 0)
}
const fn m(x: u8, y: u8, solution: u8) -> Piece {
    p(x, y, Mirror, 1 - solution, solution)
}
const fn s(x: u8, y: u8, solution: u8) -> Piece {
    p(x, y, Splitter, 1 - solution, solution)
}
const fn f(x: u8, y: u8, start: u8, solution: u8) -> Piece {
    p(x, y, Filter, start, solution)
}
const fn wall(x: u8, y: u8) -> Piece {
    p(x, y, Wall, 0, 0)
}

pub(crate) struct Chamber {
    pub name: &'static str,
    pub lesson: &'static str,
    pub hint: &'static str,
    pub pieces: &'static [Piece],
}
pub(crate) const CHAMBERS: [Chamber; 12] = [
    Chamber {
        name: "First Reflection",
        lesson: "Turn the two mirrors. Guide red light into the red lock.",
        hint: "The lower mirror must send the beam north. The upper mirror turns it east. Both should look like /.",
        pieces: &[
            source(0, 5, 0, RED),
            m(4, 5, 0),
            m(4, 1, 0),
            goal(9, 1, RED),
            wall(7, 5),
            wall(8, 5),
        ],
    },
    Chamber {
        name: "Folded Light",
        lesson: "A beam can cross its own path. Empty crossings do not turn light.",
        hint: "Fold down at D2, left at D6, up at B6, then right at B4. Follow one bend at a time.",
        pieces: &[
            source(0, 1, 0, GREEN),
            m(3, 1, 1),
            m(3, 5, 0),
            m(1, 5, 1),
            m(1, 3, 0),
            goal(9, 3, GREEN),
            wall(8, 1),
            wall(6, 5),
        ],
    },
    Chamber {
        name: "The Long Way",
        lesson: "Stone absorbs light. Route around the central barricade.",
        hint: "The useful route goes up at C6, across row 2, then down column I. The center mirror is a decoy.",
        pieces: &[
            source(0, 5, 0, BLUE),
            m(2, 5, 0),
            m(2, 1, 0),
            m(8, 1, 1),
            m(8, 5, 1),
            p(5, 3, Mirror, 0, 0),
            goal(10, 5, BLUE),
            wall(5, 5),
            wall(5, 4),
            wall(5, 2),
        ],
    },
    Chamber {
        name: "Twin Loom",
        lesson: "A splitter keeps a straight beam and adds a reflected branch.",
        hint: "Send one branch north at D6 and the other north at I6. Each upper mirror then turns right.",
        pieces: &[
            source(0, 5, 0, RED),
            s(3, 5, 0),
            m(8, 5, 0),
            m(3, 1, 0),
            m(8, 3, 0),
            goal(6, 1, RED),
            goal(10, 3, RED),
            wall(5, 3),
            wall(6, 3),
        ],
    },
    Chamber {
        name: "Cross Stitch",
        lesson: "Chain splitters to power three locks at the same time.",
        hint: "The first branch turns south, the second north. The lower-left mirror sends light west; the upper one sends it east.",
        pieces: &[
            source(0, 3, 0, GREEN),
            s(2, 3, 1),
            s(7, 3, 0),
            m(2, 5, 0),
            m(7, 1, 0),
            goal(0, 5, GREEN),
            goal(10, 3, GREEN),
            goal(10, 1, GREEN),
            wall(7, 5),
            wall(5, 1),
        ],
    },
    Chamber {
        name: "Choose A Color",
        lesson: "White contains R, G and B. A filter passes only its selected color.",
        hint: "Set the upper filter to B and send that beam north then east. Set the lower filter to R. Filters cycle R, G, B.",
        pieces: &[
            source(0, 3, 0, WHITE),
            f(2, 3, 0, 2),
            m(5, 3, 0),
            m(5, 1, 0),
            goal(10, 1, BLUE),
            source(0, 5, 0, WHITE),
            f(4, 5, 2, 0),
            goal(10, 5, RED),
            wall(8, 3),
        ],
    },
    Chamber {
        name: "Chromatic Fork",
        lesson: "Split white first, then filter each branch independently.",
        hint: "Split upward at E4. The upper branch needs a blue filter and a / mirror. The straight branch needs red.",
        pieces: &[
            source(0, 3, 0, WHITE),
            s(4, 3, 0),
            f(7, 3, 1, 0),
            goal(10, 3, RED),
            f(4, 2, 0, 2),
            m(4, 0, 0),
            goal(9, 0, BLUE),
            wall(4, 5),
            wall(8, 5),
        ],
    },
    Chamber {
        name: "Golden Junction",
        lesson: "At a lock, colors add: red plus green makes yellow (Y).",
        hint: "Fold red down, right, then down again. Filter the beam arriving from the right to green. Both must reach the same Y lock.",
        pieces: &[
            source(0, 1, 0, RED),
            m(2, 1, 1),
            m(2, 3, 1),
            m(5, 3, 1),
            goal(5, 5, 3),
            source(10, 5, 2, WHITE),
            f(8, 5, 0, 1),
            wall(5, 1),
            wall(2, 5),
        ],
    },
    Chamber {
        name: "Violet Crossing",
        lesson: "Red plus blue makes magenta (M). Extra green spoils the match.",
        hint: "Split white south at D2. Filter the lower-left branch red, the upper-right branch blue. Bring both toward F6.",
        pieces: &[
            source(0, 1, 0, WHITE),
            s(3, 1, 1),
            f(6, 1, 0, 2),
            m(9, 1, 1),
            m(9, 5, 0),
            goal(5, 5, 5),
            f(3, 3, 1, 0),
            m(3, 5, 1),
            wall(7, 3),
            wall(1, 5),
        ],
    },
    Chamber {
        name: "Three-Color Press",
        lesson: "Recombine all three primary colors to make a white lock.",
        hint: "Split north at D4 and south at H4. Route red across the top, blue across the bottom, and green straight into K4.",
        pieces: &[
            source(0, 3, 0, WHITE),
            s(3, 3, 0),
            s(7, 3, 1),
            f(9, 3, 2, 1),
            goal(10, 3, WHITE),
            f(3, 1, 1, 0),
            m(3, 0, 0),
            m(10, 0, 1),
            f(7, 5, 0, 2),
            m(7, 6, 1),
            m(10, 6, 0),
            wall(5, 1),
            wall(5, 5),
        ],
    },
    Chamber {
        name: "Signal And Noise",
        lesson: "Cross two circuits without contaminating either exact-color lock.",
        hint: "The upper source splits into red and blue for the M lock. A separate lower source filters green and bends up then right to G.",
        pieces: &[
            source(0, 1, 0, WHITE),
            s(4, 1, 1),
            f(4, 3, 1, 0),
            m(4, 5, 1),
            goal(8, 5, 5),
            f(6, 1, 1, 2),
            m(8, 1, 1),
            source(0, 6, 0, WHITE),
            f(2, 6, 2, 1),
            m(6, 6, 0),
            m(6, 4, 0),
            goal(10, 4, GREEN),
            wall(1, 3),
            wall(10, 1),
        ],
    },
    Chamber {
        name: "The Daybreak Engine",
        lesson: "Restore the foundry: power white, white and red simultaneously.",
        hint: "The top source supplies red from above and blue from below to J4. The middle source supplies green. Split the bottom source north for the R lock.",
        pieces: &[
            source(0, 1, 0, WHITE),
            s(3, 1, 1),
            f(5, 1, 1, 0),
            m(9, 1, 1),
            f(3, 2, 1, 2),
            m(3, 5, 1),
            m(9, 5, 0),
            goal(9, 3, WHITE),
            source(0, 3, 0, WHITE),
            f(2, 3, 0, 1),
            source(0, 6, 0, WHITE),
            s(5, 6, 0),
            goal(10, 6, WHITE),
            f(5, 4, 2, 0),
            goal(5, 2, RED),
            wall(0, 0),
            wall(10, 0),
            wall(10, 2),
        ],
    },
];
