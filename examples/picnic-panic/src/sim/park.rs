//! Thistledown Green, loaded from authored scene data.
//!
//! [`Park::parse`] runs the engine's schema-validated scene loader, then checks
//! the picnic's own domain rules (one spawn, three dens, two seats, one of each
//! prank prop, complete routines) before anything is built. The resulting
//! plain data drives physics colliders, the navigation grid, AI routines,
//! hiding places and every decoration the renderer draws.
use ferravex_play::authoring::{
    Component, ContentError, DataNode, Document, FieldRule, SceneData, Schema, Value, ValueKind,
};
use ferravex_render::prelude::{Mat4, Quat, Vec3, vec3};

pub const SOURCE: &str = include_str!("../../content/park.fxscene");
pub const HALF_EXTENT: f32 = 30.;
pub const FOLK: usize = 5;
pub const MAX_SPOTS: usize = 6;

/// Static box collider (and drawable block) in world space.
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub center: Vec3,
    pub half: Vec3,
    pub rotation: Quat,
    pub color: [u8; 4],
    pub occlude: bool,
    pub nav: bool,
    pub look: Look,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Plain,
    Hedge,
    Bench,
    Stool,
    Bandstand,
    Hidden,
    MusicStand,
    Toybox,
    Sandbox,
    Grass,
    Ramp,
    Hay,
}

#[derive(Clone, Copy, Debug)]
pub struct Table {
    pub center: Vec3,
    pub top: Vec3,
    pub height: f32,
    pub cloth: [u8; 4],
    pub trim: [u8; 4],
}

impl Table {
    pub fn top_y(&self) -> f32 {
        self.center.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKind {
    Oak,
    Pine,
    Birch,
}

#[derive(Clone, Copy, Debug)]
pub struct Tree {
    pub kind: TreeKind,
    pub at: Vec3,
    pub scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Bush {
    pub at: Vec3,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Seat,
    Basket,
    Cloth,
    Stand,
    Plinth,
    SledEnd,
    Pond,
    Rope,
}

#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub role: Role,
    pub at: Vec3,
    pub radius: f32,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Sit,
    Fuss,
    Chat,
    Feed,
    Inspect,
    Water,
    Rake,
    Conduct,
    Bow,
    Snack,
    Play,
    Dance,
    Sniff,
    Beg,
}

#[derive(Clone, Copy, Debug)]
pub struct Spot {
    pub at: Vec3,
    pub act: Act,
    pub wait: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoKind {
    Seesaw,
    BellArch,
    Sign,
    Flowers,
    Tent,
    Awning,
    Path,
}

#[derive(Clone, Copy, Debug)]
pub struct Deco {
    pub kind: DecoKind,
    pub at: Vec3,
    pub rotation: Quat,
    pub size: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct Bunting {
    pub from: Vec3,
    pub to: Vec3,
    pub flags: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PropKind {
    Pie,
    Marrow,
    Pinecone,
    Sheet,
    Tray,
    Blanket,
    Melon,
    Cushion,
    Crate,
    Cake,
    Dish,
    Teapot,
    Jelly,
    Apple,
}

impl PropKind {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "pie" => Self::Pie,
            "marrow" => Self::Marrow,
            "pinecone" => Self::Pinecone,
            "sheet" => Self::Sheet,
            "tray" => Self::Tray,
            "blanket" => Self::Blanket,
            "melon" => Self::Melon,
            "cushion" => Self::Cushion,
            "crate" => Self::Crate,
            "cake" => Self::Cake,
            "dish" => Self::Dish,
            "teapot" => Self::Teapot,
            "jelly" => Self::Jelly,
            "apple" => Self::Apple,
            _ => return None,
        })
    }
    /// Unique story props; the finale and set dressing can repeat.
    pub fn unique(self) -> bool {
        matches!(
            self,
            Self::Pie
                | Self::Marrow
                | Self::Pinecone
                | Self::Sheet
                | Self::Tray
                | Self::Blanket
                | Self::Melon
                | Self::Cushion
                | Self::Cake
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PropSpec {
    pub kind: PropKind,
    pub at: Vec3,
    pub rotation: Quat,
    pub owner: Option<usize>,
}

/// Authored picnicker ids; the order is the folk array order and indexes
/// [`Park::routines`] and prop owners.
pub const FOLK_NAMES: [&str; FOLK] = ["nana", "pumble", "trumble", "pip", "biscuit"];

pub struct Park {
    pub spawn: Vec3,
    pub dens: [Vec3; 3],
    pub solids: Vec<Solid>,
    pub ramps: Vec<Solid>,
    pub tables: Vec<Table>,
    pub trees: Vec<Tree>,
    pub bushes: Vec<Bush>,
    pub zones: Vec<Zone>,
    pub decos: Vec<Deco>,
    pub poles: Vec<(Vec3, f32)>,
    pub bunting: Vec<Bunting>,
    pub props: Vec<PropSpec>,
    pub shinies: Vec<Vec3>,
    pub routines: [Vec<Spot>; FOLK],
}

fn schema() -> Schema {
    let mut s = Schema::new();
    let req = FieldRule::required;
    let opt = FieldRule::optional;
    s.register(
        "solid",
        &[
            req("size", ValueKind::Vec3).range(0.02, 80.),
            req("color", ValueKind::Color),
            opt("occlude", ValueKind::Bool),
            opt("nav", ValueKind::Bool),
            opt("look", ValueKind::Text),
        ],
    )
    .unwrap();
    s.register("ramp", &[req("size", ValueKind::Vec3).range(0.1, 40.)])
        .unwrap();
    s.register(
        "table",
        &[
            req("size", ValueKind::Vec3).range(0.05, 8.),
            req("height", ValueKind::Number).range(0.3, 1.2),
            req("cloth", ValueKind::Color),
            req("trim", ValueKind::Color),
        ],
    )
    .unwrap();
    s.register(
        "tree",
        &[
            req("kind", ValueKind::Text),
            req("scale", ValueKind::Number).range(0.5, 2.),
        ],
    )
    .unwrap();
    s.register("bush", &[req("radius", ValueKind::Number).range(0.4, 2.5)])
        .unwrap();
    s.register(
        "zone",
        &[
            req("role", ValueKind::Text),
            req("radius", ValueKind::Number).range(0.2, 12.),
            opt("index", ValueKind::Integer).integer_range(0, 7),
        ],
    )
    .unwrap();
    s.register(
        "spot",
        &[
            req("folk", ValueKind::Text),
            req("order", ValueKind::Integer).integer_range(0, MAX_SPOTS as i64 - 1),
            req("act", ValueKind::Text),
            req("wait", ValueKind::Number).range(0.5, 30.),
        ],
    )
    .unwrap();
    s.register(
        "prop",
        &[req("kind", ValueKind::Text), opt("owner", ValueKind::Text)],
    )
    .unwrap();
    s.register(
        "deco",
        &[
            req("kind", ValueKind::Text),
            opt("size", ValueKind::Vec3).range(0.01, 40.),
        ],
    )
    .unwrap();
    s.register("pole", &[req("height", ValueKind::Number).range(1., 8.)])
        .unwrap();
    s.register(
        "bunting",
        &[
            req("from", ValueKind::Reference).target("pole"),
            req("to", ValueKind::Reference).target("pole"),
            req("flags", ValueKind::Integer).integer_range(3, 12),
        ],
    )
    .unwrap();
    s.register("shiny", &[]).unwrap();
    s.register("spawn", &[]).unwrap();
    s.register(
        "den",
        &[req("order", ValueKind::Integer).integer_range(0, 2)],
    )
    .unwrap();
    s
}

fn err(message: impl Into<String>) -> ContentError {
    ContentError::domain(message.into())
}

fn field<'a>(c: &'a Component, key: &str) -> &'a Value {
    c.get(key).expect("schema-validated field")
}

fn text<'a>(c: &'a Component, key: &str) -> &'a str {
    field(c, key).text().expect("schema-validated text")
}

fn number(c: &Component, key: &str) -> f32 {
    field(c, key).number().expect("schema-validated number") as f32
}

fn trs(n: &DataNode) -> Result<(Vec3, Quat, Vec3), ContentError> {
    let (s, r, p) = Mat4::from_cols_array(&n.world)
        .try_to_scale_rotation_translation()
        .ok_or_else(|| err(format!("{} needs a non-sheared transform", n.id)))?;
    if !p.is_finite() || p.x.abs() > 40. || p.z.abs() > 40. || p.y.abs() > 20. {
        return Err(err(format!("{} is outside the park", n.id)));
    }
    if s.min_element() <= 0. || s.max_element() > 4. {
        return Err(err(format!("{} needs positive scale up to four", n.id)));
    }
    Ok((s, r, p))
}

fn folk_index(name: &str) -> Option<usize> {
    FOLK_NAMES.iter().position(|n| *n == name)
}

impl Park {
    /// The authored park. Panics only if the embedded content is invalid,
    /// which the test suite rules out.
    pub fn authored() -> Self {
        Self::parse(SOURCE).expect("authored park is valid")
    }

    pub fn parse(source: &str) -> Result<Self, ContentError> {
        let data = Document::load(source, &schema())?;
        Self::from_data(&data)
    }

    fn from_data(data: &SceneData) -> Result<Self, ContentError> {
        let mut spawn = None;
        let mut dens = [None; 3];
        let mut solids = Vec::new();
        let mut ramps = Vec::new();
        let mut tables = Vec::new();
        let mut trees = Vec::new();
        let mut bushes = Vec::new();
        let mut zones = Vec::new();
        let mut decos = Vec::new();
        let mut poles = Vec::new();
        let mut bunting = Vec::new();
        let mut props = Vec::new();
        let mut shinies = Vec::new();
        let mut routines: [Vec<(usize, Spot)>; FOLK] = Default::default();
        for n in data.nodes() {
            for (kind, c) in n.components() {
                match kind {
                    "spawn" => {
                        if spawn.replace(trs(n)?.2).is_some() {
                            return Err(err("expected exactly one spawn"));
                        }
                    }
                    "den" => {
                        let order = field(c, "order").integer().unwrap() as usize;
                        if dens[order].replace(trs(n)?.2).is_some() {
                            return Err(err("den order must be unique"));
                        }
                    }
                    "solid" | "ramp" => {
                        let (s, r, p) = trs(n)?;
                        let size = Vec3::from_array(field(c, "size").vec3().unwrap()) * s;
                        let look = match c.get("look").and_then(Value::text) {
                            None => Look::Plain,
                            Some("hedge") => Look::Hedge,
                            Some("bench") => Look::Bench,
                            Some("stool") => Look::Stool,
                            Some("bandstand") => Look::Bandstand,
                            Some("hidden") => Look::Hidden,
                            Some("music_stand") => Look::MusicStand,
                            Some("toybox") => Look::Toybox,
                            Some("sandbox") => Look::Sandbox,
                            Some("grass") => Look::Grass,
                            Some("hay") => Look::Hay,
                            Some(other) => {
                                return Err(err(format!("{}: unknown look {other}", n.id)));
                            }
                        };
                        let solid = Solid {
                            center: p,
                            half: size * 0.5,
                            rotation: r,
                            color: c
                                .get("color")
                                .and_then(Value::color)
                                .unwrap_or([112, 168, 84, 255]),
                            occlude: kind == "ramp"
                                || c.get("occlude").and_then(Value::boolean).unwrap_or(false),
                            nav: c.get("nav").and_then(Value::boolean).unwrap_or(true),
                            look: if kind == "ramp" { Look::Ramp } else { look },
                        };
                        if kind == "ramp" {
                            ramps.push(solid);
                        } else {
                            solids.push(solid);
                        }
                    }
                    "table" => {
                        let (_, r, p) = trs(n)?;
                        if !r.abs_diff_eq(Quat::IDENTITY, 1e-4) {
                            return Err(err(format!("{}: tables are axis-aligned", n.id)));
                        }
                        tables.push(Table {
                            center: p,
                            top: Vec3::from_array(field(c, "size").vec3().unwrap()),
                            height: number(c, "height"),
                            cloth: field(c, "cloth").color().unwrap(),
                            trim: field(c, "trim").color().unwrap(),
                        });
                    }
                    "tree" => {
                        let kind = match text(c, "kind") {
                            "oak" => TreeKind::Oak,
                            "pine" => TreeKind::Pine,
                            "birch" => TreeKind::Birch,
                            other => return Err(err(format!("{}: unknown tree {other}", n.id))),
                        };
                        trees.push(Tree {
                            kind,
                            at: trs(n)?.2,
                            scale: number(c, "scale"),
                        });
                    }
                    "bush" => bushes.push(Bush {
                        at: trs(n)?.2,
                        radius: number(c, "radius"),
                    }),
                    "zone" => {
                        let role = match text(c, "role") {
                            "seat" => Role::Seat,
                            "basket" => Role::Basket,
                            "cloth" => Role::Cloth,
                            "stand" => Role::Stand,
                            "plinth" => Role::Plinth,
                            "sled_end" => Role::SledEnd,
                            "pond" => Role::Pond,
                            "rope" => Role::Rope,
                            other => return Err(err(format!("{}: unknown zone {other}", n.id))),
                        };
                        zones.push(Zone {
                            role,
                            at: trs(n)?.2,
                            radius: number(c, "radius"),
                            index: c.get("index").and_then(Value::integer).unwrap_or(0) as usize,
                        });
                    }
                    "deco" => {
                        let (_, r, p) = trs(n)?;
                        let kind = match text(c, "kind") {
                            "seesaw" => DecoKind::Seesaw,
                            "bell_arch" => DecoKind::BellArch,
                            "sign" => DecoKind::Sign,
                            "flowers" => DecoKind::Flowers,
                            "tent" => DecoKind::Tent,
                            "awning" => DecoKind::Awning,
                            "path" => DecoKind::Path,
                            other => return Err(err(format!("{}: unknown deco {other}", n.id))),
                        };
                        decos.push(Deco {
                            kind,
                            at: p,
                            rotation: r,
                            size: c
                                .get("size")
                                .and_then(Value::vec3)
                                .map_or(Vec3::ONE, Vec3::from_array),
                        });
                    }
                    "pole" => poles.push((trs(n)?.2, number(c, "height"))),
                    "bunting" => {
                        let end = |key: &str| -> Result<Vec3, ContentError> {
                            let id = field(c, key).reference().unwrap();
                            let node = data.node(id).ok_or_else(|| err("missing pole"))?;
                            let height = number(node.component("pole").unwrap(), "height");
                            Ok(trs(node)?.2 + Vec3::Y * height)
                        };
                        bunting.push(Bunting {
                            from: end("from")?,
                            to: end("to")?,
                            flags: field(c, "flags").integer().unwrap() as usize,
                        });
                    }
                    "prop" => {
                        let name = text(c, "kind");
                        let kind = PropKind::parse(name)
                            .ok_or_else(|| err(format!("{}: unknown prop {name}", n.id)))?;
                        let owner =
                            match c.get("owner").and_then(Value::text) {
                                None => None,
                                Some(owner) => Some(folk_index(owner).ok_or_else(|| {
                                    err(format!("{}: unknown owner {owner}", n.id))
                                })?),
                            };
                        let (_, r, p) = trs(n)?;
                        props.push(PropSpec {
                            kind,
                            at: p,
                            rotation: r,
                            owner,
                        });
                    }
                    "shiny" => shinies.push(trs(n)?.2),
                    "spot" => {
                        let who = text(c, "folk");
                        let folk = folk_index(who)
                            .ok_or_else(|| err(format!("{}: unknown folk {who}", n.id)))?;
                        let act = match text(c, "act") {
                            "sit" => Act::Sit,
                            "fuss" => Act::Fuss,
                            "chat" => Act::Chat,
                            "feed" => Act::Feed,
                            "inspect" => Act::Inspect,
                            "water" => Act::Water,
                            "rake" => Act::Rake,
                            "conduct" => Act::Conduct,
                            "bow" => Act::Bow,
                            "snack" => Act::Snack,
                            "play" => Act::Play,
                            "dance" => Act::Dance,
                            "sniff" => Act::Sniff,
                            "beg" => Act::Beg,
                            other => return Err(err(format!("{}: unknown act {other}", n.id))),
                        };
                        routines[folk].push((
                            field(c, "order").integer().unwrap() as usize,
                            Spot {
                                at: trs(n)?.2,
                                act,
                                wait: number(c, "wait"),
                            },
                        ));
                    }
                    other => return Err(err(format!("{}: unhandled component {other}", n.id))),
                }
            }
        }
        let spawn = spawn.ok_or_else(|| err("expected a spawn"))?;
        let dens = [
            dens[0].ok_or_else(|| err("missing den 0"))?,
            dens[1].ok_or_else(|| err("missing den 1"))?,
            dens[2].ok_or_else(|| err("missing den 2"))?,
        ];
        for kind in [
            PropKind::Pie,
            PropKind::Marrow,
            PropKind::Pinecone,
            PropKind::Sheet,
            PropKind::Tray,
            PropKind::Blanket,
            PropKind::Melon,
            PropKind::Cushion,
            PropKind::Cake,
        ] {
            if props.iter().filter(|p| p.kind == kind).count() != 1 {
                return Err(err(format!("expected exactly one {kind:?}")));
            }
        }
        if props.len() > super::props::MAX_PROPS {
            return Err(err("too many props"));
        }
        for role in [
            Role::Basket,
            Role::Cloth,
            Role::Stand,
            Role::Plinth,
            Role::SledEnd,
            Role::Pond,
            Role::Rope,
        ] {
            if zones.iter().filter(|z| z.role == role).count() != 1 {
                return Err(err(format!("expected exactly one {role:?} zone")));
            }
        }
        if zones.iter().filter(|z| z.role == Role::Seat).count() != 2 {
            return Err(err("expected two seats"));
        }
        if !(1..=16).contains(&shinies.len()) {
            return Err(err("expected 1..=16 shinies"));
        }
        if !decos.iter().any(|d| d.kind == DecoKind::Seesaw)
            || !decos.iter().any(|d| d.kind == DecoKind::BellArch)
        {
            return Err(err("the park needs a seesaw and a bell"));
        }
        let routines = routines.map(|mut spots| {
            spots.sort_by_key(|(order, _)| *order);
            spots
        });
        for (i, spots) in routines.iter().enumerate() {
            if spots.len() < 2 || spots.iter().enumerate().any(|(k, (order, _))| *order != k) {
                return Err(err(format!(
                    "{} needs a contiguous routine of at least two spots",
                    FOLK_NAMES[i]
                )));
            }
        }
        Ok(Self {
            spawn,
            dens,
            solids,
            ramps,
            tables,
            trees,
            bushes,
            zones,
            decos,
            poles,
            bunting,
            props,
            shinies,
            routines: routines.map(|spots| spots.into_iter().map(|(_, s)| s).collect()),
        })
    }

    pub fn zone(&self, role: Role) -> Zone {
        *self
            .zones
            .iter()
            .find(|z| z.role == role)
            .expect("validated zone")
    }

    pub fn seat(&self, index: usize) -> Option<Zone> {
        self.zones
            .iter()
            .find(|z| z.role == Role::Seat && z.index == index)
            .copied()
    }

    pub fn deco(&self, kind: DecoKind) -> Deco {
        *self
            .decos
            .iter()
            .find(|d| d.kind == kind)
            .expect("validated deco")
    }

    /// Height of walkable ground for folk (who never climb the hill).
    pub fn ground_y(&self, p: Vec3) -> f32 {
        let bandstand = vec3(-18., 0., -3.);
        let d = (p - bandstand) * vec3(1., 0., 1.);
        if d.x.abs().max(d.z.abs()) < 3.1 && d.x.abs() + d.z.abs() < 4.3 {
            0.22
        } else {
            0.
        }
    }

    /// Point inside a table's under-cloth hiding volume?
    pub fn under_table(&self, p: Vec3) -> Option<usize> {
        self.tables.iter().position(|t| {
            let d = p - t.center;
            d.x.abs() < t.top.x * 0.5 - 0.1
                && d.z.abs() < t.top.z * 0.5 - 0.1
                && d.y > -0.2
                && d.y < t.height - 0.1
        })
    }

    pub fn in_bush(&self, p: Vec3) -> Option<usize> {
        self.bushes.iter().position(|b| {
            let d = p - b.at;
            vec3(d.x, 0., d.z).length() < b.radius * 0.8 && d.y < 1.2
        })
    }

    pub fn on_hill(&self, p: Vec3) -> bool {
        p.x > -6.5 && p.x < 8.5 && p.z < -10.8
    }

    pub fn in_pond(&self, p: Vec3) -> bool {
        let pond = self.zone(Role::Pond);
        let d = p - pond.at;
        vec3(d.x, 0., d.z).length() < pond.radius && p.y < 0.6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_park_validates_and_has_every_story_piece() {
        let park = Park::authored();
        assert_eq!(park.dens.len(), 3);
        assert!(park.tables.len() >= 6);
        assert_eq!(park.bunting.len(), 2);
        assert!(park.bushes.len() >= 8);
        assert_eq!(park.shinies.len(), 10);
        for spots in &park.routines {
            assert!(spots.len() >= 3);
        }
        assert!(park.on_hill(vec3(0., 3., -25.)));
        assert!(park.in_pond(vec3(-15., 0., -17.)));
        assert_eq!(park.ground_y(vec3(-18., 0., -3.)), 0.22);
        assert!(park.under_table(vec3(-7., 0.1, 6.)).is_some());
    }

    #[test]
    fn domain_errors_are_reported_not_built() {
        // A second spawn is structurally valid scene data but breaks the park.
        let doubled = format!("{SOURCE}\nnode spawn2 {{component spawn {{}}}}\n");
        assert!(Park::parse(&doubled).is_err());
        let typo = SOURCE.replace("kind text pie;", "kind text pi;");
        assert!(Park::parse(&typo).is_err());
        assert!(Park::parse("ferravex_scene 1; node x {component nope {}}").is_err());
    }
}
