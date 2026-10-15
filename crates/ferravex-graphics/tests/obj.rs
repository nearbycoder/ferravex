use ferravex_graphics::*;
const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
const QUAD: &str = "v -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nf 1 2 3 4\n";
#[test]
fn imports_convex_face_with_flat_normals() {
    let mesh = Mesh::from_obj(QUAD, WHITE).unwrap();
    assert_eq!(mesh.triangle_count(), 2);
    assert!(mesh.vertices.iter().all(|v| v.normal == Vec3::Z));
    mesh.validate().unwrap();
}
#[test]
fn supports_relative_and_slash_indices() {
    let obj =
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\nf -3/1/1 -2/1/1 -1/1/1\nf 1//1 2//1 3//1\n";
    assert_eq!(Mesh::from_obj(obj, WHITE).unwrap().triangle_count(), 2);
}
#[test]
fn rejects_invalid_nonfinite_and_out_of_range_data() {
    for obj in [
        "v NaN 0 0",
        "v 0 0 0\nf 0 1 1",
        "v 0 0 0\nf 1 2 3",
        "v 0 0 0\nf 1/99 1/99 1/99",
        "curv 1 2 3",
    ] {
        assert!(Mesh::from_obj(obj, WHITE).is_err(), "accepted {obj}");
    }
}
#[test]
fn enforces_limits_before_output_growth() {
    let limits = ObjLimits {
        max_output_vertices: 3,
        ..ObjLimits::default()
    };
    assert_eq!(
        Mesh::from_obj_with_limits(QUAD, WHITE, limits)
            .unwrap_err()
            .kind,
        ObjErrorKind::ResourceLimit
    );
    let limits = ObjLimits {
        max_bytes: 4,
        ..ObjLimits::default()
    };
    assert!(Mesh::from_obj_with_limits(QUAD, WHITE, limits).is_err());
}
#[test]
fn rejects_nonplanar_and_concave_polygons() {
    let nonplanar = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 1\nf 1 2 3 4";
    assert_eq!(
        Mesh::from_obj(nonplanar, WHITE).unwrap_err().kind,
        ObjErrorKind::NonPlanarFace
    );
    let concave = "v 0 0 0\nv 2 0 0\nv 1 0.5 0\nv 2 2 0\nv 0 2 0\nf 1 2 3 4 5";
    assert_eq!(
        Mesh::from_obj(concave, WHITE).unwrap_err().kind,
        ObjErrorKind::ConcaveFace
    );
}
