//! Loose industry pieces: what a removed industry leaves of its road and
//! sidings when the game keeps their lanes, so that neither the free-id list
//! nor the lane table drops them (`docs/gaps.md`).
//!
//! The pieces are found by shape. A group of edges joined by their nodes and
//! joined to nothing else is loose when it is short, is built only of the
//! types industries build, and lies away from every industry. Each standing
//! industry has such a group too, its sidings and the frontage road before
//! its entrance, but always within reach of its own models.
//!
//! Distances are to any model under `industries/`, not only an industry's
//! main model: a mod's industry can be built from the base game's pieces
//! without one (two military depots of a 1917 game, made of saw mill and
//! brick works buildings).

use std::collections::HashMap;

use crate::edges::Edge;

/// Groups at least this long, metres, are kept whatever they are made of.
pub const LOOSE_MAX_LENGTH: f64 = 1500.0;

/// A group with an edge's midpoint within this many metres of an industry's
/// model is kept. In ten saves of one game and ten catalog saves, standing
/// industries' own groups were within 139 m of the industry, and the other
/// small groups of these types at least 259 m from any industry model, but
/// for one catalog pair at 146 m.
pub const INDUSTRY_REACH: f64 = 200.0;

/// Whether industries build this street type: the industry track, the
/// `ind_` frontage roads and entrances, and the small country road of an
/// access road. A larger country road is the player's or a town's.
pub fn is_industry_built(type_name: &str) -> bool {
    type_name == "industry"
        || type_name.starts_with("ind_")
        || (type_name.starts_with("country_") && type_name.ends_with("_small"))
}

/// Drop the loose industry pieces: each group of edges joined by their end
/// nodes whose types are all [`is_industry_built`], whose lengths add up to
/// less than [`LOOSE_MAX_LENGTH`] and whose every edge's midpoint is more
/// than [`INDUSTRY_REACH`] from each of `industry_models` (x, y of every
/// model instance under `industries/`). Returns how many edges were dropped.
pub fn retain_near_industries(edges: &mut Vec<Edge>, industry_models: &[(f64, f64)]) -> usize {
    let mut parent: Vec<usize> = Vec::new();
    let mut node: HashMap<u32, usize> = HashMap::new();
    let mut slot = |id: u32, parent: &mut Vec<usize>| {
        *node.entry(id).or_insert_with(|| {
            parent.push(parent.len());
            parent.len() - 1
        })
    };
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let ends: Vec<(usize, usize)> = edges
        .iter()
        .map(|e| (slot(e.a, &mut parent), slot(e.b, &mut parent)))
        .collect();
    for &(a, b) in &ends {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        parent[ra] = rb;
    }

    struct Group {
        industry_built: bool,
        length: f64,
        edges: Vec<usize>,
    }
    let mut groups: HashMap<usize, Group> = HashMap::new();
    for (k, (e, &(a, _))) in edges.iter().zip(&ends).enumerate() {
        let g = groups.entry(root(&mut parent, a)).or_insert(Group {
            industry_built: true,
            length: 0.0,
            edges: Vec::new(),
        });
        g.industry_built &= is_industry_built(e.type_name());
        g.length += e.length();
        g.edges.push(k);
    }

    let near = |e: &Edge| {
        let x = (f64::from(e.p0[0]) + f64::from(e.p1[0])) / 2.0;
        let y = (f64::from(e.p0[1]) + f64::from(e.p1[1])) / 2.0;
        industry_models
            .iter()
            .any(|&(mx, my)| (mx - x).hypot(my - y) <= INDUSTRY_REACH)
    };
    let mut drop = vec![false; edges.len()];
    for g in groups.values() {
        if g.industry_built
            && g.length < LOOSE_MAX_LENGTH
            && !g.edges.iter().any(|&k| near(&edges[k]))
        {
            for &k in &g.edges {
                drop[k] = true;
            }
        }
    }
    let before = edges.len();
    let mut k = 0;
    edges.retain(|_| {
        k += 1;
        !drop[k - 1]
    });
    before - edges.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::Family;

    fn edge(a: u32, b: u32, from: (f32, f32), to: (f32, f32), template: &str) -> Edge {
        Edge {
            a,
            b,
            p0: [from.0, from.1, 0.0],
            p1: [to.0, to.1, 0.0],
            t0: [0.0; 3],
            t1: [0.0; 3],
            template: Some(template.into()),
            family: Family::Street,
            width: Some(8.0),
            offset: 0,
        }
    }

    const FRONTAGE: &str = "street/country/ind_country_old_small";
    const ENTRANCE: &str = "street/constructions/ind_entrance_old_country";
    const SIDING: &str = "industries/shared/industry_track/industry";

    /// An entrance stub at `x`: two frontage pieces and the entrance between.
    fn stub(first: u32, x: f32) -> Vec<Edge> {
        vec![
            edge(first, first + 1, (x, 0.0), (x + 60.0, 0.0), FRONTAGE),
            edge(
                first + 1,
                first + 2,
                (x + 60.0, 0.0),
                (x + 120.0, 0.0),
                FRONTAGE,
            ),
            edge(
                first + 1,
                first + 3,
                (x + 60.0, 0.0),
                (x + 60.0, 12.0),
                ENTRANCE,
            ),
        ]
    }

    #[test]
    fn industry_types() {
        for t in [
            "industry",
            "ind_country_old_small",
            "ind_entrance_new_country",
            "country_old_small",
            "country_new_small",
        ] {
            assert!(is_industry_built(t), "{t}");
        }
        for t in [
            "country_new_medium",
            "country_old_large",
            "town_old_small",
            "standard",
        ] {
            assert!(!is_industry_built(t), "{t}");
        }
    }

    #[test]
    fn a_stub_by_an_industry_stays_and_one_far_from_all_goes() {
        let mut edges = stub(10, 0.0);
        edges.extend(stub(20, 1000.0));
        // An industry made only of pieces (no main model) 40 m off the first.
        let models = [(60.0, 40.0), (80.0, 60.0)];
        assert_eq!(retain_near_industries(&mut edges, &models), 3);
        assert!(edges.iter().all(|e| e.a < 20));
    }

    #[test]
    fn one_edge_in_reach_keeps_its_group() {
        let mut edges = vec![
            edge(1, 2, (0.0, 0.0), (100.0, 0.0), SIDING),
            edge(2, 3, (100.0, 0.0), (400.0, 0.0), SIDING),
        ];
        assert_eq!(retain_near_industries(&mut edges, &[(50.0, 190.0)]), 0);
        assert_eq!(retain_near_industries(&mut edges, &[(50.0, 210.0)]), 2);
    }

    #[test]
    fn joined_long_or_other_groups_stay() {
        // A loose siding joined to a town street, a 1.6 km run of small country
        // road, a lone piece of the player's medium country road.
        let mut edges = vec![
            edge(1, 2, (0.0, 0.0), (80.0, 0.0), SIDING),
            edge(
                2,
                3,
                (80.0, 0.0),
                (180.0, 0.0),
                "street/town/town_old_small",
            ),
        ];
        for k in 0..4 {
            let x = 400.0 * k as f32;
            edges.push(edge(
                10 + k,
                11 + k,
                (x, 900.0),
                (x + 400.0, 900.0),
                "street/country/country_old_small",
            ));
        }
        edges.push(edge(
            30,
            31,
            (0.0, -900.0),
            (90.0, -900.0),
            "street/country/country_new_medium",
        ));
        assert_eq!(retain_near_industries(&mut edges, &[]), 0);
        // Under 1500 m the country run goes.
        edges.truncate(5);
        edges.push(edge(
            30,
            31,
            (0.0, -900.0),
            (90.0, -900.0),
            "street/country/country_new_medium",
        ));
        assert_eq!(retain_near_industries(&mut edges, &[]), 3);
        assert_eq!(edges.len(), 3);
    }
}
