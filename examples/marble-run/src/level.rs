use ferravex_engine::play::authoring::*;
use ferravex_render::prelude::*;
pub const SOURCE: &str = include_str!("../content/aqueduct.fxscene");
#[derive(Clone, Copy)]
pub struct Platform {
    pub center: Vec3,
    pub size: Vec3,
    pub rotation: Quat,
    pub friction: f32,
}
impl Platform {
    pub fn at(&self, offset: Vec3) -> Vec3 {
        self.center + self.rotation * offset
    }
}
#[derive(Clone, Copy)]
pub struct Obstacle {
    pub position: Vec3,
    pub radius: f32,
    pub height: f32,
    pub rotation: Quat,
}
#[derive(Clone, Copy)]
pub struct Sweeper {
    pub position: Vec3,
    pub size: Vec3,
    pub speed: f32,
    pub phase: f32,
}
impl Sweeper {
    pub fn rotation(&self, time: f32) -> Quat {
        Quat::from_rotation_y(self.phase + self.speed * time)
    }
}
#[derive(Clone, Copy)]
pub struct Checkpoint {
    pub position: Vec3,
    pub marker: Vec3,
    pub route_index: usize,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    Box,
    Cylinder,
}
#[derive(Clone, Copy)]
pub struct Decoration {
    pub shape: Shape,
    pub position: Vec3,
    pub size: Vec3,
    pub rotation: Quat,
    pub color: Color,
    pub segments: u16,
}
#[derive(Clone, Copy)]
pub struct Tuning {
    pub gravity: f32,
    pub radius: f32,
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub speed: f32,
    pub fall_y: f32,
    pub gem_radius: f32,
    pub checkpoint_radius: f32,
    pub finish_radius: f32,
}
pub struct Level {
    pub spawn: Vec3,
    pub spawn_route: usize,
    pub platforms: Vec<Platform>,
    pub obstacles: Vec<Obstacle>,
    pub sweepers: Vec<Sweeper>,
    pub checkpoints: Vec<Checkpoint>,
    pub gems: Vec<Vec3>,
    pub finish: Vec3,
    pub route: Vec<Vec2>,
    pub decorations: Vec<Decoration>,
    pub tuning: Tuning,
}
fn value<'a>(c: &'a Component, key: &str) -> &'a Value {
    c.get(key).expect("schema-validated field")
}
fn number(c: &Component, key: &str) -> f32 {
    value(c, key).number().unwrap() as f32
}
fn order(c: &Component) -> usize {
    value(c, "order").integer().unwrap() as usize
}
fn schema() -> Schema {
    let mut s = Schema::new();
    let index = |max| FieldRule::required("order", ValueKind::Integer).integer_range(0, max);
    s.register(
        "platform",
        &[
            index(31),
            FieldRule::required("size", ValueKind::Vec3).range(0.05, 30.),
            FieldRule::optional("friction", ValueKind::Number).range(0., 2.),
        ],
    )
    .unwrap();
    s.register(
        "sweeper",
        &[
            index(15),
            FieldRule::required("size", ValueKind::Vec3).range(0.15, 8.),
            FieldRule::required("speed", ValueKind::Number).range(-2., 2.),
            FieldRule::required("phase", ValueKind::Number).range(-7., 7.),
        ],
    )
    .unwrap();
    s.register(
        "obstacle",
        &[
            index(31),
            FieldRule::required("radius", ValueKind::Number).range(0.05, 2.),
            FieldRule::required("height", ValueKind::Number).range(0.1, 5.),
        ],
    )
    .unwrap();
    s.register(
        "checkpoint",
        &[
            index(15),
            FieldRule::required("route", ValueKind::Reference).target("waypoint"),
        ],
    )
    .unwrap();
    s.register(
        "spawn",
        &[FieldRule::required("route", ValueKind::Reference).target("waypoint")],
    )
    .unwrap();
    s.register("gem", &[index(63)]).unwrap();
    s.register("finish", &[]).unwrap();
    s.register("waypoint", &[index(127)]).unwrap();
    s.register(
        "prop",
        &[
            FieldRule::required("shape", ValueKind::Text),
            FieldRule::optional("segments", ValueKind::Integer).integer_range(3, 16),
            FieldRule::required("size", ValueKind::Vec3).range(0.01, 30.),
            FieldRule::required("color", ValueKind::Color),
        ],
    )
    .unwrap();
    s.register(
        "tuning",
        &[
            FieldRule::required("gravity", ValueKind::Number).range(-30., -1.),
            FieldRule::required("radius", ValueKind::Number).range(0.2, 1.),
            FieldRule::required("density", ValueKind::Number).range(0.5, 5.),
            FieldRule::required("friction", ValueKind::Number).range(0., 2.),
            FieldRule::required("restitution", ValueKind::Number).range(0., 1.),
            FieldRule::required("linear_damping", ValueKind::Number).range(0., 5.),
            FieldRule::required("angular_damping", ValueKind::Number).range(0., 5.),
            FieldRule::required("speed", ValueKind::Number).range(1., 10.),
            FieldRule::required("fall_y", ValueKind::Number).range(-100., -2.),
            FieldRule::required("gem_radius", ValueKind::Number).range(0.6, 3.),
            FieldRule::required("checkpoint_radius", ValueKind::Number).range(0.6, 3.),
            FieldRule::required("finish_radius", ValueKind::Number).range(0.6, 3.),
        ],
    )
    .unwrap();
    s
}
fn position(n: &DataNode) -> Result<Vec3, ContentError> {
    let p = Vec3::from_array(n.position());
    if p.abs().max_element() > 256. {
        return Err(ContentError::domain(format!(
            "{} is outside course bounds",
            n.id
        )));
    }
    Ok(p)
}
fn trs(n: &DataNode) -> Result<(Vec3, Quat, Vec3), ContentError> {
    let (s, r, p) = Mat4::from_cols_array(&n.world)
        .try_to_scale_rotation_translation()
        .ok_or_else(|| {
            ContentError::domain(format!("{} requires non-sheared nonzero TRS", n.id))
        })?;
    if s.min_element() <= 0. || s.max_element() > 10. {
        return Err(ContentError::domain(format!(
            "{} needs positive scale at most ten",
            n.id
        )));
    }
    position(n)?;
    Ok((s, r, p))
}
fn ordered<T>(
    mut values: Vec<(usize, T)>,
    min: usize,
    max: usize,
    what: &str,
) -> Result<Vec<T>, ContentError> {
    if values.len() < min || values.len() > max {
        return Err(ContentError::domain(format!(
            "expected {min}..{max} {what}"
        )));
    }
    values.sort_by_key(|(i, _)| *i);
    if values.iter().enumerate().any(|(i, (n, _))| i != *n) {
        return Err(ContentError::domain(format!(
            "{what} need unique contiguous zero-based order"
        )));
    }
    Ok(values.into_iter().map(|(_, v)| v).collect())
}
fn one<'a>(data: &'a SceneData, kind: &str) -> Result<(&'a DataNode, &'a Component), ContentError> {
    let values: Vec<_> = data
        .nodes()
        .iter()
        .filter_map(|n| n.component(kind).map(|c| (n, c)))
        .collect();
    if values.len() != 1 {
        return Err(ContentError::domain(format!("expected one {kind}")));
    }
    Ok(values[0])
}
impl Level {
    pub fn parse(source: &str) -> Result<Self, ContentError> {
        let data = Document::load(source, &schema())?;
        Self::from_data(&data)
    }
    fn from_data(data: &SceneData) -> Result<Self, ContentError> {
        let platforms = ordered(
            data.of_kind("platform")
                .map(|(n, c)| {
                    let (s, r, p) = trs(n)?;
                    let size = Vec3::from_array(value(c, "size").vec3().unwrap()) * s;
                    if size.min_element() < 0.05 || size.max_element() > 30. {
                        return Err(ContentError::domain(
                            "effective platform size outside 0.05..30",
                        ));
                    }
                    Ok((
                        order(c),
                        Platform {
                            center: p,
                            size,
                            rotation: r,
                            friction: c.get("friction").and_then(Value::number).unwrap_or(0.9)
                                as f32,
                        },
                    ))
                })
                .collect::<Result<_, ContentError>>()?,
            1,
            32,
            "platforms",
        )?;
        let obstacles = ordered(
            data.of_kind("obstacle")
                .map(|(n, c)| {
                    let (s, r, p) = trs(n)?;
                    let radius = number(c, "radius") * s.x;
                    let height = number(c, "height") * s.y;
                    if (s.x - s.z).abs() > 1e-4 || radius > 2. || height > 5. {
                        return Err(ContentError::domain(
                            "cylinder obstacles require equal XZ scales, radius <=2, height <=5",
                        ));
                    }
                    Ok((
                        order(c),
                        Obstacle {
                            position: p,
                            radius,
                            height,
                            rotation: r,
                        },
                    ))
                })
                .collect::<Result<_, ContentError>>()?,
            0,
            32,
            "obstacles",
        )?;
        let route = ordered(
            data.of_kind("waypoint")
                .map(|(n, c)| {
                    let p = position(n)?;
                    if p.y.abs() > 1e-4 {
                        return Err(ContentError::domain(
                            "autoplay waypoints must be in the XZ plane",
                        ));
                    }
                    Ok((order(c), vec2(p.x, p.z)))
                })
                .collect::<Result<_, ContentError>>()?,
            1,
            128,
            "waypoints",
        )?;
        let sweepers = ordered(
            data.of_kind("sweeper")
                .map(|(n, c)| {
                    let (scale, rotation, position) = trs(n)?;
                    let size = Vec3::from_array(value(c, "size").vec3().unwrap()) * scale;
                    if size.min_element() < 0.15
                        || size.max_element() > 8.
                        || !rotation.abs_diff_eq(Quat::IDENTITY, 1e-5)
                    {
                        return Err(ContentError::domain(
                            "sweepers need identity rotation and size <=8",
                        ));
                    }
                    Ok((
                        order(c),
                        Sweeper {
                            position,
                            size,
                            speed: number(c, "speed"),
                            phase: number(c, "phase"),
                        },
                    ))
                })
                .collect::<Result<_, ContentError>>()?,
            0,
            16,
            "sweepers",
        )?;
        let route_index = |c: &Component| -> usize {
            let target = value(c, "route").reference().unwrap();
            order(data.node(target).unwrap().component("waypoint").unwrap())
        };
        let checkpoints = ordered(
            data.of_kind("checkpoint")
                .map(|(n, c)| {
                    Ok((
                        order(c),
                        Checkpoint {
                            position: position(n)?,
                            marker: Vec3::from_array(
                                ferravex_engine::play::scene::transform_point(
                                    n.world,
                                    [0., -0.727, 0.],
                                ),
                            ),
                            route_index: route_index(c),
                        },
                    ))
                })
                .collect::<Result<_, ContentError>>()?,
            0,
            16,
            "checkpoints",
        )?;
        let gems = ordered(
            data.of_kind("gem")
                .map(|(n, c)| Ok((order(c), position(n)?)))
                .collect::<Result<_, ContentError>>()?,
            1,
            64,
            "gems",
        )?;
        let spawn = one(data, "spawn")?;
        let spawn_route = route_index(spawn.1);
        let spawn = position(spawn.0)?;
        let finish = position(one(data, "finish")?.0)?;
        let tuning = one(data, "tuning")?.1;
        let tuning = Tuning {
            gravity: number(tuning, "gravity"),
            radius: number(tuning, "radius"),
            density: number(tuning, "density"),
            friction: number(tuning, "friction"),
            restitution: number(tuning, "restitution"),
            linear_damping: number(tuning, "linear_damping"),
            angular_damping: number(tuning, "angular_damping"),
            speed: number(tuning, "speed"),
            fall_y: number(tuning, "fall_y"),
            gem_radius: number(tuning, "gem_radius"),
            checkpoint_radius: number(tuning, "checkpoint_radius"),
            finish_radius: number(tuning, "finish_radius"),
        };
        // These gameplay markers are upright; meshes/colliders and prop nodes retain full TRS.
        if checkpoints
            .iter()
            .any(|p| p.marker.abs().max_element() > 256.)
            || platforms.iter().any(|p| {
                tuning.fall_y
                    >= p.center.y
                        - ((p.rotation * Vec3::X).y.abs() * p.size.x
                            + (p.rotation * Vec3::Y).y.abs() * p.size.y
                            + (p.rotation * Vec3::Z).y.abs() * p.size.z)
                            * 0.5
                        - 1.
            })
        {
            return Err(ContentError::domain(
                "fall height must be below the complete course",
            ));
        }
        let decorations = data
            .of_kind("prop")
            .map(|(n, c)| {
                let (s, r, p) = trs(n)?;
                let size = Vec3::from_array(value(c, "size").vec3().unwrap()) * s;
                if size.max_element() > 30. || size.min_element() < 0.01 {
                    return Err(ContentError::domain(
                        "effective prop dimensions exceed limits",
                    ));
                }
                let shape = match value(c, "shape").text().unwrap() {
                    "box" => Shape::Box,
                    "cylinder" => {
                        if (size.x - size.z).abs() > 1e-4 {
                            return Err(ContentError::domain(
                                "cylinder prop needs equal XZ dimensions",
                            ));
                        }
                        Shape::Cylinder
                    }
                    _ => return Err(ContentError::domain("prop shape must be box or cylinder")),
                };
                let color = value(c, "color").color().unwrap();
                Ok(Decoration {
                    shape,
                    position: p,
                    size,
                    rotation: r,
                    color: Color::from_rgba(color[0], color[1], color[2], color[3]),
                    segments: c.get("segments").and_then(Value::integer).unwrap_or(6) as u16,
                })
            })
            .collect::<Result<Vec<_>, ContentError>>()?;
        if decorations.len() > 128 {
            return Err(ContentError::domain("at most 128 course props"));
        }
        Ok(Self {
            spawn,
            spawn_route,
            platforms,
            obstacles,
            sweepers,
            checkpoints,
            gems,
            finish,
            route,
            decorations,
            tuning,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_course_and_edits_are_validated_before_use() {
        let l = Level::parse(SOURCE).unwrap();
        assert_eq!(
            (
                l.platforms.len(),
                l.gems.len(),
                l.checkpoints.len(),
                l.decorations.len()
            ),
            (11, 6, 2, 4)
        );
        assert_eq!(l.checkpoints[0].route_index, 4);
        assert_eq!(l.checkpoints[1].route_index, 8);
        assert!(
            l.checkpoints[0]
                .marker
                .abs_diff_eq(vec3(10., 0.323, -12.), 1e-5)
        );
        assert_eq!(
            Level::parse(&SOURCE.replace("gravity number -16;", "gravity number -8;"))
                .unwrap()
                .tuning
                .gravity,
            -8.
        );
        assert!(Level::parse(&SOURCE.replace("radius number 0.55;", "radius number 99;")).is_err());
        assert!(Level::parse(&SOURCE.replace("route ref /way4;", "route ref /gem0;")).is_err());
    }
}
