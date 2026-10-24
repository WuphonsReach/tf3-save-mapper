//! The road-surface strip table, which gives some edges their entity id
//! (`docs/road-network.md` in tf3-save-format, "The other tables a road edit
//! touches").
//!
//! The table is keyed by edge entity id. Each entry holds rows of quads along
//! the road, each row two sides (a flag byte 0 or 1). A row is found by its
//! bytes: `0x41`, the u32 entity id, the side byte, four bytes not read, the
//! u32 4 twice, then the side's x and y as f32. Only some edges have an
//! entry, and some entries belong to no edge in the street-edge table, so an
//! entry is paired with an edge by position: its centre line (the two sides
//! averaged at its first and last row) has to lie near the edge's line and
//! cover a good part of it.

use std::collections::HashMap;

use memchr::memchr_iter;

use crate::edges::Edge;

/// A map point, metres.
type Point = (f64, f64);

/// Bytes of one row.
const ROW_BYTES: usize = 26;
/// Ids are below this; positions within these.
const MAX_ID: u32 = 1 << 24;
const MAX_X: f32 = 40_000.0;
const MAX_Y: f32 = 60_000.0;
/// An entry's ends lie this close to the edge's line, metres,
const NEAR: f64 = 6.0;
/// and it covers this share of the edge's length.
const COVER: f64 = 0.4;
/// Metres per cell of the index of entries by their first point.
const CELL: f64 = 64.0;

/// One strip-table entry: its entity id and its rows, (side, x, y), in
/// stream order.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    pub id: u32,
    pub rows: Vec<(u8, f32, f32)>,
}

/// Every strip-table entry in the stream, in the order their first rows
/// appear. Rows found by the byte pattern do not overlap.
pub fn read_strips(data: &[u8]) -> Vec<Strip> {
    let mut out: Vec<Strip> = Vec::new();
    let mut by_id: HashMap<u32, usize> = HashMap::new();
    let mut next_free = 0;
    for at in memchr_iter(0x41, data) {
        if at < next_free || at + ROW_BYTES > data.len() {
            continue;
        }
        let row = &data[at..at + ROW_BYTES];
        if row[5] > 1 || row[10..18] != [4, 0, 0, 0, 4, 0, 0, 0] {
            continue;
        }
        next_free = at + ROW_BYTES;
        let id = u32::from_le_bytes(row[1..5].try_into().unwrap());
        let x = f32::from_le_bytes(row[18..22].try_into().unwrap());
        let y = f32::from_le_bytes(row[22..26].try_into().unwrap());
        if !(0 < id && id < MAX_ID && x.abs() < MAX_X && y.abs() < MAX_Y) {
            continue;
        }
        let k = *by_id.entry(id).or_insert_with(|| {
            out.push(Strip {
                id,
                rows: Vec::new(),
            });
            out.len() - 1
        });
        out[k].rows.push((row[5], x, y));
    }
    out
}

/// (distance of `p` from the segment `a`-`b`, where its foot lies along it
/// as a share from 0 to 1).
fn along(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy)), t)
}

/// The entity id of each edge that a strip-table entry runs along, by the
/// edge's offset in the stream. An entry needs both sides with the same
/// number of rows; of the entries whose ends lie within 6 m of the edge's
/// line and that cover at least 0.4 of it, the one that covers most wins
/// (the first found, on a tie). Dead ends and the stretch next to a junction
/// can have no entry or a short one.
pub fn edge_ids(edges: &[Edge], strips: &[Strip]) -> HashMap<usize, u32> {
    let mean = |a: (f32, f32), b: (f32, f32)| {
        (
            (f64::from(a.0) + f64::from(b.0)) / 2.0,
            (f64::from(a.1) + f64::from(b.1)) / 2.0,
        )
    };
    // (id, the centre line's first and last point)
    let mut runs: Vec<(u32, Point, Point)> = Vec::new();
    for s in strips {
        let side = |k: u8| -> Vec<(f32, f32)> {
            s.rows
                .iter()
                .filter(|r| r.0 == k)
                .map(|r| (r.1, r.2))
                .collect()
        };
        let (left, right) = (side(0), side(1));
        if !left.is_empty() && left.len() == right.len() {
            runs.push((
                s.id,
                mean(left[0], right[0]),
                mean(left[left.len() - 1], right[right.len() - 1]),
            ));
        }
    }
    let cell = |v: f64| (v / CELL).floor() as i64;
    let mut index: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, r) in runs.iter().enumerate() {
        index.entry((cell(r.1.0), cell(r.1.1))).or_default().push(k);
    }
    let mut out = HashMap::new();
    for e in edges {
        let a = (f64::from(e.p0[0]), f64::from(e.p0[1]));
        let b = (f64::from(e.p1[0]), f64::from(e.p1[1]));
        // An entry whose first point is near the edge has it in the edge's box, widened.
        let mut near: Vec<usize> = Vec::new();
        for gx in cell(a.0.min(b.0) - NEAR)..=cell(a.0.max(b.0) + NEAR) {
            for gy in cell(a.1.min(b.1) - NEAR)..=cell(a.1.max(b.1) + NEAR) {
                near.extend(index.get(&(gx, gy)).into_iter().flatten());
            }
        }
        near.sort_unstable();
        let mut best: Option<(f64, u32)> = None;
        for k in near {
            let (id, f, l) = runs[k];
            let ((df, tf), (dl, tl)) = (along(f, a, b), along(l, a, b));
            let cover = (tl - tf).abs();
            if df < NEAR && dl < NEAR && cover >= COVER && best.is_none_or(|(c, _)| cover > c) {
                best = Some((cover, id));
            }
        }
        if let Some((_, id)) = best {
            out.insert(e.offset, id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::Family;

    fn row(out: &mut Vec<u8>, id: u32, side: u8, x: f32, y: f32) {
        out.push(0x41);
        out.extend_from_slice(&id.to_le_bytes());
        out.push(side);
        out.extend_from_slice(&[9, 9, 9, 9, 4, 0, 0, 0, 4, 0, 0, 0]);
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
    }

    fn edge(offset: usize, p0: [f32; 3], p1: [f32; 3]) -> Edge {
        let d = [p1[0] - p0[0], p1[1] - p0[1], 0.0];
        Edge {
            a: 1,
            b: 2,
            p0,
            p1,
            t0: d,
            t1: d,
            template: None,
            family: Family::Street,
            width: None,
            offset,
        }
    }

    #[test]
    fn rows_are_grouped_by_entry_in_stream_order() {
        let mut s = vec![0x41, 0x41, 7];
        row(&mut s, 0x7de1, 0, 10.0, 4.0);
        row(&mut s, 0x2a, 1, 10.0, -4.0);
        row(&mut s, 0x7de1, 1, 10.0, -4.0);
        row(&mut s, 0, 0, 1.0, 1.0); // no id
        row(&mut s, 5, 2, 1.0, 1.0); // no side
        row(&mut s, 6, 0, 50_000.0, 1.0); // off the map
        let strips = read_strips(&s);
        let ids: Vec<u32> = strips.iter().map(|s| s.id).collect();
        assert_eq!(ids, [0x7de1, 0x2a]);
        assert_eq!(strips[0].rows, [(0, 10.0, 4.0), (1, 10.0, -4.0)]);
    }

    #[test]
    fn an_entry_along_an_edge_gives_its_id() {
        let mut s = Vec::new();
        for (x, id) in [(10.0, 0x111), (90.0, 0x111), (40.0, 0x222), (60.0, 0x222)] {
            row(&mut s, id, 0, x, 4.0);
            row(&mut s, id, 1, x, -4.0);
        }
        // One beside the edge, 20 m off.
        row(&mut s, 0x333, 0, 0.0, 24.0);
        row(&mut s, 0x333, 1, 0.0, 16.0);
        row(&mut s, 0x333, 0, 100.0, 24.0);
        row(&mut s, 0x333, 1, 100.0, 16.0);
        let strips = read_strips(&s);
        let edges = [
            edge(0, [0.0, 0.0, 0.0], [100.0, 0.0, 0.0]),
            edge(1, [0.0, 50.0, 0.0], [100.0, 50.0, 0.0]),
        ];
        let ids = edge_ids(&edges, &strips);
        // 0x111 covers 0.8 of the first edge, 0x222 only 0.2.
        assert_eq!(ids.get(&0), Some(&0x111));
        assert_eq!(ids.get(&1), None);
    }
}
