//! Vector shapes from a heightmap grid: the water and the land's height bands
//! as closed rings, and the height lines.
//!
//! Marching squares: a cell whose four corners fall on both sides of a level
//! is cut by a line whose ends on the cell's edges sit where the height
//! reaches the level, linear between the two corners. The cells' pieces are
//! chained into whole lines through the edges they share. A saddle (two
//! opposite corners high) is settled by the cell's mean, and a corner counts
//! as high when its height is at or above the level. The grid is padded by one
//! ring of its own highest value, so every ring closes at the picture's edge.
//!
//! Points are metres from the picture's top-left corner, grid point (i, j)
//! being at (i * spacing, j * spacing), with j running down the picture.

use std::collections::{BTreeMap, HashMap};

pub type Point = (f64, f64);

/// A grid of height values in picture order: row 0 at the top.
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub values: Vec<u16>,
}

impl Grid {
    fn at(&self, i: usize, j: usize) -> u16 {
        self.values[j * self.width + i]
    }
}

/// The cell edges a line joins, by case (top-left corner high = 1, top-right
/// 2, bottom-right 4, bottom-left 8). A saddle has two choices: the first when
/// the cell's mean is at or above the level.
const T: u8 = 0;
const R: u8 = 1;
const B: u8 = 2;
const L: u8 = 3;

fn segments(case: u8, centre_high: bool) -> &'static [(u8, u8)] {
    match case {
        1 | 14 => &[(L, T)],
        2 | 13 => &[(T, R)],
        3 | 12 => &[(L, R)],
        4 | 11 => &[(R, B)],
        6 | 9 => &[(T, B)],
        7 | 8 => &[(L, B)],
        5 if centre_high => &[(L, B), (T, R)],
        5 => &[(L, T), (R, B)],
        10 if centre_high => &[(L, T), (R, B)],
        10 => &[(T, R), (L, B)],
        _ => &[],
    }
}

/// The lines of one level: each cut point (keyed by the cell edge it lies on)
/// and its neighbours, in the order the points were first met.
#[derive(Default)]
struct Lines {
    index: HashMap<u64, u32>,
    points: Vec<Point>,
    neighbours: Vec<Vec<u32>>,
}

impl Lines {
    fn node(&mut self, key: u64, at: impl FnOnce() -> Point) -> u32 {
        if let Some(&n) = self.index.get(&key) {
            return n;
        }
        let n = self.points.len() as u32;
        self.index.insert(key, n);
        self.points.push(at());
        self.neighbours.push(Vec::new());
        n
    }

    fn join(&mut self, a: u32, b: u32) {
        self.neighbours[a as usize].push(b);
        self.neighbours[b as usize].push(a);
    }

    /// The pieces chained into polylines, open ones first (they start where a
    /// point has one neighbour).
    fn chain(&self) -> Vec<Vec<Point>> {
        let mut seen = vec![false; self.points.len()];
        let mut out = Vec::new();
        let walk = |start: u32, seen: &mut Vec<bool>| -> Vec<Point> {
            let mut pts = vec![self.points[start as usize]];
            seen[start as usize] = true;
            let mut cur = start;
            let mut prev;
            let mut next = self.neighbours[start as usize][0];
            loop {
                if seen[next as usize] {
                    return pts;
                }
                seen[next as usize] = true;
                pts.push(self.points[next as usize]);
                (prev, cur) = (cur, next);
                match self.neighbours[cur as usize].iter().find(|&&n| n != prev) {
                    Some(&n) => next = n,
                    None => return pts,
                }
                if next == start && pts.len() > 2 {
                    return pts;
                }
            }
        };
        for (n, nb) in self.neighbours.iter().enumerate() {
            if nb.len() == 1 && !seen[n] {
                out.push(walk(n as u32, &mut seen));
            }
        }
        for n in 0..self.points.len() {
            if !seen[n] {
                out.push(walk(n as u32, &mut seen));
            }
        }
        out
    }
}

/// Rings by level: 0 for the water level, k for the water level plus k
/// steps. Each ring goes round the ground below its level, closed by the
/// picture's edge where that ground reaches it; fill them with the even-odd
/// rule. Rings shorter than `min_perimeter` metres are left out. Also returns
/// the picture's extent (right, bottom), which `split_at_border` needs.
pub fn level_rings(
    grid: &Grid,
    spacing: f64,
    water: f64,
    step: f64,
    min_perimeter: f64,
) -> (BTreeMap<u32, Vec<Vec<Point>>>, Point) {
    let (w, h) = (grid.width, grid.height);
    let top = grid
        .values
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .max(water.ceil().clamp(0.0, 65535.0) as u16);
    // Band 0 is the ground below the water, band m >= 1 starts at
    // water + (m - 1) * step. Cut m lies between bands m - 1 and m.
    let band = |v: u16| -> u32 {
        let v = f64::from(v);
        if v < water {
            0
        } else {
            1 + ((v - water) / step).floor() as u32
        }
    };
    let level_of = |m: u32| water + f64::from(m - 1) * step;
    // The padded grid is (w + 2) x (h + 2): padding at index 0 and at w + 1 / h + 1.
    let (pw, ph) = (w + 2, h + 2);
    let value = |i: usize, j: usize| -> u16 {
        if i == 0 || j == 0 || i == pw - 1 || j == ph - 1 {
            top
        } else {
            grid.at(i - 1, j - 1)
        }
    };
    let is_pad = |i: usize, j: usize| i == 0 || j == 0 || i == pw - 1 || j == ph - 1;
    let band_rows: Vec<u32> = (0..ph)
        .flat_map(|j| (0..pw).map(move |i| (i, j)))
        .map(|(i, j)| band(value(i, j)))
        .collect();

    let mut levels: BTreeMap<u32, Lines> = BTreeMap::new();
    for j in 0..ph - 1 {
        for i in 0..pw - 1 {
            let q = [
                band_rows[j * pw + i],
                band_rows[j * pw + i + 1],
                band_rows[(j + 1) * pw + i + 1],
                band_rows[(j + 1) * pw + i],
            ];
            let (lo, hi) = (*q.iter().min().unwrap(), *q.iter().max().unwrap());
            if lo == hi {
                continue;
            }
            // Corner heights: top-left, top-right, bottom-right, bottom-left.
            let z = [
                f64::from(value(i, j)),
                f64::from(value(i + 1, j)),
                f64::from(value(i + 1, j + 1)),
                f64::from(value(i, j + 1)),
            ];
            for m in (lo + 1).max(1)..=hi {
                let level = level_of(m);
                let case = u8::from(z[0] >= level)
                    | u8::from(z[1] >= level) << 1
                    | u8::from(z[2] >= level) << 2
                    | u8::from(z[3] >= level) << 3;
                let centre_high = z.iter().sum::<f64>() / 4.0 >= level;
                let lines = levels.entry(m).or_default();
                for &(e1, e2) in segments(case, centre_high) {
                    let mut cut = |e: u8| {
                        // (key, first corner, the corner heights at both ends, direction)
                        let (key, u, v, a, b, du, dv) = match e {
                            T => ((j * pw + i) * 2, i, j, z[0], z[1], 1, 0),
                            B => (((j + 1) * pw + i) * 2, i, j + 1, z[3], z[2], 1, 0),
                            L => ((j * pw + i) * 2 + 1, i, j, z[0], z[3], 0, 1),
                            _ => ((j * pw + i + 1) * 2 + 1, i + 1, j, z[1], z[2], 0, 1),
                        };
                        lines.node(key as u64, || {
                            let mut t = (level - a) / (b - a);
                            // An edge from a real point to a padding point is cut at the padding point.
                            let (pad_a, pad_b) = (is_pad(u, v), is_pad(u + du, v + dv));
                            if pad_a != pad_b {
                                t = if pad_a { 0.0 } else { 1.0 };
                            }
                            (
                                (u as f64 + du as f64 * t - 1.0) * spacing,
                                (v as f64 + dv as f64 * t - 1.0) * spacing,
                            )
                        })
                    };
                    let (a, b) = (cut(e1), cut(e2));
                    lines.join(a, b);
                }
            }
        }
    }

    // The padding's points are clamped onto the picture's edge, on every side.
    let (right, bottom) = ((w - 1) as f64 * spacing, (h - 1) as f64 * spacing);
    let mut out = BTreeMap::new();
    for (m, lines) in levels {
        let rings: Vec<Vec<Point>> = lines
            .chain()
            .into_iter()
            .map(|pts| {
                pts.into_iter()
                    .map(|(u, v)| (u.clamp(0.0, right), v.clamp(0.0, bottom)))
                    .collect::<Vec<_>>()
            })
            .filter(|pts| length(pts) >= min_perimeter)
            .collect();
        if !rings.is_empty() {
            out.insert(m - 1, rings);
        }
    }
    (out, (right, bottom))
}

/// A ring (closed implicitly) as the open polylines left when the pieces that
/// run along the picture's edge are dropped, each with whether it is closed.
/// A ring that never touches the edge comes back whole and closed.
pub fn split_at_border(ring: &[Point], extent: Point) -> Vec<(Vec<Point>, bool)> {
    let (right, bottom) = extent;
    let n = ring.len();
    let on_edge = |a: Point, b: Point| {
        (a.0 == b.0 && (a.0 == 0.0 || a.0 == right))
            || (a.1 == b.1 && (a.1 == 0.0 || a.1 == bottom))
    };
    let border: Vec<bool> = (0..n)
        .map(|i| on_edge(ring[i], ring[(i + 1) % n]))
        .collect();
    let Some(first) = border.iter().position(|&b| b) else {
        return vec![(ring.to_vec(), true)];
    };
    let mut out = Vec::new();
    let mut run: Vec<Point> = Vec::new();
    for t in 1..=n {
        let i = (first + t) % n;
        if border[i] {
            if run.len() > 1 {
                out.push((std::mem::take(&mut run), false));
            }
            run.clear();
        } else {
            if run.is_empty() {
                run.push(ring[i]);
            }
            run.push(ring[(i + 1) % n]);
        }
    }
    if run.len() > 1 {
        out.push((run, false));
    }
    out
}

fn length(pts: &[Point]) -> f64 {
    pts.windows(2)
        .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
        .sum()
}

/// Douglas-Peucker: `pts` thinned so that no dropped point lies further than
/// `tol` metres from the line that replaces it. A closed ring keeps its first point.
pub fn simplify(pts: &[Point], tol: f64, closed: bool) -> Vec<Point> {
    if pts.len() < 3 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut work = pts.to_vec();
    if closed {
        work.push(pts[0]);
    }
    let mut keep = vec![false; work.len()];
    keep[0] = true;
    *keep.last_mut().unwrap() = true;
    let mut stack = vec![(0, work.len() - 1)];
    while let Some((lo, hi)) = stack.pop() {
        let ((x0, y0), (x1, y1)) = (work[lo], work[hi]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let norm = dx.hypot(dy);
        let (mut far, mut far_d) = (None, tol);
        for (i, &(x, y)) in work.iter().enumerate().take(hi).skip(lo + 1) {
            let d = if norm > 0.0 {
                (dy * (x - x0) - dx * (y - y0)).abs() / norm
            } else {
                (x - x0).hypot(y - y0)
            };
            if d > far_d {
                (far, far_d) = (Some(i), d);
            }
        }
        if let Some(far) = far {
            keep[far] = true;
            stack.push((lo, far));
            stack.push((far, hi));
        }
    }
    let mut out: Vec<Point> = work
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect();
    if closed {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(width: usize, values: &[u16]) -> Grid {
        Grid {
            width,
            height: values.len() / width,
            values: values.to_vec(),
        }
    }

    #[test]
    fn a_pit_gives_one_water_ring_round_it() {
        // A 3 x 3 grid, low in the middle; water at 5.
        let g = grid(3, &[10, 10, 10, 10, 0, 10, 10, 10, 10]);
        let (rings, extent) = level_rings(&g, 4.0, 5.0, 100.0, 0.0);
        assert_eq!(extent, (8.0, 8.0));
        let water = &rings[&0];
        assert_eq!(water.len(), 1);
        // The cuts sit halfway between the middle point (4, 4) and its neighbours.
        let ring = &water[0];
        assert_eq!(ring.len(), 4);
        for &(u, v) in ring {
            assert_eq!((u - 4.0).abs() + (v - 4.0).abs(), 2.0, "{ring:?}");
        }
        assert_eq!(split_at_border(ring, extent), vec![(ring.clone(), true)]);
    }

    #[test]
    fn a_slope_gives_lines_closed_by_the_border() {
        // Rising to the right: 0, 10, 20, 30 along every row; water at 5, steps of 10.
        let g = grid(4, &[0, 10, 20, 30, 0, 10, 20, 30, 0, 10, 20, 30]);
        let (rings, extent) = level_rings(&g, 4.0, 5.0, 10.0, 0.0);
        assert_eq!(rings.keys().copied().collect::<Vec<_>>(), [0, 1, 2]);
        for (k, rs) in &rings {
            assert_eq!(rs.len(), 1, "level {k}");
            let open = split_at_border(&rs[0], extent);
            assert_eq!(open.len(), 1, "level {k}: {open:?}");
            let (line, closed) = &open[0];
            assert!(!closed);
            // A height line at 5 + 10k crosses the rows at u = 4 * (0.5 + k),
            // from the top edge to the bottom one, except where it steps onto the edge.
            let u = 4.0 * (0.5 + *k as f64);
            let inner: Vec<_> = line.iter().filter(|p| 0.0 < p.1 && p.1 < 8.0).collect();
            assert!(inner.iter().all(|p| (p.0 - u).abs() < 1e-9), "{line:?}");
            assert!(line.iter().any(|p| p.1 == 0.0) && line.iter().any(|p| p.1 == 8.0));
        }
    }

    #[test]
    fn simplify_drops_points_on_a_line() {
        let pts = [(0.0, 0.0), (1.0, 0.01), (2.0, 0.0), (2.0, 5.0)];
        assert_eq!(
            simplify(&pts, 0.1, false),
            [(0.0, 0.0), (2.0, 0.0), (2.0, 5.0)]
        );
        assert_eq!(simplify(&pts, 0.0, false), pts);
    }
}
