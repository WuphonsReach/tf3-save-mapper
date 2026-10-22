//! The street-edge table: one record per road or rail segment
//! (`docs/road-network.md` in tf3-save-format).
//!
//! A record holds the entity ids of the edge's two end nodes, the two end
//! positions, the two end tangents, some lane data, and the edge's street
//! template as text. The table has no marker and no length, so it is found
//! by pattern: every record but the first starts right after a `.street`
//! string, and the template string follows the lane data.

use std::collections::HashSet;

use memchr::memmem;

use crate::error::{Error, Result};

/// A record's fixed part: two u32 ids and four `vec3` of f32.
const FIXED_BYTES: usize = 8 + 4 * 12;
/// The template string follows the lane data within this many bytes.
const TEMPLATE_WINDOW: usize = 800;
/// Records less than this far apart belong to one table.
const CLUSTER_GAP: usize = 1 << 20;
const MAX_ID: u32 = 1 << 24;
const MAX_X: f64 = 40_000.0;
const MAX_Y: f64 = 60_000.0;

/// Which kind of line an edge is, from the folder of its template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Street,
    Track,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Entity ids of the two end nodes.
    pub a: u32,
    pub b: u32,
    /// End positions (x east, y north, z up, metres), node `a` first.
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    /// Tangents at the two ends.
    pub t0: [f32; 3],
    pub t1: [f32; 3],
    /// The template path under `infrastructure/`, such as
    /// `street/town/town_old_small`, or `None` if its string was not found.
    pub template: Option<String>,
    /// Offset of the record's first byte in the stream.
    pub offset: usize,
}

impl Edge {
    pub fn family(&self) -> Family {
        match &self.template {
            Some(t) if t.starts_with("track/") => Family::Track,
            _ => Family::Street,
        }
    }

    /// The last part of the template path, or `unknown`.
    pub fn type_name(&self) -> &str {
        self.template
            .as_deref()
            .and_then(|t| t.rsplit('/').next())
            .unwrap_or("unknown")
    }

    /// Horizontal length between the end positions, metres.
    pub fn length(&self) -> f64 {
        hypot(
            f64::from(self.p1[0]) - f64::from(self.p0[0]),
            f64::from(self.p1[1]) - f64::from(self.p0[1]),
        )
    }

    /// The centre line as a cubic Bezier in x, y: the end positions, each
    /// with a control point a third of its tangent inward. With both tangents
    /// the difference vector this is the straight line.
    pub fn bezier(&self) -> [(f64, f64); 4] {
        let (x0, y0) = (f64::from(self.p0[0]), f64::from(self.p0[1]));
        let (x1, y1) = (f64::from(self.p1[0]), f64::from(self.p1[1]));
        let (t0, t1) = (self.t0.map(f64::from), self.t1.map(f64::from));
        [
            (x0, y0),
            (x0 + t0[0] / 3.0, y0 + t0[1] / 3.0),
            (x1 - t1[0] / 3.0, y1 - t1[1] / 3.0),
            (x1, y1),
        ]
    }

    /// True when the centre line leaves its chord by more than `tolerance` metres.
    pub fn is_curved(&self, tolerance: f64) -> bool {
        let c = self.bezier();
        let mid = (
            0.125 * (c[0].0 + c[3].0) + 0.375 * (c[1].0 + c[2].0),
            0.125 * (c[0].1 + c[3].1) + 0.375 * (c[1].1 + c[2].1),
        );
        hypot(
            mid.0 - (c[0].0 + c[3].0) / 2.0,
            mid.1 - (c[0].1 + c[3].1) / 2.0,
        ) > tolerance
    }
}

fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

fn vec3(bytes: &[u8]) -> [f32; 3] {
    let f = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    [f(0), f(1), f(2)]
}

/// Read the street-edge table. Edges whose end nodes are in the save's free-id
/// list are still returned; see [`read_free_ids`] and [`retain_live`].
pub fn read_edges(data: &[u8]) -> Result<Vec<Edge>> {
    let found: Vec<Edge> = memmem::find_iter(data, b".street")
        .filter_map(|at| parse_record(data, at + b".street".len()))
        .collect();
    let mut edges = largest_cluster(found);
    if edges.is_empty() {
        return Err(Error::NoEdges);
    }
    if let Some(first) = record_before(data, edges[0].offset) {
        edges.insert(0, first);
    }
    Ok(edges)
}

/// The record whose first byte is `start`, if the bytes there read as one.
fn parse_record(data: &[u8], start: usize) -> Option<Edge> {
    let fixed = data.get(start..start.checked_add(FIXED_BYTES)?)?;
    let a = u32::from_le_bytes(fixed[0..4].try_into().unwrap());
    let b = u32::from_le_bytes(fixed[4..8].try_into().unwrap());
    let p0 = vec3(&fixed[8..20]);
    let p1 = vec3(&fixed[20..32]);
    let t0 = vec3(&fixed[32..44]);
    let t1 = vec3(&fixed[44..56]);
    if !plausible(a, b, p0, p1, t0, t1) {
        return None;
    }
    let window = &data[start..data.len().min(start + TEMPLATE_WINDOW)];
    Some(Edge {
        a,
        b,
        p0,
        p1,
        t0,
        t1,
        template: find_template(window),
        offset: start,
    })
}

/// Checks that keep a stray match out: sensible ids, positions on the map,
/// a length between 1 m and a few km, and tangents about as long as the edge
/// (the tangents of a straight edge are its difference vector), at most five times.
fn plausible(a: u32, b: u32, p0: [f32; 3], p1: [f32; 3], t0: [f32; 3], t1: [f32; 3]) -> bool {
    if !(0 < a && a < MAX_ID && 0 < b && b < MAX_ID) || a == b {
        return false;
    }
    for v in [p0, p1] {
        let [x, y, z] = v.map(f64::from);
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return false;
        }
        if x.abs() > MAX_X || y.abs() > MAX_Y || !(-1000.0 < z && z < 8000.0) {
            return false;
        }
    }
    let length = hypot(
        f64::from(p1[0]) - f64::from(p0[0]),
        f64::from(p1[1]) - f64::from(p0[1]),
    );
    if !(1.0 < length && length < 5000.0) {
        return false;
    }
    if !t0.iter().chain(&t1).all(|c| c.is_finite()) {
        return false;
    }
    [t0, t1]
        .iter()
        .all(|t| hypot(f64::from(t[0]), f64::from(t[1])) < 5.0 * length)
}

/// The template string in `window`: a `str` (u32 length, then bytes) reading
/// `infrastructure/street/...` or `infrastructure/track/....street_template`.
/// Returns the path under `infrastructure/`, without the suffix.
fn find_template(window: &[u8]) -> Option<String> {
    const PREFIX: &[u8] = b"infrastructure/";
    const SUFFIX: &[u8] = b".street_template";
    memmem::find_iter(window, PREFIX).find_map(|pos| {
        let prefix_len = pos.checked_sub(4).map(|p| &window[p..pos])?;
        let len = usize::from(prefix_len[0]);
        if !(0x20..=0x80).contains(&len) || prefix_len[1..] != [0, 0, 0] {
            return None;
        }
        let text = window.get(pos..pos + len)?;
        let path = text.strip_suffix(SUFFIX)?;
        let under = path.strip_prefix(PREFIX)?;
        let folder_len = if under.starts_with(b"street/") {
            b"street/".len()
        } else if under.starts_with(b"track/") {
            b"track/".len()
        } else {
            return None;
        };
        let name = &under[folder_len..];
        if name.is_empty()
            || !name
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'/')
        {
            return None;
        }
        Some(String::from_utf8_lossy(under).into_owned())
    })
}

/// The table is one run of records. A stray match far from it (another
/// structure that happens to look like a record) is dropped by keeping the
/// biggest group of records whose neighbours lie less than `CLUSTER_GAP` apart.
fn largest_cluster(edges: Vec<Edge>) -> Vec<Edge> {
    let (mut best_start, mut best_len) = (0, 0);
    let (mut cur_start, mut cur_len) = (0, 0);
    let mut previous: Option<usize> = None;
    for (i, e) in edges.iter().enumerate() {
        if previous.is_some_and(|p| e.offset - p >= CLUSTER_GAP) {
            cur_start = i;
            cur_len = 0;
        }
        cur_len += 1;
        if cur_len > best_len {
            (best_start, best_len) = (cur_start, cur_len);
        }
        previous = Some(e.offset);
    }
    edges.into_iter().skip(best_start).take(best_len).collect()
}

/// The table's first record. Every other record starts right after the
/// previous record's last `.street`, which is what `read_edges` matches on;
/// the first one follows other bytes (a run of small counters), so it is
/// found from the second: a record that reads as one and starts 250 to 400
/// bytes earlier. Bytes at the wrong alignment can also pass the plausibility
/// checks, so candidates are told apart by their tangents: each is within 20 %
/// of the edge's own length. `None` if there is no candidate or more than one.
fn record_before(data: &[u8], offset: usize) -> Option<Edge> {
    if offset < 7 || &data[offset - 7..offset] != b".street" {
        return None;
    }
    let mut found = (offset.saturating_sub(400)..offset.saturating_sub(250))
        .filter_map(|start| parse_record(data, start))
        .filter(|e| {
            e.template.is_some()
                && [e.t0, e.t1].iter().all(|t| {
                    (hypot(f64::from(t[0]), f64::from(t[1])) - e.length()).abs() < 0.2 * e.length()
                })
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// Words near the end of the stream are searched for the free-id list, within this many bytes.
const TAIL_BYTES: usize = 4_000_000;
/// Shortest ascending run taken for the list's sorted part.
const FREE_MIN_RUN: usize = 1000;
/// An id is below this; the words after the list are not.
const FREE_MAX_ID: u32 = 1_000_000;

/// The free-id list as a set, or `None` if it is not found. It is a `vec<u32>`
/// near the end of the stream: a head of small ascending groups, then one long
/// ascending run. The run is the longest ascending stretch of small values in
/// the stream's last few megabytes (at any 4-byte alignment); the count word is
/// the one before the list that reaches the run's end. A candidate with an id
/// twice is not taken. A node id in the list belongs to an entity the game no
/// longer has (`docs/town-leftovers.md`).
pub fn read_free_ids(data: &[u8]) -> Option<HashSet<u32>> {
    let tail = &data[data.len().saturating_sub(TAIL_BYTES)..];
    (0..4).find_map(|alignment| free_ids_at(tail.get(alignment..)?))
}

fn free_ids_at(bytes: &[u8]) -> Option<HashSet<u32>> {
    let words: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let mut best: Option<(usize, usize)> = None; // (length, start) of the longest ascending run
    let mut start = 0;
    for i in 1..=words.len() {
        if i == words.len() || words[i] <= words[i - 1] || words[i] >= FREE_MAX_ID {
            let len = i - start;
            if len >= FREE_MIN_RUN
                && words[start] < FREE_MAX_ID
                && best.is_none_or(|(best_len, _)| len > best_len)
            {
                best = Some((len, start));
            }
            start = i;
        }
    }
    let (run_len, run_start) = best?;
    let mut end = run_start;
    while end < words.len()
        && words[end] < FREE_MAX_ID
        && (end == run_start || words[end] > words[end - 1])
    {
        end += 1;
    }
    // The count word: its index + 1 + count is the run's end.
    (run_start.saturating_sub(100_000)..run_start).find_map(|i| {
        let count = words[i] as usize;
        let reaches_end = (i + 1 + count).abs_diff(end) <= 3;
        if !(reaches_end && count > run_len) {
            return None;
        }
        let ids: HashSet<u32> = words.get(i + 1..i + 1 + count)?.iter().copied().collect();
        (ids.len() == count).then_some(ids)
    })
}

/// Drop the edges with an end node in `free`: the table keeps a deleted road
/// for a while, and the free-id list catches most of them. Returns how many
/// were dropped.
pub fn retain_live(edges: &mut Vec<Edge>, free: &HashSet<u32>) -> usize {
    let before = edges.len();
    edges.retain(|e| !free.contains(&e.a) && !free.contains(&e.b));
    before - edges.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(p0: [f32; 3], p1: [f32; 3], t0: [f32; 3], t1: [f32; 3]) -> Edge {
        Edge {
            a: 1,
            b: 2,
            p0,
            p1,
            t0,
            t1,
            template: Some("street/town/town_old_small".into()),
            offset: 0,
        }
    }

    #[test]
    fn straight_edge_is_not_curved() {
        let d = [60.0, 80.0, 0.0];
        let e = edge([0.0, 0.0, 0.0], d, d, d);
        assert_eq!(e.length(), 100.0);
        assert!(!e.is_curved(0.25));
        let c = e.bezier();
        assert_eq!((c[0], c[3]), ((0.0, 0.0), (60.0, 80.0)));
    }

    #[test]
    fn bent_edge_is_curved() {
        let e = edge(
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [100.0, 60.0, 0.0],
            [100.0, -60.0, 0.0],
        );
        assert!(e.is_curved(0.25));
    }

    #[test]
    fn names_and_family_come_from_the_template() {
        let mut e = edge([0.0; 3], [1.0; 3], [0.0; 3], [0.0; 3]);
        assert_eq!(
            (e.family(), e.type_name()),
            (Family::Street, "town_old_small")
        );
        e.template = Some("track/standard".into());
        assert_eq!((e.family(), e.type_name()), (Family::Track, "standard"));
        e.template = None;
        assert_eq!((e.family(), e.type_name()), (Family::Street, "unknown"));
    }

    #[test]
    fn implausible_records_are_refused() {
        let d = [10.0, 0.0, 0.0];
        let ok = |a, b, p1: [f32; 3]| plausible(a, b, [0.0; 3], p1, d, d);
        assert!(ok(1, 2, [10.0, 0.0, 0.0]));
        assert!(!ok(0, 2, [10.0, 0.0, 0.0]), "id 0");
        assert!(!ok(3, 3, [10.0, 0.0, 0.0]), "same node twice");
        assert!(!ok(1, 2, [0.5, 0.0, 0.0]), "shorter than a metre");
        assert!(!ok(1, 2, [9000.0, 0.0, 0.0]), "longer than 5 km");
        assert!(!ok(1, 2, [f32::NAN, 0.0, 0.0]), "not a number");
    }

    #[test]
    fn free_ids_need_a_long_ascending_run() {
        assert_eq!(read_free_ids(&[0u8; 4096]), None);
        assert_eq!(read_free_ids(&[]), None);
    }
}
