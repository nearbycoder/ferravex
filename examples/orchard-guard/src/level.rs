use crate::game::Kind;
use ferravex_play::authoring::*;
use ferravex_render::prelude::*;
pub const SOURCE: &str = include_str!("../content/orchard.fxscene");
#[derive(Clone, Copy, Debug)]
pub struct Wave {
    pub count: u8,
    pub hp: f32,
    pub fast_factor: f32,
    pub speed: f32,
    pub fast_speed: f32,
    pub interval: f32,
    pub fast_every: u32,
    pub reward: u32,
}
pub struct Level {
    pub path: Vec<Vec2>,
    pub plots: Vec<Vec2>,
    pub autoplay: Vec<(usize, Kind)>,
    pub upgrade_order: Vec<usize>,
    pub waves: Vec<Wave>,
    pub money: u32,
    pub lives: u8,
    pub kill_reward: u32,
    pub wave_hint: String,
}
fn field<'a>(c: &'a Component, name: &str) -> &'a Value {
    c.get(name).expect("schema-validated field")
}
fn number(c: &Component, name: &str) -> f32 {
    field(c, name).number().unwrap() as f32
}
fn int(c: &Component, name: &str) -> i64 {
    field(c, name).integer().unwrap()
}
fn position(n: &DataNode, min: Vec2, max: Vec2) -> Result<Vec2, ContentError> {
    let p = Vec3::from_array(n.position());
    if p.z.abs() > 0.0001 || p.x < min.x || p.x > max.x || p.y < min.y || p.y > max.y {
        return Err(ContentError::domain(format!(
            "{} is outside the 2D playfield",
            n.id
        )));
    }
    Ok(p.truncate())
}
fn schema() -> Schema {
    let mut s = Schema::new();
    s.register(
        "waypoint",
        &[FieldRule::required("order", ValueKind::Integer).integer_range(0, 63)],
    )
    .unwrap();
    s.register(
        "plot",
        &[
            FieldRule::required("index", ValueKind::Integer).integer_range(0, 7),
            FieldRule::required("priority", ValueKind::Integer).integer_range(0, 7),
            FieldRule::required("upgrade_priority", ValueKind::Integer).integer_range(0, 7),
            FieldRule::required("kind", ValueKind::Text),
        ],
    )
    .unwrap();
    s.register(
        "wave",
        &[
            FieldRule::required("order", ValueKind::Integer).integer_range(0, 15),
            FieldRule::required("count", ValueKind::Integer).integer_range(1, 128),
            FieldRule::required("hp", ValueKind::Number).range(1., 10000.),
            FieldRule::required("fast_factor", ValueKind::Number).range(0.05, 4.),
            FieldRule::required("speed", ValueKind::Number).range(1., 500.),
            FieldRule::required("fast_speed", ValueKind::Number).range(1., 500.),
            FieldRule::required("interval", ValueKind::Number).range(0.05, 10.),
            FieldRule::required("fast_every", ValueKind::Integer).integer_range(1, 128),
            FieldRule::required("reward", ValueKind::Integer).integer_range(0, 1000),
        ],
    )
    .unwrap();
    s.register(
        "economy",
        &[
            FieldRule::required("money", ValueKind::Integer).integer_range(0, 100000),
            FieldRule::required("lives", ValueKind::Integer).integer_range(1, 100),
            FieldRule::required("kill_reward", ValueKind::Integer).integer_range(0, 1000),
        ],
    )
    .unwrap();
    s
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
            "{what} need unique contiguous zero-based indices"
        )));
    }
    Ok(values.into_iter().map(|(_, v)| v).collect())
}
impl Level {
    pub fn parse(source: &str) -> Result<Self, ContentError> {
        let data = Document::load(source, &schema())?;
        Self::from_data(&data)
    }
    fn from_data(data: &SceneData) -> Result<Self, ContentError> {
        let path = ordered(
            data.of_kind("waypoint")
                .map(|(n, c)| {
                    Ok((
                        int(c, "order") as usize,
                        position(n, vec2(-100., 80.), vec2(1300., 650.))?,
                    ))
                })
                .collect::<Result<_, ContentError>>()?,
            2,
            64,
            "waypoints",
        )?;
        if path.windows(2).any(|p| p[0].distance(p[1]) < 1.) {
            return Err(ContentError::domain(
                "path segments must be at least one pixel",
            ));
        }
        let plots = ordered(
            data.of_kind("plot")
                .map(|(n, c)| {
                    let kind = match field(c, "kind").text().unwrap() {
                        "seed" => Kind::Seed,
                        "frost" => Kind::Frost,
                        _ => return Err(ContentError::domain("plot kind must be seed or frost")),
                    };
                    Ok((
                        int(c, "index") as usize,
                        (
                            position(n, vec2(25., 100.), vec2(1175., 630.))?,
                            int(c, "priority") as usize,
                            kind,
                            int(c, "upgrade_priority") as usize,
                        ),
                    ))
                })
                .collect::<Result<Vec<_>, ContentError>>()?,
            1,
            8,
            "plots",
        )?;
        let autoplay = ordered(
            plots
                .iter()
                .enumerate()
                .map(|(index, (_, priority, kind, _))| (*priority, (index, *kind)))
                .collect(),
            1,
            8,
            "plot priorities",
        )?;
        let upgrade_order = ordered(
            plots
                .iter()
                .enumerate()
                .map(|(index, (_, _, _, priority))| (*priority, index))
                .collect(),
            1,
            8,
            "upgrade priorities",
        )?;
        let waves = ordered(
            data.of_kind("wave")
                .map(|(_, c)| {
                    (
                        int(c, "order") as usize,
                        Wave {
                            count: int(c, "count") as u8,
                            hp: number(c, "hp"),
                            fast_factor: number(c, "fast_factor"),
                            speed: number(c, "speed"),
                            fast_speed: number(c, "fast_speed"),
                            interval: number(c, "interval"),
                            fast_every: int(c, "fast_every") as u32,
                            reward: int(c, "reward") as u32,
                        },
                    )
                })
                .collect(),
            1,
            16,
            "waves",
        )?;
        let economy: Vec<_> = data.of_kind("economy").collect();
        if economy.len() != 1 {
            return Err(ContentError::domain("expected one economy component"));
        }
        let c = economy[0].1;
        let wave_hint =
            "Between waves: build, upgrade, then send the next wave. Reopening restores this checkpoint."
                .into();
        Ok(Self {
            wave_hint,
            path,
            plots: plots.into_iter().map(|(p, _, _, _)| p).collect(),
            autoplay,
            upgrade_order,
            waves,
            money: int(c, "money") as u32,
            lives: int(c, "lives") as u8,
            kill_reward: int(c, "kill_reward") as u32,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_content_and_edited_waves_use_validated_data() {
        let l = Level::parse(SOURCE).unwrap();
        assert_eq!((l.path.len(), l.plots.len(), l.waves.len()), (10, 8, 4));
        let edited = SOURCE.replace("count int 10;", "count int 3;");
        assert_eq!(Level::parse(&edited).unwrap().waves[0].count, 3);
        assert!(Level::parse(&SOURCE.replace("kind text seed;", "kind text laser;")).is_err());
        assert!(Level::parse(&SOURCE.replace("count int 10;", "count int 999;")).is_err());
    }
}
