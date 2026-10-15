use crate::{Color, Mesh, Vec3, Vertex, vec3};

#[derive(Clone, Copy, Debug)]
pub struct ObjLimits {
    pub max_bytes: usize,
    pub max_positions: usize,
    pub max_faces: usize,
    pub max_face_vertices: usize,
    pub max_output_vertices: usize,
}
impl Default for ObjLimits {
    fn default() -> Self {
        Self {
            max_bytes: 8 * 1024 * 1024,
            max_positions: 100_000,
            max_faces: 100_000,
            max_face_vertices: 32,
            max_output_vertices: 750_000,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjErrorKind {
    ResourceLimit,
    InvalidRecord,
    InvalidNumber,
    InvalidIndex,
    DegenerateFace,
    NonPlanarFace,
    ConcaveFace,
    Empty,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjError {
    pub line: usize,
    pub kind: ObjErrorKind,
}
impl std::fmt::Display for ObjError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBJ line {}: {:?}", self.line, self.kind)
    }
}
impl std::error::Error for ObjError {}
impl Mesh {
    /// Bounded OBJ subset: positions, UVs/normals for index validation, and convex planar faces.
    /// Output deliberately uses flat generated normals and the supplied solid color.
    pub fn from_obj(source: &str, color: Color) -> Result<Self, ObjError> {
        Self::from_obj_with_limits(source, color, ObjLimits::default())
    }
    pub fn from_obj_with_limits(
        source: &str,
        color: Color,
        limits: ObjLimits,
    ) -> Result<Self, ObjError> {
        if source.len() > limits.max_bytes || !color.to_vec().is_finite() {
            return Err(ObjError {
                line: 0,
                kind: ObjErrorKind::ResourceLimit,
            });
        }
        let mut positions = Vec::new();
        let mut normals = 0;
        let mut uvs = 0;
        let mut faces = 0;
        let mut output = Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        };
        for (number, line) in source.lines().enumerate() {
            let number = number + 1;
            let line = line.split('#').next().unwrap_or("");
            let mut words = line.split_whitespace();
            let Some(record) = words.next() else {
                continue;
            };
            let fail = |kind| ObjError { line: number, kind };
            match record {
                "v" => {
                    if positions.len() >= limits.max_positions {
                        return Err(fail(ObjErrorKind::ResourceLimit));
                    }
                    let values = parse_values(words, 3, 3).map_err(fail)?;
                    positions.push(vec3(values[0], values[1], values[2]));
                }
                "vn" => {
                    parse_values(words, 3, 3).map_err(fail)?;
                    normals += 1;
                    if normals > limits.max_positions {
                        return Err(fail(ObjErrorKind::ResourceLimit));
                    }
                }
                "vt" => {
                    parse_values(words, 1, 3).map_err(fail)?;
                    uvs += 1;
                    if uvs > limits.max_positions {
                        return Err(fail(ObjErrorKind::ResourceLimit));
                    }
                }
                "f" => {
                    faces += 1;
                    if faces > limits.max_faces {
                        return Err(fail(ObjErrorKind::ResourceLimit));
                    }
                    let mut points = Vec::new();
                    for word in words {
                        if points.len() >= limits.max_face_vertices.min(1024) {
                            return Err(fail(ObjErrorKind::ResourceLimit));
                        }
                        let mut fields = word.split('/');
                        let position = parse_index(fields.next().unwrap_or(""), positions.len())
                            .map_err(fail)?;
                        if let Some(uv) = fields.next() {
                            if !uv.is_empty() {
                                parse_index(uv, uvs).map_err(fail)?;
                            }
                            if let Some(normal) = fields.next() {
                                if normal.is_empty() {
                                    return Err(fail(ObjErrorKind::InvalidIndex));
                                }
                                parse_index(normal, normals).map_err(fail)?;
                            } else if uv.is_empty() {
                                return Err(fail(ObjErrorKind::InvalidIndex));
                            }
                        }
                        if fields.next().is_some() {
                            return Err(fail(ObjErrorKind::InvalidIndex));
                        }
                        points.push(positions[position]);
                    }
                    if points.len() < 3 {
                        return Err(fail(ObjErrorKind::InvalidRecord));
                    }
                    let required = (points.len() - 2) * 3;
                    if output.vertices.len().saturating_add(required)
                        > limits.max_output_vertices.min(1_000_000)
                    {
                        return Err(fail(ObjErrorKind::ResourceLimit));
                    }
                    let normal = validate_polygon(&points).map_err(fail)?;
                    for i in 1..points.len() - 1 {
                        let a = points[0];
                        let b = points[i];
                        let c = points[i + 1];
                        if (b - a).cross(c - a).length_squared() <= 1.0e-20 {
                            return Err(fail(ObjErrorKind::DegenerateFace));
                        }
                        let start = output.vertices.len() as u32;
                        output.vertices.extend([
                            Vertex::new(a, normal, color),
                            Vertex::new(b, normal, color),
                            Vertex::new(c, normal, color),
                        ]);
                        output.indices.extend([start, start + 1, start + 2]);
                    }
                }
                // Names/smoothing/material references are accepted as metadata; no paths are opened.
                "o" | "g" | "s" | "usemtl" | "mtllib" => {}
                _ => return Err(fail(ObjErrorKind::InvalidRecord)),
            }
        }
        if output.vertices.is_empty() {
            return Err(ObjError {
                line: 0,
                kind: ObjErrorKind::Empty,
            });
        }
        Ok(output)
    }
}
fn parse_values<'a>(
    words: impl Iterator<Item = &'a str>,
    min: usize,
    max: usize,
) -> Result<Vec<f32>, ObjErrorKind> {
    let mut result = Vec::with_capacity(max);
    for word in words {
        if result.len() >= max {
            return Err(ObjErrorKind::InvalidRecord);
        }
        let value = word
            .parse::<f32>()
            .map_err(|_| ObjErrorKind::InvalidNumber)?;
        if !value.is_finite() {
            return Err(ObjErrorKind::InvalidNumber);
        }
        result.push(value);
    }
    if result.len() < min {
        return Err(ObjErrorKind::InvalidRecord);
    }
    Ok(result)
}
fn parse_index(word: &str, count: usize) -> Result<usize, ObjErrorKind> {
    let raw = word
        .parse::<i64>()
        .map_err(|_| ObjErrorKind::InvalidIndex)?;
    let index = if raw > 0 {
        raw - 1
    } else if raw < 0 {
        (count as i64)
            .checked_add(raw)
            .ok_or(ObjErrorKind::InvalidIndex)?
    } else {
        return Err(ObjErrorKind::InvalidIndex);
    };
    if index < 0 || index as usize >= count {
        Err(ObjErrorKind::InvalidIndex)
    } else {
        Ok(index as usize)
    }
}
fn validate_polygon(points: &[Vec3]) -> Result<Vec3, ObjErrorKind> {
    let origin = points[0];
    let normal = (1..points.len() - 1)
        .find_map(|i| {
            (points[i] - origin)
                .cross(points[i + 1] - origin)
                .try_normalize()
        })
        .ok_or(ObjErrorKind::DegenerateFace)?;
    let scale = points
        .iter()
        .map(|p| p.distance(origin))
        .fold(1.0_f32, f32::max);
    let epsilon = scale * 1.0e-5;
    if !scale.is_finite() {
        return Err(ObjErrorKind::InvalidNumber);
    }
    if points
        .iter()
        .any(|&p| (p - origin).dot(normal).abs() > epsilon)
    {
        return Err(ObjErrorKind::NonPlanarFace);
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        // Every vertex must lie on the interior side of every oriented edge.
        // This also rejects self-intersecting faces with locally consistent turns.
        if points
            .iter()
            .any(|&p| (b - a).cross(p - a).dot(normal) < -epsilon * epsilon)
        {
            return Err(ObjErrorKind::ConcaveFace);
        }
    }
    Ok(normal)
}
