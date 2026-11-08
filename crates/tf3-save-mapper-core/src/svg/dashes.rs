//! Where the dash pattern of each edge in a tunnel starts, so that the dashes
//! of tracks or roads laid side by side fall abreast and a run of edges
//! continues one pattern.
//!
//! A phase is the pattern position at the edge's start, metres: a point `s`
//! metres along the edge lies on a dash when `phase + s`, taken modulo the
//! pattern's length, is less than the dash (SVG's `stroke-dashoffset`).
//! Phases are handed on from edge to edge: through a shared node, so the
//! pattern runs on without a jump, and sideways to an edge that runs beside
//! one already done, matched where its first sample near that edge lies
//! along it. An edge that runs the other way takes the mirror image: a
//! position `m` seen from the other end is `dash - m`, which maps each dash
//! of a pattern that starts with a dash onto itself. Each group of edges reached so starts from its first edge
//! in stream order. Distances along a bend come from a polyline of its
//! curve, close to the arc length a renderer takes.

use std::collections::{HashMap, VecDeque};

use crate::edges::Edge;

/// Metres between the samples of an edge's curve, at most.
const STEP: f64 = 2.0;
/// An edge runs beside another when one of its samples lies this close to
/// one of the other's and the two run within about 15 degrees of each
/// other there.
const BESIDE: f64 = 20.0;
const PARALLEL: f64 = 0.966;
const CELL: f64 = 16.0;

/// One edge's curve as samples: point, distance along it, unit direction.
struct Line {
    points: Vec<(f64, f64)>,
    along: Vec<f64>,
    dirs: Vec<(f64, f64)>,
    length: f64,
}

impl Line {
    fn of(e: &Edge) -> Line {
        let c = e.bezier();
        let n = ((e.length() / STEP).ceil() as usize).clamp(8, 4096);
        let points: Vec<(f64, f64)> = (0..=n)
            .map(|k| super::bezier_point(&c, k as f64 / n as f64))
            .collect();
        let mut along = vec![0.0];
        for w in points.windows(2) {
            along.push(along.last().unwrap() + (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1));
        }
        let dirs = (0..points.len())
            .map(|k| {
                let (a, b) = (points[k.saturating_sub(1)], points[(k + 1).min(n)]);
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = dx.hypot(dy);
                if len > 0.0 {
                    (dx / len, dy / len)
                } else {
                    (1.0, 0.0)
                }
            })
            .collect();
        let length = *along.last().unwrap();
        Line {
            points,
            along,
            dirs,
            length,
        }
    }

    /// The distance along this line of the point nearest `q`, and the
    /// direction there; `None` when `q` lies beyond either end.
    fn locate(&self, q: (f64, f64)) -> Option<(f64, (f64, f64))> {
        let k = (0..self.points.len())
            .min_by(|&i, &j| {
                let d = |p: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1);
                d(self.points[i]).total_cmp(&d(self.points[j]))
            })
            .unwrap_or(0);
        let (p, d) = (self.points[k], self.dirs[k]);
        let s = self.along[k] + (q.0 - p.0) * d.0 + (q.1 - p.1) * d.1;
        (0.0..=self.length).contains(&s).then_some((s, d))
    }
}

/// The phase of every edge in `edges` (those of one level, all of them, not
/// only those in view, so a crop agrees with the whole map), by the edge's
/// offset, for a pattern of `dash` then `gap` metres.
pub(super) fn phases(edges: &[&Edge], dash: f64, gap: f64) -> HashMap<usize, f64> {
    let period = dash + gap;
    let lines: Vec<Line> = edges.iter().map(|e| Line::of(e)).collect();
    let mut grid: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        for (k, p) in line.points.iter().enumerate() {
            grid.entry(cell(*p)).or_default().push((i, k));
        }
    }
    let mut at_node: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        at_node.entry(e.a).or_default().push(i);
        at_node.entry(e.b).or_default().push(i);
    }
    // The pattern position on edge `i` at distance `s`, given its phase.
    let mut phase: Vec<Option<f64>> = vec![None; edges.len()];
    let mut queue = VecDeque::new();
    for seed in 0..edges.len() {
        if phase[seed].is_some() {
            continue;
        }
        phase[seed] = Some(0.0);
        queue.push_back(seed);
        while let Some(i) = queue.pop_front() {
            let fi = phase[i].unwrap();
            let (e, line) = (edges[i], &lines[i]);
            // On through the nodes.
            for (node, end_of_i) in [(e.a, 0.0), (e.b, line.length)] {
                for &j in at_node.get(&node).into_iter().flatten() {
                    if phase[j].is_some() || edges[j].family != e.family {
                        continue;
                    }
                    let here = fi + end_of_i;
                    let lj = lines[j].length;
                    // j runs on from i's end, or into i's start: same sense.
                    let same = (node == e.b) == (edges[j].a == node);
                    let fj = match (same, edges[j].a == node) {
                        (true, true) => here,
                        (true, false) => here - lj,
                        (false, true) => dash - here,
                        (false, false) => dash - here - lj,
                    };
                    phase[j] = Some(fj);
                    queue.push_back(j);
                }
            }
            // Across to the edges beside it.
            for p in &line.points {
                let (cx, cy) = cell(*p);
                for gx in cx - 1..=cx + 1 {
                    for gy in cy - 1..=cy + 1 {
                        for &(j, k) in grid.get(&(gx, gy)).into_iter().flatten() {
                            if phase[j].is_some() || edges[j].family != e.family {
                                continue;
                            }
                            let q = lines[j].points[k];
                            if (q.0 - p.0).hypot(q.1 - p.1) > BESIDE {
                                continue;
                            }
                            let Some((s, d)) = line.locate(q) else {
                                continue;
                            };
                            let dj = lines[j].dirs[k];
                            let cos = d.0 * dj.0 + d.1 * dj.1;
                            if cos.abs() < PARALLEL {
                                continue;
                            }
                            let sj = lines[j].along[k];
                            let fj = if cos > 0.0 {
                                fi + s - sj
                            } else {
                                dash - (fi + s) - sj
                            };
                            phase[j] = Some(fj);
                            queue.push_back(j);
                        }
                    }
                }
            }
        }
    }
    edges
        .iter()
        .zip(phase)
        .map(|(e, f)| (e.offset, f.unwrap_or(0.0).rem_euclid(period)))
        .collect()
}

fn cell(p: (f64, f64)) -> (i64, i64) {
    ((p.0 / CELL).floor() as i64, (p.1 / CELL).floor() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::Family;

    fn edge(offset: usize, a: u32, b: u32, from: (f32, f32), to: (f32, f32)) -> Edge {
        let d = [to.0 - from.0, to.1 - from.1, 0.0];
        Edge {
            a,
            b,
            p0: [from.0, from.1, 0.0],
            p1: [to.0, to.1, 0.0],
            t0: d,
            t1: d,
            template: Some("track/standard".into()),
            family: Family::Track,
            width: Some(4.0),
            modes: None,
            offset,
        }
    }

    /// The pattern position of edge `e` with phase `f` at map x `x`, for an
    /// edge along x.
    fn position(e: &Edge, f: f64, x: f64) -> f64 {
        let s = (x - f64::from(e.p0[0])).abs();
        f + s
    }

    fn on_dash(m: f64, dash: f64, period: f64) -> bool {
        m.rem_euclid(period) < dash
    }

    #[test]
    fn tracks_side_by_side_get_dashes_abreast_whichever_way_they_run() {
        let (dash, gap) = (12.0, 8.0);
        let edges = [
            edge(1, 1, 2, (0.0, 0.0), (100.0, 0.0)),
            // Beside it, built the other way and cut elsewhere.
            edge(2, 3, 4, (130.0, 5.0), (37.0, 5.0)),
            edge(3, 5, 6, (-50.0, 10.0), (90.0, 10.0)),
        ];
        let refs: Vec<&Edge> = edges.iter().collect();
        let ph = phases(&refs, dash, gap);
        for x in [40.0, 41.5, 55.0, 70.25, 88.0] {
            let want = on_dash(position(&edges[0], ph[&1], x), dash, 20.0);
            let reversed = ph[&2] + (130.0 - x);
            // Mirror: the reversed edge's dash spans the same stretch, up to
            // the dash's ends.
            let m0 = position(&edges[0], ph[&1], x).rem_euclid(20.0);
            if (m0 - 0.0).abs() > 0.01 && (m0 - dash).abs() > 0.01 {
                assert_eq!(on_dash(reversed, dash, 20.0), want, "x {x}");
                assert_eq!(
                    on_dash(position(&edges[2], ph[&3], x), dash, 20.0),
                    want,
                    "x {x}"
                );
            }
        }
    }

    #[test]
    fn a_run_of_edges_continues_one_pattern() {
        let edges = [
            edge(1, 1, 2, (0.0, 0.0), (33.0, 0.0)),
            edge(2, 2, 3, (33.0, 0.0), (71.0, 0.0)),
            // Built backwards from the far end.
            edge(3, 4, 3, (120.0, 0.0), (71.0, 0.0)),
        ];
        let refs: Vec<&Edge> = edges.iter().collect();
        let ph = phases(&refs, 12.0, 8.0);
        assert!((ph[&2] - (ph[&1] + 33.0).rem_euclid(20.0)).abs() < 1e-6);
        // At x 100: edge 3 is 20 m from its start; edge 1's pattern there is
        // its phase + 100.
        let m = (ph[&1] + 100.0).rem_euclid(20.0);
        let m3 = (ph[&3] + 20.0).rem_euclid(20.0);
        assert!(((12.0 - m).rem_euclid(20.0) - m3).abs() < 1e-6, "{m} {m3}");
    }
}
