//! Which edges run in a tunnel or on a bridge.
//!
//! How the save ties a tunnel or a bridge to its edge has not been read (the
//! format notes leave it open). The game builds both from model pieces laid
//! along the edge (`infrastructure/tunnel/...` and `infrastructure/bridge/...`),
//! so an edge is matched to the pieces by position: it is sampled every few
//! metres along its centre line, and a sample counts when a piece stands
//! within the edge's half width (plus a margin) of it and within a few metres
//! of its height. An edge most of whose samples count is in a tunnel or on a
//! bridge. The game cuts edges at portals and at bridge ends, so an edge is
//! wholly one or the other; the neighbour of a portal or an abutment has
//! pieces on its last few metres only, and stays a surface edge.
//!
//! The heights are compared on the straight line between the edge's two end
//! heights: a bridge's pieces sit there, while the curve the stored z
//! tangents give arches metres above them on a long span.
//!
//! Road tunnels are matched to street edges and rail tunnels to track edges,
//! so a road crossing above a rail tunnel at a similar height is not caught.
//! Bridge pieces do not say which they carry, and match both.

use std::collections::HashMap;

use crate::edges::{Edge, Family};
use crate::instances::Instance;

/// The model folders the pieces are read from.
pub const PREFIXES: [&str; 2] = [TUNNEL, BRIDGE];
const TUNNEL: &str = "infrastructure/tunnel/";
const BRIDGE: &str = "infrastructure/bridge/";

/// Metres between samples along an edge.
const STEP: f64 = 2.0;
/// A piece counts this far beyond the edge's half width, metres.
const MARGIN: f64 = 3.0;
/// A piece counts this far above or below the edge, metres.
const HEIGHT: f64 = 5.0;
/// The share of an edge's samples that makes it a tunnel or a bridge. The
/// approach edges next to a bridge (15 to 35 m, with the abutment's pieces
/// on their last metres) reach about 0.6.
const SHARE: f64 = 0.8;
/// The cell of the grid the pieces are kept in, metres.
const CELL: f64 = 32.0;
/// The half width used for an edge whose lanes did not read.
const FALLBACK_HALF: [f64; 2] = [8.0, 2.0];

/// What an edge runs in or on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Structure {
    Tunnel,
    Bridge,
}

/// The kind of a piece, from its model folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// A tunnel piece, for street or track edges, or both when the folder
    /// does not say (`None`).
    Tunnel(Option<Family>),
    Bridge,
}

impl Piece {
    fn of(model: &str) -> Option<Piece> {
        if let Some(rest) = model.strip_prefix(TUNNEL) {
            let folder = rest.split('/').next().unwrap_or_default();
            let family = if folder.contains("rail") {
                Some(Family::Track)
            } else if folder.contains("car") {
                Some(Family::Street)
            } else {
                None
            };
            Some(Piece::Tunnel(family))
        } else if model.starts_with(BRIDGE) {
            Some(Piece::Bridge)
        } else {
            None
        }
    }

    fn fits(self, family: Family) -> Option<Structure> {
        match self {
            Piece::Tunnel(f) if f.is_none_or(|f| f == family) => Some(Structure::Tunnel),
            Piece::Tunnel(_) => None,
            Piece::Bridge => Some(Structure::Bridge),
        }
    }
}

/// The pieces by grid cell: kind and position.
type Pieces = HashMap<(i64, i64), Vec<(Piece, [f64; 3])>>;

/// The edges in a tunnel or on a bridge, by the edge's offset in the stream.
/// `pieces` may hold other instances too; only those under [`PREFIXES`] are
/// used.
pub fn find_structures(edges: &[Edge], pieces: &[Instance]) -> HashMap<usize, Structure> {
    let mut grid: Pieces = HashMap::new();
    for i in pieces {
        if let Some(kind) = Piece::of(&i.model) {
            grid.entry(cell(i.x, i.y))
                .or_default()
                .push((kind, [i.x, i.y, i.z]));
        }
    }
    if grid.is_empty() {
        return HashMap::new();
    }
    edges
        .iter()
        .filter_map(|e| Some((e.offset, structure_of(e, &grid)?)))
        .collect()
}

fn cell(x: f64, y: f64) -> (i64, i64) {
    ((x / CELL).floor() as i64, (y / CELL).floor() as i64)
}

fn structure_of(e: &Edge, grid: &Pieces) -> Option<Structure> {
    let half = e
        .width
        .map_or(FALLBACK_HALF[usize::from(e.family == Family::Track)], |w| {
            f64::from(w) / 2.0
        });
    let reach = half + MARGIN;
    let c = e.bezier();
    let (z0, z1) = (f64::from(e.p0[2]), f64::from(e.p1[2]));
    let n = ((e.length() / STEP).ceil() as usize).max(1) + 1;
    let (mut tunnel, mut bridge) = (0, 0);
    for k in 0..n {
        let t = k as f64 / (n - 1) as f64;
        let (x, y) = point(&c, t);
        let z = z0 + (z1 - z0) * t;
        let (cx, cy) = cell(x, y);
        let mut found = (false, false);
        for gx in cx - 1..=cx + 1 {
            for gy in cy - 1..=cy + 1 {
                for (kind, p) in grid.get(&(gx, gy)).into_iter().flatten() {
                    if (p[0] - x).hypot(p[1] - y) > reach || (p[2] - z).abs() > HEIGHT {
                        continue;
                    }
                    match kind.fits(e.family) {
                        Some(Structure::Tunnel) => found.0 = true,
                        Some(Structure::Bridge) => found.1 = true,
                        None => {}
                    }
                }
            }
        }
        tunnel += usize::from(found.0);
        bridge += usize::from(found.1);
    }
    let need = SHARE * n as f64;
    if tunnel as f64 >= need {
        Some(Structure::Tunnel)
    } else if bridge as f64 >= need {
        Some(Structure::Bridge)
    } else {
        None
    }
}

fn point(c: &[(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (
        w[0] * c[0].0 + w[1] * c[1].0 + w[2] * c[2].0 + w[3] * c[3].0,
        w[0] * c[0].1 + w[1] * c[1].1 + w[2] * c[2].1 + w[3] * c[3].1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight edge along x from `x0` to `x1` at height `z`.
    fn edge(offset: usize, family: Family, x0: f32, x1: f32, z: f32) -> Edge {
        let d = x1 - x0;
        Edge {
            a: 1,
            b: 2,
            p0: [x0, 0.0, z],
            p1: [x1, 0.0, z],
            t0: [d, 0.0, 0.0],
            t1: [d, 0.0, 0.0],
            template: Some("track/standard".into()),
            family,
            width: Some(4.0),
            offset,
        }
    }

    /// Pieces of `model` every 4 m from `x0` to `x1`, `side` metres off
    /// the line, at height `z`.
    fn pieces(model: &str, x0: f64, x1: f64, side: f64, z: f64) -> Vec<Instance> {
        let mut out = Vec::new();
        let mut x = x0;
        while x <= x1 {
            out.push(Instance {
                slot: out.len(),
                model: model.into(),
                x,
                y: side,
                z,
                fx: 1.0,
                fy: 0.0,
            });
            x += 4.0;
        }
        out
    }

    const RAIL_TUNNEL: &str = "infrastructure/tunnel/a_rail/tunnel_a_m1_side_lft.mdl";
    const CAR_TUNNEL: &str = "infrastructure/tunnel/a_car/tunnel_a_car_m1_side_lft.mdl";
    const STONE: &str = "infrastructure/bridge/stone/railing_rep.mdl";

    #[test]
    fn a_covered_edge_is_a_tunnel_and_its_neighbours_are_not() {
        // A 100 m tunnel between two 20 m approaches; the portal's pieces
        // reach 4 m onto each approach.
        let edges = [
            edge(1, Family::Track, -20.0, 0.0, 10.0),
            edge(2, Family::Track, 0.0, 100.0, 10.0),
            edge(3, Family::Track, 100.0, 120.0, 10.0),
        ];
        let found = find_structures(&edges, &pieces(RAIL_TUNNEL, -4.0, 104.0, 2.5, 10.5));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found.get(&2), Some(&Structure::Tunnel));
    }

    #[test]
    fn a_road_over_a_rail_tunnel_is_not_a_tunnel() {
        // Same height, same line: only the family keeps it out.
        let edges = [
            edge(1, Family::Street, 0.0, 100.0, 10.0),
            edge(2, Family::Track, 0.0, 100.0, 10.0),
        ];
        let found = find_structures(&edges, &pieces(RAIL_TUNNEL, 0.0, 100.0, 0.0, 10.0));
        assert_eq!(found, HashMap::from([(2, Structure::Tunnel)]));
        let found = find_structures(&edges, &pieces(CAR_TUNNEL, 0.0, 100.0, 0.0, 10.0));
        assert_eq!(found, HashMap::from([(1, Structure::Tunnel)]));
    }

    #[test]
    fn a_track_above_a_tunnel_is_not_a_tunnel() {
        let edges = [edge(1, Family::Track, 0.0, 100.0, 40.0)];
        assert!(find_structures(&edges, &pieces(RAIL_TUNNEL, 0.0, 100.0, 0.0, 10.0)).is_empty());
    }

    #[test]
    fn pieces_beside_the_edge_do_not_count() {
        // 2 m half width + 3 m margin: 6 m off is out.
        let edges = [edge(1, Family::Track, 0.0, 100.0, 10.0)];
        assert!(find_structures(&edges, &pieces(STONE, 0.0, 100.0, 6.0, 10.0)).is_empty());
        let found = find_structures(&edges, &pieces(STONE, 0.0, 100.0, 4.5, 10.0));
        assert_eq!(found.get(&1), Some(&Structure::Bridge));
    }

    #[test]
    fn a_bridge_is_judged_on_the_straight_height_line() {
        // Steep z tangents would lift the curve's middle 25 m above the
        // pieces, which sit on the line between the end heights.
        let mut e = edge(1, Family::Street, 0.0, 200.0, 0.0);
        e.p1[2] = 10.0;
        e.t0[2] = 100.0;
        e.t1[2] = -100.0;
        let mut ps = pieces(STONE, 0.0, 200.0, 0.0, 0.0);
        for p in &mut ps {
            p.z = p.x / 20.0;
        }
        assert_eq!(
            find_structures(&[e], &ps),
            HashMap::from([(1, Structure::Bridge)])
        );
    }

    #[test]
    fn a_short_approach_with_an_abutment_on_its_end_stays_on_the_ground() {
        // 15 m edge, abutment pieces from 12 m on: with the 5 m reach they
        // cover its last 8 m, 5 of its 9 samples (as the saves show, 0.6).
        let edges = [edge(1, Family::Street, 0.0, 15.0, 5.0)];
        assert!(find_structures(&edges, &pieces(STONE, 12.0, 15.0, 0.0, 5.0)).is_empty());
    }

    #[test]
    fn other_models_are_ignored() {
        let edges = [edge(1, Family::Track, 0.0, 100.0, 10.0)];
        let ps = pieces("industries/farm/farm_sm.mdl", 0.0, 100.0, 0.0, 10.0);
        assert!(find_structures(&edges, &ps).is_empty());
    }
}
