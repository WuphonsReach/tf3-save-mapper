//! Facts read from the player's own game install at run time: the ground
//! faces of depots and industries and the boxes of station pieces, and from
//! the mods folder the scripts of a mod's depots and the boxes of the pieces
//! a modular one is built from. Nothing read here is
//! stored in the tool or written into the picture beyond the shapes drawn
//! from it.
//!
//! The files are in the `base/content` archives: depot scripts in
//! `depots/*.zip`, station model files in `stations/*.zip`, industry scripts
//! in `industries/<key>.zip`. Their tables are generated Lua, so the few
//! fields needed are picked out by name. A mod's script is a plain file in
//! its folder under the mod.io game's `mods` folder, one folder per mod.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::geometry;
use crate::zip::Archive;

/// Steam folders tried when no install folder is given, under the home folder.
const HOME_STEAM: &[&str] = &[
    ".steam/steam",
    ".local/share/Steam",
    "snap/steam/common/.local/share/Steam",
    ".var/app/com.valvesoftware.Steam/.local/share/Steam",
    "Library/Application Support/Steam",
];
/// Steam folders tried on Windows.
const WINDOWS_STEAM: &[&str] = &["C:\\Program Files (x86)\\Steam", "C:\\Program Files\\Steam"];
/// The mod.io game's mods folder, under the folder Steam keeps its data in
/// (the home folder, or the snap's or Flatpak's own) or under its
/// `.local/share`.
const MODS_FOLDERS: &[&str] = &[
    "mod.io/common/10640/mods",
    ".local/share/mod.io/common/10640/mods",
];
/// The end of a Steam folder under the folder Steam keeps its data in.
const STEAM_DATA: &str = ".local/share/Steam";
const GAME_FOLDER: &str = "Transport Fever 3";
/// A mod's script larger than this is not read: no depot script is near it.
const MAX_MOD_SCRIPT: u64 = 16 << 20;

/// A rectangle in a model's own axes: (a0, b0, a1, b1), a along its x axis.
pub type Rect = (f64, f64, f64, f64);
pub type Polygon = Vec<(f64, f64)>;

/// The game's `base/content` folder.
#[derive(Debug, Clone)]
pub struct Install {
    pub content: PathBuf,
}

impl Install {
    /// The install in `game_dir` (the folder holding `base/`), or the first
    /// found in the usual Steam places and their library folders.
    pub fn find(game_dir: Option<&Path>) -> Option<Install> {
        let candidates: Vec<PathBuf> = match game_dir {
            Some(dir) => vec![dir.to_owned()],
            None => steam_roots()
                .iter()
                .flat_map(|root| library_folders(root))
                .map(|lib| lib.join("steamapps").join("common").join(GAME_FOLDER))
                .collect(),
        };
        candidates
            .into_iter()
            .map(|root| root.join("base").join("content"))
            .find(|content| content.join("industries").is_dir())
            .map(|content| Install { content })
    }

    fn archive(&self, folder: &str, name: &str) -> Option<Archive> {
        Archive::open(&self.content.join(folder).join(name)).ok()
    }
}

/// The mod.io game's `mods` folders, one subfolder per mod: the folder
/// given, or those found next to the usual Steam folders. Nothing is written
/// there.
#[derive(Debug, Clone)]
pub struct Mods {
    pub dirs: Vec<PathBuf>,
}

impl Mods {
    /// `dir` if it is a folder; without one, every mods folder found beside
    /// the usual Steam folders (`~/snap/steam/common/mod.io/...` for the
    /// snap) and in the home folder's `.local/share`.
    pub fn find(dir: Option<&Path>) -> Option<Mods> {
        let dirs = match dir {
            Some(dir) if dir.is_dir() => vec![dir.to_owned()],
            Some(_) => Vec::new(),
            None => std::env::home_dir().map_or_else(Vec::new, |home| mods_folders(&home)),
        };
        (!dirs.is_empty()).then_some(Mods { dirs })
    }

    /// The text of the file at `rel` (a script or a model file) in the first
    /// mod folder that has it: the mods folders in turn, and each one's mod
    /// folders by name. A file over `MAX_MOD_SCRIPT` bytes counts as missing.
    fn text(&self, rel: &str) -> Option<String> {
        self.dirs.iter().find_map(|dir| {
            let mut mods: Vec<String> = std::fs::read_dir(dir)
                .ok()?
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect();
            mods.sort();
            mods.iter().find_map(|m| {
                let file = std::fs::File::open(dir.join(m).join(rel)).ok()?;
                let mut bytes = Vec::new();
                file.take(MAX_MOD_SCRIPT + 1).read_to_end(&mut bytes).ok()?;
                (bytes.len() as u64 <= MAX_MOD_SCRIPT)
                    .then(|| String::from_utf8_lossy(&bytes).into_owned())
            })
        })
    }
}

/// The mods folders that exist beside the Steam folders found under `home`,
/// then in `home` itself, each once.
fn mods_folders(home: &Path) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    for root in HOME_STEAM.iter().map(|d| home.join(d)) {
        // `.steam/steam` is a link to the real folder.
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        if root.ends_with(STEAM_DATA) {
            let mut base = root.as_path();
            for _ in Path::new(STEAM_DATA).components() {
                base = base.parent().unwrap_or(base);
            }
            bases.push(base.to_owned());
        }
    }
    bases.push(home.to_owned());
    let mut out: Vec<PathBuf> = Vec::new();
    for base in bases {
        for rel in MODS_FOLDERS {
            let dir = base.join(rel);
            if dir.is_dir() && !out.contains(&dir) {
                out.push(dir);
            }
        }
    }
    out
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::home_dir() {
        roots.extend(HOME_STEAM.iter().map(|d| home.join(d)));
    }
    if cfg!(windows) {
        roots.extend(WINDOWS_STEAM.iter().map(PathBuf::from));
    }
    roots
}

/// `root` and the other library folders its `libraryfolders.vdf` lists.
fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_owned()];
    let vdf = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(vdf) {
        for line in text.lines() {
            let parts: Vec<&str> = line.split('"').collect();
            // `"path"		"/some/folder"` splits into ["", "path", tabs, "/some/folder", ""].
            if parts.len() >= 5 && parts[1] == "path" {
                out.push(PathBuf::from(parts[3].replace("\\\\", "\\")));
            }
        }
    }
    out
}

/// A maintenance building or depot kind: its layer, its script in the
/// install, the model whose instances are drawn and its fill. A kind with no
/// archive is a mod's: its script is the file at `script` in the mod's
/// folder, and a dot stands for a building whose script has no ground face.
/// A kind with an `outline` is drawn with it in place of the script's face.
///
/// A mod's depot built from many models (a modular construction, whose
/// script works its ground faces out as it runs) names them: `pieces` are
/// drawn as their model's box, `rings` (pieces laid round their origin, a
/// turntable's) as one disc per origin out to their box's far end. Both are
/// file names without `.mdl` in `model`'s folder, read from the mods folder.
/// `model` is then the one piece each depot has, at its catchment's centre:
/// it carries the coverage circle, and the dot when the mods folder is not
/// found.
pub struct DepotKind {
    pub id: &'static str,
    pub label: &'static str,
    pub archive: Option<&'static str>,
    pub script: &'static str,
    pub model: &'static str,
    pub fill: &'static str,
    pub outline: Option<&'static [(f64, f64)]>,
    pub pieces: &'static [&'static str],
    pub rings: &'static [&'static str],
}

/// What a model is to a depot kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceRole {
    Piece,
    Ring,
}

impl DepotKind {
    /// Whether the kind is drawn from its pieces.
    pub fn has_pieces(&self) -> bool {
        !self.pieces.is_empty() || !self.rings.is_empty()
    }

    /// What `model` is to this kind, if one of its pieces or rings.
    pub fn role(&self, model: &str) -> Option<PieceRole> {
        let folder = &self.model[..self.model.rfind('/')? + 1];
        let name = model.strip_prefix(folder)?.strip_suffix(".mdl")?;
        if self.pieces.contains(&name) {
            Some(PieceRole::Piece)
        } else if self.rings.contains(&name) {
            Some(PieceRole::Ring)
        } else {
            None
        }
    }

    /// Whether instances of `model` belong to this kind's layer.
    pub fn draws(&self, model: &str) -> bool {
        model == self.model || self.role(model).is_some()
    }
}

/// The road depot's lot in its own axes, measured by us from screenshots
/// (the script's face is the box round it): the yard, and a 16 m wide
/// entrance box in front of it, centred on the entrance lane at a = 0, out
/// to the road.
const ROAD_DEPOT_LOT: &[(f64, f64)] = &[
    (-17.5, 19.8),
    (-17.5, -14.0),
    (-8.0, -14.0),
    (-8.0, -21.1),
    (8.0, -21.1),
    (8.0, -14.0),
    (38.7, -14.0),
    (38.7, 19.8),
];

/// Metres: how far an `outline`'s box may lie from the script's face for the
/// outline to be used, so a face changed by a game update is drawn as read.
const OUTLINE_FIT: f64 = 1.5;

/// By carrier, like the stations: ship dark brown, rail dark grey, road
/// lighter grey, tram the lightest; a maintenance building a step darker than
/// its carrier's depot.
pub const DEPOT_KINDS: &[DepotKind] = &[
    DepotKind {
        id: "maintenance-buildings",
        label: "Road maintenance buildings",
        archive: Some("road.zip"),
        script: "road/road_maint_station.script.lua",
        model: "depots/road/road_maint_station/road_maint_station.mdl",
        fill: "#969696",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "road-depots",
        label: "Road depots",
        archive: Some("road.zip"),
        script: "road/road_depot/road_depot.script.lua",
        model: "depots/road/road_depot/road_depot.mdl",
        fill: "#b4b4b4",
        outline: Some(ROAD_DEPOT_LOT),
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "tram-depots",
        label: "Tram depots",
        archive: Some("road.zip"),
        script: "road/tram_depot/tram_depot.script.lua",
        model: "depots/road/tram_depot/tram_depot.mdl",
        fill: "#f2f2f2",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "rail-maintenance-buildings",
        label: "Rail maintenance buildings",
        archive: Some("rail.zip"),
        script: "rail/rail_maint_station.script.lua",
        model: "depots/rail/rail_maint_station/rail_maint_station.mdl",
        fill: "#464646",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "rail-depots",
        label: "Rail depots",
        archive: Some("rail.zip"),
        script: "rail/rail_depot.script.lua",
        model: "depots/rail/rail_depot.mdl",
        fill: "#646464",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "ship-maintenance-buildings",
        label: "Ship maintenance buildings",
        archive: Some("water.zip"),
        script: "water/water_maint_station.script.lua",
        model: "depots/water/water_maint_station/water_maint_station.mdl",
        fill: "#4d2d12",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "ship-depots",
        label: "Ship depots",
        archive: Some("water.zip"),
        script: "water/water_depot.script.lua",
        model: "depots/water/water_depot.mdl",
        fill: "#7b4a24",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    DepotKind {
        id: "boathouses",
        label: "Boathouses (Boathouse mod)",
        archive: None,
        script: "content/assets/ingo_bootsschuppen/ingo_bootsschuppen_maint.script.lua",
        model: "assets/ingo_bootsschuppen/ingo_bootsschuppen.mdl",
        fill: "#7b4a24",
        outline: None,
        pieces: &[],
        rings: &[],
    },
    // The turntable pit is the centre; the stalls' sheds and open stands are
    // each laid out from it at their own heading. Doors, wires, catenary,
    // stairs and the track pieces are left out.
    DepotKind {
        id: "roundhouses",
        label: "Roundhouses (Roundhouse mod)",
        archive: None,
        script: "content/depots/rail/ringlokschuppen/mav2002_roundhouse_mod.script.lua",
        model: "depots/rail/ringlokschuppen/mav2002_rl_wanne.mdl",
        fill: "#646464",
        outline: None,
        pieces: &[
            "mav2002_rl_building_l1",
            "mav2002_rl_building_l2",
            "mav2002_rl_building_m1",
            "mav2002_rl_building_r1",
            "mav2002_rl_building_r2",
            "mav2002_rl_building_s1",
            "mav2002_rl_building_sl1",
            "mav2002_rl_building_sr1",
            "mav2002_rl_open_l1",
            "mav2002_rl_open_l2",
            "mav2002_rl_open_l3",
            "mav2002_rl_open_m1",
            "mav2002_rl_open_m2",
            "mav2002_rl_open_r1",
            "mav2002_rl_open_r2",
            "mav2002_rl_open_r3",
            "mav2002_rl_open_s1",
            "mav2002_rl_addon_l1",
            "mav2002_rl_addon_r1",
            "mav2002_rl_chimney",
        ],
        rings: &["mav2002_rl_seg", "mav2002_rl_seg_track"],
    },
    // The first track's front door marks the construction's origin, where
    // its first depot's catchment is centred; it has up to four depots, one
    // per track. The halls (front, middle, rear) and the side building's
    // ground pieces are drawn; doors, logos and hit boxes are not.
    DepotKind {
        id: "modern-depots",
        label: "Modern depots (Modern Depot mod)",
        archive: None,
        script: "content/depots/rail/depot_modern/maikc_depot_modern.script.lua",
        model: "depots/rail/depot_modern/maikc_depot_modern_f1_tuer.mdl",
        fill: "#646464",
        outline: None,
        pieces: &[
            "maikc_depot_modern_f1",
            "maikc_depot_modern_f2",
            "maikc_depot_modern_f3",
            "maikc_depot_modern_m1",
            "maikc_depot_modern_m2",
            "maikc_depot_modern_m3",
            "maikc_depot_modern_m4",
            "maikc_depot_modern_r1",
            "maikc_depot_modern_r2",
            "maikc_depot_modern_r3",
            "maikc_depot_modern_g1",
            "maikc_depot_modern_g2",
            "maikc_depot_modern_g3",
            "maikc_depot_modern_g4",
            "maikc_depot_modern_g5",
            "maikc_depot_modern_g6",
            "maikc_depot_modern_g7",
            "maikc_depot_modern_g8",
            "maikc_depot_modern_g9",
        ],
        rings: &[],
    },
];

/// What a depot's script says about its footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct DepotSpec {
    /// The first ground face, in the building's own axes; empty for a mod's
    /// building whose script has none.
    pub face: Polygon,
    /// The collider boxes that reach past the face (a ship depot's quay and
    /// water part), each (a0, a1, b0, b1).
    pub boxes: Vec<(f64, f64, f64, f64)>,
    /// The catchment radius in metres if it reads, the maintenance pool and the transport modes.
    pub radius: Option<u32>,
    pub pool: u32,
    pub modes: String,
}

/// The spec of each depot kind whose script reads, by layer id: the game's
/// kinds from the install, a mod's from the mods folder.
pub fn depot_specs(
    install: Option<&Install>,
    mods: Option<&Mods>,
) -> HashMap<&'static str, DepotSpec> {
    let mut archives: HashMap<&str, Option<Archive>> = HashMap::new();
    let mut out = HashMap::new();
    for kind in DEPOT_KINDS {
        let spec = match kind.archive {
            Some(name) => {
                let Some(install) = install else { continue };
                archives
                    .entry(name)
                    .or_insert_with(|| install.archive("depots", name))
                    .as_mut()
                    .and_then(|a| a.read_text(kind.script))
                    .and_then(|text| parse_depot(&text))
                    .map(|spec| with_outline(spec, kind.outline))
            }
            None => mods
                .and_then(|m| m.text(kind.script))
                .and_then(|text| parse_mod_depot(&text))
                .map(|mut spec| {
                    // Its pieces are its footprint.
                    if kind.has_pieces() {
                        spec.face.clear();
                        spec.boxes.clear();
                    }
                    spec
                }),
        };
        if let Some(spec) = spec {
            out.insert(kind.id, spec);
        }
    }
    out
}

/// The spec with `outline` as its face when the two boxes agree.
fn with_outline(mut spec: DepotSpec, outline: Option<&[(f64, f64)]>) -> DepotSpec {
    if let Some(outline) = outline {
        let (a0, a1, b0, b1) = geometry::bounds(outline);
        let (fa0, fa1, fb0, fb1) = geometry::bounds(&spec.face);
        let off = [a0 - fa0, a1 - fa1, b0 - fb0, b1 - fb1];
        if off.iter().all(|d| d.abs() <= OUTLINE_FIT) {
            spec.face = outline.to_vec();
        }
    }
    spec
}

/// A game depot's script: it needs a ground face.
fn parse_depot(text: &str) -> Option<DepotSpec> {
    let face = ground_face(text)?;
    Some(depot_spec(text, face))
}

/// A mod's depot script: it needs a catchment radius and may have no
/// ground face.
fn parse_mod_depot(text: &str) -> Option<DepotSpec> {
    catchment_radius(text)?;
    Some(depot_spec(text, ground_face(text).unwrap_or_default()))
}

fn depot_spec(text: &str, face: Polygon) -> DepotSpec {
    let radius = catchment_radius(text);
    let pool = number_after(text, "maintenancePool").unwrap_or(0.0) as u32;
    let modes = transport_modes(text).unwrap_or_default();
    let mut boxes = Vec::new();
    if !face.is_empty()
        && let Some(Tree::List(colliders)) = table_after(text, "[\"collider\"]")
    {
        let (fa0, fa1, fb0, fb1) = geometry::bounds(&face);
        for c in colliders {
            let pts = points(&c);
            if pts.len() != 8 {
                continue;
            }
            let b = geometry::bounds(&pts);
            if b.0 < fa0 - 0.5 || b.1 > fa1 + 0.5 || b.2 < fb0 - 0.5 || b.3 > fb1 + 0.5 {
                boxes.push(b);
            }
        }
    }
    DepotSpec {
        face,
        boxes,
        radius,
        pool,
        modes,
    }
}

/// A station piece group: its layer, the models drawn in it (paths under
/// `stations/` without `.mdl`), whether a piece is drawn as its cell (its
/// box together with those of its thin `_con` and `_end` edge pieces), its
/// fill and the group's opacity.
pub struct StationKind {
    pub id: &'static str,
    pub label: &'static str,
    pub models: &'static [&'static str],
    pub joined: bool,
    pub fill: &'static str,
    pub opacity: f64,
}

/// Water stations brown, rail grey, street (bus and tram) light grey; drawn
/// in this order, so buildings lie over platforms. Rail and street pieces
/// come in eras a, b and c under the same names. Railings, roofs, lamps,
/// paths, entrances and the freight add-ons are left out
/// (`docs/station-pieces.md` in tf3-save-format).
pub const STATION_KINDS: &[StationKind] = &[
    StationKind {
        id: "station-water-decks",
        label: "Water stations: docks and platforms",
        models: &[
            "water/models/dock_100_25",
            "water/models/dock_50_12",
            "water/models/platform_100",
            "water/models/platform_25",
        ],
        joined: true,
        fill: "#a0622d",
        opacity: 0.65,
    },
    StationKind {
        id: "station-rail-platforms",
        label: "Rail stations: platforms",
        models: &[
            "rail/era_a/perron_5",
            "rail/era_a/perron_10",
            "rail/era_a/perron_5_underpass",
            "rail/era_a/perron_5_head",
            "rail/era_a/perron_5_head_con",
            "rail/era_b/perron_5",
            "rail/era_b/perron_10",
            "rail/era_b/perron_5_underpass",
            "rail/era_b/perron_5_head",
            "rail/era_b/perron_5_head_con",
            "rail/era_c/perron_5",
            "rail/era_c/perron_10",
            "rail/era_c/perron_5_underpass",
            "rail/era_c/perron_5_head",
            "rail/era_c/perron_5_head_con",
            // Era a only.
            "rail/era_a/station_3_perron_head",
            "rail/era_a/station_3_perron_side",
            "rail/era_a/station_3_perron_side_main",
        ],
        joined: false,
        fill: "#8c8c8c",
        opacity: 0.85,
    },
    StationKind {
        id: "station-street-platforms",
        label: "Street stations: platforms and stops",
        models: &[
            "street/modular_street_station/models/era_a/pltfrm_l_rep",
            "street/modular_street_station/models/era_a/pltfrm_r_rep",
            "street/modular_street_station/models/era_a/cargo_pltfrm_l_rep",
            "street/modular_street_station/models/era_a/cargo_pltfrm_r_rep",
            "street/modular_street_station/models/era_b/pltfrm_l_rep",
            "street/modular_street_station/models/era_b/pltfrm_r_rep",
            "street/modular_street_station/models/era_b/cargo_pltfrm_l_rep",
            "street/modular_street_station/models/era_b/cargo_pltfrm_r_rep",
            "street/modular_street_station/models/era_c/pltfrm_l_rep",
            "street/modular_street_station/models/era_c/pltfrm_r_rep",
            "street/modular_street_station/models/era_c/cargo_pltfrm_l_rep",
            "street/modular_street_station/models/era_c/cargo_pltfrm_r_rep",
            "street/small_stops/small_old",
            "street/small_stops/small_mid",
            "street/small_stops/small_new",
            "street/small_stops/small_old_twosided",
            "street/small_stops/small_mid_twosided",
            "street/small_stops/small_new_twosided",
        ],
        joined: false,
        fill: "#c8c8c8",
        opacity: 0.9,
    },
    StationKind {
        id: "station-water-buildings",
        label: "Water stations: main buildings",
        models: &[
            "water/models/main_building_s0",
            "water/models/main_building_s2",
        ],
        joined: false,
        fill: "#5a3512",
        opacity: 0.85,
    },
    StationKind {
        id: "station-rail-buildings",
        label: "Rail stations: buildings",
        models: &[
            "rail/era_a/station_1",
            "rail/era_a/station_1_main",
            "rail/era_a/station_2",
            "rail/era_a/station_2_main",
            "rail/era_a/station_3",
            "rail/era_a/station_3_main",
            "rail/era_b/station_1",
            "rail/era_b/station_1_main",
            "rail/era_b/station_2",
            "rail/era_b/station_2_main",
            "rail/era_b/station_3",
            "rail/era_b/station_3_main",
            "rail/era_c/station_1",
            "rail/era_c/station_1_main",
            "rail/era_c/station_2",
            "rail/era_c/station_2_main",
            "rail/era_c/station_3",
            "rail/era_c/station_3_main",
            "rail/cargo/station_1",
            "rail/cargo/station_1_main",
            "rail/cargo/station_2",
            "rail/cargo/station_2_main",
            "rail/cargo/station_3",
            "rail/cargo/station_3_main",
        ],
        joined: true,
        fill: "#4a4a4a",
        opacity: 0.9,
    },
    StationKind {
        id: "station-street-buildings",
        label: "Street stations: buildings",
        models: &[
            "street/modular_street_station/models/sidebuildings/streetstation_sidebuilding_era_a_s2",
            "street/modular_street_station/models/sidebuildings/streetstation_sidebuilding_era_b_s2",
            "street/modular_street_station/models/sidebuildings/streetstation_sidebuilding_era_c_s2",
        ],
        joined: false,
        fill: "#9a9a9a",
        opacity: 0.9,
    },
];

/// Whether a model under `stations/` is part of a bus, tram or cargo
/// station's border: the modular station's platform tiles and its centre,
/// path and entrance pieces, which fill the polygon the game lays out. The
/// thin edge strips, the ramp ends (`spcl`) and the path extensions (`pers`)
/// are not.
pub fn is_street_border(model: &str) -> bool {
    let Some(rest) = model.strip_prefix("street/modular_street_station/models/era_") else {
        return false;
    };
    let Some(stem) = ["a/", "b/", "c/"]
        .iter()
        .find_map(|era| rest.strip_prefix(era))
        .and_then(|r| r.strip_suffix(".mdl"))
    else {
        return false;
    };
    if matches!(
        stem,
        "pltfrm_l_rep"
            | "pltfrm_r_rep"
            | "cargo_pltfrm_l_rep"
            | "cargo_pltfrm_r_rep"
            | "cargo_path_rep"
    ) {
        return true;
    }
    ["cntr_", "entr_", "path_"].iter().any(|p| {
        stem.strip_prefix(p)
            .is_some_and(|tail| !tail.contains("spcl") && !tail.contains("pers"))
    })
}

/// The box of each station piece the kinds name, and of each border piece,
/// by model path (`stations/water/models/dock_100_25.mdl`): the x and y range
/// of the `boundingInfo` in the model's `.mdl`, for a joined piece taken
/// together with its edge pieces'. A piece whose file is missing is left out.
pub fn station_boxes(install: &Install) -> HashMap<String, Rect> {
    let mut archives: HashMap<&str, Option<Archive>> = HashMap::new();
    let mut out = HashMap::new();
    for kind in STATION_KINDS {
        for &name in kind.models {
            let (folder, base) = name.rsplit_once('/').unwrap();
            let archive_name = name.split('/').next().unwrap();
            let Some(archive) = archives
                .entry(archive_name)
                .or_insert_with(|| install.archive("stations", &format!("{archive_name}.zip")))
            else {
                continue;
            };
            let mut files = vec![format!("{name}.mdl")];
            if kind.joined {
                let mut edges: Vec<String> = archive
                    .names()
                    .iter()
                    .filter(|m| is_edge_piece(m, folder, base))
                    .cloned()
                    .collect();
                edges.sort();
                files.extend(edges);
            }
            files.retain(|f| archive.contains(f));
            let parts: Vec<Rect> = files
                .iter()
                .filter_map(|f| archive.read_text(f).as_deref().and_then(bounding_box))
                .collect();
            if let Some(&first) = parts.first() {
                let joined = parts.iter().fold(first, |acc, p| {
                    (
                        acc.0.min(p.0),
                        acc.1.min(p.1),
                        acc.2.max(p.2),
                        acc.3.max(p.3),
                    )
                });
                out.insert(format!("stations/{name}.mdl"), joined);
            }
        }
    }
    if let Some(Some(street)) = archives.get_mut("street") {
        let mut border: Vec<String> = street
            .names()
            .iter()
            .filter(|m| is_street_border(m))
            .cloned()
            .collect();
        border.sort();
        for m in border {
            if let Some(b) = street.read_text(&m).as_deref().and_then(bounding_box)
                && (b.0, b.1) != (b.2, b.3)
            {
                out.entry(format!("stations/{m}")).or_insert(b);
            }
        }
    }
    out
}

/// `folder/base_con....mdl` or `folder/base_end....mdl`, directly in `folder`.
fn is_edge_piece(member: &str, folder: &str, base: &str) -> bool {
    let Some(rest) = member
        .strip_prefix(folder)
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|r| r.strip_prefix(base))
        .and_then(|r| r.strip_prefix('_'))
    else {
        return false;
    };
    (rest.starts_with("con") || rest.starts_with("end"))
        && rest.ends_with(".mdl")
        && !rest.contains('/')
}

/// The box of each depot piece and ring among `models` (model paths, as
/// instances name them), by model path: the box of its model file, which is
/// `content/` and the path in the mod's folder. A piece whose file is not
/// found is left out.
pub fn depot_piece_boxes<'a>(
    mods: &Mods,
    models: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, Rect> {
    let mut out = HashMap::new();
    for model in models {
        if out.contains_key(model)
            || !DEPOT_KINDS
                .iter()
                .any(|k| k.archive.is_none() && k.role(model).is_some())
        {
            continue;
        }
        if let Some(b) = mods
            .text(&format!("content/{model}"))
            .as_deref()
            .and_then(bounding_box)
            && b.0 < b.2
            && b.1 < b.3
        {
            out.insert(model.to_owned(), b);
        }
    }
    out
}

/// (x0, y0, x1, y1) from the last `bbMin` and `bbMax` of a model file, each
/// pair in order (a mod's file can give a minimum above its maximum).
fn bounding_box(text: &str) -> Option<Rect> {
    let last_pair = |key: &str| {
        text.match_indices(key)
            .filter_map(|(at, _)| {
                let rest = after_assignment(&text[at + key.len()..])?.strip_prefix('{')?;
                let mut nums = rest
                    .split(',')
                    .take(2)
                    .map(|s| s.trim().parse::<f64>().ok());
                Some((nums.next()??, nums.next()??))
            })
            .last()
    };
    let (min, max) = (last_pair("bbMin")?, last_pair("bbMax")?);
    Some((
        min.0.min(max.0),
        min.1.min(max.1),
        min.0.max(max.0),
        min.1.max(max.1),
    ))
}

/// The ground face of each industry whose script declares one, by industry
/// key: `<key>/<key>.script.lua` in `industries/<key>.zip`. The water
/// industries keep a collider box only; see the measured faces in the SVG writer.
pub fn industry_faces(install: &Install) -> HashMap<String, Polygon> {
    let mut out = HashMap::new();
    let Ok(dir) = std::fs::read_dir(install.content.join("industries")) else {
        return out;
    };
    let mut keys: Vec<String> = dir
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".zip").map(str::to_owned))
        .collect();
    keys.sort();
    for key in keys {
        let face = install
            .archive("industries", &format!("{key}.zip"))
            .and_then(|mut a| a.read_text(&format!("{key}/{key}.script.lua")))
            .and_then(|text| ground_face(&text));
        if let Some(face) = face {
            out.insert(key, face);
        }
    }
    out
}

/// What one field slot covers, in the industry's own axes.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldShape {
    /// Ground faces: a plot's rectangle, or a module's faces placed and turned.
    Faces(Vec<Polygon>),
    /// A water module, which has a collider box and no ground face: a disc
    /// round the slot's place, (a, b) and radius, half the box's longer side.
    Disc((f64, f64), f64),
}

/// One row of an industry's `fieldConfig.fields`.
#[derive(Debug, Clone, PartialEq)]
struct FieldRow {
    pos: (f64, f64),
    size: Option<(f64, f64)>,
    kind: Option<String>,
}

/// Each industry's field slots by key, slot 1 first (`None` where a slot's
/// shape does not read), from `fieldConfig.fields` in `<key>/<key>.con.lua`:
/// a row with a `size` is a plot of that size centred on its `pos` (farms,
/// the forest); a row with a `type` is the module file that
/// `<key>/modules.script.lua` requires under that name, a `_<N>deg` ending
/// turning it by N degrees about its `pos` (the oil well's pumpjacks, the
/// mines' pits); a row with neither is the script's one module (the oil
/// platform's drilling grounds).
pub fn industry_fields(install: &Install) -> HashMap<String, Vec<Option<FieldShape>>> {
    let mut out = HashMap::new();
    let Ok(dir) = std::fs::read_dir(install.content.join("industries")) else {
        return out;
    };
    let mut keys: Vec<String> = dir
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".zip").map(str::to_owned))
        .collect();
    keys.sort();
    for key in keys {
        let Some(mut archive) = install.archive("industries", &format!("{key}.zip")) else {
            continue;
        };
        let rows = archive
            .read_text(&format!("{key}/{key}.con.lua"))
            .map(|t| field_rows(&t))
            .unwrap_or_default();
        if rows.is_empty() {
            continue;
        }
        let requires = archive
            .read_text(&format!("{key}/modules.script.lua"))
            .map(|t| module_requires(&t))
            .unwrap_or_default();
        let mut modules: HashMap<String, Option<String>> = HashMap::new();
        let shapes = rows
            .iter()
            .map(|row| {
                field_shape(row, &requires, |file| {
                    modules
                        .entry(file.to_owned())
                        .or_insert_with(|| archive.read_text(&format!("{key}/{file}")))
                        .clone()
                })
            })
            .collect();
        out.insert(key, shapes);
    }
    out
}

/// The shape of one slot; `module` reads a module file by its name in the
/// industry's folder.
fn field_shape(
    row: &FieldRow,
    requires: &HashMap<String, String>,
    mut module: impl FnMut(&str) -> Option<String>,
) -> Option<FieldShape> {
    let (pa, pb) = row.pos;
    if let Some((w, h)) = row.size {
        let (hw, hh) = (w / 2.0, h / 2.0);
        return Some(FieldShape::Faces(vec![vec![
            (pa - hw, pb - hh),
            (pa + hw, pb - hh),
            (pa + hw, pb + hh),
            (pa - hw, pb + hh),
        ]]));
    }
    let (file, degrees) = match &row.kind {
        Some(kind) => {
            let (base, degrees) = turned(kind);
            (requires.get(base)?, degrees)
        }
        None => {
            let mut files = requires.values();
            match (files.next(), files.next()) {
                (Some(only), None) => (only, 0.0),
                _ => return None,
            }
        }
    };
    let text = module(file)?;
    let (s, c) = degrees.to_radians().sin_cos();
    let place = |&(a, b): &(f64, f64)| (pa + c * a - s * b, pb + s * a + c * b);
    let faces = ground_faces(&text);
    if !faces.is_empty() {
        return Some(FieldShape::Faces(
            faces
                .iter()
                .map(|f| f.iter().map(place).collect())
                .collect(),
        ));
    }
    let Tree::List(boxes) = table_after(&text, "[\"collider\"]")? else {
        return None;
    };
    let corners = points(boxes.first()?);
    if corners.is_empty() {
        return None;
    }
    let (a0, a1, b0, b1) = crate::geometry::bounds(&corners);
    Some(FieldShape::Disc(row.pos, (a1 - a0).max(b1 - b0) / 2.0))
}

/// A field type and its turn: `pumpjack_135deg` is `pumpjack` turned 135
/// degrees; a type with no such ending is not turned.
fn turned(kind: &str) -> (&str, f64) {
    kind.strip_suffix("deg")
        .and_then(|k| k.rsplit_once('_'))
        .and_then(|(base, n)| Some((base, n.parse::<f64>().ok()?)))
        .unwrap_or((kind, 0.0))
}

/// `local NAME = require "FILE"` lines of a modules script, NAME to FILE,
/// leaving out the shared scripts (paths starting `/` or `::`).
fn module_requires(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("local ")?;
            let (name, rest) = rest.split_once('=')?;
            let rest = rest.trim().strip_prefix("require")?.trim();
            let file = rest.strip_prefix('"')?.split('"').next()?;
            (!file.starts_with('/') && !file.starts_with(':'))
                .then(|| (name.trim().to_owned(), file.to_owned()))
        })
        .collect()
}

/// The rows of `fields = { ... }` inside `fieldConfig`, comments left out.
fn field_rows(text: &str) -> Vec<FieldRow> {
    let text: String = text
        .lines()
        .map(|l| l.split("--").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let Some(config) = text.find("fieldConfig") else {
        return Vec::new();
    };
    let Some(body) = text[config..].match_indices("fields").find_map(|(at, _)| {
        let rest = after_assignment(&text[config + at + "fields".len()..])?;
        braced(rest)
    }) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find('{') {
        let Some(item) = braced(&rest[open..]) else {
            break;
        };
        rest = &rest[open + item.len() + 2..];
        let pair = |key: &str| match table_after(item, key)? {
            Tree::List(v) => match v.as_slice() {
                [Tree::Number(a), Tree::Number(b), ..] => Some((*a, *b)),
                _ => None,
            },
            Tree::Number(_) => None,
        };
        let Some(pos) = pair("pos") else {
            continue;
        };
        let kind = item.match_indices("type").find_map(|(at, _)| {
            let rest = after_assignment(&item[at + "type".len()..])?.strip_prefix('"')?;
            rest.split('"').next().map(str::to_owned)
        });
        rows.push(FieldRow {
            pos,
            size: pair("size"),
            kind,
        });
    }
    rows
}

/// The text inside the braces at the start of `text` (which starts with
/// `{`), up to the matching close.
fn braced(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (i, ch) in text.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&text[1..i]);
                }
            }
            _ if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

/// What a warehouse model's file says about its shape on the ground, in the
/// model's own axes: its box, and the boxes of its cargo bays (where a load
/// indicator shows the stock: a bulk module's two bunkers, a liquid module's
/// tank).
#[derive(Debug, Clone, PartialEq)]
pub struct WarehouseModel {
    pub bounds: Rect,
    pub bays: Vec<Rect>,
}

/// Each warehouse model in the install's `warehouses.zip`, by model path
/// (`warehouses/wh_bulk/wh_bulk_center.mdl`). A model whose box does not
/// read is left out.
pub fn warehouse_models(install: &Install) -> HashMap<String, WarehouseModel> {
    let mut out = HashMap::new();
    let Ok(mut archive) = Archive::open(&install.content.join("warehouses.zip")) else {
        return out;
    };
    let mut names: Vec<String> = archive
        .names()
        .iter()
        .filter(|n| n.starts_with("warehouses/") && n.ends_with(".mdl"))
        .cloned()
        .collect();
    names.sort();
    for name in names {
        if let Some(model) = archive
            .read_text(&name)
            .as_deref()
            .and_then(warehouse_model)
        {
            out.insert(name, model);
        }
    }
    out
}

/// The box after `boundingInfo` and the box of each `cargoBay`. A model file
/// can hold several `bbMin` and `bbMax` pairs (the bays' come after the
/// model's), so each is read after its own key.
fn warehouse_model(text: &str) -> Option<WarehouseModel> {
    let at = text.find("boundingInfo")?;
    let bounds = box_in(&text[at..])?;
    let bays = text
        .match_indices("cargoBay")
        .filter_map(|(at, _)| box_in(&text[at..]))
        .filter(|b| b.0 < b.2 && b.1 < b.3)
        .collect();
    Some(WarehouseModel { bounds, bays })
}

/// (x0, y0, x1, y1) from the first `bbMin` and `bbMax` in `text`, each pair
/// in order.
fn box_in(text: &str) -> Option<Rect> {
    let pair = |key: &str| match table_after(text, key)? {
        Tree::List(v) => match v.as_slice() {
            [Tree::Number(a), Tree::Number(b), ..] => Some((*a, *b)),
            _ => None,
        },
        Tree::Number(_) => None,
    };
    let (min, max) = (pair("bbMin")?, pair("bbMax")?);
    Some((
        min.0.min(max.0),
        min.1.min(max.1),
        min.0.max(max.0),
        min.1.max(max.1),
    ))
}

// Scanners for the generated Lua tables.

/// A nested table of numbers: `{ { 1, 2, 0 }, { 3, 4, 0 } }`.
#[derive(Debug, Clone, PartialEq)]
enum Tree {
    Number(f64),
    List(Vec<Tree>),
}

/// The text after `key`, optional space, `=` and optional space.
fn after_assignment(rest: &str) -> Option<&str> {
    Some(rest.trim_start().strip_prefix('=')?.trim_start())
}

/// The first table of numbers assigned to `key`.
fn table_after(text: &str, key: &str) -> Option<Tree> {
    text.match_indices(key).find_map(|(at, _)| {
        let rest = after_assignment(&text[at + key.len()..])?;
        rest.starts_with('{')
            .then(|| parse_tree(rest).map(|(t, _)| t))?
    })
}

/// A table of numbers at the start of `text` (which starts with `{`), and the
/// text after it. `None` if anything but numbers, commas and tables is inside.
fn parse_tree(text: &str) -> Option<(Tree, &str)> {
    parse_tree_at(text, 0)
}

/// How many tables may sit inside one another. The scripts' own tables go a
/// few deep; a mod's script is third-party text, and this keeps a million
/// opening braces from overflowing the stack.
const MAX_TREE_DEPTH: usize = 16;

fn parse_tree_at(text: &str, depth: usize) -> Option<(Tree, &str)> {
    if depth >= MAX_TREE_DEPTH {
        return None;
    }
    let mut rest = text.strip_prefix('{')?;
    let mut items = Vec::new();
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if let Some(r) = rest.strip_prefix('}') {
            return Some((Tree::List(items), r));
        }
        if rest.starts_with('{') {
            let (t, r) = parse_tree_at(rest, depth + 1)?;
            items.push(t);
            rest = r;
        } else {
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
                .unwrap_or(rest.len());
            items.push(Tree::Number(rest[..end].parse().ok()?));
            rest = &rest[end..];
        }
    }
}

/// The first two numbers of each item of a list: points in a and b.
fn points(tree: &Tree) -> Polygon {
    let Tree::List(items) = tree else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|p| match p {
            Tree::List(v) => match v.as_slice() {
                [Tree::Number(a), Tree::Number(b), ..] => Some((*a, *b)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The first polygon of `["ground_face"]`.
fn ground_face(text: &str) -> Option<Polygon> {
    let Tree::List(faces) = table_after(text, "[\"ground_face\"]")? else {
        return None;
    };
    let face = points(faces.first()?);
    (face.len() >= 3).then_some(face)
}

/// Every polygon of `["ground_face"]` with at least three points.
fn ground_faces(text: &str) -> Vec<Polygon> {
    let Some(Tree::List(faces)) = table_after(text, "[\"ground_face\"]") else {
        return Vec::new();
    };
    faces.iter().map(points).filter(|f| f.len() >= 3).collect()
}

/// The integer at the start of `text`.
fn leading_integer(text: &str) -> Option<f64> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text[..end].parse().ok()
}

/// The first number assigned to `key`.
fn number_after(text: &str, key: &str) -> Option<f64> {
    text.match_indices(key)
        .find_map(|(at, _)| leading_integer(after_assignment(&text[at + key.len()..])?))
}

/// `catchmentArea = { radius = N` (or `= captureParams.radius or N`).
fn catchment_radius(text: &str) -> Option<u32> {
    text.match_indices("catchmentArea").find_map(|(at, _)| {
        let rest = after_assignment(&text[at + "catchmentArea".len()..])?
            .strip_prefix('{')?
            .trim_start()
            .strip_prefix("radius")?;
        let rest = after_assignment(rest)?;
        let rest = rest
            .strip_prefix("captureParams.radius or ")
            .unwrap_or(rest);
        leading_integer(rest).map(|r| r as u32)
    })
}

/// The quoted names of the first `transportModes = { ... }`, comma separated.
fn transport_modes(text: &str) -> Option<String> {
    text.match_indices("transportModes").find_map(|(at, _)| {
        let rest = after_assignment(&text[at + "transportModes".len()..])?.strip_prefix('{')?;
        let body = &rest[..rest.find('}')?];
        let names: Vec<&str> = body.split('"').skip(1).step_by(2).collect();
        Some(names.join(","))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPOT: &str = r#"
		["ground_face"] = {
				{ { 38.5, -20.25, 0.00000 }, { -17.75, -20.25, 0.00000 }, { -17.75, 19.5, 0.0 }, { 38.5, 19.5, 0.0 }, },
			},
		["collider"] = {
				{ { -21, 60, -8 }, { 28, 60, -8 }, { 28, 23, -8 }, { -21, 23, -8 }, { -21, 60, 28 }, { 28, 60, 28 }, { 28, 23, 28 }, { -21, 23, 28 }, },
				{ { 0, 0, 0 }, { 1, 0, 0 }, { 1, 1, 0 }, { 0, 1, 0 }, { 0, 0, 1 }, { 1, 0, 1 }, { 1, 1, 1 }, { 0, 1, 1 }, },
			},
		local transportModes = { "BUS", "TRUCK", }
		catchmentArea = {
				radius = captureParams.radius or 400,
			},
			maintenancePool = 12,
	"#;

    #[test]
    fn tables_nested_too_deep_are_refused_not_followed() {
        // The scripts' own tables are a few deep.
        assert!(parse_tree("{ { { 1, 2 }, }, }").is_some());
        let deep = format!("{}1{}", "{".repeat(15), "}".repeat(15));
        assert!(parse_tree(&deep).is_some());
        // A third-party script can hold any number of opening braces.
        let deep = format!("{}1{}", "{".repeat(17), "}".repeat(17));
        assert!(parse_tree(&deep).is_none());
        let deep = "{".repeat(1_000_000);
        assert!(parse_tree(&deep).is_none());
        let script = format!("[\"ground_face\"] = {deep}");
        assert!(parse_depot(&script).is_none());
    }

    #[test]
    fn a_depot_script_reads() {
        let spec = parse_depot(DEPOT).unwrap();
        assert_eq!(spec.face[0], (38.5, -20.25));
        assert_eq!(spec.face.len(), 4);
        // The first collider box reaches past the face; the second lies inside it.
        assert_eq!(spec.boxes, [(-21.0, 28.0, 23.0, 60.0)]);
        assert_eq!((spec.radius, spec.pool), (Some(400), 12));
        assert_eq!(spec.modes, "BUS,TRUCK");
        assert_eq!(parse_depot("no fields here"), None);
    }

    #[test]
    fn the_road_depot_is_drawn_as_its_lot_while_the_face_agrees() {
        let spec = with_outline(parse_depot(DEPOT).unwrap(), Some(ROAD_DEPOT_LOT));
        assert_eq!(spec.face, ROAD_DEPOT_LOT);
        let moved = DEPOT.replace("38.5", "48.5");
        let spec = with_outline(parse_depot(&moved).unwrap(), Some(ROAD_DEPOT_LOT));
        assert_eq!(spec.face.len(), 4, "a face that no longer fits is kept");
        assert_eq!(
            with_outline(parse_depot(DEPOT).unwrap(), None).face.len(),
            4
        );
    }

    /// A boathouse script in a mod's own hand-written style: a radius, a
    /// pool and two modes, and no ground face.
    const MOD_DEPOT: &str = r#"
    local depot = {
        transportModes = { "SHIP", "SMALL_SHIP", },
        catchmentArea = {
            radius = 600,
            position = { 0, 0, 0, },
        },
        maintenancePool = 2,
    }
    "#;

    /// A new empty folder under the system's temporary folder.
    fn temp_folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tf3map-install-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_mods_depot_reads_from_the_mods_folder() {
        let mods = temp_folder("mods");
        let boathouse = DEPOT_KINDS.iter().find(|k| k.archive.is_none()).unwrap();
        // A mod folder without the script, one with it, and one later by name.
        std::fs::create_dir_all(mods.join("1000")).unwrap();
        write(&mods.join("2000").join(boathouse.script), MOD_DEPOT);
        write(
            &mods.join("3000").join(boathouse.script),
            &MOD_DEPOT.replace("600", "900"),
        );
        let found = Mods::find(Some(&mods)).unwrap();
        let specs = depot_specs(None, Some(&found));
        assert_eq!(specs.len(), 1, "no install: only the mod's kind");
        let spec = &specs[boathouse.id];
        assert!(spec.face.is_empty() && spec.boxes.is_empty());
        assert_eq!((spec.radius, spec.pool), (Some(600), 2));
        assert_eq!(spec.modes, "SHIP,SMALL_SHIP");
        // Without the folder, or with no radius, there is no spec.
        assert!(depot_specs(None, None).is_empty());
        assert_eq!(parse_mod_depot("maintenancePool = 2"), None);
        assert!(Mods::find(Some(&mods.join("absent"))).is_none());
        std::fs::remove_dir_all(&mods).unwrap();
    }

    fn kind(id: &str) -> &'static DepotKind {
        DEPOT_KINDS.iter().find(|k| k.id == id).unwrap()
    }

    #[test]
    fn a_depot_built_from_pieces_names_them() {
        let roundhouse = kind("roundhouses");
        let folder = "depots/rail/ringlokschuppen/";
        assert!(roundhouse.has_pieces() && !kind("rail-depots").has_pieces());
        assert_eq!(
            roundhouse.role(&format!("{folder}mav2002_rl_building_m1.mdl")),
            Some(PieceRole::Piece)
        );
        assert_eq!(
            roundhouse.role(&format!("{folder}mav2002_rl_seg_track.mdl")),
            Some(PieceRole::Ring)
        );
        // Doors, wires and the centre piece itself are no pieces.
        for other in [
            "mav2002_rl_doors_c",
            "mav2002_rl_fahrdraht",
            "mav2002_rl_wanne",
        ] {
            assert_eq!(roundhouse.role(&format!("{folder}{other}.mdl")), None);
        }
        assert!(roundhouse.draws(roundhouse.model));
        assert!(!roundhouse.draws("depots/rail/other/mav2002_rl_building_m1.mdl"));
        // The modern depot's hit box and doors are not drawn.
        let modern = kind("modern-depots");
        assert!(
            modern
                .role("depots/rail/depot_modern/maikc_depot_modern_m1.mdl")
                .is_some()
        );
        assert!(
            modern
                .role("depots/rail/depot_modern/maikc_depot_modern_module_hitbox.mdl")
                .is_none()
        );
        assert!(
            modern
                .role("depots/rail/depot_modern/maikc_depot_modern_r1_tuer.mdl")
                .is_none()
        );
    }

    #[test]
    fn piece_boxes_and_a_pieced_depots_spec_read_from_the_mods_folder() {
        let mods = temp_folder("pieces");
        let roundhouse = kind("roundhouses");
        let shed = "depots/rail/ringlokschuppen/mav2002_rl_building_m1.mdl";
        let ring = "depots/rail/ringlokschuppen/mav2002_rl_seg.mdl";
        let door = "depots/rail/ringlokschuppen/mav2002_rl_doors_c.mdl";
        let flat = "depots/rail/ringlokschuppen/mav2002_rl_open_m1.mdl";
        let mdl = |min: (f64, f64), max: (f64, f64)| {
            format!(
                "boundingInfo = {{ bbMax = {{ {}, {}, 1, }}, bbMin = {{ {}, {}, 0, }}, }},",
                max.0, max.1, min.0, min.1
            )
        };
        let root = mods.join("5000").join("content");
        write(&root.join(shed), &mdl((28.7, -6.8), (69.0, 6.8)));
        // A minimum above the maximum, as one mod's file has.
        write(&root.join(ring), &mdl((17.6, 1.7), (13.0, -1.7)));
        write(&root.join(door), &mdl((39.0, -2.2), (39.2, 2.2)));
        write(&root.join(flat), &mdl((1.0, 0.0), (1.0, 5.0)));
        write(
            &mods.join("5000").join(roundhouse.script),
            "groundFaces = {}\nmainSubconstruction.depot = { transportModes = { \"TRAIN\", }, \
             catchmentArea = { radius = 800, }, maintenancePool = 1, }",
        );
        let found = Mods::find(Some(&mods)).unwrap();
        let boxes = depot_piece_boxes(&found, [shed, ring, door, flat, shed]);
        assert_eq!(
            boxes.len(),
            2,
            "the door is no piece; a box with no area is left out"
        );
        assert_eq!(boxes[shed], (28.7, -6.8, 69.0, 6.8));
        assert_eq!(boxes[ring], (13.0, -1.7, 17.6, 1.7));
        let spec = &depot_specs(None, Some(&found))[roundhouse.id];
        assert!(spec.face.is_empty());
        assert_eq!((spec.radius, spec.pool), (Some(800), 1));
        std::fs::remove_dir_all(&mods).unwrap();
    }

    #[test]
    fn the_mods_folder_sits_beside_the_steam_folder() {
        let home = temp_folder("home").canonicalize().unwrap();
        assert!(mods_folders(&home).is_empty());
        // The snap's Steam keeps its data under its own folder, and mod.io's next to it.
        std::fs::create_dir_all(home.join(HOME_STEAM[2])).unwrap();
        let snap = home.join("snap/steam/common").join(MODS_FOLDERS[0]);
        std::fs::create_dir_all(&snap).unwrap();
        let local = home.join(MODS_FOLDERS[1]);
        std::fs::create_dir_all(&local).unwrap();
        assert_eq!(mods_folders(&home), [snap, local]);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn the_last_bounding_box_counts() {
        let mdl = "boundingInfo = {\n bbMax = { 1, 2, 3 },\n bbMin = { -1, -2, -3 },\n},\n\
                   boundingInfo = {\n bbMax = { 12.5, 4, 3, },\n bbMin = { -12.5, -4.25, 0, },\n},";
        assert_eq!(bounding_box(mdl), Some((-12.5, -4.25, 12.5, 4.0)));
        let swapped = "bbMin = { 0, 0, 0 }, bbMax = { -30.5, -13, 4 },";
        assert_eq!(bounding_box(swapped), Some((-30.5, -13.0, 0.0, 0.0)));
        assert_eq!(bounding_box("bbMax = { 1, 2 }"), None);
    }

    #[test]
    fn street_border_pieces() {
        let p = "street/modular_street_station/models/era_b/";
        for yes in [
            "pltfrm_l_rep",
            "cargo_path_rep",
            "cntr_a",
            "entr_big_1",
            "path_x",
        ] {
            assert!(is_street_border(&format!("{p}{yes}.mdl")), "{yes}");
        }
        for no in ["pltfrm_l_end", "path_spcl_1", "entr_pers", "other"] {
            assert!(!is_street_border(&format!("{p}{no}.mdl")), "{no}");
        }
        assert!(!is_street_border(
            "street/modular_street_station/models/era_d/cntr_a.mdl"
        ));
    }

    #[test]
    fn edge_pieces_sit_beside_their_piece() {
        assert!(is_edge_piece(
            "water/models/dock_50_12_con_l.mdl",
            "water/models",
            "dock_50_12"
        ));
        assert!(is_edge_piece(
            "water/models/dock_50_12_end.mdl",
            "water/models",
            "dock_50_12"
        ));
        assert!(!is_edge_piece(
            "water/models/dock_50_12.mdl",
            "water/models",
            "dock_50_12"
        ));
        assert!(!is_edge_piece(
            "water/models/dock_50_12_con/x.mdl",
            "water/models",
            "dock_50_12"
        ));
    }

    #[test]
    fn a_warehouse_model_keeps_its_own_box_and_its_bays() {
        // The bays' pairs come after the model's, and the last pair is a bay's.
        let text = r#"
            boundingInfo = { bbMax = { 12.5, 14, 5, }, bbMin = { -12.5, -14.5, 0, }, },
            metadata = { loadIndicator = { configs = {
                bay_0 = { cargoBay = { bbMax = { 10, 9, 0, }, bbMin = { 0, -11, -4, }, type = "LEVEL", }, },
                bay_1 = { cargoBay = { bbMax = { 0, 9, 0, }, bbMin = { -10, -11, -4, }, type = "LEVEL", }, },
            }, }, },
        "#;
        assert_eq!(
            warehouse_model(text),
            Some(WarehouseModel {
                bounds: (-12.5, -14.5, 12.5, 14.0),
                bays: vec![(0.0, -11.0, 10.0, 9.0), (-10.0, -11.0, 0.0, 9.0)],
            })
        );
        assert_eq!(bounding_box(text), Some((-10.0, -11.0, 0.0, 9.0)));
        assert_eq!(warehouse_model("bbMin = { 0, 0, 0 }"), None);
    }

    const CON_FIELDS: &str = r#"
	local stockListConfig = { stocks = { } }
	local fieldConfig = {
		type = "test_field",
		fields = {
			-- plots
			{ pos = { 120, -30 }, size = { 60, 80 }, road = { 0, 1, 0, 0} },
			--{ pos = { 9, 9 }, size = { 9, 9 } },
			{ pos = { -50, 2.5e1 }, type = "pad_90deg" },
			{ pos = { 0, 0 }, type="pit_module_02" },
			{ pos = { 10, 10 }, type = "unknown" },
		},
		alignToTerrain= true,
	}
"#;

    const MODULES_SCRIPT: &str = r#"
local constructionutil = require "::/scripts/construction/constructionutil.lua"
local transf = require "/scripts/mat4.tl"
local pad = require "pad_module.lua"
local pit_module_02 = require "pit_module_02.lua"
"#;

    #[test]
    fn field_rows_read_pos_size_and_type() {
        let rows = field_rows(CON_FIELDS);
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert_eq!(rows[0].pos, (120.0, -30.0));
        assert_eq!(rows[0].size, Some((60.0, 80.0)));
        assert_eq!(rows[1].pos, (-50.0, 25.0));
        assert_eq!(rows[1].kind.as_deref(), Some("pad_90deg"));
        assert_eq!(rows[2].kind.as_deref(), Some("pit_module_02"));
        assert!(field_rows("local fieldConfig = { }").is_empty());
        assert!(field_rows("no fields here").is_empty());
    }

    #[test]
    fn module_requires_skip_the_shared_scripts() {
        let requires = module_requires(MODULES_SCRIPT);
        assert_eq!(requires.len(), 2);
        assert_eq!(requires["pad"], "pad_module.lua");
        assert_eq!(requires["pit_module_02"], "pit_module_02.lua");
        assert_eq!(turned("pumpjack_135deg"), ("pumpjack", 135.0));
        assert_eq!(turned("coal_mine_module_01"), ("coal_mine_module_01", 0.0));
        assert_eq!(turned("odd_xdeg"), ("odd_xdeg", 0.0));
    }

    #[test]
    fn field_shapes_from_size_module_faces_and_colliders() {
        let rows = field_rows(CON_FIELDS);
        let requires = module_requires(MODULES_SCRIPT);
        let module = |file: &str| {
            match file {
            "pad_module.lua" => Some(
                r#"["ground_face"] = { { { 10.0, -5.0, 0.0 }, { 10.0, 5.0, 0.0 }, { -10.0, 5.0, 0.0 }, }, },"#
                    .to_owned(),
            ),
            "pit_module_02.lua" => Some(
                r#"["collider"] = { { { 13.0, -13.0, 20.0 }, { -20.0, -13.0, 20.0 }, { -20.0, 13.0, -2.0 }, }, },"#
                    .to_owned(),
            ),
            _ => None,
        }
        };
        let shape = |i: usize| field_shape(&rows[i], &requires, module);
        assert_eq!(
            shape(0),
            Some(FieldShape::Faces(vec![vec![
                (90.0, -70.0),
                (150.0, -70.0),
                (150.0, 10.0),
                (90.0, 10.0)
            ]]))
        );
        // Turned 90 degrees about (-50, 25): (10, -5) goes to (5, 10) from there.
        let Some(FieldShape::Faces(faces)) = shape(1) else {
            panic!("{:?}", shape(1));
        };
        let near =
            |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9;
        assert!(near(faces[0][0], (-45.0, 35.0)), "{faces:?}");
        assert!(near(faces[0][2], (-55.0, 15.0)), "{faces:?}");
        // No ground face: a disc of half the collider's longer side.
        assert_eq!(shape(2), Some(FieldShape::Disc((0.0, 0.0), 16.5)));
        assert_eq!(shape(3), None);
        // A row with no type is the script's one module.
        let lone = HashMap::from([("ground".to_owned(), "pad_module.lua".to_owned())]);
        let row = FieldRow {
            pos: (0.0, 0.0),
            size: None,
            kind: None,
        };
        assert!(matches!(
            field_shape(&row, &lone, module),
            Some(FieldShape::Faces(_))
        ));
        assert_eq!(field_shape(&row, &requires, module), None);
    }
}
