//! End to end on a hand-built stream; no real save is involved.

mod common;

use common::{
    EdgeSpec, FREE_DECOY, FREE_HEAD, FREE_RUN_LEN, FREE_RUN_START, FREE_TAIL, TRACK_LANES,
};
use tf3_save_mapper_core::{
    Family, SvgOptions, container, read_edges, read_free_ids, read_header, retain_live, write_svg,
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

    let mut out = Vec::new();
    write_svg(&mut out, &header, &found, &SvgOptions::default()).unwrap();
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
    assert_eq!(svg.matches("<path ").count(), 5);
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
