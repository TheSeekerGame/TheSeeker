//! Plain quad buffers. Callers supply geometry, UV orientation, and linear vertex colors.
use bevy::asset::RenderAssetUsages;
use crate::prelude::*;
use bevy::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;

#[derive(Default)]
pub struct QuadMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl QuadMesh {
    pub fn with_capacity(quads: usize) -> Self {
        Self {
            positions: Vec::with_capacity(quads * 4),
            normals: Vec::with_capacity(quads * 4),
            uvs: Vec::with_capacity(quads * 4),
            colors: Vec::with_capacity(quads * 4),
            indices: Vec::with_capacity(quads * 6),
        }
    }

    pub fn quad(&mut self, positions: [[f32; 3]; 4], uvs: [[f32; 2]; 4]) {
        let base = self.positions.len() as u32;
        self.positions.extend(positions);
        self.normals.extend([[0.0, 0.0, 1.0]; 4]);
        self.uvs.extend(uvs);
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn rect(&mut self, min: Vec2, max: Vec2, color: [f32; 4]) {
        self.quad(
            [[min.x, min.y, 0.0], [max.x, min.y, 0.0],
             [max.x, max.y, 0.0], [min.x, max.y, 0.0]],
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        );
        self.colors.extend([color; 4]);
    }

    pub fn push_pixel(&mut self, min: IVec2, color: [f32; 4]) {
        self.rect(min.as_vec2(), (min + IVec2::ONE).as_vec2(), color);
    }

    pub fn line(&mut self, start: Vec2, end: Vec2, width: f32) {
        let dir = end - start;
        let perp = Vec2::new(-dir.y, dir.x).normalize_or_zero() * (width * 0.5);
        self.quad(
            [(start + perp).extend(0.0).to_array(), (end + perp).extend(0.0).to_array(),
             (end - perp).extend(0.0).to_array(), (start - perp).extend(0.0).to_array()],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        );
    }

    pub fn finish_colored(mut self) -> Mesh {
        let colors = std::mem::take(&mut self.colors);
        let mut mesh = self.finish();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh
    }

    pub fn finish(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        self.write(&mut mesh);
        mesh
    }

    pub fn write(self, mesh: &mut Mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
    }
}

/// Empty geometry has no mesh handle, allowing Bevy to retire the GPU allocation.
/// Nonempty updates reuse the current asset until the effect becomes empty again.
pub fn publish_mesh(meshes: &mut Assets<Mesh>, handle: &mut Handle<Mesh>, mesh: Mesh) {
    if mesh.count_vertices() == 0 {
        *handle = Handle::default();
        return;
    }
    if let Some(mut current) = meshes.get_mut(&*handle) {
        *current = mesh;
        return;
    }
    *handle = meshes.add(mesh);
}

pub fn add_mesh(meshes: &mut Assets<Mesh>, mesh: Mesh) -> Handle<Mesh> {
    let mut handle = Handle::default();
    publish_mesh(meshes, &mut handle, mesh);
    handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_geometry_releases_its_handle_and_can_be_repopulated() {
        let mut meshes = Assets::<Mesh>::default();
        let mut handle = add_mesh(&mut meshes, QuadMesh::default().finish());
        assert_eq!(handle, Handle::default());
        for _ in 0..2 {
            let mut quads = QuadMesh::default();
            quads.rect(Vec2::ZERO, Vec2::ONE, [1.0; 4]);
            publish_mesh(&mut meshes, &mut handle, quads.finish_colored());
            assert_eq!(meshes.get(&handle).unwrap().count_vertices(), 4);
            publish_mesh(&mut meshes, &mut handle, QuadMesh::default().finish());
            assert_eq!(handle, Handle::default());
        }
    }
}
