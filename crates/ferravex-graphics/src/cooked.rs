//! Original portable little-endian mesh format; decode validates before cache insertion.
use crate::{Color, Mesh, MeshError, Vec3, Vertex};
impl Mesh {
    pub fn to_cooked(&self) -> Result<Vec<u8>, MeshError> {
        self.validate()?;
        let mut out = b"GMSH0001".to_vec();
        out.extend((self.vertices.len() as u32).to_le_bytes());
        out.extend((self.indices.len() as u32).to_le_bytes());
        for v in &self.vertices {
            for f in v
                .position
                .to_array()
                .into_iter()
                .chain(v.normal.to_array())
                .chain(v.color.to_array())
            {
                out.extend(f.to_le_bytes());
            }
        }
        for i in &self.indices {
            out.extend(i.to_le_bytes());
        }
        Ok(out)
    }
    pub fn from_cooked(bytes: &[u8]) -> Result<Self, MeshError> {
        if bytes.len() < 16 || &bytes[..8] != b"GMSH0001" {
            return Err(MeshError::InvalidTriangleCount);
        }
        let vertices = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let indices = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        if vertices > 1_000_000 || indices > 6_000_000 {
            return Err(MeshError::TooLarge);
        }
        if bytes.len() != 16 + vertices * 40 + indices * 4 {
            return Err(MeshError::InvalidTriangleCount);
        }
        let mut result = Mesh {
            vertices: Vec::with_capacity(vertices),
            indices: Vec::with_capacity(indices),
        };
        for row in bytes[16..16 + vertices * 40].chunks_exact(40) {
            let f = |i: usize| f32::from_le_bytes(row[i * 4..i * 4 + 4].try_into().unwrap());
            result.vertices.push(Vertex::new(
                Vec3::new(f(0), f(1), f(2)),
                Vec3::new(f(3), f(4), f(5)),
                Color::new(f(6), f(7), f(8), f(9)),
            ));
        }
        for row in bytes[16 + vertices * 40..].chunks_exact(4) {
            result
                .indices
                .push(u32::from_le_bytes(row.try_into().unwrap()));
        }
        result.validate()?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cooked_mesh_roundtrip_rejects_truncation_bad_counts_indices_and_floats() {
        let mesh = Mesh::cuboid(Vec3::ONE, Color::from_hex(0xcabb99));
        let bytes = mesh.to_cooked().unwrap();
        assert_eq!(Mesh::from_cooked(&bytes).unwrap(), mesh);
        for end in 0..bytes.len() {
            assert!(Mesh::from_cooked(&bytes[..end]).is_err());
        }
        let mut bad = bytes.clone();
        bad[16..20].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(Mesh::from_cooked(&bad), Err(MeshError::NonFiniteVertex));
        let mut bad = bytes.clone();
        let last = bad.len() - 4;
        bad[last..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Mesh::from_cooked(&bad), Err(MeshError::IndexOutOfBounds));
        let mut bad = bytes;
        bad[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Mesh::from_cooked(&bad), Err(MeshError::TooLarge));
    }
}
