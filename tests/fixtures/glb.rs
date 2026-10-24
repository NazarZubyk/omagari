//! A tiny generated glTF binary (`.glb`) for mesh-asset tests: no files from
//! the game, nothing to download. Included with `#[path]` by the library's
//! integration tests and the editor's unit tests.
//!
//! - Mesh 0 `"Rock"`: one primitive, an octahedron 0.4 m across (24
//!   vertices, flat normals, UVs), node `"Rock"`.
//! - Mesh 1 `"Bare"`: one triangle with positions only (no normals, no
//!   UVs), node `"Bare"`.

#![allow(dead_code)]

/// Bytes of a little-endian f32 list.
fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// The `.glb` file.
pub fn rock_glb() -> Vec<u8> {
    // Octahedron: 8 faces, 3 vertices each, flat normals.
    let r = 0.2f32;
    let tips = [[0.0, r, 0.0], [0.0, -r, 0.0]];
    let ring = [[r, 0.0, 0.0], [0.0, 0.0, -r], [-r, 0.0, 0.0], [0.0, 0.0, r]];
    let (mut positions, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
    for tip in tips {
        for i in 0..4 {
            let (a, b) = (ring[i], ring[(i + 1) % 4]);
            // Outward winding: counter-clockwise seen from outside.
            let corners = if tip[1] > 0.0 { [tip, b, a] } else { [tip, a, b] };
            let e1 = [corners[1][0] - corners[0][0], corners[1][1] - corners[0][1], corners[1][2] - corners[0][2]];
            let e2 = [corners[2][0] - corners[0][0], corners[2][1] - corners[0][1], corners[2][2] - corners[0][2]];
            let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            for (c, uv) in corners.iter().zip([[0.5, 0.0], [0.0, 1.0], [1.0, 1.0]]) {
                positions.extend_from_slice(c);
                normals.extend_from_slice(&[n[0] / len, n[1] / len, n[2] / len]);
                uvs.extend_from_slice(&uv);
            }
        }
    }
    let bare = [0.0f32, 0.0, 0.0, 0.3, 0.0, 0.0, 0.0, 0.3, 0.0];

    let mut bin = Vec::new();
    let mut views = Vec::new();
    for data in [floats(&positions), floats(&normals), floats(&uvs), floats(&bare)] {
        views.push(format!(r#"{{"buffer":0,"byteOffset":{},"byteLength":{}}}"#, bin.len(), data.len()));
        bin.extend(data);
    }
    let json = format!(
        concat!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0,1]}}],"#,
            r#""nodes":[{{"name":"Rock","mesh":0}},{{"name":"Bare","mesh":1,"translation":[1.0,0.0,0.0]}}],"#,
            r#""meshes":[{{"name":"Rock","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}}}}]}},"#,
            r#"{{"name":"Bare","primitives":[{{"attributes":{{"POSITION":3}}}}]}}],"#,
            r#""accessors":["#,
            r#"{{"bufferView":0,"componentType":5126,"count":24,"type":"VEC3","min":[-{r},-{r},-{r}],"max":[{r},{r},{r}]}},"#,
            r#"{{"bufferView":1,"componentType":5126,"count":24,"type":"VEC3"}},"#,
            r#"{{"bufferView":2,"componentType":5126,"count":24,"type":"VEC2"}},"#,
            r#"{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC3","min":[0.0,0.0,0.0],"max":[0.3,0.3,0.0]}}],"#,
            r#""bufferViews":[{views}],"buffers":[{{"byteLength":{len}}}]}}"#
        ),
        r = r,
        views = views.join(","),
        len = bin.len()
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend(json);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend(bin);
    out
}

/// Writes [`rock_glb`] to `path` (creating its folder).
pub fn write_rock_glb(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, rock_glb()).unwrap();
}
