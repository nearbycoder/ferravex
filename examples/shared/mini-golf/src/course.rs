//! Authored course geometry in metres. Positive Y points down the 2D course.
use ferravex_math::{Vec2, vec2};
pub const SIZE: Vec2 = vec2(18., 12.);
pub const RADIUS: f32 = 0.19;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub center: Vec2,
    pub half: Vec2,
    pub angle: f32,
    pub spin: f32,
    pub travel: Vec2,
}
impl Wall {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            center: vec2(x, y),
            half: vec2(w * 0.5, h * 0.5),
            angle: 0.,
            spin: 0.,
            travel: Vec2::ZERO,
        }
    }
    pub fn pose(self, time: f32) -> (Vec2, f32) {
        (
            self.center + self.travel * (time * 0.9).sin(),
            self.angle + self.spin * time,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Sand,
    Water,
    Slope,
}
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub min: Vec2,
    pub size: Vec2,
    pub surface: Surface,
}
impl Patch {
    pub const fn new(x: f32, y: f32, w: f32, h: f32, surface: Surface) -> Self {
        Self {
            min: vec2(x, y),
            size: vec2(w, h),
            surface,
        }
    }
    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.min.x
            && p.y >= self.min.y
            && p.x <= self.min.x + self.size.x
            && p.y <= self.min.y + self.size.y
    }
}
#[derive(Clone, Debug)]
pub struct Hole {
    pub name: &'static str,
    pub par: u8,
    pub hint: &'static str,
    pub tee: Vec2,
    pub cup: Vec2,
    pub walls: Vec<Wall>,
    pub patches: Vec<Patch>,
    pub route: Vec<Vec2>,
}
impl Hole {
    /// The height is presentation shared with slope acceleration; walls remain vertical.
    pub fn height(&self, p: Vec2) -> f32 {
        self.patches
            .iter()
            .filter(|s| s.surface == Surface::Slope && s.contains(p))
            .map(|s| {
                let u = (p.x - s.min.x) / s.size.x;
                (u * std::f32::consts::PI).sin().powi(2)
                    * (((p.y - s.min.y) / s.size.y) * std::f32::consts::PI)
                        .sin()
                        .powi(2)
                    * 0.65
            })
            .sum()
    }
    pub fn slope(&self, p: Vec2) -> Vec2 {
        let e = 0.015;
        vec2(
            (self.height(p + vec2(e, 0.)) - self.height(p - vec2(e, 0.))) / (2. * e),
            (self.height(p + vec2(0., e)) - self.height(p - vec2(0., e))) / (2. * e),
        )
    }
}
/// Twelve different layouts. Geometry is constructed only when loading a hole.
pub fn hole(index: usize) -> Hole {
    use Surface::*;
    let mut h = Hole {
        name: "First Light",
        par: 2,
        hint: "A gentle putt. Aim at the flag; about 55% power.",
        tee: vec2(3., 6.),
        cup: vec2(14.5, 6.),
        walls: vec![],
        patches: vec![],
        route: vec![],
    };
    match index % 12 {
        0 => {}
        1 => {
            h.name = "The Bank";
            h.par = 3;
            h.hint = "A wall guards the cup. Go around its lower end.";
            h.walls.push(Wall::new(9., 4.4, 0.5, 7.));
            h.route = vec![vec2(7., 9.2), vec2(11., 9.2)];
        }
        2 => {
            h.name = "Sand & Stone";
            h.par = 3;
            h.hint = "Gold bunkers slow the ball. Skirt them or add power.";
            h.patches.push(Patch::new(7., 3.8, 4., 4.4, Sand));
            h.walls.push(Wall::new(12., 2.5, 4., 0.5));
            h.route = vec![vec2(7., 9.2), vec2(12., 9.2)];
        }
        3 => {
            h.name = "Canal Crossing";
            h.par = 3;
            h.hint = "Cross on the striped bridge. Water costs one penalty.";
            h.patches = vec![
                Patch::new(8., 0., 2., 4.8, Water),
                Patch::new(8., 7.2, 2., 4.8, Water),
            ];
        }
        4 => {
            h.name = "Split Decision";
            h.par = 3;
            h.hint = "Two routes around the island. Choose your bank.";
            h.walls = vec![Wall::new(9., 6., 4., 3.)];
            h.route = vec![vec2(5.7, 9.4), vec2(12.3, 9.4)];
        }
        5 => {
            h.name = "The Windmill";
            h.par = 3;
            h.hint = "The copper sail rotates. Time the gap or take the outside.";
            let mut w = Wall::new(9., 6., 0.45, 5.);
            w.spin = 0.75;
            h.walls.push(w);
            h.route = vec![vec2(6., 9.7), vec2(12., 9.7)];
        }
        6 => {
            h.name = "Switchback";
            h.par = 4;
            h.hint = "Three turns. Short controlled putts beat a hard ricochet.";
            h.tee = vec2(2.8, 2.7);
            h.cup = vec2(15., 9.3);
            h.walls = vec![
                Wall::new(6., 4.1, 0.45, 8.2),
                Wall::new(12., 7.9, 0.45, 8.2),
            ];
            h.route = vec![vec2(4.5, 9.8), vec2(8., 9.8), vec2(9., 2.2), vec2(14., 2.2)];
        }
        7 => {
            h.name = "High Road";
            h.par = 3;
            h.hint = "The raised green rolls back. Carry the crest with enough pace.";
            h.patches.push(Patch::new(6., 1., 6., 10., Slope));
            h.route = vec![vec2(13., 6.)];
        }
        8 => {
            h.name = "Clockwork";
            h.par = 4;
            h.hint = "Sliding gates open and close. Outside lanes stay clear.";
            let mut w = Wall::new(7., 5., 0.45, 4.);
            w.travel = vec2(0., 2.);
            let mut z = Wall::new(11., 7., 0.45, 4.);
            z.travel = vec2(0., -2.);
            h.walls = vec![w, z];
            h.route = vec![vec2(5.3, 10.2), vec2(13., 10.2)];
        }
        9 => {
            h.name = "Island Links";
            h.par = 4;
            h.hint = "The safe ribbon bends around two pools. Take one corner at a time.";
            h.patches = vec![
                Patch::new(6., 0., 2.5, 7.8, Water),
                Patch::new(10., 4.2, 2.5, 7.8, Water),
            ];
            h.tee = vec2(2.7, 8.8);
            h.cup = vec2(15., 3.);
            h.route = vec![
                vec2(5., 9.5),
                vec2(9.2, 9.5),
                vec2(9.2, 2.5),
                vec2(14., 2.5),
            ];
        }
        10 => {
            h.name = "Garden Spiral";
            h.par = 4;
            h.hint = "Follow the garden wall. The inner green rewards patient aim.";
            h.tee = vec2(3., 3.);
            h.cup = vec2(10., 6.);
            h.walls = vec![
                Wall::new(6., 5., 0.5, 6.),
                Wall::new(10., 2., 8., 0.5),
                Wall::new(14., 5., 0.5, 6.),
                Wall::new(11., 8., 6., 0.5),
            ];
            h.route = vec![vec2(4.5, 9.6), vec2(7.2, 9.6), vec2(7.2, 6.)];
        }
        _ => {
            h.name = "Copper Cup";
            h.par = 4;
            h.hint = "The finale: bridge, bunker and a moving copper gate.";
            h.tee = vec2(2.6, 9.2);
            h.cup = vec2(15., 2.8);
            h.patches = vec![
                Patch::new(6., 0., 2., 7.8, Water),
                Patch::new(9., 7.5, 3., 3., Sand),
            ];
            let mut w = Wall::new(12.5, 5.5, 0.4, 3.);
            w.travel = vec2(0., 1.5);
            h.walls.push(w);
            h.route = vec![vec2(9., 9.4), vec2(10.8, 3.), vec2(14., 2.8)];
        }
    }
    h.route.push(h.cup);
    h
}
