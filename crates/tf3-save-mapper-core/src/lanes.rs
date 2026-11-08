//! The lane table: the pieces of lane the game routes vehicles on, near the
//! end of the stream (before the free-id list).
//!
//! A record joins two lane points, each an entity id and an index within it:
//! `u32 A, u32 i, u32 B, u32 j`, a `u32` kind, then the two points' x and y as
//! `f32`. Both edges and nodes appear as entities; along an edge or across a
//! node the record runs from `(E, k)` to `(E, k + 1)`. Many records follow the
//! two `f32` 1.0 with kind 2 or 3, and those are the anchors the table is found
//! by: a cluster of them is the table, and every record within it adds its
//! two entity ids. Across 381 saves the table was one cluster, its anchors at
//! most 385 KB apart; a save whose table has a wider gap reads as two
//! clusters, and every cluster large enough is read, so the ids are whole.
//!
//! A road or rail piece whose nodes have no lanes is one the game no longer
//! routes on or draws, and that the free-id list does not catch: mostly what
//! a removed industry leaves of its access road and sidings (`docs/gaps.md`),
//! but stale pieces of any type too (in one catalog save, 133 town streets,
//! highway, tram and catenary track, most lying on a live edge's ends).

use std::collections::HashSet;

use memchr::memmem;

use crate::edges::Edge;

/// The two `f32` 1.0 before an anchor record.
const ANCHOR_PREFIX: [u8; 8] = [0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f];
const RECORD_BYTES: usize = 9 * 4;
const MAX_ID: u32 = 1 << 26;
const MAX_INDEX: u32 = 1024;
const MAX_KIND: u32 = 4;
/// Further than any map's edge from its centre.
const MAX_COORD: f32 = 60_000.0;
/// A lane piece is short: a few metres, at most a long straight's subdivision.
const MAX_PIECE: f32 = 500.0;
const MIN_PIECE: f32 = 0.01;
/// Anchors less than this far apart belong to one cluster.
const CLUSTER_GAP: usize = 1 << 20;
/// A cluster of fewer anchors than this is not part of the table (a save of
/// a few hundred edges has thousands; the smallest table seen had 124).
const MIN_ANCHORS: usize = 64;
/// The lane filter is refused when it would drop more than this share of the
/// edges: the table read is then not the table. The most seen was 15 % (a
/// small map whose dropped streets touch no live edge).
const MAX_DROP_SHARE: f64 = 1.0 / 3.0;
/// Records may sit this far before the first anchor and after the last.
const MARGIN: usize = 4096;

fn word(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

/// The record at `at`: its two entity ids and its kind, if it reads as one.
fn record_at(data: &[u8], at: usize) -> Option<(u32, u32, u32)> {
    if at + RECORD_BYTES > data.len() {
        return None;
    }
    let (a, i, b, j, kind) = (
        word(data, at),
        word(data, at + 4),
        word(data, at + 8),
        word(data, at + 12),
        word(data, at + 16),
    );
    if a == 0 || b == 0 || a >= MAX_ID || b >= MAX_ID {
        return None;
    }
    if i >= MAX_INDEX || j >= MAX_INDEX || kind > MAX_KIND {
        return None;
    }
    let f = |k: usize| f32::from_bits(word(data, at + 20 + 4 * k));
    let (x0, y0, x1, y1) = (f(0), f(1), f(2), f(3));
    if ![x0, y0, x1, y1]
        .iter()
        .all(|c| c.is_finite() && c.abs() < MAX_COORD)
    {
        return None;
    }
    if x0.abs() < 0.001 && y0.abs() < 0.001 {
        return None;
    }
    let piece = (x1 - x0).hypot(y1 - y0);
    (MIN_PIECE..MAX_PIECE)
        .contains(&piece)
        .then_some((a, b, kind))
}

/// The entity ids that have lanes, or `None` if the lane table is not found.
pub fn read_lane_ids(data: &[u8]) -> Option<HashSet<u32>> {
    let anchors: Vec<usize> = memmem::find_iter(data, &ANCHOR_PREFIX)
        .map(|p| p + ANCHOR_PREFIX.len())
        .filter(|&at| matches!(record_at(data, at), Some((_, _, 2 | 3))))
        .collect();
    // Every cluster large enough: an id read from a stray one can only keep
    // an edge, while a part of the table left out would drop live ones.
    let mut clusters = Vec::new(); // (first, last)
    let mut start = 0;
    for k in 1..=anchors.len() {
        if k == anchors.len() || anchors[k] - anchors[k - 1] > CLUSTER_GAP {
            if k - start >= MIN_ANCHORS {
                clusters.push((anchors[start], anchors[k - 1]));
            }
            start = k;
        }
    }
    if clusters.is_empty() {
        return None;
    }
    let mut ids = HashSet::new();
    for (first, last) in clusters {
        let from = first.saturating_sub(MARGIN);
        let to = (last + RECORD_BYTES + MARGIN).min(data.len());
        for at in from..to {
            if let Some((a, b, _)) = record_at(data, at) {
                ids.insert(a);
                ids.insert(b);
            }
        }
    }
    Some(ids)
}

/// Drop the edges with neither end node in `lanes`: pieces the game no longer
/// routes on (a removed industry's road and sidings, or a stale piece of any
/// type). An edge with one laned end is kept. Returns how many were dropped,
/// or `Err` with that number, dropping nothing, when it is more than a third
/// of the edges ([`MAX_DROP_SHARE`]): the ids read are then not the table's.
pub fn retain_laned(edges: &mut Vec<Edge>, lanes: &HashSet<u32>) -> Result<usize, usize> {
    let laned = |e: &Edge| lanes.contains(&e.a) || lanes.contains(&e.b);
    let unlaned = edges.iter().filter(|e| !laned(e)).count();
    if unlaned as f64 > MAX_DROP_SHARE * edges.len() as f64 {
        return Err(unlaned);
    }
    edges.retain(laned);
    Ok(unlaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(a: u32, i: u32, b: u32, j: u32, kind: u32, p: [f32; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        for w in [a, i, b, j, kind] {
            out.extend(w.to_le_bytes());
        }
        for c in p {
            out.extend(c.to_le_bytes());
        }
        out
    }

    /// A table of anchored records for entities `ids`, each a 10 m piece.
    fn table(ids: &[u32]) -> Vec<u8> {
        let mut out = vec![0xee; 40];
        for (n, &id) in ids.iter().enumerate() {
            let x = 100.0 + n as f32 * 3.5;
            out.extend(ANCHOR_PREFIX);
            out.extend(record(id, 0, id, 1, 2, [x, -250.0, x + 10.0, -250.0]));
            out.extend([0xee; 12]);
        }
        out
    }

    #[test]
    fn records_need_ids_indices_and_a_short_piece_on_the_map() {
        let ok = |a, i, kind, p| {
            let bytes = record(a, i, a, i + 1, kind, p);
            record_at(&bytes, 0).is_some()
        };
        let p = [10.0, 20.0, 12.0, 21.0];
        assert!(ok(5000, 0, 2, p));
        assert!(!ok(0, 0, 2, p), "id 0");
        assert!(!ok(5000, 5000, 2, p), "index too large");
        assert!(!ok(5000, 0, 9, p), "kind too large");
        assert!(!ok(5000, 0, 2, [10.0, 20.0, 10.0, 20.0]), "no length");
        assert!(!ok(5000, 0, 2, [10.0, 20.0, 900.0, 20.0]), "too long");
        assert!(
            !ok(5000, 0, 2, [f32::NAN, 20.0, 12.0, 21.0]),
            "not a number"
        );
        assert!(
            !ok(5000, 0, 2, [80_000.0, 20.0, 80_001.0, 21.0]),
            "off the map"
        );
    }

    #[test]
    fn a_small_stray_cluster_is_not_the_table() {
        let ids: Vec<u32> = (0..100).map(|n| 20_000 + 7 * n).collect();
        let mut data = table(&[900_001, 900_002]); // a stray cluster, too small
        data.extend(vec![0u8; CLUSTER_GAP + 1]);
        data.extend(table(&ids));
        // An unanchored record (kind 0, as across a street node) inside the table.
        data.extend(record(31_337, 0, 31_337, 1, 0, [50.0, 50.0, 50.0, 60.0]));
        data.extend(table(&ids[..10]));
        let lanes = read_lane_ids(&data).unwrap();
        assert!(ids.iter().all(|id| lanes.contains(id)));
        assert!(lanes.contains(&31_337));
        assert!(!lanes.contains(&900_001));
    }

    #[test]
    fn an_edge_is_kept_while_one_end_has_lanes() {
        let edge = |a, b| Edge {
            a,
            b,
            p0: [0.0; 3],
            p1: [10.0, 0.0, 0.0],
            t0: [10.0, 0.0, 0.0],
            t1: [10.0, 0.0, 0.0],
            template: None,
            family: crate::edges::Family::Street,
            width: None,
            modes: None,
            offset: 0,
        };
        let mut edges = vec![edge(1, 2), edge(2, 3), edge(3, 4)];
        let lanes: HashSet<u32> = [1, 2].into();
        assert_eq!(retain_laned(&mut edges, &lanes), Ok(1));
        assert_eq!(
            edges.iter().map(|e| (e.a, e.b)).collect::<Vec<_>>(),
            [(1, 2), (2, 3)]
        );
        // Two of five unlaned is more than a third: nothing goes.
        let mut edges = vec![edge(1, 2), edge(2, 1), edge(1, 1), edge(3, 4), edge(4, 3)];
        assert_eq!(retain_laned(&mut edges, &lanes), Err(2));
        assert_eq!(edges.len(), 5);
    }

    #[test]
    fn a_table_split_by_a_wide_gap_is_read_whole() {
        let ids: Vec<u32> = (0..200).map(|n| 20_000 + 7 * n).collect();
        let mut data = table(&ids[..100]);
        data.extend(vec![0u8; CLUSTER_GAP + 1]);
        data.extend(table(&ids[100..]));
        let lanes = read_lane_ids(&data).unwrap();
        assert!(ids.iter().all(|id| lanes.contains(id)));
    }

    #[test]
    fn too_few_anchors_is_no_table() {
        assert_eq!(read_lane_ids(&table(&[20_000, 20_001, 20_002])), None);
        assert_eq!(read_lane_ids(&[]), None);
    }
}
