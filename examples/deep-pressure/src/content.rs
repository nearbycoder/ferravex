//! Hand-authored rescue charts. Coordinates are simulation units, never pixels.
use ferravex_math::{Vec2, vec2};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Reef {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Reef {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn blocks(self, p: Vec2, radius: f32) -> bool {
        let nearest = vec2(
            p.x.clamp(self.x, self.x + self.w),
            p.y.clamp(self.y, self.y + self.h),
        );
        (p - nearest).length_squared() < radius * radius
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HazardKind {
    Vent,
    Jelly,
    Mine,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hazard {
    pub point: Vec2,
    pub kind: HazardKind,
    pub phase: u32,
}
impl Hazard {
    pub const fn vent(x: f32, y: f32, phase: u32) -> Self {
        Self {
            point: vec2(x, y),
            kind: HazardKind::Vent,
            phase,
        }
    }
    pub const fn jelly(x: f32, y: f32, phase: u32) -> Self {
        Self {
            point: vec2(x, y),
            kind: HazardKind::Jelly,
            phase,
        }
    }
    pub const fn mine(x: f32, y: f32) -> Self {
        Self {
            point: vec2(x, y),
            kind: HazardKind::Mine,
            phase: 0,
        }
    }
    pub fn position(self, tick: u32) -> Vec2 {
        if self.kind == HazardKind::Jelly {
            let t = (tick + self.phase) as f32 / 95.;
            self.point + vec2(t.sin() * 28., (t * 0.7).cos() * 12.)
        } else {
            self.point
        }
    }
    pub fn active(self, tick: u32) -> bool {
        self.kind != HazardKind::Vent || (tick + self.phase) % 300 >= 180
    }
    pub fn radius(self) -> f32 {
        match self.kind {
            HazardKind::Vent => 27.,
            HazardKind::Jelly => 14.,
            HazardKind::Mine => 16.,
        }
    }
}
pub(crate) struct Mission {
    pub name: &'static str,
    pub story: &'static str,
    pub advice: &'static str,
    pub depth: u32,
    pub current: f32,
    pub reefs: &'static [Reef],
    pub divers: &'static [(Vec2, &'static str)],
    pub hazards: &'static [Hazard],
}
pub(crate) const MISSIONS: [Mission; 6] = [
    Mission {
        name: "The Kelp Nursery",
        story: "Rescue 3 divers and return to base.",
        advice: "Return to base to refill air.",
        depth: 180,
        current: 0.,
        reefs: &[
            Reef::new(220., 110., 60., 170.),
            Reef::new(410., 200., 70., 55.),
        ],
        divers: &[
            (vec2(150., 310.), "Ada"),
            (vec2(370., 310.), "Bo"),
            (vec2(550., 110.), "Cy"),
        ],
        hazards: &[Hazard::jelly(350., 155., 0)],
    },
    Mission {
        name: "Copper Cable",
        story: "Rescue 4 divers and return to base.",
        advice: "Vents are safe for three seconds between bursts.",
        depth: 410,
        current: 3.,
        reefs: &[
            Reef::new(180., 100., 80., 150.),
            Reef::new(400., 220., 60., 130.),
            Reef::new(420., 90., 120., 50.),
        ],
        divers: &[
            (vec2(130., 330.), "Dara"),
            (vec2(330., 190.), "Eli"),
            (vec2(550., 330.), "Fenn"),
            (vec2(570., 170.), "Gio"),
        ],
        hazards: &[Hazard::vent(330., 350., 70), Hazard::jelly(350., 100., 95)],
    },
    Mission {
        name: "The Glass Cathedral",
        story: "Rescue 4 divers and return to base.",
        advice: "Sonar reveals mine ranges.",
        depth: 720,
        current: -5.,
        reefs: &[
            Reef::new(140., 110., 70., 190.),
            Reef::new(300., 80., 70., 150.),
            Reef::new(450., 220., 80., 70.),
        ],
        divers: &[
            (vec2(110., 330.), "Hana"),
            (vec2(270., 290.), "Iris"),
            (vec2(430., 150.), "Jules"),
            (vec2(570., 330.), "Koa"),
        ],
        hazards: &[
            Hazard::mine(400., 300.),
            Hazard::jelly(550., 80., 140),
            Hazard::vent(270., 370., 30),
        ],
    },
    Mission {
        name: "Night Train",
        story: "Rescue 5 divers and return to base.",
        advice: "Neutral ballast helps resist currents.",
        depth: 1080,
        current: 8.,
        reefs: &[
            Reef::new(180., 80., 70., 170.),
            Reef::new(310., 210., 110., 70.),
            Reef::new(480., 100., 70., 180.),
        ],
        divers: &[
            (vec2(130., 290.), "Luz"),
            (vec2(290., 130.), "Miro"),
            (vec2(450., 330.), "Nell"),
            (vec2(590., 150.), "Oren"),
            (vec2(590., 330.), "Pia"),
        ],
        hazards: &[
            Hazard::vent(280., 350., 145),
            Hazard::mine(410., 140.),
            Hazard::jelly(360., 75., 70),
        ],
    },
    Mission {
        name: "The Violet Fault",
        story: "Rescue 5 divers and return to base.",
        advice: "Deep water uses extra air and damages weak hulls.",
        depth: 1460,
        current: -9.,
        reefs: &[
            Reef::new(130., 130., 100., 70.),
            Reef::new(280., 80., 70., 180.),
            Reef::new(420., 240., 100., 60.),
            Reef::new(490., 70., 60., 90.),
        ],
        divers: &[
            (vec2(90., 330.), "Quin"),
            (vec2(250., 270.), "Rae"),
            (vec2(390., 130.), "Sora"),
            (vec2(570., 210.), "Tao"),
            (vec2(570., 350.), "Uma"),
        ],
        hazards: &[
            Hazard::vent(340., 350., 210),
            Hazard::jelly(400., 70., 240),
            Hazard::mine(220., 280.),
        ],
    },
    Mission {
        name: "Last Light Station",
        story: "Rescue 6 divers and return to base.",
        advice: "Reach the eastern chamber above or below the column.",
        depth: 1900,
        current: 11.,
        reefs: &[
            Reef::new(160., 100., 60., 200.),
            Reef::new(300., 160., 70., 180.),
            Reef::new(450., 70., 70., 200.),
        ],
        divers: &[
            (vec2(110., 330.), "Vera"),
            (vec2(270., 110.), "Wren"),
            (vec2(410., 330.), "Xavi"),
            (vec2(550., 110.), "Yara"),
            (vec2(550., 330.), "Zed"),
            (vec2(390., 70.), "Sol"),
        ],
        hazards: &[
            Hazard::mine(260., 240.),
            Hazard::vent(440., 365., 95),
            Hazard::jelly(560., 230., 180),
        ],
    },
];
pub(crate) const UPGRADE_NAMES: [&str; 3] = ["Air tank", "Hull armor", "Rescue cabin"];
pub(crate) const UPGRADE_HELP: [&str; 3] = [
    "+12 seconds of air",
    "+20 hull; less damage",
    "+1 survivor berth",
];
pub(crate) const UPGRADE_COSTS: [u32; 3] = [30, 25, 35];
