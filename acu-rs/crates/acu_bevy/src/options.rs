//! Command-line scene description for the synthetic ACU test world.
//!
//! Pieces are `ARCHIVE:ENTRY:RESOURCE` or `ENTRY:RESOURCE` when `--archive` is
//! given. The default set is Arno ("Arno's Tailored" outfit, hood up, LOD0)
//! plus his head and hood-up hair plane; see ACU-RE-0011 for identities.

use std::path::PathBuf;

use bevy::prelude::Resource;

pub const DEFAULT_ARCHIVE: &str = "DataPC_ACU_LGS_BelleEpoque.forge";

/// Archives indexed for cross-archive material/texture lookups by default
/// (ACU-RE-0013): the Arno body textures live in `DataPC_SharedGroup_00.forge`,
/// the hair-strip textures in `DataPC.forge`. Selection order is presentation
/// only and never claims patch precedence.
pub const DEFAULT_TEXTURE_ARCHIVES: &[&str] = &["DataPC_SharedGroup_00.forge", "DataPC.forge"];

/// Arno pieces: (entry, resource, label). Default outfit is "Arno's Tailored"
/// (`ACU_AvatarGear_*_02`, internal `Arno_*_2`, tier code 3), whose avatar
/// BuildTable rows are the `Arnaud_V2` pieces; see ACU-RE-0070. (`Arnaud_V3` is
/// Arno's Master set.) Head/hair identities are in ACU-RE-0011. The previous
/// FearlessAssassin set (entry 679) remains loadable via --piece.
pub const ARNO_PIECES: &[(usize, usize, &str)] = &[
    (370, 1, "CN_P_FR_Upperbody_Arnaud_V2_LOD0"),
    (364, 1, "CN_P_FR_Flap_Arnaud_V2_LOD0"),
    (368, 1, "CN_P_FR_Scarf_Arnaud_V2_LOD0"),
    (367, 1, "CN_P_FR_Hood_Arnaud_V2_LOD0"),
    (366, 1, "CN_P_FR_Pants_Arnaud_V2_LOD0"),
    (365, 1, "CN_P_FR_Belts_Arnaud_V2_LOD0"),
    (369, 1, "CN_P_FR_ThrowBlade_Arnaud_V2_LOD0"),
    (351, 1, "CN_U_FR_Arno_Head_LOD0"),
    (655, 1, "CN_U_Arno_HairPlane_HoodUp_LOD0"),
];

/// `Cloth` soft-body resources the Tailored pieces use: (file id, name).
/// UpperBodies table row 2, column 6 (ACU-RE-0016, ACU-RE-0081). Each binds
/// to the loaded piece whose mesh id its LOD0 names, in that piece's archive.
/// The scarf (column 7) is not driven yet.
pub const ARNO_CLOTHS: &[(u64, &str)] = &[(0x0000_0014_60e4_5330, "Softbody_CN_P_Flap_ArnaudV2")];

#[derive(Debug, Clone)]
pub struct Piece {
    pub archive: PathBuf,
    pub entry: usize,
    pub resource: usize,
    pub label: String,
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
    /// Archives used by map mode; default to the installed Paris pair.
    pub map_paris: Option<PathBuf>,
    pub map_assets: Option<PathBuf>,
    /// Decoded Paris collision block (`tools/world_collision.py block`) that replaces the
    /// synthetic floor and cubes; the rigged player traverses it.
    pub block: Option<PathBuf>,
    /// Spawn point in the block, arena metres (x, z) relative to its origin.
    pub spawn: Option<(f32, f32)>,
    /// Screenshot path; when set the app captures and exits.
    pub capture: Option<PathBuf>,
    /// Frames to render before capturing.
    pub frames: u32,
    /// Camera azimuth in degrees (orbit around the model).
    pub orbit_deg: f32,
    /// Camera distance and pitch overrides (metres, degrees).
    pub camera_distance: Option<f32>,
    pub camera_pitch: Option<f32>,
    /// Render target size for capture (and the windowed view).
    pub size: (u32, u32),
    /// Archives indexed for cross-archive lookups, in selection order.
    pub texture_archives: Vec<PathBuf>,
    /// Skip material/texture resolution and render the grey fallback.
    pub no_textures: bool,
    /// Optional JSON report of the resolution chain and its provenance.
    pub report: Option<PathBuf>,
    /// Row of `CN_P_Avatars_ColorSets` applied to colour-masked cloth
    /// materials (ACU-RE-0071); `None` renders the raw diffuse. Row 1 is the
    /// table's default-weighted row ("Color 1", the default assassin blue).
    pub color_set: Option<usize>,
    /// Lighting preset (`lighting::SETS`, ACU-RE-0073); default `paris-day`.
    pub lighting: String,
    /// Drive cloth-simulated pieces with the ported solver (ACU-RE-0081);
    /// `--no-cloth` leaves them on their mesh skin.
    pub cloth: bool,
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
    let mut map_paris: Option<PathBuf> = None;
    let mut map_assets: Option<PathBuf> = None;
    let mut block: Option<PathBuf> = None;
    let mut spawn: Option<(f32, f32)> = None;
    let mut capture: Option<PathBuf> = None;
    let mut frames: u32 = 60;
    let mut orbit_deg: f32 = 35.0;
    let mut camera_distance: Option<f32> = None;
    let mut camera_pitch: Option<f32> = None;
    let mut size: (u32, u32) = (1280, 720);
    let mut texture_archives: Vec<String> = Vec::new();
    let mut no_textures = false;
    let mut cloth = true;
    let mut color_set: Option<usize> = Some(1);
    let mut lighting = crate::lighting::DEFAULT.to_string();
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
            "--block" => block = Some(PathBuf::from(args.next().ok_or("--block needs a value")?)),
            "--spawn" => {
                let value = args.next().ok_or("--spawn needs a value")?;
                let (x, z) = value
                    .split_once(',')
                    .ok_or_else(|| format!("--spawn {value:?} must be X,Z"))?;
                spawn = Some((
                    x.trim().parse().map_err(|e| format!("--spawn x: {e}"))?,
                    z.trim().parse().map_err(|e| format!("--spawn z: {e}"))?,
                ));
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
            "--no-cloth" => cloth = false,
            "--color-set" => {
                let value = args.next().ok_or("--color-set needs a value")?;
                color_set = if value == "none" {
                    None
                } else {
                    Some(
                        value
                            .parse()
                            .map_err(|e| format!("--color-set {value:?}: {e}"))?,
                    )
                };
            }
            "--lighting" => lighting = args.next().ok_or("--lighting needs a value")?,
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
                    "block-climb",
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
            "--orbit-deg" => {
                orbit_deg = args
                    .next()
                    .ok_or("--orbit-deg needs a value")?
                    .parse()
                    .map_err(|e| format!("--orbit-deg: {e}"))?;
            }
            "--camera-distance" => {
                camera_distance = Some(
                    args.next()
                        .ok_or("--camera-distance needs a value")?
                        .parse()
                        .map_err(|e| format!("--camera-distance: {e}"))?,
                )
            }
            "--camera-pitch" => {
                camera_pitch = Some(
                    args.next()
                        .ok_or("--camera-pitch needs a value")?
                        .parse()
                        .map_err(|e| format!("--camera-pitch: {e}"))?,
                )
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
    let rigged = piece_args.is_empty() && !static_scene && map_cell.is_none();
    if map_cell.is_some() {
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
        color_set,
        lighting,
        rigged,
        scenario,
        ticks,
        game_dir,
        pieces,
        map_cell,
        map_paris,
        map_assets,
        block,
        spawn,
        capture,
        frames,
        orbit_deg,
        camera_distance,
        camera_pitch,
        size,
        texture_archives,
        no_textures,
        report,
        cloth,
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
    --capture PATH     Render headless, save one PNG screenshot, then exit
    --report PATH      Write a JSON report of the material/texture chain
    --frames N         Frames rendered before the capture (default 60)
    --orbit-deg DEG    Camera azimuth around the model (default 35)
    --camera-distance M  Camera distance from the target (metres)
    --camera-pitch DEG   Camera elevation (degrees)
    --size WxH         Render target size (default 1280x720)
    --texture-archive FILE
                       Extra archive indexed for cross-archive lookups
                       (repeatable; default: DataPC_SharedGroup_00.forge,
                       DataPC.forge)
    --no-textures      Render the grey fallback without touching materials
    --no-cloth         Skin cloth pieces (the Tailored flap) instead of
                       simulating them with the ported solver
    --color-set N|none Colour-set row for colour-masked cloth (default 1, the
                       default assassin blue; none = raw diffuse)
    --lighting NAME    paris-day (default), paris-sunset|night|dawn, versailles-day|sunset|night|dawn,
                       saint-denis, saint-denis-moon, prologue-night, rebirth, rift-belle-epoque-sunset,
                       rift-medieval-dawn, rift-ww2-night, synthetic; L / Shift+L cycle at runtime
                       (native exposure/shadow settings, synthetic sun and fill; ACU-RE-0073)
    --static           Disable rig and test movement (old inspection mode)
    --scenario NAME    idle/bind/deform/walk/run/turn/jump/contact/profile-up/profile-down; walk/run-left/right/back/circle
                       reversals: visible loop, four right/left pairs (2 s each)
    --ticks N          Fixed replay ticks before capture (max 36000)
    --block FILE       Traverse a decoded collision block (ACUBLK01 from
                       tools/world_collision.py block) instead of the grey floor
    --spawn X,Z        Spawn point in the block, arena metres from its origin
                       (block-climb: RT + up held toward -Z)

Without --piece the scene shows Arno (Arno's Tailored outfit, hood up, LOD0)
standing at the origin on a synthetic grey ground plane. All geometry is read
in memory from the install; nothing is extracted or written back.
"
    .to_string()
}
