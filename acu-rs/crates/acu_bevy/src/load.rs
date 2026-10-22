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

use std::collections::{HashMap, HashSet};
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
        // Map mode resolves materials from the assets archive itself.
        if options.map_cell.is_some() {
            let assets = options
                .map_assets
                .clone()
                .unwrap_or_else(|| options.game_dir.join("DataPC_ACU_Paris_assets.forge"));
            if !catalog_paths.iter().any(|p| p == &assets) {
                catalog_paths.push(assets);
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

/// Resource types needed by map mode (ACU-RE-0034/0035/0036).
const TYPE_CELL: &str = "ac2bbf68";
const TYPE_ENTITY: &str = "0984415e";
const TYPE_SELECTOR: &str = "51dc6b80";
const TYPE_WORLD_MESH: &str = "415d9568";

/// `LODSelector` record table (ACU-RE-0037): a 19-byte header, five 35-byte records and a
/// five-byte trailer. Every record is `u64 id, u32 tag a360319a, u8/u8 flags, u64 _assets
/// entry id, f32 switch distance, 9 zero bytes`; the ids are consecutive and start one to
/// sixteen above the selector id. `flag_a == 1` marks a level that carries a mesh
/// (`1/1` for the first levels, `1/0` for one deeper level); `3/0` is an empty level.
const SELECTOR_HEADER: usize = 19;
const SELECTOR_STRIDE: usize = 35;
const SELECTOR_RECORDS: usize = 5;
const SELECTOR_TRAILER: usize = 5;
const SELECTOR_TAG: u32 = 0xa360_319a;
const SELECTOR_FLAG_MESH: u8 = 1;

/// Highest number of placeable meshes one invocation will spawn.
const MAP_PIECE_LIMIT: usize = 400;

/// One decoded LOD record that references a mesh entry.
#[derive(Debug, Clone, Copy)]
struct LodRecord {
    level: usize,
    /// Assets (or Paris) entry file id that carries the `415d9568` mesh.
    entry_id: u64,
    /// Recorded switch distance in metres; meaning is not yet proven beyond ordering.
    distance: f32,
}

/// One-entry package cache with a file-id -> resource-position map. A cell's children,
/// their selectors and their meshes repeatedly come from the same entry, and the map is
/// what lets a reference slot be resolved to a *type* instead of guessed.
#[derive(Default)]
struct PackageCache {
    entry: Option<usize>,
    package: Option<package::Package>,
    ids: HashMap<u64, usize>,
}

impl PackageCache {
    fn ensure(&mut self, forge: &mut Forge, entry: usize) -> Result<()> {
        if self.entry != Some(entry) {
            let package = forge.read_package(entry)?;
            let mut ids = HashMap::with_capacity(package.resources.len());
            for (position, resource) in package.resources.iter().enumerate() {
                if let Ok(value) = u64::from_str_radix(&resource.file_id, 16) {
                    ids.entry(value).or_insert(position);
                }
            }
            self.package = Some(package);
            self.ids = ids;
            self.entry = Some(entry);
        }
        Ok(())
    }

    fn view(&self) -> Result<(&package::Package, &HashMap<u64, usize>)> {
        let package = self
            .package
            .as_ref()
            .ok_or("package cache empty after load")?;
        Ok((package, &self.ids))
    }
}

/// Locate the reference in an entity payload that resolves, inside the entity's own entry,
/// to a `LODSelector` resource. Returns that resource's position and id.
///
/// The slot is not at a fixed offset: a light entity's first slot is a light, an FX
/// entity's first slot is an FX component, and multi-component entities keep their
/// selector near the end of the payload. Resolving each 8-byte window against the entry's
/// id table and requiring type `51dc6b80` means only a real selector resource can match;
/// no name, prefix or neighbouring-id rule is involved. Entities that reference more than
/// one selector keep the first in payload order (a presentation choice for composites).
fn find_selector(
    package: &package::Package,
    ids: &HashMap<u64, usize>,
    bytes: &[u8],
) -> Option<(usize, u64)> {
    for offset in 0..bytes.len().saturating_sub(7) {
        let value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
        if value == 0 {
            continue;
        }
        let Some(&position) = ids.get(&value) else {
            continue;
        };
        if package.resources[position].type_id == TYPE_SELECTOR {
            return Some((position, value));
        }
    }
    None
}

/// What the entity's own entry yielded for one child id.
enum EntityLink {
    Ready {
        bytes: Vec<u8>,
        selector_id: u64,
        selector_bytes: Vec<u8>,
    },
    NoEntity,
    NoSelector,
}

/// Resolve child id -> entity payload -> its LODSelector payload, all inside one entry.
fn entity_link(
    cache: &mut PackageCache,
    forge: &mut Forge,
    entity_entry: usize,
    child: u64,
) -> Result<EntityLink> {
    cache.ensure(forge, entity_entry)?;
    let (package, ids) = cache.view()?;
    let Some(&position) = ids.get(&child) else {
        return Ok(EntityLink::NoEntity);
    };
    let resource = &package.resources[position];
    if resource.type_id != TYPE_ENTITY {
        return Ok(EntityLink::NoEntity);
    }
    let bytes = package.bytes[resource.payload.clone()].to_vec();
    let Some((selector_position, selector_id)) = find_selector(package, ids, &bytes) else {
        return Ok(EntityLink::NoSelector);
    };
    let selector = &package.resources[selector_position];
    Ok(EntityLink::Ready {
        bytes,
        selector_id,
        selector_bytes: package.bytes[selector.payload.clone()].to_vec(),
    })
}

/// Decode the selector's LOD records; any structural mismatch is an error, never a guess.
fn selector_records(bytes: &[u8], id: u64) -> Result<Vec<LodRecord>> {
    let expected = SELECTOR_HEADER + SELECTOR_RECORDS * SELECTOR_STRIDE + SELECTOR_TRAILER;
    if bytes.len() != expected {
        return Err(format!(
            "selector {id:016x}: payload {} B, expected {expected}",
            bytes.len()
        )
        .into());
    }
    if bytes[0] != 1 {
        return Err(format!("selector {id:016x}: version {}", bytes[0]).into());
    }
    let self_id = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
    if self_id != id {
        return Err(format!("selector {id:016x}: header self {self_id:016x}").into());
    }
    let type_id = u32::from_le_bytes(bytes[9..13].try_into().unwrap());
    if type_id != 0x51dc_6b80 {
        return Err(format!("selector {id:016x}: header type {type_id:08x}").into());
    }
    let trailer = SELECTOR_HEADER + SELECTOR_RECORDS * SELECTOR_STRIDE;
    if bytes.len() != trailer + SELECTOR_TRAILER {
        return Err(format!("selector {id:016x}: trailer missing").into());
    }
    let mut records = Vec::new();
    let mut previous = id;
    for level in 0..SELECTOR_RECORDS {
        let at = SELECTOR_HEADER + level * SELECTOR_STRIDE;
        let record_id = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        // Record ids increase inside the selector's own id block but need not be
        // adjacent: one decal selector jumps self+3 -> self+7 because its middle slots
        // are absent. A non-increasing id means this is not the table we think it is.
        if record_id <= previous {
            return Err(format!(
                "selector {id:016x}: record {level} id {record_id:016x} does not increase"
            )
            .into());
        }
        previous = record_id;
        let tag = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap());
        if tag != SELECTOR_TAG {
            return Err(format!("selector {id:016x}: record {level} tag {tag:08x}").into());
        }
        let flag_a = bytes[at + 12];
        // Second flag is read but not yet interpreted; observed 1 on the first levels and
        // 0 on the deepest mesh level and on empty levels.
        let _flag_b = bytes[at + 13];
        let entry_id = u64::from_le_bytes(bytes[at + 14..at + 22].try_into().unwrap());
        let distance = f32::from_le_bytes(bytes[at + 22..at + 26].try_into().unwrap());
        if flag_a != SELECTOR_FLAG_MESH || entry_id == 0 {
            continue;
        }
        records.push(LodRecord {
            level,
            entry_id,
            distance,
        });
    }
    Ok(records)
}

fn id_map(forge: &Forge) -> HashMap<u64, usize> {
    let mut map = HashMap::with_capacity(forge.entries.len());
    for entry in &forge.entries {
        if let Ok(value) = u64::from_str_radix(&entry.file_id, 16) {
            map.insert(value, entry.index);
        }
    }
    map
}

/// Child ids of a GridCellDataBlock: `17 + count*10 + 12` bytes exactly.
fn cell_children(bytes: &[u8]) -> Result<Vec<u64>> {
    if bytes.len() < 29 {
        return Err("cell block shorter than its header".into());
    }
    let count = u32::from_le_bytes(bytes[13..17].try_into().unwrap()) as usize;
    if count > 100_000 || 17 + count * 10 + 12 != bytes.len() {
        return Err(format!(
            "cell child count {count} does not fit {len} bytes",
            len = bytes.len()
        )
        .into());
    }
    let mut children = Vec::with_capacity(count);
    for index in 0..count {
        let at = 17 + index * 10;
        children.push(u64::from_le_bytes(
            bytes[at + 2..at + 10].try_into().unwrap(),
        ));
    }
    Ok(children)
}

/// The entity's affine matrix: `u8 version, u64 self, u32 type, u8 flag, f32[16]`.
/// Rows 0-2 are rotation, row 3 is translation (ACU-RE-0035), stored row-major.
fn entity_matrix(bytes: &[u8]) -> Option<[f32; 16]> {
    if bytes.len() < 78 {
        return None;
    }
    let mut matrix = [0f32; 16];
    for index in 0..16 {
        let at = 14 + index * 4;
        matrix[index] = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    }
    if (matrix[15] - 1.0).abs() > 1e-3 {
        return None;
    }
    if matrix.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(matrix)
}

fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

fn transform_direction(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2],
    ]
}

/// Census index: resource id -> every entry that carries it, in file order.
type Index = HashMap<u64, Vec<usize>>;
fn read_index(name: &str) -> Result<Index> {
    let candidates = [
        PathBuf::from(name),
        PathBuf::from("..").join(name),
        PathBuf::from("../..").join(name),
    ];
    let path = candidates.iter().find(|p| p.exists()).ok_or_else(|| {
        format!("resource index {name} not found; run tools/build_resource_index.py")
    })?;
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..8] != b"ACUSEL01" {
        return Err(format!("{}: not an ACUSEL01 index", path.display()).into());
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if bytes.len() != 12 + count * 12 {
        return Err(format!("{}: length does not match {count} rows", path.display()).into());
    }
    let mut map: Index = HashMap::with_capacity(count);
    for index in 0..count {
        let at = 12 + index * 12;
        let id = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let entry = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
        map.entry(id).or_default().push(entry);
    }
    Ok(map)
}

/// Resolve a selector record's mesh id, which may be any of:
///   1. a `415d9568` resource inside the entity's own entry (self-contained cells),
///   2. an `_assets` entry whose single mesh carries that id (one mesh per entry),
///   3. a `415d9568` resource anywhere in the Paris archive (mesh index),
///   4. a Paris entry whose single mesh carries that id.
/// Every branch requires the resolved resource's type to be `415d9568`; no id is ever
/// dereferenced on the strength of a namespace or name rule.
fn resolve_mesh(
    cache: &mut PackageCache,
    paris: &mut Forge,
    assets: &mut Forge,
    entity_entry: usize,
    id: u64,
    assets_entries: &HashMap<u64, usize>,
    paris_entries: &HashMap<u64, usize>,
    mesh_index: &Index,
) -> Result<Option<(bool, usize, usize)>> {
    cache.ensure(paris, entity_entry)?;
    {
        let (package, ids) = cache.view()?;
        if let Some(&position) = ids.get(&id)
            && package.resources[position].type_id == TYPE_WORLD_MESH
        {
            return Ok(Some((false, entity_entry, position)));
        }
    }
    if let Some(&entry) = assets_entries.get(&id) {
        let package = assets.read_package(entry)?;
        if let Some(position) = package
            .resources
            .iter()
            .position(|r| r.type_id == TYPE_WORLD_MESH)
        {
            return Ok(Some((true, entry, position)));
        }
    }
    let key = format!("{id:016x}");
    if let Some(entries) = mesh_index.get(&id) {
        for &entry in entries {
            let package = paris.read_package(entry)?;
            if let Some(position) = package
                .resources
                .iter()
                .position(|r| r.file_id == key && r.type_id == TYPE_WORLD_MESH)
            {
                return Ok(Some((false, entry, position)));
            }
        }
    }
    if let Some(&entry) = paris_entries.get(&id) {
        let package = paris.read_package(entry)?;
        if let Some(position) = package
            .resources
            .iter()
            .position(|r| r.type_id == TYPE_WORLD_MESH)
        {
            return Ok(Some((false, entry, position)));
        }
    }
    Ok(None)
}

/// Spawn one world cell: children -> Entity -> LODSelector -> LOD mesh, with each entity's
/// matrix baked into the vertices so the existing spawn and camera path works.
///
/// The chain is decoded, not searched (ACU-RE-0037): the entity payload references a
/// `LODSelector` resource in its own entry (found by resolving each reference slot against
/// that entry's id table and keeping the one whose type is `51dc6b80`); the selector's
/// five-record table gives the `_assets` entry ids of its LOD meshes; the entry carries one
/// `415d9568` mesh. Records with `flags 3/0` are empty LOD levels. Every unresolved link is
/// counted and named on stderr; nothing is substituted.
pub fn load_map(options: &Options, loader: &mut Loader) -> Result<Vec<LoadedPiece>> {
    let limits = Limits::default();
    let paris_path = options
        .map_paris
        .clone()
        .unwrap_or_else(|| options.game_dir.join("DataPC_ACU_Paris.forge"));
    let assets_path = options
        .map_assets
        .clone()
        .unwrap_or_else(|| options.game_dir.join("DataPC_ACU_Paris_assets.forge"));
    let mut paris = Forge::open(&paris_path, limits)?;
    let paris_ids = id_map(&paris);
    let entity_index = read_index("local/paris-entities.bin")?;
    let mesh_index = read_index("local/paris-world-meshes.bin")?;
    let mut assets = Forge::open(&assets_path, limits)?;
    let assets_entries = id_map(&assets);

    let spec = options
        .map_cell
        .clone()
        .ok_or("map mode without --map-cell")?;
    let cell_entry = match spec.parse::<usize>() {
        Ok(index) => index,
        Err(_) => {
            let id = u64::from_str_radix(spec.trim_start_matches("0x"), 16)
                .map_err(|_| "map cell must be an entry index or a hex file id")?;
            *paris_ids
                .get(&id)
                .ok_or("cell file id is not an entry in the Paris archive")?
        }
    };

    let mut cache = PackageCache::default();
    let children = {
        cache.ensure(&mut paris, cell_entry)?;
        let (package, _) = cache.view()?;
        let block = package
            .resources
            .iter()
            .find(|r| r.type_id == TYPE_CELL)
            .ok_or("entry has no GridCellDataBlock resource")?;
        cell_children(&package.bytes[block.payload.clone()])?
    };
    eprintln!("map: cell {} with {} children", spec, children.len());

    let wanted_lod = options.map_lod as usize;
    let mut pieces = Vec::new();
    let mut selectors_used: HashSet<u64> = HashSet::new();
    let mut meshes_seen: HashSet<String> = HashSet::new();
    let mut lods_seen: HashSet<usize> = HashSet::new();
    let mut reported: HashSet<u64> = HashSet::new();
    let mut skipped_matrix = 0usize;
    let mut skipped_no_entity = 0usize;
    let mut skipped_no_selector = 0usize;
    let mut skipped_no_lod = 0usize;
    let mut skipped_no_mesh = 0usize;

    for child in children {
        if pieces.len() >= MAP_PIECE_LIMIT {
            eprintln!("map: piece limit {MAP_PIECE_LIMIT} reached");
            break;
        }
        let Some(entries) = entity_index.get(&child).filter(|list| !list.is_empty()) else {
            skipped_no_entity += 1;
            continue;
        };
        let mut link = None;
        let mut saw_entity = false;
        for &entry in entries {
            match entity_link(&mut cache, &mut paris, entry, child)? {
                EntityLink::Ready {
                    bytes,
                    selector_id,
                    selector_bytes,
                } => {
                    saw_entity = true;
                    link = Some((entry, bytes, selector_id, selector_bytes));
                    break;
                }
                EntityLink::NoSelector => {
                    saw_entity = true;
                    break;
                }
                EntityLink::NoEntity => continue,
            }
        }
        let Some((entity_entry, entity_bytes, selector_id, selector_bytes)) = link else {
            if saw_entity {
                skipped_no_selector += 1;
            } else {
                skipped_no_entity += 1;
            }
            continue;
        };
        let Some(matrix) = entity_matrix(&entity_bytes) else {
            skipped_matrix += 1;
            continue;
        };
        let records = match selector_records(&selector_bytes, selector_id) {
            Ok(records) => records,
            Err(error) => {
                eprintln!("map: {error}");
                skipped_no_lod += 1;
                continue;
            }
        };
        // Prefer the requested LOD; fall back to the finest available, never to a wrong level.
        let chosen = records
            .iter()
            .find(|record| record.level >= wanted_lod)
            .or_else(|| records.last());
        let Some(record) = chosen else {
            skipped_no_lod += 1;
            continue;
        };
        if reported.insert(selector_id) {
            let table = records
                .iter()
                .map(|entry| format!("{}@{:.0}m", entry.level, entry.distance))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "map: selector {selector_id:016x} LOD table [{table}], chose LOD{}",
                record.level
            );
        }
        let Some((from_assets, mesh_entry, resource)) = resolve_mesh(
            &mut cache,
            &mut paris,
            &mut assets,
            entity_entry,
            record.entry_id,
            &assets_entries,
            &paris_ids,
            &mesh_index,
        )?
        else {
            eprintln!(
                "map: selector {selector_id:016x} LOD{} mesh {:016x} is in no selected archive",
                record.level, record.entry_id
            );
            skipped_no_mesh += 1;
            continue;
        };
        let piece = Piece {
            archive: if from_assets {
                assets_path.clone()
            } else {
                paris_path.clone()
            },
            entry: mesh_entry,
            resource,
            label: format!("cell-{child:016x}"),
        };
        let mut loaded = if from_assets {
            load_piece(loader, &mut assets, &piece)?
        } else {
            load_piece(loader, &mut paris, &piece)?
        };
        for vertex in &mut loaded.vertices {
            vertex.position = transform_point(&matrix, vertex.position);
            vertex.normal = transform_direction(&matrix, vertex.normal);
            if let Some(tangent) = vertex.tangent {
                vertex.tangent = Some(transform_direction(&matrix, tangent));
            }
            if let Some(binormal) = vertex.binormal {
                vertex.binormal = Some(transform_direction(&matrix, binormal));
            }
        }
        selectors_used.insert(selector_id);
        meshes_seen.insert(loaded.file_id.clone());
        lods_seen.insert(record.level);
        pieces.push(loaded);
    }
    let lod_list: Vec<String> = {
        let mut levels: Vec<usize> = lods_seen.iter().copied().collect();
        levels.sort_unstable();
        levels.iter().map(|level| level.to_string()).collect()
    };
    eprintln!(
        "map: {} pieces, {} distinct meshes, {} distinct selectors, LODs [{}]; \
         skipped: {} without a matrix, {} without an entity, {} without a selector, \
         {} without a LOD record, {} without a mesh",
        pieces.len(),
        meshes_seen.len(),
        selectors_used.len(),
        lod_list.join(","),
        skipped_matrix,
        skipped_no_entity,
        skipped_no_selector,
        skipped_no_lod,
        skipped_no_mesh
    );
    Ok(pieces)
}

/// Load every piece, reusing one open Forge per archive.
pub fn load_all(options: &Options) -> Result<Loaded> {
    let limits = Limits::default();
    let mut forges: HashMap<PathBuf, Forge> = HashMap::new();
    let mut loader = Loader::new(options);
    let mut pieces = Vec::new();
    if options.map_cell.is_some() {
        pieces = load_map(options, &mut loader)?;
    }
    let pet: &[Piece] = if options.map_cell.is_some() {
        &[]
    } else {
        &options.pieces
    };
    for piece in pet {
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
