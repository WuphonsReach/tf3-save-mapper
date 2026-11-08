//! `tf3map` end to end on the core tests' hand-built save: which edges the
//! filters leave out, in what order, and what the summary line says.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../../tf3-save-mapper-core/tests/common/mod.rs"]
mod common;

use common::{EdgeSpec, FREE_RUN_START, UNLANED_NODES};

/// The layers that need the game install, left out: tests never need one.
/// The model table is still read, for the loose-piece filter. The warehouses
/// read the install too but are drawn without it (with nothing inside), so
/// they stay in.
const NO_INSTALL: [&str; 3] = ["--no-industries", "--no-depots", "--no-stations"];

/// Two town roads; an industry siding joined to them; a road with an end in
/// the free-id list; a town street whose nodes have no lanes; and a short
/// group of industry road joined to nothing, far from the farm and the
/// hangar (the stream's two `industries/` models).
fn edges() -> Vec<EdgeSpec> {
    let stub = "street/country/ind_country_old_small";
    vec![
        EdgeSpec {
            a: 100,
            b: 101,
            p0: [-1000.0, 500.0, 37.5],
            p1: [-900.0, 500.0, 52.25],
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 101,
            b: 102,
            p0: [-900.0, 500.0, 52.25],
            p1: [-900.0, 400.0, 81.75],
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 102,
            b: 103,
            p0: [-900.0, 400.0, 81.75],
            p1: [-840.0, 440.0, 90.0],
            template: "industries/shared/industry_track/industry",
            lanes: &[0x4080],
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 101,
            b: FREE_RUN_START + 500,
            p0: [-900.0, 500.0, 52.25],
            p1: [-850.0, 560.0, 60.0],
            template: "street/country/country_new_large",
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: UNLANED_NODES[0],
            b: UNLANED_NODES[1],
            p0: [0.0, -800.0, 40.0],
            p1: [60.0, -800.0, 40.0],
            template: "street/town/town_new_small",
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 0x7200,
            b: 0x7201,
            p0: [200.0, -500.0, 60.0],
            p1: [260.0, -500.0, 60.0],
            template: stub,
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 0x7201,
            b: 0x7202,
            p0: [260.0, -500.0, 60.0],
            p1: [320.0, -500.0, 60.0],
            template: stub,
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 0x7202,
            b: 0x7203,
            p0: [320.0, -500.0, 60.0],
            p1: [320.0, -488.0, 60.0],
            template: "street/constructions/ind_entrance_old_country",
            ..EdgeSpec::default()
        },
    ]
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tf3map-pipeline-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Draw the save with `args` (and [`NO_INSTALL`]): the SVG and the summary,
/// the last line of stderr up to its "; wrote".
fn draw(dir: &Path, args: &[&str]) -> (String, String) {
    let save = dir.join("synthetic.sav");
    std::fs::write(&save, common::sav(&common::stream(&edges()))).unwrap();
    let svg = dir.join("synthetic.svg");
    let out = Command::new(env!("CARGO_BIN_EXE_tf3map"))
        .arg(&save)
        .arg("-o")
        .arg(&svg)
        .args(NO_INSTALL)
        .args(args)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    let summary = stderr.lines().last().unwrap_or_default();
    let summary = summary.split("; wrote").next().unwrap().to_owned();
    (std::fs::read_to_string(svg).unwrap(), summary)
}

fn edge_layers(svg: &str) -> Vec<&str> {
    svg.split("<g id=\"")
        .skip(1)
        .filter_map(|g| g.split('"').next())
        .filter(|id| id.starts_with("road-") || id.starts_with("rail-"))
        .collect()
}

#[test]
fn the_filters_leave_out_the_dead_the_loose_and_the_sidings() {
    let dir = scratch("default");
    let (svg, summary) = draw(&dir, &[]);
    // The deleted road, the unlaned street and the three loose pieces, then
    // the siding.
    assert_eq!(
        summary,
        "2 road edges, 0 track edges (5 deleted ones and 1 industry sidings left out), \
         0 industries, 0 depots, 0 station pieces, 1 warehouses (2 modules)"
    );
    assert_eq!(edge_layers(&svg), ["road-town_old_small"]);
    assert_eq!(svg.matches("<path data-a").count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_warehouses_can_be_left_out() {
    let dir = scratch("no-warehouses");
    let (svg, summary) = draw(&dir, &[]);
    assert!(svg.contains("<g id=\"warehouses\""));
    let (svg, without) = draw(&dir, &["--no-warehouses"]);
    assert_eq!(
        without,
        summary.trim_end_matches(", 1 warehouses (2 modules)")
    );
    assert!(!svg.contains("class=\"warehouse"));
    assert!(
        svg.contains(" warehouses=\"off\""),
        "the view record says so"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keep_dead_brings_back_all_but_the_sidings() {
    let dir = scratch("keep-dead");
    let (svg, summary) = draw(&dir, &["--keep-dead"]);
    assert_eq!(
        summary,
        "7 road edges, 0 track edges (0 deleted ones and 1 industry sidings left out), \
         0 industries, 0 depots, 0 station pieces, 1 warehouses (2 modules)"
    );
    assert_eq!(
        edge_layers(&svg),
        [
            "road-country_new_large",
            "road-ind_country_old_small",
            "road-ind_entrance_old_country",
            "road-town_new_small",
            "road-town_old_small",
        ]
    );
    assert!(svg.contains(" keep-dead=\"on\""), "the view record says so");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_png_leaves_out_the_same_edges() {
    let dir = scratch("png");
    let save = dir.join("synthetic.sav");
    std::fs::write(&save, common::sav(&common::stream(&edges()))).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tf3map"))
        .arg(&save)
        .args(["--png", "-o"])
        .arg(dir.join("synthetic.png"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(
        stderr.contains("2 road edges, 0 track edges (5 deleted ones and 1 industry sidings"),
        "{stderr}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
