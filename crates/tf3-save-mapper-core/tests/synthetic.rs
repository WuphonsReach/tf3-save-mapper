//! End to end on a hand-built stream; no real save is involved.

mod common;

use common::{EdgeSpec, FREE_RUN_LEN, FREE_RUN_START};
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
            template: "street/town/town_old_small",
        },
        EdgeSpec {
            a: 101,
            b: 102,
            p0: [-900.0, 500.0, 52.25],
            p1: [-900.0, 400.0, 81.75],
            template: "street/town/town_old_small",
        },
        EdgeSpec {
            a: 102,
            b: 103,
            p0: [-900.0, 400.0, 81.75],
            p1: [-800.0, 400.0, 120.5],
            template: "track/standard",
        },
        // The last end node is in the free-id list: a deleted road the table kept.
        EdgeSpec {
            a: 103,
            b: FREE_RUN_START + 500,
            p0: [-800.0, 400.0, 120.5],
            p1: [-700.0, 400.0, 95.0],
            template: "street/country/country_new_large",
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
            (103, FREE_RUN_START + 500)
        ]
    );
    assert_eq!(found[0].p0, [-1000.0, 500.0, 37.5]);
    assert_eq!(found[0].type_name(), "town_old_small");
    assert_eq!(found[2].family(), Family::Track);
    assert_eq!(
        found[3].template.as_deref(),
        Some("street/country/country_new_large")
    );
}

#[test]
fn free_id_list_drops_the_deleted_road() {
    let stream = common::stream(&edges());
    let free = read_free_ids(&stream).expect("free-id list");
    assert_eq!(free.len(), (3 + FREE_RUN_LEN) as usize);
    assert!(free.contains(&FREE_RUN_START) && free.contains(&500_000));
    let mut found = read_edges(&stream).unwrap();
    assert_eq!(retain_live(&mut found, &free), 1);
    assert_eq!(found.len(), 3);
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
    assert!(
        !svg.contains("country_new_large"),
        "the deleted road is not drawn"
    );
    assert!(svg.contains("seed synthetic seed"));
    assert_eq!(svg.matches("<path ").count(), 3);
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
