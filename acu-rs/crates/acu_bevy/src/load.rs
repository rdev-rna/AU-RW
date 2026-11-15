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
    Limits, Result, bc1, build_table,
    catalog::{Catalog, CatalogLimits},
    color_mask,
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
    /// Colour-set recolouring applied to this material (ACU-RE-0071).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_mask: Option<serde_json::Value>,
    /// Template surface inputs (mask, detail maps, parameters; ACU-RE-0084).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloth_surface: Option<serde_json::Value>,
    /// Registered textures and parameters for the per-pixel cloth shader.
    #[serde(skip)]
    pub surface: Option<ClothSurface>,
}

/// Inputs of the cloth template's per-pixel surface (ACU-RE-0084).
#[derive(Debug, Clone)]
pub struct ClothSurface {
    pub params: color_mask::Surface,
    /// t34 mask (linear): R colour zone, G band, B specular occlusion.
    pub mask: usize,
    /// Detail normal + gloss maps t36 (metal), t38 (cloth / mid), t39 (base).
    pub details: [Option<usize>; 3],
}

/// DXGI formats the Bevy view uploads (`main.rs`).
const UPLOADABLE: [u32; 13] = [71, 72, 74, 75, 77, 78, 98, 99, 28, 29, 87, 91, 83];

/// The texture map a material's `TextureSelector` parameter names.
fn selector_map(m: &material::Material, hash: u32) -> Option<String> {
    let name = format!("{hash:08x}");
    m.parameters
        .iter()
        .find(|p| p.name_hash == name)
        .and_then(|p| match &p.value {
            material::ParameterValue::Object(o) => match &o.data {
                material::ObjectData::TextureSelector(s) => Some(s.texture_map.file_id.clone()),
                _ => None,
            },
            _ => None,
        })
        .filter(|id| id != "0000000000000000")
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
    /// Selected `CN_P_Avatars_ColorSets` row and what it writes.
    pub color_set: Option<serde_json::Value>,
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
    color_set: Option<usize>,
    /// Decoded once: the selected colour-set row's material overrides.
    color_row: Option<Option<ColorSetRow>>,
}

/// `CN_P_Avatars_ColorSets` (BelleEpoque 334/127), ACU-RE-0071.
const COLOR_SET_TABLE: &str = "0000001e7c52e192";

#[derive(Debug, Clone)]
struct ColorOverride {
    column: i32,
    /// Material match mask of the column's `MaterialPropertyPathNodeSolver`.
    mask: [u8; 16],
    target: u32,
    value: [f32; 4],
}

#[derive(Debug, Clone)]
struct ColorSetRow {
    row: usize,
    overrides: Vec<ColorOverride>,
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
            color_set: options.color_set,
            color_row: None,
        }
    }

    /// Decode the colour-set table once and keep the selected row's
    /// single-property material writes. Other columns are reported, not applied.
    fn color_row(&mut self) -> Result<Option<ColorSetRow>> {
        if let Some(row) = &self.color_row {
            return Ok(row.clone());
        }
        let Some(index) = self.color_set else {
            self.color_row = Some(None);
            return Ok(None);
        };
        let resolved = self
            .resolve(COLOR_SET_TABLE, None, 0)?
            .ok_or("colour-set table 0000001e7c52e192 not found in the selected archives")?;
        let table = build_table::parse(&resolved.payload, COLOR_SET_TABLE)?;
        let row = table.rows.get(index).ok_or_else(|| {
            format!(
                "--color-set {index}: table has rows 0..{}",
                table.rows.len()
            )
        })?;
        let (mut overrides, mut skipped) = (Vec::new(), Vec::new());
        for (column, property) in &row.components {
            let path = &table
                .column(*column)
                .ok_or("row references a missing column")?
                .path;
            let solver = path.first().and_then(|n| n.solver.as_ref());
            let targets: Vec<&str> = path
                .iter()
                .filter(|n| n.solver.is_none())
                .map(|n| n.target_property.as_str())
                .collect();
            match (solver, targets.as_slice(), &property.value) {
                (Some(s), [t], build_table::Value::Vector4(v)) => overrides.push(ColorOverride {
                    column: *column,
                    mask: s.match_mask,
                    target: u32::from_str_radix(t, 16)?,
                    value: *v,
                }),
                _ => skipped.push(serde_json::json!({
                    "column": column, "path": targets, "value": property.value,
                    "reason": "not a single-property vector write; no albedo effect modelled"})),
            }
        }
        self.scene.color_set = Some(serde_json::json!({
            "row": index, "tag": format!("{:08x}", row.tag), "weight": row.weight,
            "table": COLOR_SET_TABLE, "source": resolved.source,
            "payload_sha256": resolved.payload_sha256, "rows": table.rows.len(),
            "applied_columns": overrides.iter().map(|o| serde_json::json!({
                "column": o.column, "target": format!("{:08x}", o.target), "value": o.value})).collect::<Vec<_>>(),
            "skipped_columns": skipped,
            "finding": "ACU-RE-0071"}));
        let row = ColorSetRow {
            row: index,
            overrides,
        };
        self.color_row = Some(Some(row.clone()));
        Ok(Some(row))
    }

    /// Bake the template's recoloured albedo for one material, or `None` when
    /// no colour set is selected.
    fn color_mask_bake(
        &mut self,
        m: &material::Material,
        diffuse: usize,
        package: Option<&package::Package>,
        entry: usize,
    ) -> Result<Option<(usize, serde_json::Value)>> {
        let Some(row) = self.color_row()? else {
            return Ok(None);
        };
        let mut params = color_mask::Params::from_material(m)
            .ok_or("material lacks the template's colour parameters")?;
        let own = params;
        let mut applied = Vec::new();
        for o in &row.overrides {
            let matches = o
                .mask
                .iter()
                .zip(&m.match_mask.material_match_mask)
                .any(|(a, b)| a & b != 0);
            if matches && params.set(o.target, o.value) {
                applied.push(format!("{:08x}", o.target));
            }
        }
        let selector = |hash: u32| {
            let name = format!("{hash:08x}");
            m.parameters
                .iter()
                .find(|p| p.name_hash == name)
                .and_then(|p| match &p.value {
                    material::ParameterValue::Object(o) => match &o.data {
                        material::ObjectData::TextureSelector(s) => {
                            Some(s.texture_map.file_id.clone())
                        }
                        _ => None,
                    },
                    _ => None,
                })
        };
        let mask_id = selector(color_mask::MASK_SELECTOR).ok_or("no mask (t34) selector")?;
        let tint_id = selector(color_mask::TINT_SELECTOR).ok_or("no tint (t35) selector")?;
        let mask = self
            .texture(&mask_id, package, entry, false)?
            .ok_or_else(|| format!("mask {mask_id} unavailable"))?;
        let tint_index = self
            .texture(&tint_id, package, entry, true)?
            .ok_or_else(|| format!("tint map {tint_id} unavailable"))?;
        let tint = uniform_tint(&self.scene.textures[tint_index])?;
        let (width, height, pixels, mips) = bake_color_mask(
            &self.scene.textures[diffuse],
            &self.scene.textures[mask],
            tint,
            &params.prepare(),
        )?;
        let d = &self.scene.textures[diffuse];
        let baked = LoadedTexture {
            srgb: true,
            file_id: d.file_id.clone(),
            name: format!("{} recoloured (colour set row {})", d.name, row.row),
            source: format!(
                "derived: diffuse {} + mask {mask_id} + ColorSets row {} (ACU-RE-0071)",
                d.file_id, row.row
            ),
            payload_sha256: String::new(),
            pixel_sha256: sha256(&pixels),
            dxgi_format: 29,
            gamma: 1,
            width,
            height,
            mip_count: mips,
            storage: "derived",
            pixels,
        };
        let index = self.scene.textures.len();
        self.scene.textures.push(baked);
        Ok(Some((
            index,
            serde_json::json!({"row": row.row, "own": own, "applied": applied, "params": params,
                "mask": mask_id, "tint_map": tint_id, "tint": tint, "baked_texture": index,
                "size": [width, height], "template": color_mask::TEMPLATE_CLOTH_COLOR_MASKED_ALPHA_TEST}),
        )))
    }

    /// Resolve the cloth template's mask and detail maps for the per-pixel
    /// surface shader.
    fn cloth_surface(
        &mut self,
        m: &material::Material,
        package: Option<&package::Package>,
        entry: usize,
    ) -> Result<(ClothSurface, serde_json::Value)> {
        let params = color_mask::Surface::from_material(m)
            .ok_or("material lacks the template's surface parameters")?;
        let mask_id = selector_map(m, color_mask::MASK_SELECTOR).ok_or("no mask (t34) selector")?;
        let mask = self
            .texture(&mask_id, package, entry, false)?
            .ok_or_else(|| format!("mask {mask_id} unavailable"))?;
        let t = &self.scene.textures[mask];
        if !UPLOADABLE.contains(&t.dxgi_format) || t.gamma != 0 {
            return Err(format!(
                "mask {mask_id}: format {} gamma {} not supported",
                t.dxgi_format, t.gamma
            )
            .into());
        }
        let mut details = [None; 3];
        let mut report = Vec::new();
        for (slot, hash) in [
            color_mask::DETAIL_METAL,
            color_mask::DETAIL_CLOTH,
            color_mask::DETAIL_BASE,
        ]
        .into_iter()
        .enumerate()
        {
            let id = selector_map(m, hash);
            let index = match &id {
                Some(id) => self.texture(id, package, entry, false)?,
                None => None,
            }
            .filter(|&i| UPLOADABLE.contains(&self.scene.textures[i].dxgi_format));
            details[slot] = index;
            report.push(serde_json::json!({
                "param": format!("{hash:08x}"),
                "map": id,
                "texture": index.map(|i| self.scene.textures[i].name.clone()),
                "dxgi_format": index.map(|i| self.scene.textures[i].dxgi_format),
            }));
        }
        let surface = ClothSurface {
            params,
            mask,
            details,
        };
        Ok((
            surface,
            serde_json::json!({"params": params, "mask": mask_id, "details": report,
                "template": color_mask::TEMPLATE_CLOTH_COLOR_MASKED_ALPHA_TEST}),
        ))
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
        let mut diffuse_texture = diffuse_texture;
        let mut color_mask_report = None;
        if let Some(di) = diffuse_texture
            && parsed.template.file_id == color_mask::TEMPLATE_CLOTH_COLOR_MASKED_ALPHA_TEST
        {
            match self.color_mask_bake(&parsed, di, package, entry) {
                Ok(Some((index, report))) => {
                    diffuse_texture = Some(index);
                    color_mask_report = Some(report);
                }
                Ok(None) => {}
                Err(e) => notes.push(format!("colour mask not applied: {e}")),
            }
        }
        let (mut surface, mut cloth_surface) = (None, None);
        if parsed.template.file_id == color_mask::TEMPLATE_CLOTH_COLOR_MASKED_ALPHA_TEST {
            match self.cloth_surface(&parsed, package, entry) {
                Ok((s, report)) => {
                    surface = Some(s);
                    cloth_surface = Some(report);
                }
                Err(e) => notes.push(format!("cloth surface not applied: {e}")),
            }
        }
        let index = self.scene.materials.len();
        self.scene.materials.push(LoadedMaterial {
            color_mask: color_mask_report,
            cloth_surface,
            surface,
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

/// The blue value of a uniform texture (the template's t35 tint map: the
/// shader samples it with swizzle `.xzwy` into `.yz`, so the tint mix is
/// channel z and the coverage factor channel w); fails on a non-uniform map,
/// whose UV transform this bake does not model.
fn uniform_tint(t: &LoadedTexture) -> Result<f32> {
    if t.dxgi_format != 71 {
        return Err(format!(
            "tint map {}: unsupported storage {}",
            t.file_id, t.dxgi_format
        )
        .into());
    }
    let px = bc1::decode(t.width, t.height, &t.pixels)?;
    let r = px[0][2];
    if px.iter().any(|p| p[2] != r) {
        return Err(format!(
            "tint map {} is not uniform; its UV transform is not modelled",
            t.file_id
        )
        .into());
    }
    Ok(if t.gamma == 1 {
        color_mask::srgb_to_linear(r)
    } else {
        r as f32 / 255.0
    })
}

fn bilinear(px: &[[f32; 4]], w: u32, h: u32, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (w as i64, h as i64);
    let x = u * w as f32 - 0.5;
    let y = v * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xi: i64, yi: i64| px[(yi.rem_euclid(h) * w + xi.rem_euclid(w)) as usize];
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    );
    std::array::from_fn(|i| {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        top + (bottom - top) * fy
    })
}

/// CPU bake of the colour-masked template albedo (ACU-RE-0071) at the larger
/// of the diffuse/mask resolutions, with a box-filtered mip chain. Returns
/// (width, height, RGBA8 sRGB levels, level count).
fn bake_color_mask(
    diffuse: &LoadedTexture,
    mask: &LoadedTexture,
    tint: f32,
    prepared: &color_mask::Prepared,
) -> Result<(u32, u32, Vec<u8>, u32)> {
    if diffuse.dxgi_format != 71 || mask.dxgi_format != 71 {
        return Err(format!(
            "unsupported storage: diffuse {} mask {} (BC1 only)",
            diffuse.dxgi_format, mask.dxgi_format
        )
        .into());
    }
    if mask.gamma != 0 {
        return Err(format!("mask {} is not linear (gamma {})", mask.file_id, mask.gamma).into());
    }
    let decode = |c: u8| {
        if diffuse.gamma == 1 {
            color_mask::srgb_to_linear(c)
        } else {
            c as f32 / 255.0
        }
    };
    let d: Vec<[f32; 4]> = bc1::decode(diffuse.width, diffuse.height, &diffuse.pixels)?
        .iter()
        .map(|p| {
            [
                decode(p[0]),
                decode(p[1]),
                decode(p[2]),
                p[3] as f32 / 255.0,
            ]
        })
        .collect();
    let k: Vec<[f32; 4]> = bc1::decode(mask.width, mask.height, &mask.pixels)?
        .iter()
        .map(|p| p.map(|c| c as f32 / 255.0))
        .collect();
    let (w, h) = (
        diffuse.width.max(mask.width),
        diffuse.height.max(mask.height),
    );
    let mut level: Vec<[f32; 4]> = (0..(w * h) as usize)
        .map(|i| {
            let (x, y) = ((i % w as usize) as f32, (i / w as usize) as f32);
            let (u, v) = ((x + 0.5) / w as f32, (y + 0.5) / h as f32);
            let c = bilinear(&d, diffuse.width, diffuse.height, u, v);
            let m = bilinear(&k, mask.width, mask.height, u, v);
            let a = prepared.albedo([c[0], c[1], c[2]], [m[0], m[1], m[2]], tint);
            [a[0], a[1], a[2], c[3]]
        })
        .collect();
    let (mut lw, mut lh) = (w as usize, h as usize);
    let mut out = Vec::new();
    let mut mips = 0;
    loop {
        for p in &level {
            out.extend([
                color_mask::linear_to_srgb(p[0]),
                color_mask::linear_to_srgb(p[1]),
                color_mask::linear_to_srgb(p[2]),
                (p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
            ]);
        }
        mips += 1;
        if lw == 1 && lh == 1 {
            break;
        }
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        level = (0..nw * nh)
            .map(|i| {
                let (x, y) = (i % nw, i / nw);
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = level[(y * 2 + dy).min(lh - 1) * lw + (x * 2 + dx).min(lw - 1)];
                    for (a, v) in acc.iter_mut().zip(p) {
                        *a += v * 0.25;
                    }
                }
                acc
            })
            .collect();
        (lw, lh) = (nw, nh);
    }
    Ok((w, h, out, mips))
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

/// Highest number of placeable meshes one invocation will spawn.
const MAP_PIECE_LIMIT: usize = 400;

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

fn read_index(name: &str) -> Result<HashMap<u64, usize>> {
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
    let mut map = HashMap::with_capacity(count);
    for index in 0..count {
        let at = 12 + index * 12;
        let id = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let entry = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
        map.insert(id, entry);
    }
    Ok(map)
}

/// Spawn one world cell: children -> Entity -> LODSelector -> world mesh, with each
/// entity's matrix baked into the vertices so the existing spawn and camera path works.
///
/// Selector and mesh ids are *resource* ids, so both are resolved through compact
/// indices built from the census (`tools/build_resource_index.py`). An entity without a
/// decodable matrix, or whose selector resolves to nothing, is skipped and counted,
/// never guessed.
pub fn load_map(options: &Options, loader: &mut Loader) -> Result<Vec<LoadedPiece>> {
    let limits = Limits::default();
    let paris_path = options
        .map_paris
        .clone()
        .unwrap_or_else(|| options.game_dir.join("DataPC_ACU_Paris.forge"));
    let _assets_path = options
        .map_assets
        .clone()
        .unwrap_or_else(|| options.game_dir.join("DataPC_ACU_Paris_assets.forge"));
    let mut paris = Forge::open(&paris_path, limits)?;
    let paris_ids = id_map(&paris);
    let selector_index = read_index("local/paris-selectors.bin")?;
    let mesh_index = read_index("local/paris-world-meshes.bin")?;
    let entity_index = read_index("local/paris-entities.bin")?;
    let selector_names: HashMap<String, usize> = serde_json::from_str(
        &std::fs::read_to_string("local/paris-selector-names.json")
            .or_else(|_| std::fs::read_to_string("../local/paris-selector-names.json"))?,
    )?;
    let mut assets = Forge::open(&_assets_path, limits)?;
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

    let package = paris.read_package(cell_entry)?;
    let block = package
        .resources
        .iter()
        .find(|r| r.type_id == TYPE_CELL)
        .ok_or("entry has no GridCellDataBlock resource")?;
    let children = cell_children(&package.bytes[block.payload.clone()])?;
    eprintln!("map: cell {} with {} children", spec, children.len());

    let mut resolved: HashMap<u64, Vec<(usize, usize, bool)>> = HashMap::new();
    let mut pieces = Vec::new();
    let mut skipped_matrix = 0usize;
    let mut skipped_no_mesh = 0usize;
    for child in children {
        if pieces.len() >= MAP_PIECE_LIMIT {
            eprintln!("map: piece limit {MAP_PIECE_LIMIT} reached");
            break;
        }
        let Some(&entry_index) = entity_index.get(&child) else {
            continue;
        };
        let child_package = paris.read_package(entry_index)?;
        let Some(entity) = child_package
            .resources
            .iter()
            .find(|r| r.type_id == TYPE_ENTITY)
        else {
            continue;
        };
        let entity_bytes = &child_package.bytes[entity.payload.clone()];
        let Some(matrix) = entity_matrix(entity_bytes) else {
            skipped_matrix += 1;
            continue;
        };

        let entity_name = entity.name.clone();
        // Harvest the meshes once per selector, not once per entity.
        let meshes = match resolved.get(&child) {
            Some(list) => list.clone(),
            None => {
                let mut list = Vec::new();
                // The selector reference sits at a fixed offset: in four unrelated
                // entities (CanalKit, Wall, Gate, WallRiver) the id is at 100 and again
                // at 122, 22 bytes apart, with the materials further on. Scan only as a
                // fallback, because a bare id match elsewhere in the payload collides
                // with unrelated selectors.
                let fixed = if entity_bytes.len() >= 108 {
                    Some(u64::from_le_bytes(
                        entity_bytes[100..108].try_into().unwrap(),
                    ))
                } else {
                    None
                };
                let mut candidates: Vec<u64> = Vec::new();
                if let Some(id) = fixed {
                    if selector_index.contains_key(&id) {
                        candidates.push(id);
                    }
                }
                let scan: Vec<u64> = if candidates.is_empty() {
                    (0..entity_bytes.len().saturating_sub(7))
                        .map(|offset| {
                            u64::from_le_bytes(entity_bytes[offset..offset + 8].try_into().unwrap())
                        })
                        .collect()
                } else {
                    std::mem::take(&mut candidates)
                };
                for candidate in scan {
                    let Some(&selector_entry) = selector_index.get(&candidate) else {
                        continue;
                    };
                    let selector_key = format!("{candidate:016x}");
                    let selector_package = paris.read_package(selector_entry)?;
                    let Some(selector) = selector_package
                        .resources
                        .iter()
                        .find(|r| r.type_id == TYPE_SELECTOR && r.file_id == selector_key)
                        .or_else(|| {
                            selector_package
                                .resources
                                .iter()
                                .find(|r| r.type_id == TYPE_SELECTOR)
                        })
                    else {
                        continue;
                    };
                    let selector_name = selector.name.clone();
                    let selector_bytes = &selector_package.bytes[selector.payload.clone()];
                    // Accept the fixed-offset reference always (composite entities use a
                    // differently named parent), but a scanned collision only when its
                    // name prefixes the entity's: FRV_Wall_WL_09_042 -> FRV_Wall_WL_09.
                    if Some(candidate) != fixed && !entity_name.starts_with(&selector.name) {
                        continue;
                    }
                    for inner in 0..selector_bytes.len().saturating_sub(7) {
                        let mesh_id = u64::from_le_bytes(
                            selector_bytes[inner..inner + 8].try_into().unwrap(),
                        );
                        // The selector's LOD objects live in its own id block (in the
                        // samples, _LOD1 sits 8 above the selector). Accepting any
                        // entry-table match picked unrelated shared meshes.
                        if (mesh_id >> 32) != (candidate >> 32) {
                            continue;
                        }
                        let (mesh_entry, from_assets) = match mesh_index.get(&mesh_id) {
                            Some(&entry) => (entry, false),
                            None => match assets_entries.get(&mesh_id) {
                                Some(&entry) => (entry, true),
                                None => continue,
                            },
                        };
                        let mesh_package = if from_assets {
                            assets.read_package(mesh_entry)?
                        } else {
                            paris.read_package(mesh_entry)?
                        };
                        let Some(resource) = mesh_package
                            .resources
                            .iter()
                            .position(|r| r.type_id == TYPE_WORLD_MESH)
                        else {
                            continue;
                        };
                        // Keep the LOD whose name belongs to this selector; the payload
                        // also references shared meshes that are not this object.
                        let mesh_name = mesh_package.resources[resource].name.clone();
                        if !selector_name.is_empty()
                            && !mesh_name.is_empty()
                            && !mesh_name.starts_with(selector_name.as_str())
                        {
                            continue;
                        }
                        list.push((mesh_entry, resource, from_assets));
                        // One LOD per entity: spawning every level stacks identical
                        // geometry and destroys the scene bounds.
                        break;
                    }
                    if !list.is_empty() {
                        break;
                    }
                }
                resolved.insert(child, list.clone());
                list
            }
        };
        if meshes.is_empty() {
            skipped_no_mesh += 1;
            continue;
        }
        for (mesh_entry, resource, from_assets) in meshes {
            let piece = Piece {
                archive: if from_assets {
                    _assets_path.clone()
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
            pieces.push(loaded);
            if pieces.len() >= MAP_PIECE_LIMIT {
                break;
            }
        }
    }
    eprintln!(
        "map: {} placed, {} entities without a matrix, {} without a resolvable mesh",
        pieces.len(),
        skipped_matrix,
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
