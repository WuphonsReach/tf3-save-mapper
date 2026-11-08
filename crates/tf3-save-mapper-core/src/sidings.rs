//! The filter for the industries' own sidings.
//!
//! An industry lays out its own sidings (`industry` track) with the rest of
//! the network, and they are in the edge table. They are part of the
//! industry's drawing in the game, not the player's network, and the map
//! shows the industry's footprint there instead. Every siding is left out,
//! footprint or not: a player cannot build `industry` track, and across 22
//! saves every live siding drawn lay wholly inside a footprint. The entrance
//! road that reaches out to the network stays.

use crate::edges::{Edge, Family};

/// The track type an industry lays for its sidings.
const SIDING: &str = "industry";

/// Leave out every siding (`industry` track). Returns how many.
pub fn retain_without_sidings(edges: &mut Vec<Edge>) -> usize {
    let before = edges.len();
    edges.retain(|e| !(e.family == Family::Track && e.type_name() == SIDING));
    before - edges.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(template: &str, family: Family) -> Edge {
        Edge {
            a: 1,
            b: 2,
            p0: [0.0; 3],
            p1: [100.0, 0.0, 0.0],
            t0: [100.0, 0.0, 0.0],
            t1: [100.0, 0.0, 0.0],
            template: Some(template.into()),
            family,
            width: Some(4.0),
            modes: None,
            offset: 0,
        }
    }

    #[test]
    fn every_siding_goes_and_nothing_else() {
        let mut edges = vec![
            edge("industries/shared/industry_track/industry", Family::Track),
            edge("industries/shared/industry_track/industry", Family::Track),
            edge(
                "street/constructions/ind_entrance_old_country",
                Family::Street,
            ),
            edge("track/standard", Family::Track),
            edge("street/town/town_old_small", Family::Street),
        ];
        assert_eq!(retain_without_sidings(&mut edges), 2);
        let left: Vec<_> = edges.iter().map(|e| e.type_name()).collect();
        assert_eq!(
            left,
            ["ind_entrance_old_country", "standard", "town_old_small"]
        );
    }
}
