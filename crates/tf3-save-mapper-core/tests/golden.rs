//! Checks against a real save. Committed tests cannot hold a `.sav`, so this
//! reads the path from `TF3_MAPPER_TEST_SAVE` and does nothing when it is unset.
//! `TF3_MAPPER_TEST_EDGES`, if set, is the edge count the reference reader
//! (`street_edges.py` in tf3-save-format) finds, free-id list not applied.

use std::collections::{HashMap, HashSet};
use std::fs::File;

use tf3_save_mapper_core::{
    container, industry_models, read_edges, read_free_ids, read_header, read_instances,
    read_model_table, read_terrain, retain_live,
};

#[test]
fn real_save_reads_and_agrees_with_itself() {
    let Some(path) = std::env::var_os("TF3_MAPPER_TEST_SAVE") else {
        eprintln!("TF3_MAPPER_TEST_SAVE is not set: skipping the golden test");
        return;
    };
    let stream = container::decompress(File::open(path).unwrap()).unwrap();

    let header = read_header(&stream).unwrap();
    assert!(header.map_width > 0 && header.map_height > 0);
    let half = (
        f64::from(header.map_width) / 2.0,
        f64::from(header.map_height) / 2.0,
    );

    let mut edges = read_edges(&stream).unwrap();
    if let Ok(want) = std::env::var("TF3_MAPPER_TEST_EDGES") {
        assert_eq!(edges.len(), want.parse::<usize>().unwrap());
    }
    // The table keeps some deleted roads, and their node ids can be reused: leave those out.
    let free = read_free_ids(&stream);
    if let Some(free) = &free {
        retain_live(&mut edges, free);
    }

    // Edge ends lie on the map and a node has one position wherever it appears. Neither holds
    // for every record: airport runways reach a little past the map edge, and a few nodes are
    // named by leftover records whose ids were reused (the reference reader finds the same
    // ones). So allow 1 % of the ends outside and 3 % of the nodes split.
    let mut node_at: HashMap<u32, [f32; 3]> = HashMap::new();
    let mut split = HashSet::new();
    let mut outside = 0;
    for e in &edges {
        for (id, p) in [(e.a, e.p0), (e.b, e.p1)] {
            if f64::from(p[0]).abs() > half.0 || f64::from(p[1]).abs() > half.1 {
                outside += 1;
            }
            let first = node_at.entry(id).or_insert(p);
            if (first[0] - p[0]).hypot(first[1] - p[1]) > 1.0 {
                split.insert(id);
            }
        }
    }
    assert!(
        outside * 100 <= edges.len() * 2,
        "{outside} edge ends are off the map"
    );
    assert!(
        split.len() * 100 <= node_at.len() * 3,
        "{} of {} nodes have two positions",
        split.len(),
        node_at.len()
    );

    if let Some(free) = free {
        assert!(free.len() > 1000);
    }

    // Most edges' lanes read, and their widths are a few metres to a few tens.
    let widths: Vec<f32> = edges.iter().filter_map(|e| e.width).collect();
    assert!(
        widths.len() * 100 >= edges.len() * 95,
        "{} of {} widths read",
        widths.len(),
        edges.len()
    );
    assert!(widths.iter().all(|w| (1.0..=100.0).contains(w)));

    // The terrain covers the map the header gives.
    let terrain = read_terrain(&stream).unwrap();
    assert_eq!(terrain.size(), (header.map_width, header.map_height));
    assert_eq!(terrain.heights.len(), terrain.columns() * terrain.rows());

    // Saves with a model table: every instance read stands on (or just off) the map.
    if let Ok(table) = read_model_table(&stream) {
        let prefixes = ["industries/", "depots/", "stations/"];
        let found = read_instances(&stream, &table, &prefixes);
        let inside = found
            .iter()
            .filter(|i| i.x.abs() <= half.0 + 200.0 && i.y.abs() <= half.1 + 200.0)
            .count();
        assert!(
            inside * 100 >= found.len() * 99,
            "{inside} of {} on the map",
            found.len()
        );
        let mains = industry_models(&table, &found);
        let industries = found
            .iter()
            .filter(|i| mains.contains_key(&i.model))
            .count();
        eprintln!("{} instances, {industries} industries", found.len());
    }
}
