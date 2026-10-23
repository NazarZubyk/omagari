//! Meshes of shape layers ([`ShapeDef`]): positions in metres at scale 1,
//! centred on the origin with Y up, normals, and UVs (`u` around / along,
//! `v` up / across, both 0..1). Closed shapes are drawn from outside (back
//! faces are culled by the effect material); flat shapes (ring, quad, arc)
//! and open tubes get a second, flipped copy so they show from both sides.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::format::{arc_half_width, arc_point, ShapeDef};

/// Segments around round shapes.
pub const SEGMENTS: u32 = 48;
/// Rings from the bottom to the top of a sphere.
pub const SPHERE_RINGS: u32 = 24;
/// Rings per half-sphere of a capsule.
pub const CAPSULE_CAP_RINGS: u32 = 12;
/// Segments around a torus's tube.
pub const TORUS_TUBE_SEGMENTS: u32 = 16;
/// Segments around a flat ring.
pub const RING_SEGMENTS: u32 = 64;

/// Vertices and triangles being built. Triangles are wound so their face
/// normal agrees with the vertex normals (front faces are counter-clockwise
/// seen from where the normals point).
#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Builder {
    fn vertex(&mut self, p: Vec3, n: Vec3, uv: Vec2) -> u32 {
        self.positions.push(p.to_array());
        self.normals.push(n.normalize_or(Vec3::Y).to_array());
        self.uvs.push(uv.to_array());
        (self.positions.len() - 1) as u32
    }

    /// One triangle, wound to face its vertices' normals. Degenerate
    /// triangles (a pole, a pointed end) are kept: they draw nothing.
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let p = |i: u32| Vec3::from_array(self.positions[i as usize]);
        let n = |i: u32| Vec3::from_array(self.normals[i as usize]);
        let face = (p(b) - p(a)).cross(p(c) - p(a));
        if face.dot(n(a) + n(b) + n(c)) < 0.0 {
            self.indices.extend([a, c, b]);
        } else {
            self.indices.extend([a, b, c]);
        }
    }

    /// A grid of `columns + 1` × `rows + 1` vertices from `vertex(i, j)`
    /// (indices: column-major rows), two triangles per cell.
    fn grid(&mut self, columns: u32, rows: u32, mut vertex: impl FnMut(u32, u32) -> (Vec3, Vec3, Vec2)) {
        let first = self.positions.len() as u32;
        for j in 0..=rows {
            for i in 0..=columns {
                let (p, n, uv) = vertex(i, j);
                self.vertex(p, n, uv);
            }
        }
        let at = |i: u32, j: u32| first + j * (columns + 1) + i;
        for j in 0..rows {
            for i in 0..columns {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                self.triangle(a, b, c);
                self.triangle(a, c, d);
            }
        }
    }

    /// A surface of revolution around Y: `profile` = (radius, y, normal in
    /// the (radius, y) plane, v) from bottom to top; `u` runs around.
    fn revolve(&mut self, segments: u32, profile: &[(f32, f32, Vec2, f32)]) {
        let rows = profile.len() as u32 - 1;
        self.grid(segments, rows, |i, j| {
            let u = i as f32 / segments as f32;
            let (s, c) = (u * TAU).sin_cos();
            let (r, y, n, v) = profile[j as usize];
            let dir = Vec3::new(c, 0.0, -s);
            (dir * r + Vec3::Y * y, dir * n.x + Vec3::Y * n.y, Vec2::new(u, v))
        });
    }

    /// A flat disc cap at height `y` facing `up` (±1), UVs planar.
    fn disc(&mut self, segments: u32, radius: f32, y: f32, up: f32) {
        let normal = Vec3::Y * up;
        let centre = self.vertex(Vec3::Y * y, normal, Vec2::splat(0.5));
        let first = self.positions.len() as u32;
        for i in 0..=segments {
            let (s, c) = (i as f32 / segments as f32 * TAU).sin_cos();
            let p = Vec3::new(c * radius, y, -s * radius);
            self.vertex(p, normal, Vec2::new(c * 0.5 + 0.5, -s * 0.5 + 0.5));
        }
        for i in 0..segments {
            self.triangle(centre, first + i, first + i + 1);
        }
    }

    /// Adds a flipped copy of the triangles from vertex `from` on (the back
    /// side of flat shapes and open tubes).
    fn add_back_faces(&mut self, from: u32) {
        let count = self.positions.len() as u32;
        let offset = count - from;
        for v in from..count {
            let i = v as usize;
            let n = Vec3::from_array(self.normals[i]);
            self.positions.push(self.positions[i]);
            self.normals.push((-n).to_array());
            self.uvs.push(self.uvs[i]);
        }
        let triangles: Vec<[u32; 3]> = self
            .indices
            .chunks(3)
            .filter(|t| t.iter().all(|&i| i >= from && i < count))
            .map(|t| [t[0], t[1], t[2]])
            .collect();
        for [a, b, c] in triangles {
            self.indices.extend([a + offset, c + offset, b + offset]);
        }
    }

    fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// The mesh of a shape (dimensions below 0 are treated as 0).
pub fn shape_mesh(shape: &ShapeDef) -> Mesh {
    let mut b = Builder::default();
    let pos = |x: f32| x.max(0.0);
    match *shape {
        ShapeDef::Sphere { radius } => {
            let r = pos(radius);
            let profile: Vec<_> = (0..=SPHERE_RINGS)
                .map(|j| {
                    let v = j as f32 / SPHERE_RINGS as f32;
                    let phi = v * PI - PI * 0.5;
                    let (s, c) = phi.sin_cos();
                    (r * c, r * s, Vec2::new(c, s), v)
                })
                .collect();
            b.revolve(SEGMENTS, &profile);
        }
        ShapeDef::Capsule { radius, length } => {
            let (r, half) = (pos(radius), pos(length) * 0.5);
            let total = (2.0 * (half + r)).max(1e-6);
            let mut profile = Vec::new();
            for (centre, from, to) in [(-half, -PI * 0.5, 0.0), (half, 0.0, PI * 0.5)] {
                for j in 0..=CAPSULE_CAP_RINGS {
                    let phi = from + (to - from) * j as f32 / CAPSULE_CAP_RINGS as f32;
                    let (s, c) = phi.sin_cos();
                    let y = centre + r * s;
                    profile.push((r * c, y, Vec2::new(c, s), (y + half + r) / total));
                }
            }
            b.revolve(SEGMENTS, &profile);
        }
        ShapeDef::Cylinder { radius, height, open_ends } => {
            let (r, h) = (pos(radius), pos(height) * 0.5);
            b.revolve(SEGMENTS, &[(r, -h, Vec2::X, 0.0), (r, h, Vec2::X, 1.0)]);
            if open_ends {
                b.add_back_faces(0);
            } else {
                b.disc(SEGMENTS, r, h, 1.0);
                b.disc(SEGMENTS, r, -h, -1.0);
            }
        }
        ShapeDef::Cone { radius, height } => {
            let (r, h) = (pos(radius), pos(height));
            let n = Vec2::new(h, r).normalize_or(Vec2::X);
            b.revolve(SEGMENTS, &[(r, -h * 0.5, n, 0.0), (0.0, h * 0.5, n, 1.0)]);
            b.disc(SEGMENTS, r, -h * 0.5, -1.0);
        }
        ShapeDef::Torus { major, minor } => {
            let (big, small) = (pos(major), pos(minor));
            let profile: Vec<_> = (0..=TORUS_TUBE_SEGMENTS)
                .map(|j| {
                    let v = j as f32 / TORUS_TUBE_SEGMENTS as f32;
                    let (s, c) = (v * TAU - PI).sin_cos();
                    (big + small * c, small * s, Vec2::new(c, s), v)
                })
                .collect();
            b.revolve(SEGMENTS, &profile);
        }
        ShapeDef::Ring { inner, outer } => {
            let (a, o) = (pos(inner.min(outer)), pos(outer.max(inner)));
            b.grid(RING_SEGMENTS, 1, |i, j| {
                let u = i as f32 / RING_SEGMENTS as f32;
                let (s, c) = (u * TAU).sin_cos();
                let r = if j == 0 { a } else { o };
                (Vec3::new(c * r, 0.0, -s * r), Vec3::Y, Vec2::new(u, j as f32))
            });
            b.add_back_faces(0);
        }
        ShapeDef::Quad { size, .. } => {
            let (w, d) = (pos(size.x), pos(size.y));
            b.grid(1, 1, |i, j| {
                let (u, v) = (i as f32, j as f32);
                (Vec3::new((u - 0.5) * w, 0.0, (v - 0.5) * d), Vec3::Y, Vec2::new(u, v))
            });
            b.add_back_faces(0);
        }
        ShapeDef::Arc { radius, width, angle_deg, taper } => {
            let segments = ((angle_deg.abs() / 4.0).ceil() as u32).clamp(8, 128);
            let r = pos(radius);
            b.grid(segments, 1, |i, j| {
                let u = i as f32 / segments as f32;
                let half = arc_half_width(u, width, taper);
                let rr = if j == 0 { (r - half).max(0.0) } else { r + half };
                (arc_point(u, angle_deg) * rr, Vec3::Y, Vec2::new(u, j as f32))
            });
            b.add_back_faces(0);
        }
    }
    b.build()
}

/// The vertex count of a shape's mesh (for tests and the docs).
pub fn shape_vertex_count(shape: &ShapeDef) -> usize {
    shape_mesh(shape).count_vertices()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use crate::format::QuadFacingDef;

    fn attr3(mesh: &Mesh, a: bevy::mesh::MeshVertexAttribute) -> Vec<Vec3> {
        match mesh.attribute(a) {
            Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|p| Vec3::from_array(*p)).collect(),
            other => panic!("{:?}", other.map(|o| o.len())),
        }
    }

    fn uvs(mesh: &Mesh) -> Vec<Vec2> {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(v)) => v.iter().map(|p| Vec2::from_array(*p)).collect(),
            _ => panic!("no uvs"),
        }
    }

    fn indices(mesh: &Mesh) -> Vec<u32> {
        match mesh.indices() {
            Some(Indices::U32(i)) => i.clone(),
            _ => panic!("no u32 indices"),
        }
    }

    /// Unit normals; every non-degenerate triangle faces its normals;
    /// UVs in 0..1; bounds as the definition says.
    fn check(shape: ShapeDef) -> Mesh {
        let mesh = shape_mesh(&shape);
        let positions = attr3(&mesh, Mesh::ATTRIBUTE_POSITION);
        let normals = attr3(&mesh, Mesh::ATTRIBUTE_NORMAL);
        assert_eq!(positions.len(), normals.len());
        for n in normals.iter() {
            assert!((n.length() - 1.0).abs() < 1e-4, "{:?}: normal {}", shape, n);
        }
        for uv in uvs(&mesh) {
            assert!((-1e-5..=1.0 + 1e-5).contains(&uv.x) && (-1e-5..=1.0 + 1e-5).contains(&uv.y), "{:?}: uv {}", shape, uv);
        }
        let idx = indices(&mesh);
        assert_eq!(idx.len() % 3, 0);
        for t in idx.chunks(3) {
            let (a, b, c) = (positions[t[0] as usize], positions[t[1] as usize], positions[t[2] as usize]);
            let face = (b - a).cross(c - a);
            if face.length() < 1e-7 {
                continue;
            }
            let n = normals[t[0] as usize] + normals[t[1] as usize] + normals[t[2] as usize];
            assert!(face.dot(n) > 0.0, "{:?}: triangle {:?} faces away from its normals", shape, t);
        }
        let (lo, hi) = shape.bounds();
        let (mlo, mhi) = positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
        // The mesh samples curves coarser than the bounds do.
        assert!((mlo - lo).abs().max_element() < 0.02 && (mhi - hi).abs().max_element() < 0.02, "{:?}: mesh {} {} vs {} {}", shape, mlo, mhi, lo, hi);
        mesh
    }

    #[test]
    fn closed_shapes_have_outward_normals_and_expected_counts() {
        let s = SEGMENTS as usize + 1;
        let mesh = check(ShapeDef::Sphere { radius: 1.5 });
        assert_eq!(mesh.count_vertices(), s * (SPHERE_RINGS as usize + 1));
        for (p, n) in attr3(&mesh, Mesh::ATTRIBUTE_POSITION).iter().zip(attr3(&mesh, Mesh::ATTRIBUTE_NORMAL)) {
            assert!((p.length() - 1.5).abs() < 1e-4);
            assert!(p.normalize().dot(n) > 0.999, "sphere normals point outward");
        }
        let mesh = check(ShapeDef::Capsule { radius: 0.3, length: 1.0 });
        assert_eq!(mesh.count_vertices(), s * 2 * (CAPSULE_CAP_RINGS as usize + 1));
        let mesh = check(ShapeDef::Cylinder { radius: 0.5, height: 2.0, open_ends: false });
        assert_eq!(mesh.count_vertices(), s * 2 + 2 * (s + 1), "side + two caps");
        let mesh = check(ShapeDef::Cone { radius: 0.5, height: 1.0 });
        assert_eq!(mesh.count_vertices(), s * 2 + (s + 1));
        let mesh = check(ShapeDef::Torus { major: 1.0, minor: 0.2 });
        assert_eq!(mesh.count_vertices(), s * (TORUS_TUBE_SEGMENTS as usize + 1));
    }

    #[test]
    fn flat_shapes_lie_in_xz_and_show_both_sides() {
        for shape in [
            ShapeDef::Ring { inner: 0.5, outer: 1.0 },
            ShapeDef::Quad { size: Vec2::new(2.0, 1.0), facing: QuadFacingDef::Fixed },
            ShapeDef::Arc { radius: 1.0, width: 0.4, angle_deg: 120.0, taper: 0.0 },
            ShapeDef::Arc { radius: 1.0, width: 0.4, angle_deg: 200.0, taper: 1.0 },
        ] {
            let mesh = check(shape.clone());
            let positions = attr3(&mesh, Mesh::ATTRIBUTE_POSITION);
            assert!(positions.iter().all(|p| p.y == 0.0), "{:?} is flat in XZ", shape);
            let normals = attr3(&mesh, Mesh::ATTRIBUTE_NORMAL);
            let up = normals.iter().filter(|n| n.y > 0.99).count();
            let down = normals.iter().filter(|n| n.y < -0.99).count();
            assert_eq!(up, down, "{:?}: a back copy", shape);
            assert_eq!(up + down, normals.len());
        }
        // Ring: v from the inner (0) to the outer edge (1).
        let mesh = shape_mesh(&ShapeDef::Ring { inner: 0.5, outer: 1.0 });
        assert_eq!(mesh.count_vertices(), 2 * 2 * (RING_SEGMENTS as usize + 1));
        for (p, uv) in attr3(&mesh, Mesh::ATTRIBUTE_POSITION).iter().zip(uvs(&mesh)) {
            let r = p.length();
            assert!((uv.y == 0.0 && (r - 0.5).abs() < 1e-5) || (uv.y == 1.0 && (r - 1.0).abs() < 1e-5));
        }
        // Arc: u along the arc from the left end, the middle in front (-Z),
        // pointed ends with taper 1.
        let mesh = shape_mesh(&ShapeDef::Arc { radius: 1.0, width: 0.4, angle_deg: 90.0, taper: 1.0 });
        for (p, uv) in attr3(&mesh, Mesh::ATTRIBUTE_POSITION).iter().zip(uvs(&mesh)) {
            if uv.x == 0.0 {
                assert!(p.x < 0.0 && (p.length() - 1.0).abs() < 1e-5, "left end, pointed: {}", p);
            }
            if (uv.x - 0.5).abs() < 1e-6 {
                assert!(p.x.abs() < 1e-5 && p.z < 0.0, "middle in front: {}", p);
                assert!((p.length() - if uv.y == 0.0 { 0.8 } else { 1.2 }).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn open_tubes_are_drawn_inside_and_out() {
        let open = check(ShapeDef::Cylinder { radius: 0.5, height: 1.0, open_ends: true });
        assert_eq!(open.count_vertices(), 2 * 2 * (SEGMENTS as usize + 1));
        let normals = attr3(&open, Mesh::ATTRIBUTE_NORMAL);
        let positions = attr3(&open, Mesh::ATTRIBUTE_POSITION);
        let inward = positions.iter().zip(normals.iter()).filter(|(p, n)| Vec3::new(p.x, 0.0, p.z).dot(**n) < 0.0).count();
        assert_eq!(inward * 2, normals.len());
    }
}
