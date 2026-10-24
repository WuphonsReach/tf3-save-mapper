//! Plane geometry for the drawn buildings: outlines round groups of boxes,
//! and where a stamp goes beside an outline. Points are picture metres.

use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;

pub type Point = (f64, f64);

/// Whether (x, y) lies in the convex polygon `poly`, in either winding.
pub fn inside(poly: &[Point], x: f64, y: f64) -> bool {
    let mut sign = 0.0;
    for (i, &(x0, y0)) in poly.iter().enumerate() {
        let (x1, y1) = poly[(i + 1) % poly.len()];
        let c = (x1 - x0) * (y - y0) - (y1 - y0) * (x - x0);
        if c != 0.0 {
            if sign != 0.0 && (c > 0.0) != (sign > 0.0) {
                return false;
            }
            sign = c.signum();
        }
    }
    true
}

/// The area centroid of a polygon when it lies inside the polygon, else
/// `None` (a shape bent round its own centroid, or one with no area).
pub fn centroid_inside(face: &[Point]) -> Option<Point> {
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    let edges = || (0..face.len()).map(|i| (face[i], face[(i + 1) % face.len()]));
    for ((a0, b0), (a1, b1)) in edges() {
        let cross = a0 * b1 - a1 * b0;
        area += cross;
        cx += (a0 + a1) * cross;
        cy += (b0 + b1) * cross;
    }
    if area.abs() < 1e-6 {
        return None;
    }
    let (cx, cy) = (cx / (3.0 * area), cy / (3.0 * area));
    let mut is_inside = false;
    for ((a0, b0), (a1, b1)) in edges() {
        if (b0 > cy) != (b1 > cy) && cx < a0 + (cy - b0) * (a1 - a0) / (b1 - b0) {
            is_inside = !is_inside;
        }
    }
    is_inside.then_some((cx, cy))
}

/// `pt` moved away from the nearest stretch of `outer` (polylines round the
/// boxes `polys`) until it is `clearance` metres from it, on the side that is
/// not inside a box, and the nearest point of that stretch; or (`pt`, `None`)
/// when no stretch lies within `reach` metres.
pub fn clear_of_edge(
    pt: Point,
    clearance: f64,
    polys: &[Vec<Point>],
    outer: &[Vec<Point>],
    reach: f64,
) -> (Point, Option<Point>) {
    let mut best: Option<(f64, f64, f64, f64, f64)> = None;
    for run in outer {
        for w in run.windows(2) {
            let ((x0, y0), (x1, y1)) = (w[0], w[1]);
            let (dx, dy) = (x1 - x0, y1 - y0);
            let n2 = dx * dx + dy * dy;
            if n2 == 0.0 {
                continue;
            }
            let t = (((pt.0 - x0) * dx + (pt.1 - y0) * dy) / n2).clamp(0.0, 1.0);
            let (px, py) = (x0 + t * dx, y0 + t * dy);
            let dist = (pt.0 - px).hypot(pt.1 - py);
            if dist <= reach && best.is_none_or(|b| dist < b.0) {
                let n = n2.sqrt();
                best = Some((dist, px, py, -dy / n, dx / n));
            }
        }
    }
    let Some((_, px, py, mut nx, mut ny)) = best else {
        return (pt, None);
    };
    if polys
        .iter()
        .any(|c| inside(c, px + 0.5 * nx, py + 0.5 * ny))
    {
        (nx, ny) = (-nx, -ny); // the normal that leaves the boxes
    }
    ((px + clearance * nx, py + clearance * ny), Some((px, py)))
}

/// Round to thousandths, as a key: the grid lines of `closed_outline`.
fn milli(v: f64) -> i64 {
    (v * 1000.0).round() as i64
}

/// Closed loops round the union of the boxes `polys` (four corners each)
/// with every gap up to 2 g metres closed, so thin gaps between the pieces of
/// one station leave no line inside. Each box is grown by g, the union taken
/// and shrunk by g again, in the frame of the boxes' own axes, on the grid of
/// the boxes' edge coordinates and their multiples of g. Boxes within 2 g of
/// each other and with the same heading (or a quarter turn aside, as a
/// station's pieces are) are one group.
pub fn closed_outline(polys: &[Vec<Point>], g: f64) -> Vec<Vec<Point>> {
    let n = polys.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut a: usize) -> usize {
        while parent[a] != a {
            parent[a] = parent[parent[a]];
            a = parent[a];
        }
        a
    }
    let aabb: Vec<(f64, f64, f64, f64)> = polys
        .iter()
        .map(|c| {
            let b = bounds(c);
            (b.0 - g, b.1 + g, b.2 - g, b.3 + g)
        })
        .collect();
    let heading: Vec<f64> = polys.iter().map(|c| quarter_heading(c)).collect();
    for a in 0..n {
        for b in a + 1..n {
            let (p, q) = (aabb[a], aabb[b]);
            let turn = (heading[a] - heading[b]).abs();
            let aligned = turn.min(FRAC_PI_2 - turn) < HEADING_TOLERANCE;
            if aligned && p.0 <= q.1 && q.0 <= p.1 && p.2 <= q.3 && q.2 <= p.3 {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                parent[ra] = rb;
            }
        }
    }
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for a in 0..n {
        let root = find(&mut parent, a);
        match groups.iter_mut().find(|(r, _)| *r == root) {
            Some((_, members)) => members.push(a),
            None => groups.push((root, vec![a])),
        }
    }
    let mut loops = Vec::new();
    for (_, members) in groups {
        let c0 = &polys[members[0]];
        let ang = (c0[1].1 - c0[0].1).atan2(c0[1].0 - c0[0].0);
        let (ca, sa) = ((-ang).cos(), (-ang).sin());
        let rects: Vec<(f64, f64, f64, f64)> = members
            .iter()
            .map(|&m| {
                let pts: Vec<Point> = polys[m]
                    .iter()
                    .map(|q| (q.0 * ca - q.1 * sa, q.0 * sa + q.1 * ca))
                    .collect();
                bounds(&pts)
            })
            .collect();
        let axis = |lo: fn(&(f64, f64, f64, f64)) -> [f64; 2]| -> Vec<f64> {
            let mut keys: Vec<i64> = rects
                .iter()
                .flat_map(lo)
                .flat_map(|v| (-2..=2).map(move |k| milli(v + f64::from(k) * g)))
                .collect();
            keys.sort_unstable();
            keys.dedup();
            keys.into_iter().map(|k| k as f64 / 1000.0).collect()
        };
        let xs = axis(|r| [r.0, r.1]);
        let ys = axis(|r| [r.2, r.3]);
        let left = |v: f64| xs.partition_point(|&x| milli(x) < milli(v));
        let below = |v: f64| ys.partition_point(|&y| milli(y) < milli(v));
        let (nx, ny) = (xs.len() - 1, ys.len() - 1);
        let mut grown = vec![false; nx * ny];
        for r in &rects {
            for ix in left(r.0 - g)..left(r.1 + g) {
                for iy in below(r.2 - g)..below(r.3 + g) {
                    grown[ix * ny + iy] = true;
                }
            }
        }
        let mut pre = vec![0u32; (nx + 1) * (ny + 1)];
        for ix in 0..nx {
            for iy in 0..ny {
                pre[(ix + 1) * (ny + 1) + iy + 1] = pre[ix * (ny + 1) + iy + 1]
                    + pre[(ix + 1) * (ny + 1) + iy]
                    - pre[ix * (ny + 1) + iy]
                    + u32::from(grown[ix * ny + iy]);
            }
        }
        // A cell stays when every cell within g of it is grown.
        let lower = |axis: &[f64], v: f64| axis.partition_point(|&a| a <= v).saturating_sub(1);
        let upper = |axis: &[f64], v: f64| axis.partition_point(|&a| a < v);
        let mut kept = vec![false; nx * ny];
        for ix in 0..nx {
            if xs[ix] - g < xs[0] - 1e-6 || xs[ix + 1] + g > xs[nx] + 1e-6 {
                continue;
            }
            let (ia, ib) = (
                lower(&xs, xs[ix] - g + 1e-6),
                upper(&xs, xs[ix + 1] + g - 1e-6),
            );
            for iy in 0..ny {
                if ys[iy] - g < ys[0] - 1e-6 || ys[iy + 1] + g > ys[ny] + 1e-6 {
                    continue;
                }
                let (ja, jb) = (
                    lower(&ys, ys[iy] - g + 1e-6),
                    upper(&ys, ys[iy + 1] + g - 1e-6),
                );
                let total = pre[ib * (ny + 1) + jb] + pre[ia * (ny + 1) + ja]
                    - pre[ia * (ny + 1) + jb]
                    - pre[ib * (ny + 1) + ja];
                kept[ix * ny + iy] = total as usize == (ib - ia) * (jb - ja);
            }
        }
        // Directed boundary edges with the kept cell on the left, traced into loops.
        let is_kept = |ix: usize, iy: usize| kept[ix * ny + iy];
        let mut edges: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
        let mut order: Vec<(usize, usize)> = Vec::new();
        let mut add = |from: (usize, usize), to: (usize, usize)| {
            edges
                .entry(from)
                .or_insert_with(|| {
                    order.push(from);
                    Vec::new()
                })
                .push(to);
        };
        for ix in 0..nx {
            for iy in 0..ny {
                if !is_kept(ix, iy) {
                    continue;
                }
                if iy == 0 || !is_kept(ix, iy - 1) {
                    add((ix, iy), (ix + 1, iy));
                }
                if ix == nx - 1 || !is_kept(ix + 1, iy) {
                    add((ix + 1, iy), (ix + 1, iy + 1));
                }
                if iy == ny - 1 || !is_kept(ix, iy + 1) {
                    add((ix + 1, iy + 1), (ix, iy + 1));
                }
                if ix == 0 || !is_kept(ix - 1, iy) {
                    add((ix, iy + 1), (ix, iy));
                }
            }
        }
        let (ca, sa) = (ang.cos(), ang.sin());
        let mut next_start = 0;
        loop {
            while next_start < order.len() && !edges.contains_key(&order[next_start]) {
                next_start += 1;
            }
            let Some(&start) = order.get(next_start) else {
                break;
            };
            let mut trail = vec![start];
            let mut cur = start;
            loop {
                let outgoing = edges.get_mut(&cur).unwrap();
                let next = outgoing.pop().unwrap();
                if outgoing.is_empty() {
                    edges.remove(&cur);
                }
                if next == start {
                    break;
                }
                trail.push(next);
                cur = next;
            }
            let m = trail.len();
            let pts: Vec<Point> = (0..m)
                .filter(|&k| {
                    let (a, v, b) = (trail[(k + m - 1) % m], trail[k], trail[(k + 1) % m]);
                    !((a.0 == v.0 && v.0 == b.0) || (a.1 == v.1 && v.1 == b.1))
                })
                .map(|k| {
                    let (x, y) = (xs[trail[k].0], ys[trail[k].1]);
                    (x * ca - y * sa, x * sa + y * ca)
                })
                .collect();
            if pts.len() >= 3 {
                loops.push(pts);
            }
        }
    }
    loops
}

/// Radians: boxes whose headings differ by less than this (a quarter turn
/// aside counting as the same) can be one group.
const HEADING_TOLERANCE: f64 = 0.01;

/// The heading of a box's first edge, modulo a quarter turn: in [0, pi / 2).
fn quarter_heading(c: &[Point]) -> f64 {
    (c[1].1 - c[0].1)
        .atan2(c[1].0 - c[0].0)
        .rem_euclid(FRAC_PI_2)
}

/// (x0, x1, y0, y1) of a set of points.
pub fn bounds(pts: &[Point]) -> (f64, f64, f64, f64) {
    pts.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(x0, x1, y0, y1), &(x, y)| (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
    )
}

/// The stretches of the boxes' edges that no other box covers just outside,
/// as polylines: the group's outline with no line where two boxes touch.
/// Every metre of an edge is tested for a box 0.3 m beyond it.
pub fn outer_runs(polys: &[Vec<Point>]) -> Vec<Vec<Point>> {
    let boxes: Vec<(f64, f64, f64, f64)> = polys.iter().map(|c| bounds(c)).collect();
    // 50 m cells holding the boxes that reach them, so a test looks only nearby.
    let cell = |v: f64| (v / 50.0).floor() as i64;
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (n, b) in boxes.iter().enumerate() {
        for gx in cell(b.0)..=cell(b.1) {
            for gy in cell(b.2)..=cell(b.3) {
                buckets.entry((gx, gy)).or_default().push(n);
            }
        }
    }
    let mut outer = Vec::new();
    for (me, c) in polys.iter().enumerate() {
        let k = c.len() as f64;
        let (cx, cy) = (
            c.iter().map(|q| q.0).sum::<f64>() / k,
            c.iter().map(|q| q.1).sum::<f64>() / k,
        );
        for i in 0..c.len() {
            let ((x0, y0), (x1, y1)) = (c[i], c[(i + 1) % c.len()]);
            let length = (x1 - x0).hypot(y1 - y0);
            let steps = (length as usize).max(1);
            let unit = if length > 0.0 { length } else { 1.0 };
            let (mut nx, mut ny) = ((y1 - y0) / unit, -(x1 - x0) / unit);
            if nx * (x0 - cx) + ny * (y0 - cy) < 0.0 {
                (nx, ny) = (-nx, -ny); // the normal that points away from this box
            }
            let mut run: Option<Vec<Point>> = None;
            let mut last = false;
            for st in 0..steps {
                let (t0, t1) = (st as f64 / steps as f64, (st + 1) as f64 / steps as f64);
                let mx = x0 + (x1 - x0) * (t0 + t1) / 2.0 + 0.3 * nx;
                let my = y0 + (y1 - y0) * (t0 + t1) / 2.0 + 0.3 * ny;
                let covered = buckets.get(&(cell(mx), cell(my))).is_some_and(|near| {
                    near.iter().any(|&o| {
                        let b = boxes[o];
                        o != me
                            && b.0 <= mx
                            && mx <= b.1
                            && b.2 <= my
                            && my <= b.3
                            && inside(&polys[o], mx, my)
                    })
                });
                if run.is_none() || covered != last {
                    if let Some(r) = run.take()
                        && !last
                    {
                        outer.push(r);
                    }
                    run = Some(vec![(x0 + (x1 - x0) * t0, y0 + (y1 - y0) * t0)]);
                    last = covered;
                }
                run.as_mut()
                    .unwrap()
                    .push((x0 + (x1 - x0) * t1, y0 + (y1 - y0) * t1));
            }
            if let Some(r) = run
                && !last
            {
                outer.push(r);
            }
        }
    }
    outer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    #[test]
    fn two_boxes_with_a_small_gap_get_one_outline() {
        let loops = closed_outline(
            &[rect(0.0, 0.0, 10.0, 10.0), rect(12.0, 0.0, 22.0, 10.0)],
            3.0,
        );
        assert_eq!(loops.len(), 1, "{loops:?}");
        let b = bounds(&loops[0]);
        assert!(
            (b.0 - 0.0).abs() < 1e-6 && (b.1 - 22.0).abs() < 1e-6,
            "{b:?}"
        );
        assert_eq!(loops[0].len(), 4, "a plain rectangle: {loops:?}");
    }

    #[test]
    fn boxes_at_another_heading_stay_apart() {
        // A box turned 30 degrees, overlapping the first: not one station.
        let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let turned: Vec<Point> = rect(12.0, 0.0, 22.0, 10.0)
            .iter()
            .map(|&(x, y)| (x * c - y * s, x * s + y * c))
            .collect();
        let loops = closed_outline(&[rect(0.0, 0.0, 10.0, 10.0), turned], 3.0);
        assert_eq!(loops.len(), 2, "{loops:?}");
        // A quarter turn is the same heading.
        let quarter = vec![(22.0, 0.0), (22.0, 10.0), (12.0, 10.0), (12.0, 0.0)];
        assert_eq!(
            closed_outline(&[rect(0.0, 0.0, 10.0, 10.0), quarter], 3.0).len(),
            1
        );
    }

    #[test]
    fn far_boxes_stay_apart() {
        let loops = closed_outline(
            &[rect(0.0, 0.0, 10.0, 10.0), rect(30.0, 0.0, 40.0, 10.0)],
            3.0,
        );
        assert_eq!(loops.len(), 2);
    }

    #[test]
    fn touching_boxes_have_no_line_between_them() {
        let runs = outer_runs(&[rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 0.0, 20.0, 10.0)]);
        // No run lies along x = 10 between the two.
        for r in &runs {
            assert!(!r.iter().all(|p| (p.0 - 10.0).abs() < 1e-9), "{r:?}");
        }
        let length: f64 = runs
            .iter()
            .flat_map(|r| r.windows(2))
            .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
            .sum();
        assert!((length - 60.0).abs() < 1e-6, "{length}");
    }

    #[test]
    fn centroid_of_an_l_shape_lies_outside_it() {
        let l = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 1.0),
            (1.0, 1.0),
            (1.0, 10.0),
            (0.0, 10.0),
        ];
        assert_eq!(centroid_inside(&l), None);
        let sq = rect(0.0, 0.0, 4.0, 2.0);
        assert_eq!(centroid_inside(&sq), Some((2.0, 1.0)));
    }

    #[test]
    fn a_stamp_moves_off_the_edge_away_from_the_box() {
        let boxes = [rect(0.0, 0.0, 10.0, 10.0)];
        let outer = outer_runs(&boxes);
        let (p, edge) = clear_of_edge((5.0, 9.0), 4.0, &boxes, &outer, 40.0);
        assert_eq!(edge, Some((5.0, 10.0)));
        assert!(
            (p.0 - 5.0).abs() < 1e-9 && (p.1 - 14.0).abs() < 1e-9,
            "{p:?}"
        );
    }
}
