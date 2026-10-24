//! Facts read from the player's own game install at run time: the ground
//! faces of depots and industries and the boxes of station pieces, and from
//! the mods folder the scripts of a mod's depots. Nothing read here is
//! stored in the tool or written into the picture beyond the shapes drawn
//! from it.
//!
//! The files are in the `base/content` archives: depot scripts in
//! `depots/*.zip`, station model files in `stations/*.zip`, industry scripts
//! in `industries/<key>.zip`. Their tables are generated Lua, so the few
//! fields needed are picked out by name. A mod's script is a plain file in
//! its folder under the mod.io game's `mods` folder, one folder per mod.

use std::collections::HashMap;
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

    /// The text of the file at `rel` in the first mod folder that has it:
    /// the mods folders in turn, and each one's mod folders by name.
    fn script(&self, rel: &str) -> Option<String> {
        self.dirs.iter().find_map(|dir| {
            let mut mods: Vec<String> = std::fs::read_dir(dir)
                .ok()?
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect();
            mods.sort();
            mods.iter().find_map(|m| {
                let bytes = std::fs::read(dir.join(m).join(rel)).ok()?;
                Some(String::from_utf8_lossy(&bytes).into_owned())
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
pub struct DepotKind {
    pub id: &'static str,
    pub label: &'static str,
    pub archive: Option<&'static str>,
    pub script: &'static str,
    pub model: &'static str,
    pub fill: &'static str,
}

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
    },
    DepotKind {
        id: "road-depots",
        label: "Road depots",
        archive: Some("road.zip"),
        script: "road/road_depot/road_depot.script.lua",
        model: "depots/road/road_depot/road_depot.mdl",
        fill: "#b4b4b4",
    },
    DepotKind {
        id: "tram-depots",
        label: "Tram depots",
        archive: Some("road.zip"),
        script: "road/tram_depot/tram_depot.script.lua",
        model: "depots/road/tram_depot/tram_depot.mdl",
        fill: "#f2f2f2",
    },
    DepotKind {
        id: "rail-maintenance-buildings",
        label: "Rail maintenance buildings",
        archive: Some("rail.zip"),
        script: "rail/rail_maint_station.script.lua",
        model: "depots/rail/rail_maint_station/rail_maint_station.mdl",
        fill: "#464646",
    },
    DepotKind {
        id: "rail-depots",
        label: "Rail depots",
        archive: Some("rail.zip"),
        script: "rail/rail_depot.script.lua",
        model: "depots/rail/rail_depot.mdl",
        fill: "#646464",
    },
    DepotKind {
        id: "ship-maintenance-buildings",
        label: "Ship maintenance buildings",
        archive: Some("water.zip"),
        script: "water/water_maint_station.script.lua",
        model: "depots/water/water_maint_station/water_maint_station.mdl",
        fill: "#4d2d12",
    },
    DepotKind {
        id: "ship-depots",
        label: "Ship depots",
        archive: Some("water.zip"),
        script: "water/water_depot.script.lua",
        model: "depots/water/water_depot.mdl",
        fill: "#7b4a24",
    },
    DepotKind {
        id: "boathouses",
        label: "Boathouses (Boathouse mod)",
        archive: None,
        script: "content/assets/ingo_bootsschuppen/ingo_bootsschuppen_maint.script.lua",
        model: "assets/ingo_bootsschuppen/ingo_bootsschuppen.mdl",
        fill: "#7b4a24",
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
            }
            None => mods
                .and_then(|m| m.script(kind.script))
                .and_then(|text| parse_mod_depot(&text)),
        };
        if let Some(spec) = spec {
            out.insert(kind.id, spec);
        }
    }
    out
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
/// in this order, so buildings lie over platforms. Railings, roofs, lamps,
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
            "street/small_stops/small_old",
            "street/small_stops/small_mid",
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

/// (x0, y0, x1, y1) from the last `bbMin` and `bbMax` of a model file.
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
    Some((min.0, min.1, max.0, max.1))
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
}
