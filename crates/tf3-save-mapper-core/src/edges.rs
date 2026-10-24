//! The street-edge table: one record per road or rail segment
//! (`docs/road-network.md` in tf3-save-format).
//!
//! A record holds the entity ids of the edge's two end nodes, the two end
//! positions, the two end tangents, the objects and decorations on the edge,
//! its lanes, and four strings: a mod name, the street type, a mod name and
//! the base type. The table has no marker and no length, so it is found by
//! pattern: every record but the first starts right after the previous
//! record's base type, which ends in `.street`.

use std::collections::HashSet;

use memchr::memmem;

use crate::error::{Error, Result};

/// A record's fixed part: two u32 ids and four `vec3` of f32.
const FIXED_BYTES: usize = 8 + 4 * 12;
/// The street type string ends within this many bytes of the record's start
/// (441 the most seen).
const TEMPLATE_WINDOW: usize = 800;
const TEMPLATE_SUFFIX: &[u8] = b".street_template";
/// TRAIN and ELECTRIC_TRAIN, by their place in the game's `TransportMode` list.
const TRAIN_MODES: u32 = (1 << 7) | (1 << 8);
/// Records less than this far apart belong to one table.
const CLUSTER_GAP: usize = 1 << 20;
const MAX_ID: u32 = 1 << 24;
const MAX_X: f64 = 40_000.0;
const MAX_Y: f64 = 60_000.0;

/// Which kind of line an edge is: a track when one of its lanes carries trains.
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
    /// The street type's resource path without `.street_template`, and
    /// without `infrastructure/` for the types under it:
    /// `street/town/town_old_small`, but
    /// `industries/shared/industry_track/industry`. `None` if the record's
    /// strings did not read.
    pub template: Option<String>,
    /// From the lanes' transport modes; from the folder when the lanes do not
    /// read, and `Street` when that is not `street/` or `track/` either.
    pub family: Family,
    /// Offset of the record's first byte in the stream.
    pub offset: usize,
}

impl Edge {
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

    /// True when an inner control point lies more than `tolerance` metres
    /// from the chord. The curve stays inside its control points' hull, so no
    /// bend that leaves the chord by more is missed; the curve's middle alone
    /// would miss an S-bend, whose middle can lie on the chord.
    pub fn is_curved(&self, tolerance: f64) -> bool {
        let c = self.bezier();
        distance_from_segment(c[1], c[0], c[3]).max(distance_from_segment(c[2], c[0], c[3]))
            > tolerance
    }
}

/// Distance of `p` from the segment `a`-`b`, which has a length.
fn distance_from_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    hypot(p.0 - (a.0 + t * dx), p.1 - (a.1 + t * dy))
}

fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().unwrap()))
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
    let (template, family) = match record_type(data, start) {
        Some((template, family)) => (Some(template), family),
        None => (None, Family::Street),
    };
    Some(Edge {
        a,
        b,
        p0,
        p1,
        t0,
        t1,
        template,
        family,
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

/// The template and family of the record starting at `start`. The street
/// type is the record's first `.street_template` string after the tangents,
/// and a mod name and the base type (ending in `.street`) must follow it.
/// No later string is tried: that would be the next record's.
fn record_type(data: &[u8], start: usize) -> Option<(String, Family)> {
    let window = data.get(start + FIXED_BYTES..data.len().min(start + TEMPLATE_WINDOW))?;
    let suffix_at = start + FIXED_BYTES + memmem::find(window, TEMPLATE_SUFFIX)?;
    let end = suffix_at + TEMPLATE_SUFFIX.len();
    // Back over the text to its length word, whose top byte is 0.
    let mut text_at = suffix_at;
    while text_at > start + FIXED_BYTES + 4 && data[text_at - 1] >= 0x20 {
        text_at -= 1;
    }
    let length_at = text_at - 4;
    if u32_at(data, length_at)? as usize != end - text_at {
        return None;
    }
    let mod_len = u32_at(data, end)? as usize;
    if mod_len > 256 {
        return None;
    }
    let base_at = end + 4 + mod_len;
    let base_len = u32_at(data, base_at)? as usize;
    if base_len > 512
        || !data
            .get(base_at + 4..base_at + 4 + base_len)?
            .ends_with(b".street")
    {
        return None;
    }
    let path = String::from_utf8_lossy(&data[text_at..suffix_at]);
    let template = path
        .strip_prefix("infrastructure/")
        .unwrap_or(&path)
        .to_owned();
    let family = match lane_modes(data, start, length_at) {
        Some(modes) if modes & TRAIN_MODES != 0 => Family::Track,
        Some(_) => Family::Street,
        None if path.starts_with("infrastructure/track/") => Family::Track,
        None => Family::Street,
    };
    Some((template, family))
}

/// The transport-mode bits of all the record's lanes together, or `None` when
/// the bytes from the tangents up to the street type's length word at
/// `type_at` do not read as: two u32, the objects on the edge
/// (`vec<(u32, u32)>`), its decorations (`vec<(u32, u8)>`), its lanes, 9 bytes
/// and a mod name. A lane is 21 bytes: f32 speed, width and height, u8, u32
/// modes, f32 offset.
fn lane_modes(data: &[u8], start: usize, type_at: usize) -> Option<u32> {
    let mut at = start + FIXED_BYTES + 8;
    let objects = u32_at(data, at)? as usize;
    if objects > 64 {
        return None;
    }
    at += 4 + 8 * objects;
    let decorations = u32_at(data, at)? as usize;
    if decorations > 64 {
        return None;
    }
    at += 4 + 5 * decorations;
    let lanes = u32_at(data, at)? as usize;
    if !(1..=32).contains(&lanes) {
        return None;
    }
    at += 4;
    let mut modes = 0;
    for lane in 0..lanes {
        modes |= u32_at(data, at + 21 * lane + 13)?;
    }
    at += 21 * lanes;
    let mod_len = u32_at(data, at + 9)? as usize;
    (at + 13 + mod_len == type_at).then_some(modes)
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
/// An id is below this (node ids pass 1,000,000 in long games).
const FREE_MAX_ID: u32 = MAX_ID;

/// The free-id list as a set, or `None` if it is not found. It is a `vec<u32>`
/// near the end of the stream, followed by a trailer: u32 2, a u32 and a short
/// `str` of digits. Its longest ascending stretch of small values (in the
/// stream's last few megabytes, at any 4-byte alignment) lies inside the list,
/// mostly at its end. The list ends at the first trailer from the run's end
/// on, and the count word is the nearest word before the run that reaches that
/// trailer exactly. A candidate with an id twice is not taken. A node id in
/// the list belongs to an entity the game no longer has
/// (`docs/town-leftovers.md`).
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
    let is_trailer = |j: usize| {
        let digits = words[j + 2] as usize;
        let text_at = 4 * (j + 3);
        words[j] == 2
            && (1..=32).contains(&digits)
            && bytes
                .get(text_at..text_at + digits)
                .is_some_and(|t| t.iter().all(u8::is_ascii_digit))
    };
    (run_start + run_len..words.len().saturating_sub(3))
        .filter(|&j| is_trailer(j))
        .find_map(|end| {
            // The count word: its index + 1 + count is the trailer.
            (0..run_start).rev().find_map(|i| {
                if i + 1 + words[i] as usize != end {
                    return None;
                }
                let ids: HashSet<u32> = words[i + 1..end].iter().copied().collect();
                let all_ids = ids.len() == end - i - 1 && ids.iter().all(|&id| id < FREE_MAX_ID);
                all_ids.then_some(ids)
            })
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
            family: Family::Street,
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
    fn an_s_bend_is_curved_though_its_middle_is_on_the_chord() {
        // Equal tangents to both sides of the chord: the middle is (50, 0),
        // but the curve is 5.6 m off the chord a quarter of the way along.
        let t = [100.0, 60.0, 0.0];
        let e = edge([0.0, 0.0, 0.0], [100.0, 0.0, 0.0], t, t);
        assert!(e.is_curved(0.25));
    }

    #[test]
    fn type_name_is_the_last_part_of_the_template() {
        let mut e = edge([0.0; 3], [1.0; 3], [0.0; 3], [0.0; 3]);
        assert_eq!(e.type_name(), "town_old_small");
        e.template = Some("industries/shared/industry_track/industry".into());
        assert_eq!(e.type_name(), "industry");
        e.template = None;
        assert_eq!(e.type_name(), "unknown");
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
