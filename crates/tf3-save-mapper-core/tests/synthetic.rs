//! End to end on a hand-built stream; no real save is involved.

mod common;

use std::collections::HashMap;

use common::{
    BOATHOUSE_MODEL, BUS_AT, EdgeSpec, FARM_AT, FREE_DECOY, FREE_HEAD, FREE_RUN_LEN,
    FREE_RUN_START, FREE_TAIL, LONE_STRIP_ID, MAINT_AT, MAINT_MODEL, PIT, STRIP_ID, STRIP_ROWS,
    TRACK_LANES,
};
use tf3_save_mapper_core::install::DepotSpec;
use tf3_save_mapper_core::svg::{
    Crop, EdgeLabel, GridLines, GridStep, North, Rose, shown_edges, unmatched_labels,
};
use tf3_save_mapper_core::{
    Family, Layers, SvgOptions, container, edge_ids, industry_models, read_edges, read_free_ids,
    read_header, read_instances, read_model_table, read_strips, read_terrain, retain_live,
    write_png, write_svg,
};

fn edges() -> Vec<EdgeSpec> {
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
            objects: 2,
            decorations: 3,
            ..EdgeSpec::default()
        },
        EdgeSpec {
            a: 102,
            b: 103,
            p0: [-900.0, 400.0, 81.75],
            p1: [-800.0, 400.0, 120.5],
            template: "track/standard",
            lanes: TRACK_LANES,
            ..EdgeSpec::default()
        },
        // A mod's type with characters a narrow match refuses, before a track
        // record: the reader must not take the next record's type for it.
        EdgeSpec {
            a: 103,
            b: 104,
            p0: [-800.0, 400.0, 120.5],
            p1: [-760.0, 430.0, 118.0],
            template: "street/some-mod/wide+road_4",
            mod_name: "some_mod",
            ..EdgeSpec::default()
        },
        // An industry siding: a track by its lanes, outside `infrastructure/track/`.
        EdgeSpec {
            a: 104,
            b: 105,
            p0: [-760.0, 430.0, 118.0],
            p1: [-700.0, 470.0, 109.5],
            template: "industries/shared/industry_track/industry",
            lanes: &[0x4080],
            ..EdgeSpec::default()
        },
        // The last end node is in the free-id list: a deleted road the table kept.
        EdgeSpec {
            a: 105,
            b: FREE_RUN_START + 500,
            p0: [-700.0, 470.0, 109.5],
            p1: [-650.0, 400.0, 95.0],
            template: "street/country/country_new_large",
            ..EdgeSpec::default()
        },
    ]
}

#[test]
fn header_gives_size_climate_and_seed() {
    let stream = common::stream(&edges());
    let h = read_header(&stream).unwrap();
    assert_eq!(h.version, 604);
    assert_eq!(
        (h.map_width, h.map_height),
        (common::MAP_WIDTH, common::MAP_HEIGHT)
    );
    assert_eq!(h.start_year, 1950);
    assert_eq!(h.climate.as_deref(), Some(common::CLIMATE));
    assert_eq!(h.map_seed.as_deref(), Some(common::SEED));
}

#[test]
fn finds_every_record_including_the_first_and_skips_the_decoy() {
    let stream = common::stream(&edges());
    let found = read_edges(&stream).unwrap();
    let ids: Vec<(u32, u32)> = found.iter().map(|e| (e.a, e.b)).collect();
    assert_eq!(
        ids,
        [
            (100, 101),
            (101, 102),
            (102, 103),
            (103, 104),
            (104, 105),
            (105, FREE_RUN_START + 500)
        ]
    );
    assert_eq!(found[0].p0, [-1000.0, 500.0, 37.5]);
    let types: Vec<(&str, Family)> = found.iter().map(|e| (e.type_name(), e.family)).collect();
    assert_eq!(
        types,
        [
            ("town_old_small", Family::Street),
            ("town_old_small", Family::Street),
            ("standard", Family::Track),
            ("wide+road_4", Family::Street),
            ("industry", Family::Track),
            ("country_new_large", Family::Street),
        ]
    );
    assert_eq!(
        found[4].template.as_deref(),
        Some("industries/shared/industry_track/industry")
    );
    // The lanes' widths added up: 3.5, 3.75, 4 and 4.25 m for a road's four.
    let widths: Vec<Option<f32>> = found.iter().map(|e| e.width).collect();
    assert_eq!(
        widths,
        [
            Some(15.5),
            Some(15.5),
            Some(7.25),
            Some(15.5),
            Some(3.5),
            Some(15.5)
        ]
    );
}

#[test]
fn terrain_reads_tile_by_tile() {
    let stream = common::stream(&edges());
    let t = read_terrain(&stream).unwrap();
    assert_eq!((t.tiles_x, t.tiles_y), common::TILES);
    assert_eq!(t.size(), (common::MAP_WIDTH, common::MAP_HEIGHT));
    assert_eq!(
        (t.offset_z, t.water_level),
        (common::OFFSET_Z, common::WATER_LEVEL)
    );
    assert!((t.water_units() - 1000.0).abs() < 1e-3);
    // Tiles share their edge points; every point reads back where it was written.
    for (c, r) in [
        (0, 0),
        (64, 0),
        (65, 3),
        (300, 200),
        (1024, 512),
        (777, 333),
    ] {
        assert_eq!(t.at(c, r), common::height(c, r), "({c}, {r})");
    }
}

#[test]
fn the_farm_is_found_by_its_main_model() {
    let stream = common::stream(&edges());
    let table = read_model_table(&stream).unwrap();
    assert_eq!(table.paths[&900], "assets/some_mod/barn.mdl");
    let found = read_instances(&stream, &table, &["industries/"]);
    assert_eq!(found.len(), 1, "{found:?}");
    let farm = &found[0];
    assert_eq!(
        (farm.x, farm.y),
        (f64::from(FARM_AT.0), f64::from(FARM_AT.1))
    );
    assert!((farm.fx - 0.6).abs() < 1e-6 && (farm.fy - 0.8).abs() < 1e-6);
    assert_eq!(industry_models(&table, &found)[&farm.model], "farm");
}

#[test]
fn free_id_list_drops_the_deleted_road() {
    let stream = common::stream(&edges());
    let free = read_free_ids(&stream).expect("free-id list");
    let ids = FREE_HEAD.len() + FREE_RUN_LEN as usize + FREE_TAIL.len();
    assert_eq!(free.len(), ids, "the list ends at its trailer");
    assert!(
        FREE_HEAD
            .iter()
            .chain(&FREE_TAIL)
            .all(|id| free.contains(id))
    );
    assert!(free.contains(&FREE_RUN_START));
    assert!(
        !FREE_DECOY.iter().any(|w| free.contains(w)),
        "the nearest count word is the list's"
    );
    let mut found = read_edges(&stream).unwrap();
    assert_eq!(retain_live(&mut found, &free), 1);
    assert_eq!(found.len(), 5);
}

#[test]
fn a_save_file_goes_all_the_way_to_an_svg() {
    let file = common::sav(&common::stream(&edges()));
    let stream = container::decompress(&file[..]).unwrap();
    let header = read_header(&stream).unwrap();
    let mut found = read_edges(&stream).unwrap();
    retain_live(&mut found, &read_free_ids(&stream).unwrap());

    let table = read_model_table(&stream).unwrap();
    let instances = read_instances(&stream, &table, &["industries/"]);
    let mains = industry_models(&table, &instances);
    let layers = Layers {
        terrain: Some(read_terrain(&stream).unwrap()),
        industries: instances
            .iter()
            .map(|i| (i.clone(), mains[&i.model].clone()))
            .collect(),
        ..Layers::default()
    };

    let mut out = Vec::new();
    write_svg(&mut out, &header, &found, &layers, &SvgOptions::default()).unwrap();
    let svg = String::from_utf8(out).unwrap();

    // x + 2048, 1024 - y: the first edge runs from (-1000, 500) to (-900, 500).
    assert!(svg.contains("d=\"M1048.0 524.0L1148.0 524.0\""), "{svg}");
    assert!(svg.contains("id=\"road-town_old_small\""));
    assert!(svg.contains("id=\"rail-standard\""));
    assert!(svg.contains("id=\"rail-industry\""));
    assert!(
        !svg.contains("country_new_large"),
        "the deleted road is not drawn"
    );
    assert!(svg.contains("seed synthetic seed"));
    assert_eq!(svg.matches("<path data-a").count(), 5);
    // The edges are as wide as their lanes.
    assert!(
        svg.contains("stroke=\"#323232\" stroke-width=\"15.5\""),
        "{svg}"
    );

    // The pit is the one stretch of water: a ring round its centre, which is
    // at picture (4 * 300, 2048 - 4 * 200) in metres.
    let water = svg
        .split("class=\"water\"")
        .nth(1)
        .and_then(|w| w.split(" d=\"").nth(1))
        .and_then(|d| d.split('"').next())
        .unwrap();
    assert_eq!(water.matches('M').count(), 1, "{water}");
    let (cu, cv) = (4.0 * PIT.0 as f64, 2048.0 - 4.0 * PIT.1 as f64);
    for point in water.trim_end_matches('Z')[1..].split('L') {
        let mut uv = point.split(' ').map(|n| n.parse::<f64>().unwrap());
        let (u, v) = (uv.next().unwrap(), uv.next().unwrap());
        let r = (u - cu).hypot(v - cv);
        assert!(
            (70.0..=95.0).contains(&r),
            "({u}, {v}) is {r} m from the pit's centre"
        );
    }
    assert!(svg.contains("class=\"band\""));
    assert!(svg.contains("id=\"contours-index\""));
    assert!(svg.find("id=\"terrain\"") < svg.find("id=\"roads\""));

    assert!(svg.contains("id=\"industry-farm\""));
    assert!(svg.contains("<symbol id=\"industry-icon-farm\""));
    assert!(svg.contains("id=\"compass\""));
    assert!(svg.find("id=\"rails\"") < svg.find("id=\"industries\""));
}

/// The synthetic save drawn with `options`: its live edges, the terrain and the farm.
fn render(options: &SvgOptions) -> String {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let mut found = read_edges(&stream).unwrap();
    retain_live(&mut found, &read_free_ids(&stream).unwrap());
    let table = read_model_table(&stream).unwrap();
    let instances = read_instances(&stream, &table, &["industries/"]);
    let mains = industry_models(&table, &instances);
    let layers = Layers {
        terrain: Some(read_terrain(&stream).unwrap()),
        industries: instances
            .iter()
            .map(|i| (i.clone(), mains[&i.model].clone()))
            .collect(),
        ..Layers::default()
    };
    let mut out = Vec::new();
    write_svg(&mut out, &header, &found, &layers, options).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn the_default_grid_marks_every_kilometre() {
    let svg = render(&SvgOptions::default());
    // 4096 x 2048 m: a third of 2048 is 682 m, but a whole map gets 1000 m at least.
    assert!(svg.contains("inkscape:label=\"Grid 1000 m\""), "{svg}");
    let at = |id: &str| svg.find(&format!("id=\"{id}\"")).unwrap();
    assert!(at("terrain") < at("grid") && at("grid") < at("roads"));
    assert!(at("industries") < at("grid-labels") && at("grid-labels") < at("compass"));
    // x = 0 runs down the middle of the picture, dotted.
    assert!(svg.contains("<path d=\"M2048 0V2048\" fill=\"none\" stroke=\"#6b3fa0\""));
    assert!(svg.contains(">x -1000</text>") && svg.contains(">y -1000</text>"));

    let off = render(&SvgOptions {
        grid: GridStep::Off,
        ..SvgOptions::default()
    });
    assert!(!off.contains("id=\"grid"));
}

#[test]
fn a_finer_grid_has_minor_marks_under_the_kilometre_ones() {
    let svg = render(&SvgOptions {
        grid: GridStep::Every(250),
        grid_lines: GridLines::Dots,
        ..SvgOptions::default()
    });
    assert!(svg.contains("Grid 250 m"));
    let minor = svg
        .split("id=\"grid-minor\"")
        .nth(1)
        .and_then(|s| s.split("id=\"grid-major\"").next())
        .unwrap();
    // Minor lines are the multiples of 250 m that are not of 1000 m: 12 up, 6 across.
    assert_eq!(minor.matches("stroke-dashoffset").count(), 18, "{minor}");
    assert!(svg.contains(">x 250</text>"));
}

#[test]
fn a_crop_keeps_the_map_frame_and_draws_only_what_shows() {
    let svg = render(&SvgOptions {
        crop: Some(Crop {
            x: -900.0,
            y: 450.0,
            radius: 120.0,
        }),
        metres_per_pixel: 4.0,
        zoom: 2,
        ..SvgOptions::default()
    });
    // Widened to the 4 m grid: x -1020 to -780, y 328 to 572, so the
    // picture's corner is at (-1020 + 2048, 1024 - 572) in map frame metres.
    assert!(
        svg.contains("width=\"120\" height=\"122\" viewBox=\"1028 452 240 244\""),
        "{svg}"
    );
    assert!(svg.contains("<rect x=\"1028\" y=\"452\" width=\"240\" height=\"244\""));
    // The first edge keeps its whole-map coordinates.
    assert!(svg.contains("d=\"M1048.0 524.0L1148.0 524.0\""));
    // Four edges reach the crop; the one from (-760, 430) on and the farm do not.
    assert_eq!(svg.matches("<path data-a").count(), 4, "{svg}");
    assert!(!svg.contains("data-a=\"68\""));
    assert!(!svg.contains("id=\"industries\""));
    // The terrain is cut from the crop's part of the grid only.
    assert!(svg.contains("id=\"terrain\" class=\"layer\" data-layer=\"terrain\" inkscape:groupmode=\"layer\" inkscape:label=\"Terrain\" transform=\"translate(1028 452)\""));
    assert!(svg.contains("<rect width=\"240\" height=\"244\""));
    // The default grid on a crop: a third of 240 m, rounded down to 50 m.
    assert!(svg.contains("Grid 50 m"));
    assert!(svg.contains(">x -900</text>"));
}

#[test]
fn a_crop_round_the_farm_draws_it() {
    let svg = render(&SvgOptions {
        crop: Some(Crop {
            x: f64::from(FARM_AT.0) + 60.0,
            y: f64::from(FARM_AT.1),
            radius: 40.0,
        }),
        metres_per_pixel: 4.0,
        ..SvgOptions::default()
    });
    // The badge is 64 m across on the origin, 60 m west of the crop's centre.
    assert!(svg.contains("id=\"industry-farm\""), "{svg}");
    assert_eq!(svg.matches("<path data-a").count(), 0);
}

#[test]
fn a_crop_off_the_map_is_refused() {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let crop = Crop {
        x: 5000.0,
        y: 0.0,
        radius: 100.0,
    };
    assert_eq!(crop.bounds(&header), None);
    let options = SvgOptions {
        crop: Some(crop),
        ..SvgOptions::default()
    };
    let err = write_svg(&mut Vec::new(), &header, &[], &Layers::default(), &options).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    // A crop that reaches past the edge is cut to the map.
    let edge = Crop {
        x: 2000.0,
        y: -1000.0,
        radius: 100.0,
    };
    assert_eq!(
        edge.bounds(&header),
        Some((1900.0, 2048.0, -1024.0, -900.0))
    );
}

/// A road maintenance building's spec as an install's script would give it.
fn maint_spec() -> DepotSpec {
    DepotSpec {
        face: vec![(-10.0, -12.5), (15.0, -12.5), (15.0, 12.5), (-10.0, 12.5)],
        boxes: vec![],
        radius: Some(600),
        pool: 16,
        modes: "BUS,TRUCK,TRAM,ELECTRIC_TRAM".into(),
    }
}

/// The synthetic save's depots drawn with `options`: as footprints, as
/// coverage circles, or both, with `specs` standing in for the install.
fn render_depots(
    options: &SvgOptions,
    footprints: bool,
    coverage: bool,
    specs: HashMap<&'static str, DepotSpec>,
) -> String {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let table = read_model_table(&stream).unwrap();
    let rows = read_instances(&stream, &table, &["depots/", "assets/ingo_bootsschuppen/"]);
    let layers = Layers {
        depots: if footprints { rows.clone() } else { vec![] },
        coverage: if coverage { rows } else { vec![] },
        depot_specs: specs,
        ..Layers::default()
    };
    let mut out = Vec::new();
    write_svg(&mut out, &header, &[], &layers, options).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn coverage_is_a_dashed_circle_under_the_roads() {
    let specs = || HashMap::from([("maintenance-buildings", maint_spec())]);
    let svg = render_depots(&SvgOptions::default(), true, true, specs());
    // x + 2048, 1024 - y; 600 m round the origin; 1.5, 10 and 6 pixels at 8 m per pixel.
    assert!(
        svg.contains(&format!(
            "<g id=\"coverage-{0}\" class=\"coverage maintenance-buildings\" data-slot=\"{0}\" \
             data-model=\"{MAINT_MODEL}\" data-x=\"1200.5\" data-y=\"-600.2\" data-pool=\"16\" \
             data-radius=\"600\" data-modes=\"BUS,TRUCK,TRAM,ELECTRIC_TRAM\"><circle \
             cx=\"3248.50\" cy=\"1624.25\" r=\"600\" fill=\"#4b4b4b\" fill-opacity=\"0.06\" \
             stroke=\"#4b4b4b\" stroke-width=\"12\" stroke-dasharray=\"80 48\"/></g>",
            maint_slot()
        )),
        "{svg}"
    );
    let at = |id: &str| svg.find(&format!("id=\"{id}\"")).unwrap();
    assert!(at("grid") < at("coverage") && at("coverage") < at("roads"));
    assert!(
        at("rails") < at("maintenance-buildings"),
        "the footprint over the network"
    );

    // Off by default: footprints only.
    let svg = render_depots(&SvgOptions::default(), true, false, specs());
    assert!(!svg.contains("id=\"coverage\""));
    assert!(svg.contains("class=\"depot maintenance-buildings\""));
    // Coverage alone, and without a spec no circle and no layer.
    let svg = render_depots(&SvgOptions::default(), false, true, specs());
    assert!(svg.contains("id=\"coverage-maintenance-buildings\""));
    assert!(!svg.contains("class=\"depot "));
    let svg = render_depots(&SvgOptions::default(), false, true, HashMap::new());
    assert!(!svg.contains("id=\"coverage\""));
}

#[test]
fn a_mods_boathouse_gets_its_catchment_when_its_script_is_found() {
    // What the mods folder's script gives: no face, a radius, a pool, modes.
    let boathouse = DepotSpec {
        face: vec![],
        boxes: vec![],
        radius: Some(600),
        pool: 2,
        modes: "SHIP,SMALL_SHIP".into(),
    };
    let dot = "<circle cx=\"597.50\" cy=\"1344.40\" r=\"4\" fill=\"#7b4a24\" stroke=\"#000\" \
               stroke-width=\"1.5\"/>";
    let svg = render_depots(
        &SvgOptions::default(),
        true,
        true,
        HashMap::from([("boathouses", boathouse)]),
    );
    let group = svg
        .lines()
        .find(|l| l.contains("class=\"depot boathouses\""))
        .unwrap();
    assert!(
        group.ends_with(&format!(
            "data-model=\"{BOATHOUSE_MODEL}\" data-x=\"-1450.5\" data-y=\"-320.4\" \
             data-pool=\"2\" data-radius=\"600\" data-modes=\"SHIP,SMALL_SHIP\">"
        )),
        "{group}"
    );
    assert!(svg.contains(dot), "{svg}");
    assert!(svg.contains("id=\"coverage-boathouses\""));
    assert!(svg.contains(
        "<circle cx=\"597.50\" cy=\"1344.40\" r=\"600\" fill=\"#3d2512\" fill-opacity=\"0.06\""
    ));

    // Without the mod's script: the same dot, no catchment and no circle.
    let svg = render_depots(&SvgOptions::default(), true, true, HashMap::new());
    let group = svg
        .lines()
        .find(|l| l.contains("class=\"depot boathouses\""))
        .unwrap();
    assert!(
        group.ends_with("data-x=\"-1450.5\" data-y=\"-320.4\">"),
        "{group}"
    );
    assert!(svg.contains(dot));
    assert!(!svg.contains("id=\"coverage\""));
}

/// The maintenance building's place among the stream's instance records.
fn maint_slot() -> usize {
    let stream = common::stream(&edges());
    let table = read_model_table(&stream).unwrap();
    let rows = read_instances(&stream, &table, &["depots/"]);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].model, MAINT_MODEL);
    rows[0].slot
}

#[test]
fn a_crop_keeps_a_circle_that_reaches_it() {
    let crop = |dx: f64| SvgOptions {
        crop: Some(Crop {
            x: f64::from(MAINT_AT.0) + dx,
            y: f64::from(MAINT_AT.1),
            radius: 50.0,
        }),
        metres_per_pixel: 4.0,
        ..SvgOptions::default()
    };
    let specs = || HashMap::from([("maintenance-buildings", maint_spec())]);
    // 500 m east: the circle reaches the crop, the footprint does not.
    let svg = render_depots(&crop(500.0), true, true, specs());
    assert!(
        svg.contains("class=\"coverage maintenance-buildings\""),
        "{svg}"
    );
    assert!(!svg.contains("class=\"depot "));
    // The line and dashes keep their pixels at 4 m per pixel.
    assert!(svg.contains("stroke-width=\"6\" stroke-dasharray=\"40 24\""));
    // 700 m east, the crop's near side is 648 m off: nothing.
    let svg = render_depots(&crop(700.0), true, true, specs());
    assert!(!svg.contains("id=\"coverage\""), "{svg}");
}

/// The synthetic save's vehicles drawn with `options`, the ones in `names` named.
fn render_vehicles(options: &SvgOptions, names: &[(usize, &str)]) -> String {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let table = read_model_table(&stream).unwrap();
    let layers = Layers {
        vehicles: read_instances(&stream, &table, &["vehicle/"]),
        vehicle_names: names.iter().map(|&(s, n)| (s, n.to_owned())).collect(),
        ..Layers::default()
    };
    let mut out = Vec::new();
    write_svg(&mut out, &header, &[], &layers, options).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn vehicles_are_found_and_drawn_by_folder() {
    let stream = common::stream(&edges());
    let table = read_model_table(&stream).unwrap();
    let found = read_instances(&stream, &table, &["vehicle/"]);
    let models: Vec<(&str, usize)> = found.iter().map(|i| (i.model.as_str(), i.slot)).collect();
    assert_eq!(
        models,
        [
            ("vehicle/bus/town_bus/town_bus.mdl", 1),
            ("vehicle/waggon/box/box_waggon.mdl", 2)
        ]
    );
    assert_eq!(
        (found[0].x, found[0].y),
        (f64::from(BUS_AT.0), f64::from(BUS_AT.1))
    );

    let svg = render_vehicles(
        &SvgOptions::default(),
        &[(1, "Road Vehicle 31"), (7, "nobody")],
    );
    let at = |id: &str| svg.find(&format!("id=\"{id}\"")).unwrap();
    assert!(at("vehicles-bus") < at("vehicles-train"), "{svg}");
    assert!(at("vehicles") < at("grid-labels"));
    assert!(svg.contains("class=\"vehicle bus\" data-slot=\"1\""));
    assert!(svg.contains("class=\"vehicle waggon\" data-slot=\"2\""));
    // At 8 m per pixel the name is 36 m high; the bus faces south-south-east.
    assert!(svg.contains("font-size=\"36\""));
    assert!(svg.contains(">RV31</text>"), "{svg}");
    assert!(svg.contains("rotate(73.7)"), "{svg}");
    assert_eq!(
        vehicle_layer(&svg).matches("<text").count(),
        1,
        "slot 7 is no vehicle"
    );
}

/// The `vehicles` layer's text.
fn vehicle_layer(svg: &str) -> &str {
    let start = svg.find("id=\"vehicles\"").unwrap();
    let end = svg.find("id=\"grid-labels\"").unwrap();
    &svg[start..end]
}

#[test]
fn a_crop_draws_only_the_vehicles_in_it() {
    let svg = render_vehicles(
        &SvgOptions {
            crop: Some(Crop {
                x: f64::from(BUS_AT.0),
                y: f64::from(BUS_AT.1),
                radius: 30.0,
            }),
            metres_per_pixel: 4.0,
            ..SvgOptions::default()
        },
        &[],
    );
    assert!(svg.contains("class=\"vehicle bus\""), "{svg}");
    assert!(
        !svg.contains("class=\"vehicle waggon\""),
        "55 m east, outside"
    );
    assert!(!vehicle_layer(&svg).contains("<text"), "no names asked for");
}

#[test]
fn the_strip_table_gives_the_first_edge_its_id() {
    let stream = common::stream(&edges());
    let strips = read_strips(&stream);
    let ids: Vec<u32> = strips.iter().map(|s| s.id).collect();
    assert_eq!(
        ids,
        [STRIP_ID, LONE_STRIP_ID],
        "nothing else reads as a row"
    );
    assert_eq!(strips[0].rows, STRIP_ROWS);
    let found = read_edges(&stream).unwrap();
    let known = edge_ids(&found, &strips);
    assert_eq!(known.len(), 1);
    assert_eq!(known[&found[0].offset], STRIP_ID);
}

#[test]
fn labels_name_the_edges_asked_for() {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let found = read_edges(&stream).unwrap();
    let layers = Layers {
        labels: vec![
            EdgeLabel {
                a: 101,
                b: 100,
                text: None,
            },
            EdgeLabel {
                a: 101,
                b: 102,
                text: Some("High Street".into()),
            },
            EdgeLabel {
                a: 7,
                b: 8,
                text: None,
            },
        ],
        edge_ids: edge_ids(&found, &read_strips(&stream)),
        ..Layers::default()
    };
    let mut out = Vec::new();
    write_svg(&mut out, &header, &found, &layers, &SvgOptions::default()).unwrap();
    let svg = String::from_utf8(out).unwrap();
    // The first edge's middle is (-950, 500): picture (1098, 524).
    assert!(
        svg.contains(
            "<g class=\"edge-label\" data-a=\"64\" data-b=\"65\"><text transform=\"translate(1098.0 524.0) rotate(0.0)\""
        ),
        "{svg}"
    );
    assert!(svg.contains(">64&gt;65 e7de1</text>"));
    assert!(svg.contains(">High Street</text>"));
    assert!(svg.contains("<path data-a=\"64\" data-b=\"65\" data-edge=\"7de1\""));
    // Three end nodes; the one they share is half in each colour.
    assert_eq!(svg.matches("class=\"node\"").count(), 3);
    assert!(svg.contains("data-edges=\"64>65 65>66\""));
    let at = |id: &str| svg.find(&format!("id=\"{id}\"")).unwrap();
    assert!(at("rails") < at("labels") && at("labels") < at("grid-labels"));
    let shown = shown_edges(&header, &found, &SvgOptions::default());
    let missing = unmatched_labels(&layers.labels, &shown);
    assert_eq!(missing, [&layers.labels[2]]);
}

#[test]
fn north_to_the_left_turns_every_layer() {
    let svg = render(&SvgOptions {
        north: North::Sun,
        ..SvgOptions::default()
    });
    // Up is +x and right is -y: the 4096 x 2048 m map stands 2048 wide and 4096 high.
    assert!(svg.contains("viewBox=\"0 0 2048 4096\""), "{svg}");
    assert!(svg.contains("<rect width=\"2048\" height=\"4096\""));
    // (x, y) is at (1024 - y, 2048 - x): the first edge runs up the picture.
    assert!(svg.contains("d=\"M524.0 3048.0L524.0 2948.0\""), "{svg}");
    // The pit's water ring lies round (1024 + 224, 2048 + 848).
    let water = svg
        .split("class=\"water\"")
        .nth(1)
        .and_then(|w| w.split(" d=\"").nth(1))
        .and_then(|d| d.split('"').next())
        .unwrap();
    let (x, y) = (-2048.0 + 4.0 * PIT.0 as f64, -1024.0 + 4.0 * PIT.1 as f64);
    let (cu, cv) = (1024.0 - y, 2048.0 - x);
    for point in water.trim_end_matches('Z')[1..].split('L') {
        let mut uv = point.split(' ').map(|n| n.parse::<f64>().unwrap());
        let (u, v) = (uv.next().unwrap(), uv.next().unwrap());
        let r = (u - cu).hypot(v - cv);
        assert!(
            (70.0..=95.0).contains(&r),
            "({u}, {v}) is {r} m from the pit"
        );
    }
    // Lines of constant y run up the picture and are labelled along the top.
    assert!(svg.contains(">y 0</text>") && svg.contains(">x 0</text>"));
    assert!(svg.contains("<path d=\"M1024 0V4096\""));
    // The farm's badge: its origin at (1024 - 300, 2048 + 500).
    assert!(
        svg.contains("<circle cx=\"724.00\" cy=\"2548.00\""),
        "{svg}"
    );
    // The rose's arrow points left.
    let rose = svg.split("id=\"compass\"").nth(1).unwrap();
    assert!(
        rose.contains("<circle cx=\"1904.2\" cy=\"3952.2\" r=\"125.0\""),
        "{rose}"
    );
    assert!(rose.contains("M1904.2 3952.2L1811.4 3952.2"), "{rose}");
}

#[test]
fn the_rose_goes_where_it_is_asked_at_its_size() {
    let at = |rose: Option<Rose>, size: Option<f64>| {
        let svg = render(&SvgOptions {
            rose,
            rose_size: size,
            grid: GridStep::Off,
            ..SvgOptions::default()
        });
        svg.split("id=\"compass\"")
            .nth(1)
            .and_then(|r| r.split("<circle ").nth(1))
            .map(|c| c.split(" fill").next().unwrap().to_owned())
    };
    // 250 m by default; 2048 / 8 is more.
    assert_eq!(
        at(Some(Rose::LowerRight), None).as_deref(),
        Some("cx=\"3952.2\" cy=\"1904.2\" r=\"125.0\"")
    );
    assert_eq!(
        at(Some(Rose::UpperLeft), Some(400.0)).as_deref(),
        Some("cx=\"230.0\" cy=\"230.0\" r=\"200.0\"")
    );
    let farm = Rose::At(f64::from(FARM_AT.0), f64::from(FARM_AT.1));
    assert_eq!(
        at(Some(farm), Some(50.0)).as_deref(),
        Some("cx=\"1548.0\" cy=\"724.0\" r=\"25.0\"")
    );
    assert_eq!(at(None, None), None);
}

#[test]
fn a_stream_with_no_table_has_no_edges() {
    let mut stream = common::header(604);
    stream.extend_from_slice(&[0; 4096]);
    assert!(matches!(
        read_edges(&stream),
        Err(tf3_save_mapper_core::Error::NoEdges)
    ));
}

#[test]
fn metres_per_pixel_size_the_picture_and_thin_the_terrain() {
    // 16 m per pixel: 256 x 128 pixels, the terrain cut from every fourth
    // grid point (257 x 129 of them, 16 m apart).
    let svg = render(&SvgOptions {
        metres_per_pixel: 16.0,
        ..SvgOptions::default()
    });
    assert!(
        svg.contains("width=\"256\" height=\"128\" viewBox=\"0 0 4096 2048\""),
        "{svg}"
    );
    assert!(svg.contains("<rect width=\"4096\" height=\"2048\" fill=\"#"));
    // Height lines are half a pixel wide, in metres.
    assert!(svg.contains("id=\"contours-minor\" class=\"layer\" data-layer=\"contours-minor\" inkscape:groupmode=\"layer\" inkscape:label=\"Height lines: minor\" fill=\"none\" stroke=\"#b8996a\" stroke-width=\"8\""));

    // 12 m per pixel keeps every third point: 342 of the 1025, the last at
    // 4092 m, so the terrain stops 4 m short of the map's right edge.
    let svg = render(&SvgOptions {
        metres_per_pixel: 12.0,
        ..SvgOptions::default()
    });
    assert!(svg.contains("width=\"342\" height=\"171\""), "{svg}");
    assert!(svg.contains("<rect width=\"4092\" height=\"2040\" fill=\"#"));

    // Finer than the grid on a crop: the picture has more pixels, the
    // terrain the same 4 m points.
    let crop = Crop {
        x: -900.0,
        y: 450.0,
        radius: 120.0,
    };
    let svg = render(&SvgOptions {
        crop: Some(crop),
        metres_per_pixel: 2.0,
        ..SvgOptions::default()
    });
    assert!(
        svg.contains("width=\"120\" height=\"122\" viewBox=\"1028 452 240 244\""),
        "{svg}"
    );
    assert!(svg.contains("<rect width=\"240\" height=\"244\" fill=\"#"));
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("not base64: {c}"),
    };
    let mut out = Vec::new();
    for quad in text.as_bytes().chunks(4) {
        let digits: Vec<u8> = quad
            .iter()
            .filter(|&&c| c != b'=')
            .map(|&c| value(c))
            .collect();
        let n = digits
            .iter()
            .chain(std::iter::repeat(&0))
            .take(4)
            .fold(0u32, |n, &d| n << 6 | u32::from(d));
        out.extend(&n.to_be_bytes()[1..digits.len()]);
    }
    out
}

/// The width, height and RGB pixels of a PNG.
fn decode_png(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut buf = vec![0; dec.output_buffer_size().unwrap()];
    let info = dec.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (info.width, info.height, buf)
}

/// The terrain picture embedded in a raster SVG.
fn raster_pixels(svg: &str) -> (u32, u32, Vec<u8>) {
    let data = svg
        .split("xlink:href=\"data:image/png;base64,")
        .nth(1)
        .and_then(|d| d.split('"').next())
        .unwrap();
    decode_png(&base64_decode(data))
}

fn pixel((w, _, rgb): &(u32, u32, Vec<u8>), i: u32, j: u32) -> [u8; 3] {
    let o = 3 * (j * w + i) as usize;
    [rgb[o], rgb[o + 1], rgb[o + 2]]
}

// The expected pixels were drawn by the prototype's own `render` from the
// same heights (tests/common/mod.rs `height`).

#[test]
fn a_raster_terrain_is_one_shaded_picture_under_the_network() {
    let svg = render(&SvgOptions {
        raster: true,
        metres_per_pixel: 4.0,
        ..SvgOptions::default()
    });
    assert!(svg.contains(
        "<g id=\"terrain\" class=\"layer\" data-layer=\"terrain\" inkscape:groupmode=\"layer\" \
         inkscape:label=\"Terrain\">\
         <image width=\"4096\" height=\"2048\" xlink:href=\"data:image/png;base64,"
    ));
    assert!(!svg.contains("class=\"band\"") && !svg.contains("id=\"contours"));
    assert!(svg.find("id=\"terrain\"") < svg.find("id=\"grid\""));
    assert!(svg.find("id=\"grid\"") < svg.find("id=\"roads\""));
    assert!(svg.contains("id=\"industry-farm\"") && svg.contains("id=\"compass\""));
    let img = raster_pixels(&svg);
    assert_eq!((img.0, img.1), (1024, 512));
    for (i, j, rgb) in [
        (0, 0, [146, 166, 105]),
        (96, 0, [86, 78, 56]),
        (97, 0, [80, 91, 57]),
        (500, 100, [154, 164, 106]),
        (1023, 511, [162, 159, 106]),
        // The pit, 40 m under water.
        (300, 312, [84, 135, 182]),
    ] {
        assert_eq!(pixel(&img, i, j), rgb, "({i}, {j})");
    }

    // 8 m per pixel keeps every second point.
    let svg = render(&SvgOptions {
        raster: true,
        ..SvgOptions::default()
    });
    let img = raster_pixels(&svg);
    assert_eq!((img.0, img.1), (512, 256));
    assert_eq!(pixel(&img, 150, 156), [84, 135, 182]);
    assert_eq!(pixel(&img, 511, 255), [161, 159, 106]);
}

#[test]
fn a_fine_raster_crop_fills_in_the_grid_and_turns_with_north() {
    let svg = render(&SvgOptions {
        raster: true,
        crop: Some(Crop {
            x: -900.0,
            y: 450.0,
            radius: 120.0,
        }),
        metres_per_pixel: 2.0,
        north: North::Sun,
        ..SvgOptions::default()
    });
    // x -1020 to -780 and y 328 to 572: with north to the left, 244 m across
    // and 240 m down, from the corner at x = -780, y = 572.
    assert!(
        svg.contains(
            "inkscape:label=\"Terrain\" transform=\"translate(452 2828)\">\
             <image width=\"244\" height=\"240\""
        ),
        "{}",
        &svg[..2000]
    );
    let img = raster_pixels(&svg);
    assert_eq!((img.0, img.1), (122, 120));
    for (i, j, rgb) in [
        (0, 0, [129, 135, 88]),
        (1, 0, [124, 129, 84]),
        (0, 1, [117, 122, 79]),
        (121, 119, [119, 111, 76]),
    ] {
        assert_eq!(pixel(&img, i, j), rgb, "({i}, {j})");
    }
}

/// The synthetic save's live edges and terrain as a flat PNG, drawn with `options`.
fn render_png(options: &SvgOptions) -> Vec<u8> {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let mut found = read_edges(&stream).unwrap();
    retain_live(&mut found, &read_free_ids(&stream).unwrap());
    let terrain = read_terrain(&stream).unwrap();
    let mut out = Vec::new();
    write_png(&mut out, &header, &found, Some(&terrain), options).unwrap();
    out
}

/// FNV-1a over the pixels, and the number of road and rail pixels.
fn fingerprint((_, _, rgb): &(u32, u32, Vec<u8>)) -> (u64, usize, usize) {
    let hash = rgb.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    let count = |c: [u8; 3]| rgb.chunks(3).filter(|p| *p == c).count();
    (hash, count([50, 50, 50]), count([205, 30, 30]))
}

/// The JSON text of the PNG's `tf3map:view` chunk.
fn view_record(png: &[u8]) -> String {
    let key = b"tf3map:view\0\0\0\0\0";
    let at = png.windows(key.len()).position(|w| w == key).unwrap() + key.len();
    let len = u32::from_be_bytes(
        png[at - key.len() - 8..at - key.len() - 4]
            .try_into()
            .unwrap(),
    );
    String::from_utf8(png[at..at - key.len() + len as usize].to_vec()).unwrap()
}

// The fingerprints were taken from the prototype's own `render` and
// `draw_network` on the same heights and edges, given the lanes' widths.

#[test]
fn a_png_draws_the_network_into_the_raster_terrain() {
    let png = render_png(&SvgOptions {
        metres_per_pixel: 4.0,
        ..SvgOptions::default()
    });
    let img = decode_png(&png);
    assert_eq!((img.0, img.1), (1024, 512));
    assert_eq!(fingerprint(&img), (0xe431_126d_4ad9_cd67, 281, 70));
    assert_eq!(
        view_record(&png),
        format!(
            "{{\"map-width\": 4096, \"map-height\": 2048, \"metres-per-pixel\": 4, \
             \"orientation\": \"north-up (game): image up is +y, image right is +x\", \
             \"north\": \"game\", \"x-min\": -2048, \"x-max\": 2048, \"y-min\": -1024, \
             \"y-max\": 1024, \"generator\": \"tf3map\", \"generator-version\": \"{}\"}}",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn a_fine_png_crop_turns_with_north_and_widens_the_lines() {
    let png = render_png(&SvgOptions {
        crop: Some(Crop {
            x: -900.0,
            y: 450.0,
            radius: 120.0,
        }),
        metres_per_pixel: 1.0,
        north: North::Sun,
        ..SvgOptions::default()
    });
    let img = decode_png(&png);
    assert_eq!((img.0, img.1), (244, 240));
    assert_eq!(fingerprint(&img), (0x2bcd_7e3a_c6c3_1f8a, 3997, 749));
    let view = view_record(&png);
    assert!(
        view.contains(
            "\"north\": \"sun\", \"x-min\": -1020, \"x-max\": -780, \"y-min\": 328, \
             \"y-max\": 572, \"x-top\": -780, \"y-left\": 572, \"crop-x\": -900, \
             \"crop-y\": 450, \"crop-radius\": 120, \"generator\""
        ),
        "{view}"
    );
}

#[test]
fn a_png_without_terrain_is_paper_and_network() {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let found = read_edges(&stream).unwrap();
    let mut out = Vec::new();
    write_png(&mut out, &header, &found, None, &SvgOptions::default()).unwrap();
    let img = decode_png(&out);
    // 8 m per pixel: the whole map's 513 x 257 points every second one.
    assert_eq!((img.0, img.1), (512, 256));
    let (_, road, rail) = fingerprint(&img);
    assert!(road > 0 && rail > 0);
    let paper = img.2.chunks(3).filter(|p| *p == [255, 255, 255]).count();
    assert_eq!(paper + road + rail, 512 * 256);
}

#[test]
fn the_view_record_lists_what_was_set_away_from_the_default() {
    let svg = render(&SvgOptions {
        contour_step: 10.0,
        bands: false,
        grid: GridStep::Every(250),
        zoom: 2,
        rose: None,
        settings: vec![("industries", "off".to_owned())],
        ..SvgOptions::default()
    });
    assert!(
        svg.contains(
            "y-max=\"1024\" contour-step=\"10\" zoom=\"2\" bands=\"false\" grid-step=\"250\" \
             rose=\"off\" industries=\"off\" generator=\"tf3map\""
        ),
        "{}",
        svg.lines().find(|l| l.contains("<metadata>")).unwrap_or("")
    );
    let plain = render(&SvgOptions::default());
    assert!(plain.contains("y-max=\"1024\" generator=\"tf3map\""));
}

#[test]
fn the_view_record_names_the_build_when_given() {
    let svg = render(&SvgOptions {
        generator_commit: Some("0123456789-dirty"),
        ..SvgOptions::default()
    });
    assert!(svg.contains(&format!(
        "generator-version=\"{}\" generator-commit=\"0123456789-dirty\"/></metadata>",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(!render(&SvgOptions::default()).contains("generator-commit"));
}

#[test]
fn the_svg_carries_the_view_record_of_the_png() {
    let svg = render(&SvgOptions::default());
    assert!(
        svg.contains(&format!(
            "<metadata><tf3map:view xmlns:tf3map=\"urn:tf3-save-format:terrain-map:1\" \
             map-width=\"4096\" map-height=\"2048\" metres-per-pixel=\"8\" \
             orientation=\"north-up (game): image up is +y, image right is +x\" north=\"game\" \
             x-min=\"-2048\" x-max=\"2048\" y-min=\"-1024\" y-max=\"1024\" generator=\"tf3map\" \
             generator-version=\"{}\"/></metadata>",
            env!("CARGO_PKG_VERSION")
        )),
        "{}",
        svg.lines().find(|l| l.contains("<metadata>")).unwrap_or("")
    );
    let cropped = render(&SvgOptions {
        crop: Some(Crop {
            x: -900.0,
            y: 450.0,
            radius: 120.0,
        }),
        north: North::Sun,
        ..SvgOptions::default()
    });
    assert!(cropped.contains(
        "x-min=\"-1020\" x-max=\"-780\" y-min=\"328\" y-max=\"572\" x-top=\"-780\" \
         y-left=\"572\" crop-x=\"-900\" crop-y=\"450\" crop-radius=\"120\""
    ));
}

#[test]
fn every_layer_is_marked_as_one_and_no_item_group_is() {
    let stream = common::stream(&edges());
    let header = read_header(&stream).unwrap();
    let mut found = read_edges(&stream).unwrap();
    retain_live(&mut found, &read_free_ids(&stream).unwrap());
    let table = read_model_table(&stream).unwrap();
    let industries = read_instances(&stream, &table, &["industries/"]);
    let mains = industry_models(&table, &industries);
    let depots = read_instances(&stream, &table, &["depots/", "assets/ingo_bootsschuppen/"]);
    let layers = Layers {
        terrain: Some(read_terrain(&stream).unwrap()),
        industries: industries
            .iter()
            .map(|i| (i.clone(), mains[&i.model].clone()))
            .collect(),
        depots: depots.clone(),
        coverage: depots,
        depot_specs: HashMap::from([("maintenance-buildings", maint_spec())]),
        vehicles: read_instances(&stream, &table, &["vehicle/"]),
        labels: vec![EdgeLabel {
            a: 100,
            b: 101,
            text: None,
        }],
        ..Layers::default()
    };
    let mut out = Vec::new();
    write_svg(&mut out, &header, &found, &layers, &SvgOptions::default()).unwrap();
    let svg = String::from_utf8(out).unwrap();
    assert!(svg.starts_with(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" "
    ));

    let (mut layers_seen, mut items_seen) = (Vec::new(), 0);
    for tag in svg.split("<g ").skip(1) {
        let tag = &tag[..tag.find('>').unwrap()];
        if tag.contains("inkscape:groupmode=\"layer\"") {
            // id, class, data-layer and Inkscape's two, in the prototype's order.
            let id = tag.strip_prefix("id=\"").unwrap();
            let id = &id[..id.find('"').unwrap()];
            assert!(
                tag.starts_with(&format!(
                    "id=\"{id}\" class=\"layer\" data-layer=\"{id}\" \
                     inkscape:groupmode=\"layer\" inkscape:label=\""
                )),
                "{tag}"
            );
            layers_seen.push(id.to_owned());
        } else {
            assert!(
                !tag.contains("class=\"layer\"") && !tag.contains("data-layer"),
                "{tag}"
            );
            items_seen += 1;
        }
    }
    for id in [
        "background",
        "terrain",
        "contours-minor",
        "grid",
        "coverage",
        "coverage-maintenance-buildings",
        "roads",
        "road-town_old_small",
        "rails",
        "maintenance-buildings",
        "industries",
        "industry-farm",
        "vehicles",
        "vehicles-bus",
        "labels",
        "edge-labels",
        "nodes",
        "grid-labels",
        "compass",
    ] {
        assert!(layers_seen.iter().any(|l| l == id), "{id}: {layers_seen:?}");
    }
    // The vehicles, depots, coverage circles, badge, labels and nodes.
    assert!(items_seen >= 8, "{items_seen}");
    assert!(svg.contains("<g id=\"vehicle-1\" class=\"vehicle bus\""));
    assert!(svg.contains("class=\"depot maintenance-buildings\""));
}
