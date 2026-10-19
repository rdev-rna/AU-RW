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
pub const DEFAULT_TEXTURE_ARCHIVES: &[&str] = &["DataPC_SharedGroup_00.forge", "DataPC.forge"];

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

#[derive(Debug, Resource)]
pub struct Options {
    pub rigged: bool,
    pub scenario: String,
    pub ticks: u32,
    pub game_dir: PathBuf,
    pub pieces: Vec<Piece>,
    /// Screenshot path; when set the app captures and exits.
    pub capture: Option<PathBuf>,
    /// Frames to render before capturing.
    pub frames: u32,
    /// Camera azimuth in degrees (orbit around the model).
    pub orbit_deg: f32,
    /// Render target size for capture (and the windowed view).
    pub size: (u32, u32),
    /// Archives indexed for cross-archive lookups, in selection order.
    pub texture_archives: Vec<PathBuf>,
    /// Skip material/texture resolution and render the grey fallback.
    pub no_textures: bool,
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
    let mut capture: Option<PathBuf> = None;
    let mut frames: u32 = 60;
    let mut orbit_deg: f32 = 35.0;
    let mut size: (u32, u32) = (1280, 720);
    let mut texture_archives: Vec<String> = Vec::new();
    let mut no_textures = false;
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
    let rigged = piece_args.is_empty() && !static_scene;
    if piece_args.is_empty() {
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
        capture,
        frames,
        orbit_deg,
        size,
        texture_archives,
        no_textures,
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
    --capture PATH     Render headless, save one PNG screenshot, then exit
    --report PATH      Write a JSON report of the material/texture chain
    --frames N         Frames rendered before the capture (default 60)
    --orbit-deg DEG    Camera azimuth around the model (default 35)
    --size WxH         Render target size (default 1280x720)
    --texture-archive FILE
                       Extra archive indexed for cross-archive lookups
                       (repeatable; default: DataPC_SharedGroup_00.forge,
                       DataPC.forge)
    --no-textures      Render the grey fallback without touching materials
    --static           Disable rig and test movement (old inspection mode)
    --scenario NAME    idle/bind/deform/walk/run/turn/jump/contact/profile-up/profile-down
    --ticks N          Fixed replay ticks before capture (max 36000)

Without --piece the scene shows Arno (Fearless Assassin outfit, hood up, LOD0)
standing at the origin on a synthetic grey ground plane. All geometry is read
in memory from the install; nothing is extracted or written back.
"
    .to_string()
}
