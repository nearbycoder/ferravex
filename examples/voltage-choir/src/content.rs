//! Original authored charts and melodies. A bit is one lane: left/down/up/right.
//! Every row spans one four-beat bar in eighth-note subdivisions.

pub(crate) const TRACK_COUNT: usize = 6;
pub(crate) const MAX_NOTES: usize = 768;
pub(crate) const LANE_NAMES: [&str; 4] = ["Left", "Down", "Up", "Right"];
/// Letter keys that mirror the arrow keys, shown on key caps.
pub(crate) const LANE_KEYS: [&str; 4] = ["A", "S", "W", "D"];

pub(crate) struct Track {
    pub title: &'static str,
    pub venue: &'static str,
    pub story: &'static str,
    pub lesson: &'static str,
    pub beat_ticks: u32,
    pub target: u32,
    pub patterns: &'static [[u8; 8]],
    pub arrangement: &'static [usize],
    pub melody: [i8; 16],
    pub bass: [u8; 4],
}
impl Track {
    pub fn bars(&self) -> u32 {
        self.arrangement.len() as u32
    }
    pub fn length(&self) -> u32 {
        self.bars() * 4 * self.beat_ticks
    }
    pub fn bpm(&self) -> u32 {
        3600 / self.beat_ticks
    }
    pub fn mask(&self, subdivision: u32) -> u8 {
        let bar = subdivision as usize / 8;
        self.arrangement.get(bar).map_or(0, |&pattern| {
            self.patterns[pattern][subdivision as usize % 8]
        })
    }
    pub fn music(&self, subdivision: u32) -> [Option<u8>; 4] {
        if subdivision >= self.bars() * 8 {
            return [None; 4];
        }
        let lead = self.melody[subdivision as usize % 16];
        let root = self.bass[(subdivision as usize / 16) % 4];
        [
            (lead >= 0).then_some(lead.max(0) as u8),
            subdivision.is_multiple_of(4).then_some(root),
            subdivision.is_multiple_of(8).then_some(root + 19),
            (subdivision % 8 == 4).then_some(root + 12),
        ]
    }
}

const SPARK: [[u8; 8]; 4] = [
    [1, 0, 2, 0, 4, 0, 8, 0],
    [8, 0, 4, 0, 2, 0, 1, 0],
    [1, 0, 4, 0, 2, 0, 8, 0],
    [1, 0, 0, 0, 8, 0, 0, 0],
];
const COPPER: [[u8; 8]; 6] = [
    [1, 0, 2, 4, 8, 0, 4, 0],
    [8, 0, 4, 2, 1, 0, 2, 0],
    [1, 2, 4, 0, 8, 4, 2, 0],
    [1, 0, 8, 0, 9, 0, 0, 0],
    [2, 0, 4, 0, 6, 0, 8, 0],
    [1, 2, 0, 4, 0, 8, 4, 0],
];
const GLASS: [[u8; 8]; 6] = [
    [0, 1, 0, 4, 0, 2, 8, 0],
    [0, 8, 0, 2, 0, 4, 1, 0],
    [1, 0, 6, 0, 8, 0, 6, 0],
    [0, 2, 4, 0, 0, 8, 1, 0],
    [9, 0, 0, 4, 6, 0, 0, 2],
    [1, 0, 4, 2, 8, 0, 2, 4],
];
const NIGHT: [[u8; 8]; 6] = [
    [1, 2, 4, 8, 4, 2, 1, 0],
    [8, 4, 2, 1, 2, 4, 8, 0],
    [9, 0, 2, 4, 9, 0, 4, 2],
    [1, 4, 2, 8, 1, 4, 2, 8],
    [6, 0, 1, 8, 6, 0, 8, 1],
    [1, 2, 4, 0, 8, 4, 2, 0],
];
const AURORA: [[u8; 8]; 8] = [
    [1, 0, 6, 0, 8, 2, 4, 0],
    [8, 0, 6, 0, 1, 4, 2, 0],
    [9, 0, 2, 4, 1, 8, 6, 0],
    [2, 4, 1, 0, 8, 1, 4, 0],
    [5, 0, 10, 0, 1, 2, 4, 8],
    [3, 0, 12, 0, 8, 4, 2, 1],
    [0, 9, 0, 6, 1, 4, 8, 2],
    [1, 0, 4, 0, 2, 0, 8, 0],
];
const CHOIR: [[u8; 8]; 8] = [
    [1, 2, 4, 8, 9, 0, 6, 0],
    [8, 4, 2, 1, 6, 0, 9, 0],
    [5, 0, 2, 8, 10, 0, 1, 4],
    [3, 0, 4, 8, 12, 0, 2, 1],
    [1, 4, 2, 8, 6, 0, 9, 0],
    [9, 0, 6, 0, 1, 2, 4, 8],
    [0, 1, 0, 8, 6, 0, 9, 0],
    [9, 0, 0, 0, 6, 0, 0, 0],
];

pub(crate) const TRACKS: [Track; TRACK_COUNT] = [
    Track {
        title: "First Spark",
        venue: "Rooftop Workshop",
        beat_ticks: 36,
        target: 600,
        story: "Single notes.",
        lesson: "Tap as notes reach the bright line.",
        patterns: &SPARK,
        arrangement: &[0, 1, 0, 2, 0, 1, 2, 3, 0, 2, 1, 3],
        melody: [
            60, -1, 64, -1, 67, -1, 64, -1, 62, -1, 65, -1, 69, 67, 65, -1,
        ],
        bass: [48, 53, 55, 48],
    },
    Track {
        title: "Copper Steps",
        venue: "Midnight Tram",
        beat_ticks: 30,
        target: 620,
        story: "Notes and chords.",
        lesson: "Linked notes: press both lanes together.",
        patterns: &COPPER,
        arrangement: &[0, 1, 0, 2, 3, 4, 0, 1, 5, 2, 3, 4, 0, 1, 2, 3],
        melody: [
            62, 65, -1, 69, 67, -1, 65, 62, 60, 62, 65, -1, 69, 67, 65, -1,
        ],
        bass: [50, 46, 53, 48],
    },
    Track {
        title: "Glass Rain",
        venue: "Winter Garden",
        beat_ticks: 30,
        target: 650,
        story: "Offbeats and rests.",
        lesson: "Extra taps break your combo.",
        patterns: &GLASS,
        arrangement: &[0, 1, 0, 3, 2, 4, 0, 1, 5, 3, 2, 4, 0, 1, 3, 5, 2, 4, 0, 1],
        melody: [
            -1, 67, -1, 71, -1, 74, 72, -1, -1, 69, 67, -1, 64, -1, 66, 67,
        ],
        bass: [43, 48, 45, 50],
    },
    Track {
        title: "Night Circuit",
        venue: "Electric Arcade",
        beat_ticks: 24,
        target: 650,
        story: "Fast lane changes.",
        lesson: "Read ahead of the judgment line.",
        patterns: &NIGHT,
        arrangement: &[0, 1, 5, 0, 2, 3, 4, 1, 0, 1, 3, 5, 2, 4, 3, 0, 1, 5, 2, 4],
        melody: [
            64, 67, 71, 76, 74, 71, 67, -1, 62, 66, 69, 74, 72, 69, 66, -1,
        ],
        bass: [40, 45, 43, 47],
    },
    Track {
        title: "Aurora Current",
        venue: "Northlight Tower",
        beat_ticks: 24,
        target: 680,
        story: "Build and use Encore.",
        lesson: "X at 100 charge: eight beats of protection.",
        patterns: &AURORA,
        arrangement: &[
            0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 4, 5, 2, 3, 6, 7, 4, 5, 0, 1, 2, 6, 3, 7,
        ],
        melody: [
            69, -1, 73, 76, 78, 76, 73, -1, 68, 71, -1, 76, 75, 71, 68, -1,
        ],
        bass: [45, 50, 47, 52],
    },
    Track {
        title: "Voltage Choir",
        venue: "The Whole City",
        beat_ticks: 24,
        target: 700,
        story: "Final track: mixed patterns.",
        lesson: "Combine accurate taps, chords and Encore.",
        patterns: &CHOIR,
        arrangement: &[
            0, 1, 2, 3, 4, 5, 6, 7, 0, 2, 1, 3, 5, 4, 6, 7, 2, 3, 0, 1, 4, 5, 6, 7, 0, 1, 2, 3, 4,
            5, 6, 7,
        ],
        melody: [
            60, 64, 67, 72, 74, 72, 67, 64, 62, 65, 69, 74, 76, 74, 69, 65,
        ],
        bass: [48, 53, 50, 55],
    },
];
