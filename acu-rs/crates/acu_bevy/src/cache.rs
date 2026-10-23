//! Whole-map dump: the resolved scene written once, read back without touching Forge.
//!
//! Decoding 8,330 cells takes tens of minutes; the result is pure data. This caches that
//! result so a later start is a file read. Versioned: a layout change bumps `VERSION` and the
//! reader refuses the file rather than misreading it.

use std::path::Path;

use crate::load::{
    Loaded, LoadedMaterial, LoadedPart, LoadedPiece, LoadedTexture, MapInstance, Scene,
};

const MAGIC: &[u8; 8] = b"ACUCACH1";

pub fn write(path: &Path, loaded: &Loaded) -> Result<(), String> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, 2);
    put_u32(&mut out, loaded.rig_pieces as u32);

    put_u64(&mut out, loaded.pieces.len() as u64);
    for piece in &loaded.pieces {
        put_str(&mut out, &piece.label);
        put_str(&mut out, &piece.file_id);
        put_u64(&mut out, piece.vertices.len() as u64);
        for vertex in &piece.vertices {
            for value in vertex.position {
                put_f32(&mut out, value);
            }
            for value in vertex.normal {
                put_f32(&mut out, value);
            }
            let mut flags = 0u8;
            if vertex.uv0.is_some() {
                flags |= 1;
            }
            if vertex.tangent.is_some() {
                flags |= 2;
            }
            if vertex.binormal.is_some() {
                flags |= 4;
            }
            out.push(flags);
            for value in vertex.uv0.unwrap_or([0.0, 0.0]) {
                put_f32(&mut out, value);
            }
            for value in vertex.tangent.unwrap_or([1.0, 0.0, 0.0]) {
                put_f32(&mut out, value);
            }
            for value in vertex.binormal.unwrap_or([0.0, 1.0, 0.0]) {
                put_f32(&mut out, value);
            }
        }
        put_u64(&mut out, piece.parts.len() as u64);
        for part in &piece.parts {
            put_u64(&mut out, part.vertex_base as u64);
            put_u64(&mut out, part.vertex_count as u64);
            put_i32(
                &mut out,
                part.material.map(|index| index as i32).unwrap_or(-1),
            );
            put_u64(&mut out, part.triangles.len() as u64);
            for triangle in &part.triangles {
                for index in triangle {
                    put_u32(&mut out, *index);
                }
            }
        }
    }

    put_u64(&mut out, loaded.instances.len() as u64);
    for instance in &loaded.instances {
        put_u32(&mut out, instance.piece as u32);
        for value in instance.matrix {
            put_f32(&mut out, value);
        }
    }

    put_u64(&mut out, loaded.scene.materials.len() as u64);
    for material in &loaded.scene.materials {
        put_str(&mut out, &material.file_id);
        put_i32(
            &mut out,
            material
                .diffuse_texture
                .map(|index| index as i32)
                .unwrap_or(-1),
        );
        out.push(u8::from(material.alpha_test_enabled));
        out.push(material.alpha_test_value);
        put_str(&mut out, &material.source);
    }

    put_u64(&mut out, loaded.scene.textures.len() as u64);
    for texture in &loaded.scene.textures {
        put_str(&mut out, &texture.file_id);
        put_u32(&mut out, texture.dxgi_format);
        put_u32(&mut out, texture.gamma);
        put_u32(&mut out, texture.width);
        put_u32(&mut out, texture.height);
        put_u32(&mut out, texture.mip_count);
        out.push(u8::from(texture.srgb));
        put_u64(&mut out, texture.pixels.len() as u64);
        out.extend_from_slice(&texture.pixels);
    }

    std::fs::write(path, &out)
        .map_err(|error| format!("cache write {}: {error}", path.display()))?;
    Ok(())
}

pub fn read(path: &Path) -> Result<Loaded, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("cache read {}: {error}", path.display()))?;
    let mut cursor = Cursor {
        bytes: &bytes,
        at: 0,
    };
    if cursor.take(8) != MAGIC {
        return Err("cache: not an ACU map cache".into());
    }
    let version = cursor.u32()?;
    if version != 2 {
        return Err(format!("cache: version {version} not supported"));
    }

    let rig_pieces = cursor.u32()? as usize;
    let piece_count = cursor.u64()? as usize;
    let mut pieces = Vec::with_capacity(piece_count);
    for _ in 0..piece_count {
        let label = cursor.string()?;
        let file_id = cursor.string()?;
        let vertex_count = cursor.u64()? as usize;
        let mut vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            let position = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let normal = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let flags = cursor.byte()?;
            let uv = [cursor.f32()?, cursor.f32()?];
            let tangent = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            let binormal = [cursor.f32()?, cursor.f32()?, cursor.f32()?];
            vertices.push(acu_assets::vertex::Vertex {
                position,
                position_w: 0,
                normal,
                normal_w: 0,
                tangent: (flags & 2 != 0).then_some(tangent),
                tangent_w: None,
                binormal: (flags & 4 != 0).then_some(binormal),
                binormal_w: None,
                uv0: (flags & 1 != 0).then_some(uv),
                color: None,
                joints: None,
                joint_weights: None,
            });
        }
        let part_count = cursor.u64()? as usize;
        let mut parts = Vec::with_capacity(part_count);
        for _ in 0..part_count {
            let vertex_base = cursor.u64()? as usize;
            let vertex_count = cursor.u64()? as usize;
            let material = cursor.i32()?;
            let triangle_count = cursor.u64()? as usize;
            let mut triangles = Vec::with_capacity(triangle_count);
            for _ in 0..triangle_count {
                triangles.push([cursor.u32()?, cursor.u32()?, cursor.u32()?]);
            }
            parts.push(LoadedPart {
                palette: Vec::new(),
                primitive: 0,
                vertex_base,
                vertex_count,
                material: (material >= 0).then_some(material as usize),
                submesh_index: None,
                triangles,
            });
        }
        pieces.push(LoadedPiece {
            bones: Vec::new(),
            label,
            archive: std::path::PathBuf::new(),
            entry: 0,
            resource: 0,
            file_id,
            entry_stored_sha256: String::new(),
            payload_sha256: String::new(),
            quantization_factor: 1.0,
            vertex_source: "cache",
            index_source: "cache",
            mesh_materials: Vec::new(),
            vertices,
            parts,
        });
    }

    let instance_count = cursor.u64()? as usize;
    let mut instances = Vec::with_capacity(instance_count);
    for _ in 0..instance_count {
        let piece = cursor.u32()? as usize;
        let mut matrix = [0f32; 16];
        for slot in &mut matrix {
            *slot = cursor.f32()?;
        }
        instances.push(MapInstance { piece, matrix });
    }

    let material_count = cursor.u64()? as usize;
    let mut materials = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        let file_id = cursor.string()?;
        let diffuse = cursor.i32()?;
        let alpha_test_enabled = cursor.byte()? != 0;
        let alpha_test_value = cursor.byte()?;
        let source = cursor.string()?;
        materials.push(LoadedMaterial {
            normal_texture: None,
            normal_map: None,
            alpha_test_enabled,
            alpha_test_value,
            file_id,
            payload_sha256: String::new(),
            source,
            selector_map_types: Vec::new(),
            texture_set: String::new(),
            texture_set_source: None,
            selector_diffuse: None,
            set_diffuse: None,
            textures: Vec::new(),
            notes: Vec::new(),
            diffuse_texture: (diffuse >= 0).then_some(diffuse as usize),
        });
    }

    let texture_count = cursor.u64()? as usize;
    let mut textures = Vec::with_capacity(texture_count);
    for _ in 0..texture_count {
        let file_id = cursor.string()?;
        let dxgi_format = cursor.u32()?;
        let gamma = cursor.u32()?;
        let width = cursor.u32()?;
        let height = cursor.u32()?;
        let mip_count = cursor.u32()?;
        let srgb = cursor.byte()? != 0;
        let length = cursor.u64()? as usize;
        let pixels = cursor.take(length).to_vec();
        textures.push(LoadedTexture {
            srgb,
            file_id: file_id.clone(),
            name: file_id,
            source: "cache".into(),
            payload_sha256: String::new(),
            pixel_sha256: String::new(),
            dxgi_format,
            gamma,
            width,
            height,
            mip_count,
            storage: "resident",
            pixels,
        });
    }

    let max_piece = instances.iter().map(|i| i.piece).max().unwrap_or(0);
    eprintln!(
        "cache: read {} pieces, {} instances (max piece index {}), {} materials, {} textures",
        pieces.len(),
        instances.len(),
        max_piece,
        materials.len(),
        textures.len()
    );
    Ok(Loaded {
        pieces,
        instances,
        rig_pieces,
        scene: Scene {
            rig: None,
            replay: None,
            pieces: Vec::new(),
            materials,
            textures,
        },
    })
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> &'a [u8] {
        let end = (self.at + count).min(self.bytes.len());
        let slice = &self.bytes[self.at..end];
        self.at = end;
        slice
    }
    fn byte(&mut self) -> Result<u8, String> {
        self.take(1)
            .first()
            .copied()
            .ok_or_else(|| "cache: truncated".to_string())
    }
    fn u32(&mut self) -> Result<u32, String> {
        let slice: [u8; 4] = self
            .take(4)
            .try_into()
            .map_err(|_| "cache: truncated u32".to_string())?;
        Ok(u32::from_le_bytes(slice))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn u64(&mut self) -> Result<u64, String> {
        let slice: [u8; 8] = self
            .take(8)
            .try_into()
            .map_err(|_| "cache: truncated u64".to_string())?;
        Ok(u64::from_le_bytes(slice))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn string(&mut self) -> Result<String, String> {
        let length = self.u32()? as usize;
        let bytes = self.take(length);
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_bits().to_le_bytes());
}
fn put_str(out: &mut Vec<u8>, value: &str) {
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}
