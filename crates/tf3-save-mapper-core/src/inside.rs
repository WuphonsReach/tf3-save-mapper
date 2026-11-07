//! The filter for an industry's own roads and tracks inside its footprint.
//!
//! An industry lays out its own sidings (`industry` track) and access roads
//! with the rest of the network, and they are in the edge table. Inside the
//! industry they are part of its drawing in the game, not the player's
//! network, and the map shows the industry's footprint there instead. An edge
//! of a type an industry builds is left out when the whole of it lies inside
//! one footprint. The entrance road that crosses the footprint's edge to
//! meet the network stays, and so does everything near an industry with no
//! footprint (one whose script has no ground face and that has no measured
//! one).
//!
//! Across 22 saves every live `industry` track edge drawn lay wholly inside
//! a footprint, and no entrance road did.

use std::collections::HashMap;

use crate::edges::Edge;
use crate::geometry::{bounds, in_polygon};
use crate::install::Polygon;
use crate::instances::Instance;
use crate::loose::is_industry_built;

/// The points tested along an edge, ends included.
const SAMPLES: usize = 11;

/// Each industry's footprint in map metres: its ground face by industry key
/// (`faces`), placed from its main model's origin and facing.
pub fn industry_footprints(
    industries: &[(Instance, String)],
    faces: &HashMap<String, Polygon>,
) -> Vec<Polygon> {
    industries
        .iter()
        .filter_map(|(i, key)| {
            let face = faces.get(key)?;
            Some(face.iter().map(|&(a, b)| i.place(a, b)).collect())
        })
        .collect()
}

/// Leave out each edge of an industry-built type whose centre line lies
/// wholly inside one of `footprints` (map metres), tested at its ends and
/// at evenly spaced points between. Returns how many were left out.
pub fn retain_outside_footprints(edges: &mut Vec<Edge>, footprints: &[Polygon]) -> usize {
    let boxes: Vec<(f64, f64, f64, f64)> = footprints.iter().map(|f| bounds(f)).collect();
    let before = edges.len();
    edges.retain(|e| {
        if !is_industry_built(e.type_name()) {
            return true;
        }
        let pts = samples(e);
        let (x0, x1, y0, y1) = bounds(&pts);
        !footprints.iter().zip(&boxes).any(|(f, b)| {
            b.0 <= x0
                && x1 <= b.1
                && b.2 <= y0
                && y1 <= b.3
                && pts.iter().all(|&(x, y)| in_polygon(f, x, y))
        })
    });
    before - edges.len()
}

/// `SAMPLES` points along the edge's centre line, from its start to its end.
fn samples(e: &Edge) -> Vec<(f64, f64)> {
    let c = e.bezier();
    (0..SAMPLES)
        .map(|n| {
            let t = n as f64 / (SAMPLES - 1) as f64;
            let u = 1.0 - t;
            let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (
                w0 * c[0].0 + w1 * c[1].0 + w2 * c[2].0 + w3 * c[3].0,
                w0 * c[0].1 + w1 * c[1].1 + w2 * c[2].1 + w3 * c[3].1,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::Family;

    const SIDING: &str = "industries/shared/industry_track/industry";
    const ENTRANCE: &str = "street/constructions/ind_entrance_old_country";

    fn edge(template: &str, p0: (f32, f32), p1: (f32, f32), bend: f32) -> Edge {
        let d = [p1.0 - p0.0, p1.1 - p0.1, 0.0];
        Edge {
            a: 1,
            b: 2,
            p0: [p0.0, p0.1, 0.0],
            p1: [p1.0, p1.1, 0.0],
            // A bend to one side: both tangents turned the same way.
            t0: [d[0], d[1] + bend, 0.0],
            t1: [d[0], d[1] - bend, 0.0],
            template: Some(template.into()),
            family: Family::Track,
            width: Some(4.0),
            offset: 0,
        }
    }

    #[test]
    fn a_footprint_is_its_face_placed_by_its_main_model() {
        let industry = Instance {
            slot: 1,
            model: "industries/farm/farm_sm.mdl".into(),
            x: 100.0,
            y: 50.0,
            fx: 0.0,
            fy: 1.0,
        };
        let faces = HashMap::from([(
            "farm".to_owned(),
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 4.0)],
        )]);
        let other = (industry.clone(), "quarry".to_owned());
        let prints = industry_footprints(&[(industry, "farm".into()), other], &faces);
        assert_eq!(prints, [vec![(100.0, 50.0), (100.0, 60.0), (96.0, 60.0)]]);
    }

    #[test]
    fn only_industry_edges_wholly_inside_a_footprint_go() {
        // An L-shaped footprint: its notch is outside.
        let footprint = vec![
            (0.0, 0.0),
            (200.0, 0.0),
            (200.0, 100.0),
            (100.0, 100.0),
            (100.0, 200.0),
            (0.0, 200.0),
        ];
        let mut edges = vec![
            edge(SIDING, (20.0, 20.0), (180.0, 20.0), 0.0),
            edge(SIDING, (20.0, 180.0), (20.0, 40.0), 0.0),
            // Both ends inside, but its chord crosses the notch.
            edge(SIDING, (180.0, 80.0), (80.0, 180.0), 0.0),
            // Both ends inside, but it bends out past the edge.
            edge(SIDING, (20.0, 10.0), (180.0, 10.0), -150.0),
            // The entrance road reaches out to the network.
            edge(ENTRANCE, (50.0, 50.0), (50.0, -40.0), 0.0),
            // A player's road inside stays.
            edge(
                "street/town/town_old_small",
                (30.0, 30.0),
                (60.0, 30.0),
                0.0,
            ),
        ];
        assert_eq!(retain_outside_footprints(&mut edges, &[footprint]), 2);
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0].p0[..2], [180.0, 80.0]);
        assert_eq!(edges[1].p0[..2], [20.0, 10.0]);
        // No footprints: nothing goes.
        let mut all = vec![edge(SIDING, (20.0, 20.0), (180.0, 20.0), 0.0)];
        assert_eq!(retain_outside_footprints(&mut all, &[]), 0);
    }
}
