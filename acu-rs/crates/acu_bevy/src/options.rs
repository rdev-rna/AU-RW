//! Command-line scene description for the synthetic ACU test world.
//!
//! Pieces are `ARCHIVE:ENTRY:RESOURCE` or `ENTRY:RESOURCE` when `--archive` is
//! given. The default set is Arno (Fearless Assassin outfit, hood up, LOD0)
//! plus his head and hood-up hair plane; see ACU-RE-0011 for identities.

use std::path::PathBuf;

use bevy::prelude::Resource;

pub const DEFAULT_ARCHIVE: &str = "DataPC_ACU_LGS_BelleEpoque.forge";

/// Archives indexed for cross-archive material/texture lookups by default
/// (ACU-RE-0013): the Arno body textures live in `DataPC_SharedGroup_00.forge`,
/// the hair-strip textures in `DataPC.forge`. Selection order is presentation
/// only and never claims patch precedence.
/// Archives the Paris material set resolves against. SharedGroup_01..03 are not optional:
/// measured over 110 streamed cells, omitting them left 2,525 of 14,298 parts with no diffuse
/// map (their maps live in SharedGroup_01); including them leaves 294.
pub const DEFAULT_TEXTURE_ARCHIVES: &[&str] = &[
    "DataPC_SharedGroup_00.forge",
    "DataPC_SharedGroup_01.forge",
    "DataPC_SharedGroup_02.forge",
    "DataPC_SharedGroup_03.forge",
    "DataPC.forge",
];

/// Arno pieces: (entry, resource, label). Identities and payload hashes are in
/// ACU-RE-0011.
pub const ARNO_PIECES: &[(usize, usize, &str)] = &[
    (679, 35, "CN_P_Arno_FearlessAssassin_Upperbody_LOD0"),
    (679, 39, "CN_P_Arno_FearlessAssassin_Upperbody_Down_LOD0"),
    (679, 43, "CN_P_Arno_FearlessAssassin_Hood_Up_LOD0"),
    (679, 23, "CN_P_Arno_FearlessAssassin_Legs_LOD0"),
    (679, 15, "CN_P_Arno_FearlessAssassin_Belt_LOD0"),
    (679, 19, "CN_P_Arno_FearlessAssassin_Flaps_LOD0"),
    (679, 27, "CN_P_Arno_FearlessAssassin_scarf_LOD0"),
    (679, 11, "CN_P_Arno_FearlessAssassin_ShoulderCover_LOD0"),
    (679, 31, "CN_P_Arno_FearlessAssassin_ThrowBlade_LOD0"),
    (351, 1, "CN_U_FR_Arno_Head_LOD0"),
    (655, 1, "CN_U_Arno_HairPlane_HoodUp_LOD0"),
];

#[derive(Debug, Clone)]
pub struct Piece {
    pub archive: PathBuf,
    pub entry: usize,
    pub resource: usize,
    pub label: String,
}

fn parse_point(value: &str) -> Result<[f32; 3], String> {
    let parts: Vec<f32> = value
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{value:?}: {e}"))?;
    let [x, y, z] = parts.as_slice() else {
        return Err(format!("{value:?} needs exactly three numbers X,Y,Z"));
    };
    Ok([*x, *y, *z])
}

#[derive(Debug, Resource)]
pub struct Options {
    pub rigged: bool,
    pub scenario: String,
    pub ticks: u32,
    pub game_dir: PathBuf,
    pub pieces: Vec<Piece>,
    /// Map scene: a world cell to spawn, given as an entry index or a 16-digit file id.
    pub map_cell: Option<String>,
    /// JSON list of cells to spawn in one scene (`[{"entry_index": N}, ...]`).
    pub map_cells: Option<PathBuf>,
    /// Count resolvable pieces without decoding meshes.
    pub map_dry_run: bool,
    /// Spawn the Arno rig inside the map so the scene can be walked.
    pub map_player: bool,
    /// Player start position in Bevy coordinates (x, y up, z).
    pub spawn: Option<[f32; 3]>,
    /// Spawn only placements within this many metres of the spawn point.
    pub map_radius: Option<f32>,
    /// Write a cell entry -> placement centroid table while scanning.
    pub map_cell_centres: Option<PathBuf>,
    /// Stream cells around the player with this radius in metres.
    pub map_stream: Option<f32>,
    /// Only compute cell centroids (entity matrices), do not resolve placement.
    pub map_centres_only: bool,
    /// Spawn building groups only within this many metres of the spawn point.
    pub map_detail_radius: Option<f32>,
    /// Third-person camera distance in metres (default 12).
    pub map_camera_distance: Option<f32>,
    /// Stream exactly this cell entry (diagnostic: isolates the streaming spawn path).
    pub map_stream_cell: Option<usize>,
    /// Walk forward automatically (streaming stress test).
    pub auto_walk: bool,
    /// Read the resolved map from this cache instead of decoding the archives.
    pub cache_read: Option<PathBuf>,
    /// Write the resolved map to this cache after decoding.
    pub cache_write: Option<PathBuf>,
    /// Fixed camera position (bevy metres) for deterministic stills.
    pub camera_at: Option<[f32; 3]>,
    /// Fixed camera target (bevy metres) for deterministic stills.
    pub look_at: Option<[f32; 3]>,
    /// Cap on placed pieces; defaults to 400 for one cell and 200000 for a list.
    pub map_piece_limit: Option<usize>,
    /// Preferred LOD level for map meshes; the finest available is used when absent.
    pub map_lod: u8,
    /// Draw the map as one instanced entity per unique part (default off).
    pub map_instanced: bool,
    /// Archives used by map mode; default to the installed Paris pair.
    pub map_paris: Option<PathBuf>,
    pub map_assets: Option<PathBuf>,
    /// Screenshot path; when set the app captures and exits.
    pub capture: Option<PathBuf>,
    /// Frames to render before capturing.
    pub frames: u32,
    /// Camera azimuth in degrees (orbit around the model).
    pub orbit_deg: f32,
    /// Camera elevation as a slope (0.28 = ~16 degrees, 0.6 = ~31).
    pub orbit_elevation: f32,
    /// Render target size for capture (and the windowed view).
    pub size: (u32, u32),
    /// Archives indexed for cross-archive lookups, in selection order.
    pub texture_archives: Vec<PathBuf>,
    /// Skip material/texture resolution and render the grey fallback.
    pub no_textures: bool,
    /// Texture residency pool in MiB (default 100, the engine's 104857600-byte budget).
    pub texture_pool_mb: u32,
    /// Per-frame streaming time slice in milliseconds (default 4.0).
    pub texture_slice_ms: f32,
    /// Per-frame upload cap (default 5, the engine's per-pass item cap).
    pub texture_per_frame: usize,
    /// Upload every texture up front; restores the pre-streaming path for A/B.
    pub no_texture_streaming: bool,
    /// Optional JSON report of the resolution chain and its provenance.
    pub report: Option<PathBuf>,
}

fn parse_piece(
    game_dir: &std::path::Path,
    default_archive: &std::path::Path,
    text: &str,
) -> Result<Piece, String> {
    let parts: Vec<&str> = text.split(':').collect();
    let (archive, entry, resource) = match parts.as_slice() {
        [entry, resource] => (default_archive.to_path_buf(), *entry, *resource),
        [archive, entry, resource] => {
            let path = PathBuf::from(archive);
            let path =
                if path.is_absolute() || path.parent().is_some_and(|p| !p.as_os_str().is_empty()) {
                    path
                } else {
                    game_dir.join(path)
                };
            (path, *entry, *resource)
        }
        _ => {
            return Err(format!(
                "piece {text:?} must be ARCHIVE:ENTRY:RESOURCE or ENTRY:RESOURCE"
            ));
        }
    };
    let entry = entry
        .parse()
        .map_err(|e| format!("piece {text:?} entry: {e}"))?;
    let resource = resource
        .parse()
        .map_err(|e| format!("piece {text:?} resource: {e}"))?;
    Ok(Piece {
        archive,
        entry,
        resource,
        label: text.to_string(),
    })
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut game_dir: Option<PathBuf> = std::env::var_os("ACU_GAME_DIR").map(PathBuf::from);
    let mut archive: Option<PathBuf> = None;
    let mut piece_args: Vec<String> = Vec::new();
    let mut map_cell: Option<String> = None;
    let mut map_cells: Option<PathBuf> = None;
    let mut map_dry_run = false;
    let mut map_player = false;
    let mut spawn: Option<[f32; 3]> = None;
    let mut map_radius: Option<f32> = None;
    let mut map_cell_centres: Option<PathBuf> = None;
    let mut map_stream: Option<f32> = None;
    let mut map_centres_only = false;
    let mut map_detail_radius: Option<f32> = None;
    let mut map_camera_distance: Option<f32> = None;
    let mut map_stream_cell: Option<usize> = None;
    let mut auto_walk = false;
    let mut cache_read: Option<PathBuf> = None;
    let mut cache_write: Option<PathBuf> = None;
    let mut camera_at: Option<[f32; 3]> = None;
    let mut look_at: Option<[f32; 3]> = None;
    let mut map_piece_limit: Option<usize> = None;
    let mut map_lod: u8 = 0;
    let mut map_instanced = false;
    let mut map_paris: Option<PathBuf> = None;
    let mut map_assets: Option<PathBuf> = None;
    let mut capture: Option<PathBuf> = None;
    let mut frames: u32 = 60;
    let mut orbit_deg: f32 = 35.0;
    let mut orbit_elevation: f32 = 0.28;
    let mut size: (u32, u32) = (1280, 720);
    let mut texture_archives: Vec<String> = Vec::new();
    let mut no_textures = false;
    let mut texture_pool_mb: u32 = crate::texture_stream::DEFAULT_POOL_MB;
    let mut texture_slice_ms: f32 = crate::texture_stream::DEFAULT_SLICE_MS;
    let mut texture_per_frame: usize = crate::texture_stream::DEFAULT_PER_FRAME;
    let mut no_texture_streaming = false;
    let mut static_scene = false;
    let mut scenario = "idle".to_string();
    let mut ticks = 0u32;
    let mut report: Option<PathBuf> = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game-dir" => {
                game_dir = Some(PathBuf::from(
                    args.next().ok_or("--game-dir needs a value")?,
                ));
            }
            "--archive" => {
                archive = Some(PathBuf::from(args.next().ok_or("--archive needs a value")?));
            }
            "--piece" => piece_args.push(args.next().ok_or("--piece needs a value")?),
            "--map-cell" => map_cell = Some(args.next().ok_or("--map-cell needs a value")?),
            "--map-cells" => {
                map_cells = Some(PathBuf::from(
                    args.next().ok_or("--map-cells needs a path")?,
                ))
            }
            "--map-dry-run" => map_dry_run = true,
            "--map-player" => map_player = true,
            "--map-cell-centres" => {
                map_cell_centres = Some(PathBuf::from(
                    args.next().ok_or("--map-cell-centres needs a path")?,
                ))
            }
            "--map-centres-only" => map_centres_only = true,
            "--camera-at" => {
                let value = args.next().ok_or("--camera-at needs X,Y,Z")?;
                camera_at = Some(parse_point(&value)?);
            }
            "--look-at" => {
                let value = args.next().ok_or("--look-at needs X,Y,Z")?;
                look_at = Some(parse_point(&value)?);
            }
            "--auto-walk" => auto_walk = true,
            "--cache-read" => {
                cache_read = Some(PathBuf::from(
                    args.next().ok_or("--cache-read needs a path")?,
                ))
            }
            "--cache-write" => {
                cache_write = Some(PathBuf::from(
                    args.next().ok_or("--cache-write needs a path")?,
                ))
            }
            "--map-stream-cell" => {
                map_stream_cell = Some(
                    args.next()
                        .ok_or("--map-stream-cell needs a cell entry")?
                        .parse()
                        .map_err(|e| format!("--map-stream-cell: {e}"))?,
                )
            }
            "--map-camera-distance" => {
                map_camera_distance = Some(
                    args.next()
                        .ok_or("--map-camera-distance needs metres")?
                        .parse()
                        .map_err(|e| format!("--map-camera-distance: {e}"))?,
                )
            }
            "--map-detail-radius" => {
                map_detail_radius = Some(
                    args.next()
                        .ok_or("--map-detail-radius needs metres")?
                        .parse()
                        .map_err(|e| format!("--map-detail-radius: {e}"))?,
                )
            }
            "--map-stream" => {
                map_stream = Some(
                    args.next()
                        .ok_or("--map-stream needs metres")?
                        .parse()
                        .map_err(|e| format!("--map-stream: {e}"))?,
                )
            }
            "--map-radius" => {
                map_radius = Some(
                    args.next()
                        .ok_or("--map-radius needs metres")?
                        .parse()
                        .map_err(|e| format!("--map-radius: {e}"))?,
                )
            }
            "--spawn" => {
                let value = args.next().ok_or("--spawn needs X,Y,Z")?;
                let parts: Vec<f32> = value
                    .split(',')
                    .map(|part| part.trim().parse::<f32>())
                    .collect::<Result<_, _>>()
                    .map_err(|e| format!("--spawn {value:?}: {e}"))?;
                let [x, y, z] = parts.as_slice() else {
                    return Err("--spawn needs exactly three numbers X,Y,Z".into());
                };
                spawn = Some([*x, *y, *z]);
            }
            "--map-piece-limit" => {
                map_piece_limit = Some(
                    args.next()
                        .ok_or("--map-piece-limit needs a count")?
                        .parse()
                        .map_err(|e| format!("--map-piece-limit: {e}"))?,
                )
            }
            "--map-lod" => {
                map_lod = args
                    .next()
                    .ok_or("--map-lod needs a value")?
                    .parse()
                    .map_err(|e| format!("--map-lod: {e}"))?;
                if map_lod > 4 {
                    return Err("--map-lod must be 0..4".into());
                }
            }
            "--map-instanced" => map_instanced = true,
            "--map-paris" => {
                map_paris = Some(PathBuf::from(
                    args.next().ok_or("--map-paris needs a value")?,
                ))
            }
            "--map-assets" => {
                map_assets = Some(PathBuf::from(
                    args.next().ok_or("--map-assets needs a value")?,
                ))
            }
            "--capture" => {
                capture = Some(PathBuf::from(args.next().ok_or("--capture needs a value")?))
            }
            "--report" => {
                report = Some(PathBuf::from(args.next().ok_or("--report needs a value")?))
            }
            "--texture-archive" => {
                texture_archives.push(args.next().ok_or("--texture-archive needs a value")?)
            }
            "--no-textures" => no_textures = true,
            "--texture-pool-mb" => {
                texture_pool_mb = args
                    .next()
                    .ok_or("--texture-pool-mb needs MiB")?
                    .parse()
                    .map_err(|e| format!("--texture-pool-mb: {e}"))?;
            }
            "--texture-slice-ms" => {
                texture_slice_ms = args
                    .next()
                    .ok_or("--texture-slice-ms needs milliseconds")?
                    .parse()
                    .map_err(|e| format!("--texture-slice-ms: {e}"))?;
                if texture_slice_ms < 0.0 {
                    return Err("--texture-slice-ms must be >= 0".into());
                }
            }
            "--texture-per-frame" => {
                texture_per_frame = args
                    .next()
                    .ok_or("--texture-per-frame needs a count")?
                    .parse()
                    .map_err(|e| format!("--texture-per-frame: {e}"))?;
            }
            "--no-texture-streaming" => no_texture_streaming = true,
            "--static" => static_scene = true,
            "--scenario" => {
                scenario = args.next().ok_or("--scenario needs a value")?;
                if ![
                    "idle",
                    "bind",
                    "deform",
                    "walk",
                    "run",
                    "turn",
                    "turn-arc",
                    "reversals",
                    "walk-left",
                    "walk-right",
                    "walk-back",
                    "walk-circle",
                    "run-left",
                    "run-right",
                    "run-back",
                    "run-circle",
                    "jump",
                    "contact",
                    "profile-up",
                    "profile-down",
                ]
                .contains(&scenario.as_str())
                {
                    return Err("unsupported scenario".into());
                }
            }
            "--ticks" => {
                ticks = args
                    .next()
                    .ok_or("--ticks needs a value")?
                    .parse()
                    .map_err(|_| "invalid ticks")?;
                if ticks > 36000 {
                    return Err("ticks exceeds replay limit".into());
                }
            }
            "--frames" => {
                frames = args
                    .next()
                    .ok_or("--frames needs a value")?
                    .parse()
                    .map_err(|e| format!("--frames: {e}"))?;
            }
            "--orbit-elevation" => {
                orbit_elevation = args
                    .next()
                    .ok_or("--orbit-elevation needs a value")?
                    .parse()
                    .map_err(|e| format!("--orbit-elevation: {e}"))?;
                if !(0.0..=4.0).contains(&orbit_elevation) {
                    return Err("--orbit-elevation must be 0..4".into());
                }
            }
            "--orbit-deg" => {
                orbit_deg = args
                    .next()
                    .ok_or("--orbit-deg needs a value")?
                    .parse()
                    .map_err(|e| format!("--orbit-deg: {e}"))?;
            }
            "--size" => {
                let value = args.next().ok_or("--size needs a value")?;
                let (w, h) = value
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("--size {value:?} must be WxH"))?;
                size = (
                    w.parse().map_err(|e| format!("--size width: {e}"))?,
                    h.parse().map_err(|e| format!("--size height: {e}"))?,
                );
                if size.0 == 0 || size.1 == 0 {
                    return Err("--size must be non-zero".into());
                }
            }
            "--help" | "-h" => {
                return Err(help());
            }
            other => return Err(format!("unknown argument {other:?}\n\n{}", help())),
        }
    }
    let game_dir = game_dir.ok_or_else(|| {
        format!(
            "unavailable: set ACU_GAME_DIR or pass --game-dir\n\n{}",
            help()
        )
    })?;
    let archive = archive
        .map(|a| {
            if a.is_absolute() || a.parent().is_some_and(|p| !p.as_os_str().is_empty()) {
                a
            } else {
                game_dir.join(a)
            }
        })
        .unwrap_or_else(|| game_dir.join(DEFAULT_ARCHIVE));
    let mut pieces = Vec::new();
    let map_mode = map_cell.is_some() || map_cells.is_some();
    let rigged = piece_args.is_empty() && !static_scene && (!map_mode || map_player);
    if map_mode && !map_player {
        // Map mode supplies its own pieces; the Arno set and --piece are ignored.
    } else if piece_args.is_empty() {
        for (entry, resource, label) in ARNO_PIECES {
            pieces.push(Piece {
                archive: archive.clone(),
                entry: *entry,
                resource: *resource,
                label: (*label).to_string(),
            });
        }
    } else {
        for text in piece_args {
            pieces.push(parse_piece(&game_dir, &archive, &text)?);
        }
    }
    let texture_archives = if no_textures {
        Vec::new()
    } else {
        let selected: Vec<String> = if texture_archives.is_empty() {
            DEFAULT_TEXTURE_ARCHIVES
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        } else {
            texture_archives
        };
        selected
            .into_iter()
            .map(|name| {
                let path = PathBuf::from(&name);
                if path.is_absolute() || path.parent().is_some_and(|p| !p.as_os_str().is_empty()) {
                    path
                } else {
                    game_dir.join(path)
                }
            })
            .collect()
    };
    Ok(Options {
        rigged,
        scenario,
        ticks,
        game_dir,
        pieces,
        map_cell,
        map_cells,
        map_dry_run,
        map_player,
        spawn,
        map_radius,
        map_cell_centres,
        map_stream,
        map_centres_only,
        map_detail_radius,
        map_camera_distance,
        map_stream_cell,
        auto_walk,
        cache_read,
        cache_write,
        camera_at,
        look_at,
        map_piece_limit,
        map_lod,
        map_instanced,
        map_paris,
        map_assets,
        capture,
        frames,
        orbit_deg,
        orbit_elevation,
        size,
        texture_archives,
        no_textures,
        texture_pool_mb,
        texture_slice_ms,
        texture_per_frame,
        no_texture_streaming,
        report,
    })
}

pub fn help() -> String {
    "\
acu_bevy - synthetic ACU test world with assets read from the original install

USAGE:
    acu_bevy [OPTIONS]

OPTIONS:
    --game-dir DIR     Original install (default: ACU_GAME_DIR)
    --archive FILE     Default archive for ENTRY:RESOURCE pieces
                       (default: DataPC_ACU_LGS_BelleEpoque.forge)
    --piece P          ARCHIVE:ENTRY:RESOURCE or ENTRY:RESOURCE (repeatable)
    --map-cell CELL    Spawn a world cell (entry index or 16-digit file id)
    --map-cells FILE   Spawn every cell in a JSON list (entry_index per row)
    --map-lod N        Preferred mesh LOD level 0..4 (default 0)
    --map-instanced    Draw the map as one instanced entity per unique part (default off)
    --map-dry-run      Count resolvable pieces without decoding meshes
    --map-player       Spawn Arno in the map and enable the walk controls
    --spawn X,Y,Z      Player start in Bevy coordinates (x, y up, z)
    --map-radius M     Spawn only placements within M metres of the spawn point
    --map-cell-centres PATH  Write cell entry -> placement centroid JSON
    --map-stream M     Stream cells within M metres of the player as they move
    --map-centres-only  Only write cell centroids (fast; no placement resolution)
    --map-detail-radius M  Spawn building groups only within M metres of the spawn
    --map-camera-distance M  Third-person camera distance (default 12 m)
    --map-stream-cell E  Stream only cell entry E (diagnostic)
    --auto-walk        Walk forward automatically (streaming stress test)
    --cache-write PATH Write the resolved map to PATH after decoding
    --cache-read PATH  Load the resolved map from PATH instead of decoding
    --camera-at X,Y,Z  Fix the camera here (deterministic stills)
    --look-at X,Y,Z    Look at this point when --camera-at is set
    --map-piece-limit N  Cap placed pieces (400 one cell, 200000 for a list)
    --map-paris FILE   Paris archive used by map mode
    --map-assets FILE  Assets archive used by map mode (LOD meshes)
    --capture PATH     Render headless, save one PNG screenshot, then exit
    --report PATH      Write a JSON report of the material/texture chain
    --frames N         Frames rendered before the capture (default 60)
    --orbit-deg DEG    Camera azimuth around the model (default 35)
    --orbit-elevation E  Camera elevation as a slope (default 0.28 = ~16 deg)
    --size WxH         Render target size (default 1280x720)
    --texture-archive FILE
                       Extra archive indexed for cross-archive lookups
                       (repeatable; default: DataPC_SharedGroup_00.forge,
                       DataPC.forge)
    --no-textures      Render the grey fallback without touching materials
    --texture-pool-mb N  Texture residency pool in MiB (default 100 = the engine's
                       104857600-byte budget)
    --texture-slice-ms F  Per-frame streaming time slice in ms (default 4.0)
    --texture-per-frame N  Texture uploads per frame (default 5, the engine's cap)
    --no-texture-streaming  Upload all textures up front (pre-streaming path, A/B)
    --static           Disable rig and test movement (old inspection mode)
    --scenario NAME    idle/bind/deform/walk/run/turn/jump/contact/profile-up/profile-down; walk/run-left/right/back/circle
                       reversals: visible loop, four right/left pairs (2 s each)
    --ticks N          Fixed replay ticks before capture (max 36000)

Without --piece the scene shows Arno (Fearless Assassin outfit, hood up, LOD0)
standing at the origin on a synthetic grey ground plane. All geometry is read
in memory from the install; nothing is extracted or written back.
"
    .to_string()
}
