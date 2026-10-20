//! Read and decode scene pieces, their material chains and textures from the
//! original install, in memory.
//!
//! Everything here is read-only over the Forge archives; no extraction or
//! install mutation. Provenance (archive, entry, resource, file ID, payload
//! hash) is retained for every piece, material and texture.
//!
//! Chain (ACU-RE-0013): mesh instancing record -> material -> texture set ->
//! diffuse texture map. The mesh and its materials are usually in the same
//! entry; texture sets and textures often live in other entries or archives,
//! so those hops consult a `Catalog` index over explicitly selected archives.
//! Candidate selection among duplicates is a presentation choice, logged and
//! reported, never a claim about the original engine's patch precedence.

use std::collections::HashMap;
use std::path::PathBuf;

use acu_assets::{
    Limits, Result,
    catalog::{Catalog, CatalogLimits},
    forge::Forge,
    material, mesh, package, sha256, texture,
};
use serde::Serialize;

use crate::options::{Options, Piece};

/// Resolved bytes with provenance, owned so the source package can be dropped.
struct Resolved {
    payload: Vec<u8>,
    name: String,
    source: String,
    payload_sha256: String,
    candidates: usize,
}

#[derive(Debug, Serialize)]
pub struct LoadedTexture {
    pub srgb: bool,
    pub file_id: String,
    pub name: String,
    pub source: String,
    pub payload_sha256: String,
    pub pixel_sha256: String,
    pub dxgi_format: u32,
    pub gamma: u32,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub storage: &'static str,
    /// Concatenated mips as stored; uploaded as-is for block formats.
    #[serde(skip)]
    pub pixels: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct LoadedMaterial {
    #[serde(skip)]
    pub normal_texture: Option<usize>,
    pub normal_map: Option<String>,
    pub alpha_test_enabled: bool,
    pub alpha_test_value: u8,
    pub file_id: String,
    pub payload_sha256: String,
    pub source: String,
    /// `TextureMapType` values of every `TextureSelector` parameter, in order.
    pub selector_map_types: Vec<i32>,
    pub texture_set: String,
    pub texture_set_source: Option<String>,
    /// Diffuse map ID from the material's own `MapDiffuse` selector.
    pub selector_diffuse: Option<String>,
    /// Diffuse map ID from the texture set's `Diffuse` reference.
    pub set_diffuse: Option<String>,
    pub textures: Vec<String>,
    pub notes: Vec<String>,
    /// Registered diffuse texture, for rendering only.
    #[serde(skip)]
    pub diffuse_texture: Option<usize>,
}

#[derive(Debug)]
pub struct LoadedPart {
    pub palette: Vec<u16>,
    pub primitive: usize,
    pub vertex_base: usize,
    pub vertex_count: usize,
    /// Index into `Scene::materials` when the part's instancing record bound one.
    pub material: Option<usize>,
    pub submesh_index: Option<u16>,
    /// Triangles in the piece's decoded vertex space.
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Debug)]
pub struct LoadedPiece {
    pub bones: Vec<mesh::MeshBone>,
    pub label: String,
    pub archive: PathBuf,
    pub entry: usize,
    pub resource: usize,
    pub file_id: String,
    pub entry_stored_sha256: String,
    pub payload_sha256: String,
    pub quantization_factor: f32,
    pub vertex_source: &'static str,
    pub index_source: &'static str,
    pub mesh_materials: Vec<String>,
    pub vertices: Vec<acu_assets::vertex::Vertex>,
    pub parts: Vec<LoadedPart>,
}

impl LoadedPiece {
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &self.vertices {
            for i in 0..3 {
                min[i] = min[i].min(v.position[i]);
                max[i] = max[i].max(v.position[i]);
            }
        }
        (min, max)
    }
}

/// Serialized view of a loaded piece (no vertex/triangle payloads).
#[derive(Debug, Serialize)]
pub struct PieceReport {
    pub label: String,
    pub archive: String,
    pub entry: usize,
    pub resource: usize,
    pub file_id: String,
    pub entry_stored_sha256: String,
    pub payload_sha256: String,
    pub quantization_factor: f32,
    pub vertex_source: &'static str,
    pub index_source: &'static str,
    pub mesh_materials: Vec<String>,
    pub vertices: usize,
    pub parts: Vec<PartReport>,
}

#[derive(Debug, Serialize)]
pub struct PartReport {
    pub primitive: usize,
    pub vertex_base: usize,
    pub vertex_count: usize,
    pub material: Option<String>,
    pub material_source: Option<String>,
    pub submesh_index: Option<u16>,
    pub triangles: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct Scene {
    pub rig: Option<serde_json::Value>,
    pub replay: Option<serde_json::Value>,
    pub pieces: Vec<PieceReport>,
    pub materials: Vec<LoadedMaterial>,
    pub textures: Vec<LoadedTexture>,
}

/// Loaded scene: render payload plus the reportable chain.
pub struct Loaded {
    pub pieces: Vec<LoadedPiece>,
    pub scene: Scene,
}

pub struct Loader {
    no_textures: bool,
    entry_archive: PathBuf,
    limits: Limits,
    /// Archives indexed for cross-archive lookups (piece archives then the
    /// configured texture archives), in selection order.
    catalog_paths: Vec<PathBuf>,
    catalog: Option<Catalog>,
    materials: HashMap<String, usize>,
    textures: HashMap<String, usize>,
    scene: Scene,
}

impl Loader {
    fn new(options: &Options) -> Self {
        let mut catalog_paths: Vec<PathBuf> = Vec::new();
        for piece in &options.pieces {
            if !catalog_paths.iter().any(|p| p == &piece.archive) {
                catalog_paths.push(piece.archive.clone());
            }
        }
        for path in &options.texture_archives {
            if !catalog_paths.iter().any(|p| p == path) {
                catalog_paths.push(path.clone());
            }
        }
        Self {
            no_textures: options.no_textures,
            entry_archive: PathBuf::new(),
            limits: Limits::default(),
            catalog_paths,
            catalog: None,
            materials: HashMap::new(),
            textures: HashMap::new(),
            scene: Scene::default(),
        }
    }

    fn catalog(&mut self) -> Result<&mut Catalog> {
        if self.catalog.is_none() {
            let limits = self.limits;
            self.catalog = Some(Catalog::open(
                &self.catalog_paths,
                limits,
                CatalogLimits::default(),
            )?);
        }
        Ok(self
            .catalog
            .as_mut()
            .ok_or("catalog unavailable after indexing")?)
    }

    /// Resolve a resource's payload bytes: the current entry first, then the
    /// catalog. `None` when no selected archive carries the file ID.
    fn resolve(
        &mut self,
        file_id: &str,
        package: Option<&package::Package>,
        entry: usize,
    ) -> Result<Option<Resolved>> {
        if let Some(package) = package
            && let Some((index, resource)) = package
                .resources
                .iter()
                .enumerate()
                .find(|(_, r)| r.file_id == file_id)
        {
            let payload = &package.bytes[resource.payload.clone()];
            return Ok(Some(Resolved {
                payload: payload.to_vec(),
                name: resource.name.clone(),
                source: format!(
                    "archive {} entry {entry} resource {index}",
                    self.entry_archive.display()
                ),
                payload_sha256: sha256(payload),
                candidates: 1,
            }));
        }
        let catalog = self.catalog()?;
        let candidates = catalog.candidates(file_id)?;
        let Some(first) = candidates.first() else {
            return Ok(None);
        };
        let count = candidates.len();
        if count > 1 {
            eprintln!(
                "note: {file_id} has {count} candidates; using the first (presentation choice)"
            );
            for candidate in candidates {
                let archive = catalog
                    .archive_path(candidate.archive_index)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| format!("#{}", candidate.archive_index));
                eprintln!(
                    "  candidate archive={archive} entry={} resource={} package={}",
                    candidate.entry_index, candidate.resource_index, candidate.package_file_id
                );
            }
        }
        let candidate = first.clone();
        let loaded = catalog.load(&candidate)?;
        let resource = loaded.resource();
        let payload = loaded.package.bytes[resource.payload.clone()].to_vec();
        let name = resource.name.clone();
        let archive = catalog
            .archive_path(candidate.archive_index)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| format!("#{}", candidate.archive_index));
        Ok(Some(Resolved {
            source: format!(
                "archive {archive} entry {} resource {}",
                candidate.entry_index, candidate.resource_index
            ),
            name,
            payload_sha256: sha256(&payload),
            payload,
            candidates: count,
        }))
    }

    /// Resolve one material chain and register its diffuse texture.
    fn material(
        &mut self,
        file_id: &str,
        package: Option<&package::Package>,
        entry: usize,
    ) -> Result<Option<usize>> {
        if self.no_textures {
            return Ok(None);
        }
        if let Some(index) = self.materials.get(file_id) {
            return Ok(Some(*index));
        }
        let Some(resolved) = self.resolve(file_id, package, entry)? else {
            eprintln!("note: material {file_id} not found in any selected archive");
            return Ok(None);
        };
        let parsed = material::parse(&resolved.payload, file_id, self.limits)
            .map_err(|e| format!("material {file_id} at {}: {e}", resolved.source))?;
        let mut notes = Vec::new();
        if resolved.candidates > 1 {
            notes.push(format!(
                "{} duplicate candidates; first used",
                resolved.candidates
            ));
        }
        let mut selector_map_types = Vec::new();
        let mut selector_diffuse = None;
        let mut normal_map = None;
        let mut textures = Vec::new();
        for parameter in &parsed.parameters {
            let material::ParameterValue::Object(value) = &parameter.value else {
                continue;
            };
            let material::ObjectData::TextureSelector(selector) = &value.data else {
                continue;
            };
            selector_map_types.push(selector.map_type);
            let map = selector.texture_map.file_id.clone();
            if map != "0000000000000000" && !textures.contains(&map) {
                textures.push(map.clone());
            }
            if selector.map_type == 0
                && selector.specification_method == 0
                && selector_diffuse.is_none()
                && map != "0000000000000000"
            {
                selector_diffuse = Some(map);
            }
            if selector.map_type == 1
                && selector.specification_method == 0
                && selector.texture_map.file_id != "0000000000000000"
            {
                normal_map = Some(selector.texture_map.file_id.clone());
            }
        }
        let set_id = parsed.texture_set.file_id.clone();
        let (mut texture_set_source, mut set_diffuse) = (None, None);
        if set_id != "0000000000000000" {
            match self.resolve(&set_id, package, entry)? {
                Some(set) => {
                    let parsed_set =
                        material::parse_texture_set(&set.payload, &set_id, self.limits).map_err(
                            |e| {
                                format!(
                                    "texture set {set_id} at {} for material {file_id}: {e}",
                                    set.source
                                )
                            },
                        )?;
                    texture_set_source = Some(set.source.clone());
                    if parsed_set.diffuse.file_id != "0000000000000000" {
                        set_diffuse = Some(parsed_set.diffuse.file_id.clone());
                        if !textures.contains(&parsed_set.diffuse.file_id) {
                            textures.push(parsed_set.diffuse.file_id.clone());
                        }
                    }
                    if normal_map.is_none() && parsed_set.normal.file_id != "0000000000000000" {
                        normal_map = Some(parsed_set.normal.file_id.clone());
                    }
                }
                None => notes.push(format!("texture set {set_id} not found")),
            }
        }
        if let (Some(selector), Some(set)) = (&selector_diffuse, &set_diffuse)
            && selector != set
        {
            notes.push(format!(
                "selector diffuse {selector} disagrees with set diffuse {set}"
            ));
        }
        let diffuse = selector_diffuse.clone().or(set_diffuse.clone());
        let diffuse_texture = match &diffuse {
            Some(diffuse) => self.texture(diffuse, package, entry, true)?,
            None => None,
        };
        let normal_texture = match &normal_map {
            Some(id) => {
                let index = self.texture(id, package, entry, false)?;
                match index {
                    Some(i) if [71, 98, 83].contains(&self.scene.textures[i].dxgi_format) => {
                        Some(i)
                    }
                    Some(_) => {
                        notes.push(format!(
                            "normal {id}: unsupported normal storage; not applied"
                        ));
                        None
                    }
                    None => {
                        notes.push(format!("normal {id} unavailable"));
                        None
                    }
                }
            }
            None => None,
        };
        if diffuse.is_some() && diffuse_texture.is_none() {
            notes.push(format!(
                "diffuse map {:?} not found in any selected archive",
                diffuse
            ));
        }
        let index = self.scene.materials.len();
        self.scene.materials.push(LoadedMaterial {
            normal_map,
            normal_texture,
            alpha_test_enabled: parsed.flags.alpha_test_enabled,
            alpha_test_value: parsed.alpha_test_value,
            file_id: file_id.to_string(),
            payload_sha256: resolved.payload_sha256,
            source: resolved.source,
            selector_map_types,
            texture_set: set_id,
            texture_set_source,
            selector_diffuse,
            set_diffuse,
            textures,
            notes,
            diffuse_texture,
        });
        self.materials.insert(file_id.to_string(), index);
        Ok(Some(index))
    }

    /// Resolve one texture resource and register it.
    fn texture(
        &mut self,
        file_id: &str,
        package: Option<&package::Package>,
        entry: usize,
        srgb: bool,
    ) -> Result<Option<usize>> {
        if let Some(index) = self.textures.get(file_id) {
            return Ok(Some(*index));
        }
        let Some(resolved) = self.resolve(file_id, package, entry)? else {
            return Ok(None);
        };
        let parsed = texture::parse_resource(&resolved.payload, file_id)?;
        let (storage, loaded) = match parsed {
            texture::TextureResource::Resident(resident) => (
                "resident",
                LoadedTexture {
                    srgb,
                    file_id: resident.file_id.clone(),
                    name: resolved.name.clone(),
                    source: resolved.source.clone(),
                    payload_sha256: resolved.payload_sha256.clone(),
                    pixel_sha256: sha256(resident.pixels),
                    dxgi_format: resident.dxgi_format,
                    gamma: resident.gamma,
                    width: resident.width,
                    height: resident.height,
                    mip_count: resident.mip_count,
                    storage: "resident",
                    pixels: resident.pixels.to_vec(),
                },
            ),
            texture::TextureResource::NoInlinePixels(header) => {
                eprintln!("note: texture {file_id} has no inline pixels; not uploaded");
                (
                    "no_inline_pixels",
                    LoadedTexture {
                        srgb,
                        file_id: header.file_id.clone(),
                        name: resolved.name.clone(),
                        source: resolved.source.clone(),
                        payload_sha256: resolved.payload_sha256.clone(),
                        pixel_sha256: String::new(),
                        dxgi_format: 0,
                        gamma: header.gamma,
                        width: header.width,
                        height: header.height,
                        mip_count: header.mip_count,
                        storage: "no_inline_pixels",
                        pixels: Vec::new(),
                    },
                )
            }
        };
        debug_assert_eq!(loaded.storage, storage);
        let index = self.scene.textures.len();
        self.scene.textures.push(loaded);
        self.textures.insert(file_id.to_string(), index);
        Ok(Some(index))
    }
}

fn load_piece(loader: &mut Loader, forge: &mut Forge, piece: &Piece) -> Result<LoadedPiece> {
    loader.entry_archive = piece.archive.clone();
    let limits = loader.limits;
    let raw = forge.raw(piece.entry)?;
    let entry_stored_sha256 = sha256(&raw);
    let package = crate::io::decode_package(raw, limits)?;
    let resource = package
        .resources
        .get(piece.resource)
        .ok_or_else(|| format!("entry {} has no resource {}", piece.entry, piece.resource))?;
    let payload = &package.bytes[resource.payload.clone()];
    let parsed = mesh::parse(payload, &resource.file_id, limits)?;
    // Parts and vertices come from two decode passes over the same immutable
    // buffers; both are deterministic and bounded by the mesh size.
    let raw_parts = parsed.decode_parts(limits)?;
    let mut parts = Vec::with_capacity(raw_parts.len());
    for part in raw_parts {
        let material = match &part.material {
            Some(binding) => loader.material(&binding.file_id, Some(&package), piece.entry)?,
            None => None,
        };
        parts.push(LoadedPart {
            palette: parsed
                .compiled
                .instancing
                .get(part.primitive)
                .map(|record| record.hardware_to_mesh_bones.clone())
                .unwrap_or_default(),
            primitive: part.primitive,
            vertex_base: part.vertex_base,
            vertex_count: part.vertex_count,
            material,
            submesh_index: part.material.as_ref().map(|m| m.submesh_index),
            triangles: part.triangles,
        });
    }
    let vertices = parsed
        .decode(limits)
        .map_err(|e| format!("piece {} decode: {e}", piece.label))?
        .vertices;
    Ok(LoadedPiece {
        bones: parsed.bones,
        label: if resource.name.is_empty() {
            piece.label.clone()
        } else {
            resource.name.clone()
        },
        archive: piece.archive.clone(),
        entry: piece.entry,
        resource: piece.resource,
        file_id: resource.file_id.clone(),
        entry_stored_sha256,
        payload_sha256: sha256(payload),
        quantization_factor: parsed.compiled.quantization_factor,
        vertex_source: if parsed.compiled.mesh_data.vertices.is_empty() {
            "clustered"
        } else {
            "mesh_data"
        },
        index_source: if parsed.compiled.mesh_data.indices.is_empty() {
            "clustered"
        } else {
            "mesh_data"
        },
        mesh_materials: parsed.materials.iter().map(|m| m.file_id.clone()).collect(),
        vertices,
        parts,
    })
}

/// Load every piece, reusing one open Forge per archive.
pub fn load_all(options: &Options) -> Result<Loaded> {
    let limits = Limits::default();
    let mut forges: HashMap<PathBuf, Forge> = HashMap::new();
    let mut loader = Loader::new(options);
    let mut pieces = Vec::new();
    for piece in &options.pieces {
        let forge = match forges.get_mut(&piece.archive) {
            Some(forge) => forge,
            None => {
                let forge = Forge::open(&piece.archive, limits)?;
                forges.entry(piece.archive.clone()).or_insert(forge)
            }
        };
        pieces.push(load_piece(&mut loader, forge, piece)?);
    }
    let mut scene = loader.scene;
    scene.pieces = pieces
        .iter()
        .map(|piece| PieceReport {
            label: piece.label.clone(),
            archive: piece.archive.display().to_string(),
            entry: piece.entry,
            resource: piece.resource,
            file_id: piece.file_id.clone(),
            entry_stored_sha256: piece.entry_stored_sha256.clone(),
            payload_sha256: piece.payload_sha256.clone(),
            quantization_factor: piece.quantization_factor,
            vertex_source: piece.vertex_source,
            index_source: piece.index_source,
            mesh_materials: piece.mesh_materials.clone(),
            vertices: piece.vertices.len(),
            parts: piece
                .parts
                .iter()
                .map(|part| PartReport {
                    primitive: part.primitive,
                    vertex_base: part.vertex_base,
                    vertex_count: part.vertex_count,
                    material: part
                        .material
                        .map(|index| scene.materials[index].file_id.clone()),
                    material_source: part
                        .material
                        .map(|index| scene.materials[index].source.clone()),
                    submesh_index: part.submesh_index,
                    triangles: part.triangles.len(),
                })
                .collect(),
        })
        .collect();
    Ok(Loaded { pieces, scene })
}
