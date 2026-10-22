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

/// One placement of a unique mesh: the piece index plus the entity's native matrix.
pub struct MapInstance {
    pub piece: usize,
    pub matrix: [f32; 16],
}

/// Loaded scene: render payload plus the reportable chain.
pub struct Loaded {
    pub pieces: Vec<LoadedPiece>,
    /// Fill-mode placements; `pieces` then holds one entry per unique mesh, in local space.
    pub instances: Vec<MapInstance>,
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
        if options.map_cell.is_some() || options.map_cells.is_some() {
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

/// Decode one piece from an already-decoded package. Map mode reuses the cached cell
/// package here: a cell entry can decode to tens of megabytes and holds many meshes, so
/// re-inflating it per mesh is the difference between seconds and an hour.
fn piece_from(
    loader: &mut Loader,
    package: &package::Package,
    piece: &Piece,
    entry_stored_sha256: String,
) -> Result<LoadedPiece> {
    loader.entry_archive = piece.archive.clone();
    let limits = loader.limits;
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

/// Read a piece's entry from an archive and decode it.
fn load_piece(loader: &mut Loader, forge: &mut Forge, piece: &Piece) -> Result<LoadedPiece> {
    let limits = loader.limits;
    let raw = forge.raw(piece.entry)?;
    let stored = sha256(&raw);
    let package = crate::io::decode_package(raw, limits)?;
    piece_from(loader, &package, piece, stored)
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
    stored_sha256: String,
    ids: HashMap<u64, usize>,
}

impl PackageCache {
    fn ensure(&mut self, forge: &mut Forge, entry: usize) -> Result<()> {
        if self.entry != Some(entry) {
            let limits = forge.limits;
            let raw = forge.raw(entry)?;
            let stored_sha256 = sha256(&raw);
            let package = crate::io::decode_package(raw, limits)?;
            let mut ids = HashMap::with_capacity(package.resources.len());
            for (position, resource) in package.resources.iter().enumerate() {
                if let Ok(value) = u64::from_str_radix(&resource.file_id, 16) {
                    ids.entry(value).or_insert(position);
                }
            }
            self.package = Some(package);
            self.stored_sha256 = stored_sha256;
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
/// to a resource of `type_id`. Returns that resource's position and id.
///
/// The slot is not at a fixed offset: a light entity's first slot is a light, an FX
/// entity's first slot is an FX component, a terrain-chunk entity references its
/// `415d9568` mesh directly, and multi-component entities keep their selector near the end
/// of the payload. Resolving each 8-byte window against the entry's id table and requiring
/// the wanted type means only a real resource of that kind can match; no name, prefix or
/// neighbouring-id rule is involved. An entity that references several resources of the
/// same type keeps the first in payload order (a presentation choice for composites).
fn find_component(
    package: &package::Package,
    ids: &HashMap<u64, usize>,
    bytes: &[u8],
    type_id: &str,
) -> Option<(usize, u64)> {
    for offset in 0..bytes.len().saturating_sub(7) {
        let value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
        if value == 0 {
            continue;
        }
        let Some(&position) = ids.get(&value) else {
            continue;
        };
        if package.resources[position].type_id == type_id {
            return Some((position, value));
        }
    }
    None
}

/// The geometry an entity carries: a `LODSelector`, or a direct world mesh (terrain chunks
/// and other largemesh pieces reference `415d9568` themselves), or none.
enum GeometryLink {
    Selector { id: u64, bytes: Vec<u8> },
    Mesh(u64),
    None,
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
#[allow(clippy::too_many_arguments)]
fn resolve_mesh(
    cache: &mut PackageCache,
    paris: &mut Forge,
    assets: &mut Forge,
    entity_entry: usize,
    id: u64,
    assets_entries: &HashMap<u64, usize>,
    paris_entries: &HashMap<u64, usize>,
    mesh_index: &Index,
    locations: &MeshLocationIndex,
    extra_archives: &[PathBuf],
    extra_forges: &mut Vec<Option<Forge>>,
) -> Result<Option<(MeshArchive, usize, usize)>> {
    cache.ensure(paris, entity_entry)?;
    {
        let (package, ids) = cache.view()?;
        if let Some(&position) = ids.get(&id)
            && package.resources[position].type_id == TYPE_WORLD_MESH
        {
            return Ok(Some((MeshArchive::Paris, entity_entry, position)));
        }
    }
    if let Some(&entry) = assets_entries.get(&id) {
        let package = assets.read_package(entry)?;
        if let Some(position) = package
            .resources
            .iter()
            .position(|r| r.type_id == TYPE_WORLD_MESH)
        {
            return Ok(Some((MeshArchive::Assets, entry, position)));
        }
    }
    let key = format!("{id:016x}");
    if let Some(entries) = mesh_index.get(&id) {
        for &entry in entries {
            let Ok(package) = paris.read_package(entry) else {
                continue;
            };
            if let Some(position) = package
                .resources
                .iter()
                .position(|r| r.file_id == key && r.type_id == TYPE_WORLD_MESH)
            {
                return Ok(Some((MeshArchive::Paris, entry, position)));
            }
        }
    }
    if let Some(&(slot, entry)) = locations.map.get(&id) {
        let slot = slot as usize;
        if slot < extra_archives.len() {
            if extra_forges.len() <= slot {
                extra_forges.resize_with(slot + 1, || None);
            }
            if extra_forges[slot].is_none() {
                extra_forges[slot] = Some(Forge::open(&extra_archives[slot], Limits::default())?);
            }
            let package = extra_forges[slot]
                .as_mut()
                .ok_or("extra archive failed to open")?
                .read_package(entry as usize)?;
            if let Some(position) = package
                .resources
                .iter()
                .position(|r| r.file_id == key && r.type_id == TYPE_WORLD_MESH)
            {
                return Ok(Some((
                    MeshArchive::Extra(slot as u8),
                    entry as usize,
                    position,
                )));
            }
        }
    }
    if let Some(&entry) = paris_entries.get(&id) {
        let Ok(package) = paris.read_package(entry) else {
            return Ok(None);
        };
        if let Some(position) = package
            .resources
            .iter()
            .position(|r| r.type_id == TYPE_WORLD_MESH)
        {
            return Ok(Some((MeshArchive::Paris, entry, position)));
        }
    }
    Ok(None)
}

/// Which archive a resolved mesh came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeshArchive {
    Assets,
    Paris,
    Extra(u8),
}

/// id -> (archive slot, entry) for meshes outside `DataPC_ACU_Paris.forge`, built from the
/// asset index by `tools/build_mesh_locations.py`. The census index stores only an entry
/// number, so without this a mesh in `DataPC_ACU_Paris_assets_patch_03.forge` entry 38 was
/// looked up in `DataPC_ACU_Paris.forge` entry 38 and lost.
struct MeshLocationIndex {
    slots: Vec<PathBuf>,
    map: HashMap<u64, (u8, u32)>,
}

fn read_mesh_locations(
    bin_name: &str,
    json_name: &str,
    game_dir: &std::path::Path,
) -> Result<MeshLocationIndex> {
    let find = |name: &str| -> Result<PathBuf> {
        let candidates = [
            PathBuf::from(name),
            PathBuf::from("..").join(name),
            PathBuf::from("../..").join(name),
        ];
        candidates
            .iter()
            .find(|p| p.exists())
            .cloned()
            .ok_or_else(|| format!("{name} not found; run tools/build_mesh_locations.py").into())
    };
    let text = std::fs::read_to_string(find(json_name)?)?;
    let names: Vec<String> = serde_json::from_str(&text)?;
    let slots: Vec<PathBuf> = names.iter().map(|name| game_dir.join(name)).collect();
    let bytes = std::fs::read(find(bin_name)?)?;
    if bytes.len() < 12 || &bytes[0..8] != b"ACUMES02" {
        return Err(format!("{bin_name}: not an ACUMES02 index").into());
    }
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if bytes.len() != 12 + count * 13 {
        return Err(format!("{bin_name}: length does not match {count} rows").into());
    }
    let mut map = HashMap::with_capacity(count);
    for index in 0..count {
        let at = 12 + index * 13;
        let id = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let entry = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap());
        let slot = bytes[at + 12];
        map.insert(id, (slot, entry));
    }
    Ok(MeshLocationIndex { slots, map })
}

/// Totals across every cell spawned by one invocation.
#[derive(Default)]
struct MapTotals {
    pieces: usize,
    cells: usize,
    cells_with_pieces: usize,
    children: usize,
    entities: usize,
    selectors: HashSet<u64>,
    mesh_refs: HashSet<u64>,
    lods: HashSet<usize>,
    reported: HashSet<u64>,
    skipped_matrix: usize,
    skipped_no_entity: usize,
    skipped_no_selector: usize,
    skipped_no_lod: usize,
    skipped_no_mesh: usize,
    skipped_decode: usize,
    decode_errors: HashSet<u64>,
    skipped_bad_matrix: usize,
    skipped_bad_vertices: usize,
    bad_geometry: HashSet<u64>,
    direct_meshes: usize,
    skipped_fake: usize,
    skipped_radius: usize,
}

/// State shared by every cell of one invocation.
struct MapContext<'a> {
    paris: &'a mut Forge,
    assets: &'a mut Forge,
    loader: &'a mut Loader,
    cache: &'a mut PackageCache,
    mesh_index: &'a Index,
    selector_index: &'a Index,
    locations: &'a MeshLocationIndex,
    extra_archives: &'a [PathBuf],
    extra_forges: &'a mut Vec<Option<Forge>>,
    assets_entries: &'a HashMap<u64, usize>,
    paris_ids: &'a HashMap<u64, usize>,
    paris_path: &'a PathBuf,
    assets_path: &'a PathBuf,
    wanted_lod: usize,
    /// Optional distance filter: native-space centre and radius in metres.
    centre: Option<[f32; 3]>,
    radius: Option<f32>,
    dry_run: bool,
    budget: usize,
    pieces: Vec<LoadedPiece>,
    /// mesh id -> index in `pieces`; each unique mesh is decoded once.
    mesh_cache: HashMap<u64, usize>,
    instances: Vec<MapInstance>,
    totals: MapTotals,
}

impl MapContext<'_> {
    fn cell_children(&mut self, cell_entry: usize) -> Result<Vec<u64>> {
        self.cache.ensure(&mut *self.paris, cell_entry)?;
        let (package, _) = self.cache.view()?;
        let block = package
            .resources
            .iter()
            .find(|r| r.type_id == TYPE_CELL)
            .ok_or("entry has no GridCellDataBlock resource")?;
        cell_children(&package.bytes[block.payload.clone()])
    }

    /// Spawn one world cell. The cell package carries the entities it places, so they are
    /// taken from its own resource table: walking the child list instead needs one package
    /// read per child, and a cell whose children live in many entries re-reads (and
    /// re-inflates) those packages thousands of times.
    ///
    /// The chain is decoded, not searched (ACU-RE-0037): each entity payload references a
    /// `LODSelector` resource in the same entry (found by resolving each reference slot
    /// against that entry's id table and keeping the one whose type is `51dc6b80`); the
    /// selector's five-record table gives the mesh ids of its LOD levels; the mesh id is
    /// resolved by type in the entity entry, an `_assets` entry, the Paris mesh index or a
    /// Paris entry. Records with `flags 3/0` are empty levels. Nothing is substituted.
    fn spawn_cell(&mut self, cell_entry: usize) -> Result<()> {
        let children = match self.cell_children(cell_entry) {
            Ok(children) => children,
            Err(error) => {
                eprintln!("map: cell {cell_entry}: {error}");
                self.totals.skipped_no_entity += 1;
                return Ok(());
            }
        };
        let entities: Vec<usize> = {
            let (package, _) = self.cache.view()?;
            package
                .resources
                .iter()
                .enumerate()
                .filter(|(_, resource)| resource.type_id == TYPE_ENTITY)
                .map(|(position, _)| position)
                .collect()
        };
        self.totals.cells += 1;
        self.totals.children += children.len();
        self.totals.entities += entities.len();
        let before = self.totals.pieces;
        for position in entities {
            if self.totals.pieces >= self.budget {
                eprintln!("map: piece limit {} reached", self.budget);
                break;
            }
            self.spawn_entity(cell_entry, position)?;
        }
        if self.totals.pieces > before {
            self.totals.cells_with_pieces += 1;
        }
        Ok(())
    }

    /// Mesh ids an entity payload references, even when the mesh is not in the entity's
    /// own entry. Candidates are restricted to ids that exist as `415d9568` resources.
    fn mesh_candidates(&self, bytes: &[u8]) -> Vec<u64> {
        let mut out = Vec::new();
        for offset in 0..bytes.len().saturating_sub(7) {
            let value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            if value == 0 || out.contains(&value) {
                continue;
            }
            if self.mesh_index.contains_key(&value)
                || self.assets_entries.contains_key(&value)
                || self.locations.map.contains_key(&value)
            {
                out.push(value);
                if out.len() >= 32 {
                    break;
                }
            }
        }
        out
    }

    /// Resource name of a mesh id, wherever it lives: `_assets` entry, Paris mesh index,
    /// or an archive from the locations index.
    fn mesh_name(&mut self, id: u64) -> Result<Option<String>> {
        if let Some(&entry) = self.assets_entries.get(&id) {
            let Ok(package) = self.assets.read_package(entry) else {
                return Ok(None);
            };
            if let Some(position) = package
                .resources
                .iter()
                .position(|r| r.type_id == TYPE_WORLD_MESH)
            {
                return Ok(Some(package.resources[position].name.clone()));
            }
        }
        let key = format!("{id:016x}");
        if let Some(entries) = self.mesh_index.get(&id) {
            for &entry in entries.iter().take(4) {
                let Ok(package) = self.paris.read_package(entry) else {
                    continue;
                };
                if let Some(position) = package
                    .resources
                    .iter()
                    .position(|r| r.file_id == key && r.type_id == TYPE_WORLD_MESH)
                {
                    return Ok(Some(package.resources[position].name.clone()));
                }
            }
        }
        if let Some(&(slot, entry)) = self.locations.map.get(&id) {
            let slot = slot as usize;
            if slot < self.extra_archives.len() {
                if self.extra_forges.len() <= slot {
                    self.extra_forges.resize_with(slot + 1, || None);
                }
                if self.extra_forges[slot].is_none() {
                    self.extra_forges[slot] =
                        Some(Forge::open(&self.extra_archives[slot], Limits::default())?);
                }
                if let Some(forge) = self.extra_forges[slot].as_mut() {
                    if let Ok(package) = forge.read_package(entry as usize) {
                        if let Some(position) = package
                            .resources
                            .iter()
                            .position(|r| r.file_id == key && r.type_id == TYPE_WORLD_MESH)
                        {
                            return Ok(Some(package.resources[position].name.clone()));
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    /// Selector resource referenced from another entry: first payload id that the selector
    /// index knows, resolved and validated like a local one.
    fn global_selector(&mut self, bytes: &[u8]) -> Result<Option<(u64, Vec<u8>)>> {
        let mut tried = 0;
        for offset in 0..bytes.len().saturating_sub(7) {
            let value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            if value == 0 || !self.selector_index.contains_key(&value) {
                continue;
            }
            tried += 1;
            if tried > 4 {
                break;
            }
            let key = format!("{value:016x}");
            for &entry in self
                .selector_index
                .get(&value)
                .into_iter()
                .flatten()
                .take(4)
            {
                let Ok(package) = self.paris.read_package(entry) else {
                    continue;
                };
                if let Some(position) = package
                    .resources
                    .iter()
                    .position(|r| r.type_id == TYPE_SELECTOR && r.file_id == key)
                {
                    let payload =
                        package.bytes[package.resources[position].payload.clone()].to_vec();
                    return Ok(Some((value, payload)));
                }
            }
        }
        Ok(None)
    }

    /// Spawn the entity at `position` inside `cell_entry`'s package.
    fn spawn_entity(&mut self, entity_entry: usize, position: usize) -> Result<()> {
        let (entity_bytes, link) = {
            let (package, ids) = self.cache.view()?;
            let entity = &package.resources[position];
            let bytes = package.bytes[entity.payload.clone()].to_vec();
            let link = if let Some((selector_position, id)) =
                find_component(package, ids, &bytes, TYPE_SELECTOR)
            {
                GeometryLink::Selector {
                    id,
                    bytes: package.bytes[package.resources[selector_position].payload.clone()]
                        .to_vec(),
                }
            } else if let Some((_, id)) = find_component(package, ids, &bytes, TYPE_WORLD_MESH) {
                GeometryLink::Mesh(id)
            } else {
                GeometryLink::None
            };
            (bytes, link)
        };
        let Some(matrix) = entity_matrix(&entity_bytes) else {
            self.totals.skipped_matrix += 1;
            return Ok(());
        };
        // A garbage matrix (or a decoder returning uninitialised vertices) would otherwise
        // poison the whole map's bounds and fly the camera out of the world.
        if matrix.iter().any(|v| !v.is_finite() || v.abs() > 100_000.0) {
            self.totals.skipped_bad_matrix += 1;
            if self
                .totals
                .bad_geometry
                .insert(matrix[12] as u64 ^ matrix[13] as u64)
            {
                eprintln!("map: entity matrix out of range: {matrix:?}");
            }
            return Ok(());
        }
        if let (Some(centre), Some(radius)) = (self.centre, self.radius) {
            let dx = matrix[12] - centre[0];
            let dy = matrix[13] - centre[1];
            let dz = matrix[14] - centre[2];
            if dx * dx + dy * dy + dz * dz > radius * radius {
                self.totals.skipped_radius += 1;
                return Ok(());
            }
        }
        let mut link = link;
        if matches!(link, GeometryLink::None) {
            // The mesh or selector may live in another package; both fallbacks are
            // type-checked against the census indices, and a mesh candidate must also
            // share its name with the entity (the payload can mention unrelated props).
            if let Some((id, payload)) = self.global_selector(&entity_bytes)? {
                link = GeometryLink::Selector { id, bytes: payload };
            } else {
                let entity_name = {
                    let (package, _) = self.cache.view()?;
                    package.resources[position].name.clone()
                };
                let candidates = self.mesh_candidates(&entity_bytes);
                if std::env::var_os("ACU_MAP_TRACE").is_some()
                    && entity_name.starts_with("Largemesh")
                {
                    eprintln!("fallback: {entity_name} candidates {candidates:?}");
                }
                let mut best: Option<(usize, u64)> = None;
                for id in candidates {
                    let Some(name) = self.mesh_name(id)? else {
                        continue;
                    };
                    let common = name
                        .chars()
                        .zip(entity_name.chars())
                        .take_while(|(a, b)| a == b)
                        .count();
                    if common >= 12 && best.map(|(score, _)| common > score).unwrap_or(true) {
                        best = Some((common, id));
                    }
                }
                if let Some((score, id)) = best {
                    if std::env::var_os("ACU_MAP_TRACE").is_some()
                        && entity_name.starts_with("Largemesh")
                    {
                        eprintln!("fallback: {entity_name} -> {id:016x} score {score}");
                    }
                    link = GeometryLink::Mesh(id);
                }
            }
        }
        let (mesh_id, level) = match link {
            GeometryLink::Selector { id, bytes } => {
                let records = match selector_records(&bytes, id) {
                    Ok(records) => records,
                    Err(error) => {
                        eprintln!("map: {error}");
                        self.totals.skipped_no_lod += 1;
                        return Ok(());
                    }
                };
                // Prefer the requested LOD; fall back to the finest available, never to a
                // wrong level.
                let chosen = records
                    .iter()
                    .find(|record| record.level >= self.wanted_lod)
                    .or_else(|| records.last());
                let Some(record) = chosen else {
                    self.totals.skipped_no_lod += 1;
                    return Ok(());
                };
                if self.totals.reported.insert(id) {
                    let table = records
                        .iter()
                        .map(|entry| format!("{}@{:.0}m", entry.level, entry.distance))
                        .collect::<Vec<_>>()
                        .join(" ");
                    eprintln!(
                        "map: selector {id:016x} LOD table [{table}], chose LOD{}",
                        record.level
                    );
                }
                self.totals.selectors.insert(id);
                (record.entry_id, Some(record.level))
            }
            GeometryLink::Mesh(id) => {
                self.totals.direct_meshes += 1;
                (id, None)
            }
            GeometryLink::None => {
                self.totals.skipped_no_selector += 1;
                if std::env::var_os("ACU_MAP_MISSING").is_some() {
                    let (package, _) = self.cache.view()?;
                    eprintln!(
                        "map-missing: {} {}",
                        package.resources[position].file_id, package.resources[position].name
                    );
                }
                return Ok(());
            }
        };
        let Some((source, mesh_entry, resource)) = resolve_mesh(
            &mut *self.cache,
            &mut *self.paris,
            &mut *self.assets,
            entity_entry,
            mesh_id,
            self.assets_entries,
            self.paris_ids,
            self.mesh_index,
            self.locations,
            self.extra_archives,
            self.extra_forges,
        )?
        else {
            eprintln!("map: mesh {mesh_id:016x} is in no selected archive");
            self.totals.skipped_no_mesh += 1;
            return Ok(());
        };
        self.totals.mesh_refs.insert(mesh_id);
        if let Some(level) = level {
            self.totals.lods.insert(level);
        }
        self.totals.pieces += 1;
        if self.dry_run {
            return Ok(());
        }
        // One decode per unique mesh; each placement becomes an instance with its matrix.
        let piece_index = match self.mesh_cache.get(&mesh_id) {
            Some(&index) => index,
            None => {
                let archive = match source {
                    MeshArchive::Assets => self.assets_path.clone(),
                    MeshArchive::Paris => self.paris_path.clone(),
                    MeshArchive::Extra(slot) => self
                        .extra_archives
                        .get(slot as usize)
                        .cloned()
                        .unwrap_or_else(|| self.paris_path.clone()),
                };
                let piece = Piece {
                    archive,
                    entry: mesh_entry,
                    resource,
                    label: format!("mesh-{mesh_id:016x}"),
                };
                if std::env::var_os("ACU_MAP_TRACE").is_some() {
                    eprintln!(
                        "map: decode mesh {mesh_id:016x} source={source:?} entry={mesh_entry} resource={resource}"
                    );
                }
                let loaded = match source {
                    MeshArchive::Assets => load_piece(&mut *self.loader, &mut *self.assets, &piece),
                    MeshArchive::Paris if mesh_entry == entity_entry => {
                        // Same entry we already decoded for this cell: reuse its bytes.
                        self.cache.ensure(&mut *self.paris, entity_entry)?;
                        let (package, _) = self.cache.view()?;
                        let stored = self.cache.stored_sha256.clone();
                        piece_from(&mut *self.loader, package, &piece, stored)
                    }
                    MeshArchive::Paris => load_piece(&mut *self.loader, &mut *self.paris, &piece),
                    MeshArchive::Extra(slot) => {
                        let slot = slot as usize;
                        if self.extra_forges.len() <= slot {
                            self.extra_forges.resize_with(slot + 1, || None);
                        }
                        if self.extra_forges[slot].is_none() {
                            self.extra_forges[slot] =
                                Some(Forge::open(&self.extra_archives[slot], Limits::default())?);
                        }
                        let forge = self.extra_forges[slot]
                            .as_mut()
                            .ok_or("extra archive failed to open")?;
                        load_piece(&mut *self.loader, forge, &piece)
                    }
                };
                let loaded = match loaded {
                    Ok(loaded) => loaded,
                    Err(error) => {
                        // One unsupported mesh must not abort the map; it is named and
                        // counted here instead.
                        if self.totals.decode_errors.insert(mesh_id) {
                            eprintln!(
                                "map: mesh {mesh_id:016x} (entry {mesh_entry}) not decoded: {error}"
                            );
                        }
                        self.totals.skipped_decode += 1;
                        self.totals.pieces -= 1;
                        self.totals.mesh_refs.remove(&mesh_id);
                        return Ok(());
                    }
                };
                // `FAKE_*` resources are distant scenery backdrops (fake ground, coast and
                // city silhouettes) that span the whole map. Rendered from above they hide
                // the city and inflate the scene bounds, so map mode leaves them out and
                // says how many it dropped.
                if loaded.label.starts_with("FAKE_") {
                    self.totals.skipped_fake += 1;
                    self.totals.pieces -= 1;
                    self.totals.mesh_refs.remove(&mesh_id);
                    return Ok(());
                }
                if loaded.vertices.iter().any(|v| {
                    v.position
                        .iter()
                        .any(|c| !c.is_finite() || c.abs() > 100_000.0)
                }) {
                    if self.totals.bad_geometry.insert(mesh_id) {
                        eprintln!("map: mesh {mesh_id:016x} has out-of-range vertices; skipped");
                    }
                    self.totals.skipped_bad_vertices += 1;
                    self.totals.pieces -= 1;
                    self.totals.mesh_refs.remove(&mesh_id);
                    return Ok(());
                }
                let index = self.pieces.len();
                self.pieces.push(loaded);
                self.mesh_cache.insert(mesh_id, index);
                index
            }
        };
        self.instances.push(MapInstance {
            piece: piece_index,
            matrix,
        });
        Ok(())
    }
}

/// One cell or a JSON list of cells, spawned through the decoded chain.
pub fn load_map(
    options: &Options,
    loader: &mut Loader,
) -> Result<(Vec<LoadedPiece>, Vec<MapInstance>)> {
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
    let mesh_index = read_index("local/paris-world-meshes.bin")?;
    let selector_index = read_index("local/paris-selectors.bin")?;
    let locations = read_mesh_locations(
        "local/paris-mesh-locations.bin",
        "local/paris-mesh-locations.json",
        &options.game_dir,
    )?;
    let extra_archives = locations.slots.clone();
    let mut extra_forges: Vec<Option<Forge>> = (0..extra_archives.len()).map(|_| None).collect();
    let mut assets = Forge::open(&assets_path, limits)?;
    let assets_entries = id_map(&assets);

    let mut cell_entries: Vec<usize> = Vec::new();
    if let Some(path) = &options.map_cells {
        #[derive(serde::Deserialize)]
        struct Row {
            entry_index: usize,
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("--map-cells {}: {e}", path.display()))?;
        let rows: Vec<Row> = serde_json::from_str(&text)
            .map_err(|e| format!("--map-cells {}: {e}", path.display()))?;
        cell_entries.extend(rows.into_iter().map(|row| row.entry_index));
    }
    if let Some(spec) = &options.map_cell {
        let entry = match spec.parse::<usize>() {
            Ok(index) => index,
            Err(_) => {
                let id = u64::from_str_radix(spec.trim_start_matches("0x"), 16)
                    .map_err(|_| "map cell must be an entry index or a hex file id")?;
                *paris_ids
                    .get(&id)
                    .ok_or("cell file id is not an entry in the Paris archive")?
            }
        };
        cell_entries.push(entry);
    }
    if cell_entries.is_empty() {
        return Err("map mode without --map-cell or --map-cells".into());
    }
    let budget = options
        .map_piece_limit
        .unwrap_or(if options.map_cells.is_some() {
            200_000
        } else {
            MAP_PIECE_LIMIT
        });

    let mut cache = PackageCache::default();
    let mut context = MapContext {
        paris: &mut paris,
        assets: &mut assets,
        loader,
        cache: &mut cache,
        mesh_index: &mesh_index,
        selector_index: &selector_index,
        locations: &locations,
        extra_archives: &extra_archives,
        extra_forges: &mut extra_forges,
        assets_entries: &assets_entries,
        paris_ids: &paris_ids,
        paris_path: &paris_path,
        assets_path: &assets_path,
        wanted_lod: options.map_lod as usize,
        // --spawn is Bevy (x, y up, z); an instance matrix is native (x, y, z up), so
        // native = (x, -z, y).
        centre: options.spawn.map(|[x, y, z]| [x, -z, y]),
        radius: options.map_radius,
        dry_run: options.map_dry_run,
        budget,
        pieces: Vec::new(),
        mesh_cache: HashMap::new(),
        instances: Vec::new(),
        totals: MapTotals::default(),
    };
    eprintln!(
        "map: {} cells to spawn, piece budget {budget}",
        cell_entries.len()
    );
    for cell in cell_entries {
        if context.totals.pieces >= budget {
            eprintln!("map: piece budget reached before cell {cell}");
            break;
        }
        context.spawn_cell(cell)?;
    }

    let totals = &context.totals;
    let mut lods: Vec<usize> = totals.lods.iter().copied().collect();
    lods.sort_unstable();
    let lod_list: Vec<String> = lods.iter().map(|level| level.to_string()).collect();
    eprintln!(
        "map: {} pieces from {}/{} cells ({} entities, {} child refs), {} distinct mesh ids, \
         {} distinct selectors, LODs [{}]; skipped: {} without a matrix, {} without a \
         selector, {} without a LOD record, {} without a mesh, {} undecodable meshes, \
         {} out-of-range matrices, {} out-of-range meshes, {} direct-mesh entities, \
         {} FAKE backdrops, {} outside the radius, {} cells without a cell block",
        totals.pieces,
        totals.cells_with_pieces,
        totals.cells,
        totals.entities,
        totals.children,
        totals.mesh_refs.len(),
        totals.selectors.len(),
        lod_list.join(","),
        totals.skipped_matrix,
        totals.skipped_no_selector,
        totals.skipped_no_lod,
        totals.skipped_no_mesh,
        totals.skipped_decode,
        totals.skipped_bad_matrix,
        totals.skipped_bad_vertices,
        totals.direct_meshes,
        totals.skipped_fake,
        totals.skipped_radius,
        totals.skipped_no_entity
    );
    Ok((context.pieces, context.instances))
}

/// Load every piece, reusing one open Forge per archive.
pub fn load_all(options: &Options) -> Result<Loaded> {
    let limits = Limits::default();
    let mut forges: HashMap<PathBuf, Forge> = HashMap::new();
    let mut loader = Loader::new(options);
    let mut pieces = Vec::new();
    let mut instances = Vec::new();
    if options.map_cell.is_some() || options.map_cells.is_some() {
        (pieces, instances) = load_map(options, &mut loader)?;
    }
    // Map mode ignores the rig unless --map-player asked for it.
    let map_mode = options.map_cell.is_some() || options.map_cells.is_some();
    let pet: &[Piece] = if map_mode && !options.map_player {
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
    Ok(Loaded {
        pieces,
        instances,
        scene,
    })
}
