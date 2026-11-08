//! The depot layers, one per kind in `install::DEPOT_KINDS`: each building's
//! ground face from its script (the road depot's lot, our own outline, in
//! place of its face), placed from the instance's origin and
//! facing, opaque (so it hides the ends of the road lanes under it) with the
//! stations' black outline and no marks inside. A ship depot that reaches
//! into the water is the outline of its face and collider boxes together. A
//! kind with no face (the Boathouse mod's, whose script is read from the
//! mods folder when it is found, and which is drawn without it when not) is
//! a dot on its origin. Only the exact model of each kind is drawn, so a
//! depot's `_sm` twin is not.
//!
//! A mod's depot built from pieces (the roundhouse, the modern depot) is
//! drawn from its pieces' boxes, read from the mods folder, each placed from
//! its own instance, and a turntable's ring pieces as one disc. They are
//! filled like a station's pieces, a same-colour edge hiding the seams, with
//! the black outline only where no other piece lies beyond an edge. Each
//! piece is grouped under the depot whose centre piece is nearest. Without
//! the boxes, the centre piece is a dot.

use std::collections::{HashMap, HashSet};

use super::{Cut, OUTLINE, Picture, escape, fmt1, fmt2, layer, loops_d, points_attr, runs_d};
use crate::geometry::{bounds, closed_outline, outer_runs};
use crate::install::{DEPOT_KINDS, DepotKind, DepotSpec, PieceRole, Rect};
use crate::instances::Instance;

/// Metres: the radius of the dot for a building with no face.
const DOT: f64 = 4.0;
/// Metres: a piece is grouped under the nearest centre piece within this.
const PIECE_REACH: f64 = 400.0;
/// The sides of a ring's disc.
const RING_SIDES: usize = 64;

/// Each piece and ring of `kind` among `instances` whose box is known, as
/// its corners in picture metres; a ring once per origin, as a disc out to
/// the far end of its box along its own axis.
fn piece_shapes<'a>(
    picture: &Picture,
    kind: &DepotKind,
    instances: &'a [Instance],
    boxes: &HashMap<String, Rect>,
) -> Vec<(&'a Instance, Vec<(f64, f64)>)> {
    let mut rings: HashSet<(i64, i64)> = HashSet::new();
    let mut out = Vec::new();
    for i in instances {
        let (Some(role), Some(&(x0, y0, x1, y1))) = (kind.role(&i.model), boxes.get(&i.model))
        else {
            continue;
        };
        let local = |a: f64, b: f64| picture.pic(i.place(a, b));
        match role {
            PieceRole::Piece => out.push((
                i,
                vec![local(x0, y0), local(x1, y0), local(x1, y1), local(x0, y1)],
            )),
            PieceRole::Ring => {
                let at = ((i.x * 10.0).round() as i64, (i.y * 10.0).round() as i64);
                if !rings.insert(at) {
                    continue;
                }
                let r = x0.abs().max(x1.abs());
                let (cx, cy) = picture.pic((i.x, i.y));
                let disc = (0..RING_SIDES)
                    .map(|n| {
                        let t = std::f64::consts::TAU * n as f64 / RING_SIDES as f64;
                        (cx + r * t.cos(), cy + r * t.sin())
                    })
                    .collect();
                out.push((i, disc));
            }
        }
    }
    out
}

/// The lines of a kind drawn from its pieces: under each centre piece's
/// group the pieces nearest it, then those near none, then the outline. A
/// centre piece with no pieces near it is a dot.
fn pieces_layer(
    picture: &Picture,
    kind: &DepotKind,
    instances: &[Instance],
    shapes: &[(&Instance, Vec<(f64, f64)>)],
    spec: Option<&DepotSpec>,
    cut: Cut,
) -> Vec<String> {
    let centres: Vec<&Instance> = instances.iter().filter(|i| i.model == kind.model).collect();
    let mut owned: Vec<Vec<usize>> = vec![Vec::new(); centres.len()];
    let mut loose: Vec<usize> = Vec::new();
    for (n, (i, _)) in shapes.iter().enumerate() {
        let nearest = centres
            .iter()
            .enumerate()
            .map(|(c, a)| (c, (a.x - i.x).hypot(a.y - i.y)))
            .filter(|&(_, d)| d <= PIECE_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((c, _)) => owned[c].push(n),
            None => loose.push(n),
        }
    }
    let fill = kind.fill;
    let shown = |n: &usize| cut.meets(&shapes[*n].1, OUTLINE / 2.0);
    let piece = |n: usize| {
        let (i, pts) = &shapes[n];
        format!(
            "<g class=\"depot-piece\" data-slot=\"{}\" data-model=\"{}\" data-x=\"{}\" data-y=\"{}\">\
             <polygon points=\"{}\" fill=\"{fill}\" stroke=\"{fill}\" stroke-width=\"0.5\"/></g>",
            i.slot,
            escape(&i.model),
            fmt1(i.x),
            fmt1(i.y),
            points_attr(pts)
        )
    };
    let mut body = Vec::new();
    for (c, centre) in centres.iter().enumerate() {
        let open = format!(
            "<g id=\"depot-{}\" class=\"depot {}\"{}>",
            centre.slot,
            kind.id,
            data_attrs(kind, centre, spec)
        );
        if owned[c].is_empty() {
            let dot = drawn(picture, centre, None);
            if cut.meets(&dot, OUTLINE / 2.0) {
                let (cx, cy) = picture.pic((centre.x, centre.y));
                body.push(open);
                body.push(format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{DOT}\" fill=\"{fill}\" stroke=\"#000\" stroke-width=\"{OUTLINE}\"/>",
                    fmt2(cx),
                    fmt2(cy)
                ));
                body.push("</g>".into());
            }
            continue;
        }
        let rows: Vec<usize> = owned[c].iter().copied().filter(shown).collect();
        if !rows.is_empty() {
            body.push(open);
            body.extend(rows.into_iter().map(piece));
            body.push("</g>".into());
        }
    }
    let rows: Vec<usize> = loose.into_iter().filter(shown).collect();
    if !rows.is_empty() {
        body.push(format!("<g class=\"depot {}\">", kind.id));
        body.extend(rows.into_iter().map(piece));
        body.push("</g>".into());
    }
    let polys: Vec<Vec<(f64, f64)>> = shapes.iter().map(|(_, p)| p.clone()).collect();
    let runs: Vec<Vec<(f64, f64)>> = outer_runs(&polys)
        .into_iter()
        .filter(|r| cut.meets(r, OUTLINE / 2.0))
        .collect();
    if !runs.is_empty() {
        body.push(format!(
            "<path class=\"depot-outer\" d=\"{}\" fill=\"none\" stroke=\"#000\" stroke-width=\"{OUTLINE}\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            runs_d(&runs)
        ));
    }
    if body.is_empty() {
        return body;
    }
    body.insert(0, layer(kind.id, kind.label, " pointer-events=\"none\""));
    body.push("</g>".into());
    body
}

/// The corners of what is drawn for a building, picture metres: its face and
/// collider boxes, or the box round its dot.
fn drawn(picture: &Picture, i: &Instance, spec: Option<&DepotSpec>) -> Vec<(f64, f64)> {
    let local = |a: f64, b: f64| picture.pic(i.place(a, b));
    match spec {
        Some(s) if !s.boxes.is_empty() => std::iter::once(bounds(&s.face))
            .chain(s.boxes.iter().copied())
            .flat_map(|(x0, x1, y0, y1)| {
                [local(x0, y0), local(x1, y0), local(x1, y1), local(x0, y1)]
            })
            .collect(),
        Some(s) if !s.face.is_empty() => s.face.iter().map(|&(a, b)| local(a, b)).collect(),
        _ => {
            let (cx, cy) = local(0.0, 0.0);
            vec![(cx - DOT, cy - DOT), (cx + DOT, cy + DOT)]
        }
    }
}

pub(super) fn layers(
    picture: &Picture,
    instances: &[Instance],
    specs: &HashMap<&'static str, DepotSpec>,
    boxes: &HashMap<String, Rect>,
    cut: Cut,
) -> Vec<String> {
    let mut out = Vec::new();
    for kind in DEPOT_KINDS {
        let spec = specs.get(kind.id);
        if kind.archive.is_some() && spec.is_none() {
            continue;
        }
        if kind.has_pieces() {
            let shapes = piece_shapes(picture, kind, instances, boxes);
            if !shapes.is_empty() {
                out.extend(pieces_layer(picture, kind, instances, &shapes, spec, cut));
                continue;
            }
        }
        let rows: Vec<&Instance> = instances
            .iter()
            .filter(|i| i.model == kind.model && cut.meets(&drawn(picture, i, spec), OUTLINE / 2.0))
            .collect();
        if rows.is_empty() {
            continue;
        }
        out.push(layer(kind.id, kind.label, " pointer-events=\"none\""));
        for i in rows {
            out.push(format!(
                "<g id=\"depot-{}\" class=\"depot {}\"{}>",
                i.slot,
                kind.id,
                data_attrs(kind, i, spec)
            ));
            let local = |a: f64, b: f64| picture.pic(i.place(a, b));
            let fill = kind.fill;
            match spec {
                Some(s) if !s.boxes.is_empty() => {
                    let parts: Vec<Vec<(f64, f64)>> = std::iter::once(bounds(&s.face))
                        .chain(s.boxes.iter().copied())
                        .map(|(x0, x1, y0, y1)| {
                            vec![local(x0, y0), local(x1, y0), local(x1, y1), local(x0, y1)]
                        })
                        .collect();
                    out.push(format!(
                        "<path d=\"{}\" fill=\"{fill}\" stroke=\"#000\" stroke-width=\"{OUTLINE}\" stroke-linejoin=\"round\"/>",
                        loops_d(&closed_outline(&parts, 0.05))
                    ));
                }
                Some(s) if !s.face.is_empty() => {
                    let pts: Vec<(f64, f64)> = s.face.iter().map(|&(a, b)| local(a, b)).collect();
                    out.push(format!(
                        "<polygon points=\"{}\" fill=\"{fill}\" stroke=\"#000\" stroke-width=\"{OUTLINE}\" stroke-linejoin=\"round\"/>",
                        points_attr(&pts)
                    ));
                }
                _ => {
                    let (cx, cy) = local(0.0, 0.0);
                    out.push(format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{DOT}\" fill=\"{fill}\" stroke=\"#000\" stroke-width=\"{OUTLINE}\"/>",
                        fmt2(cx),
                        fmt2(cy)
                    ));
                }
            }
            out.push("</g>".into());
        }
        out.push("</g>".into());
    }
    out
}

/// ` data-slot data-model data-x data-y`, and with the script's spec
/// ` data-pool data-radius data-modes` (the radius only if it read).
pub(super) fn data_attrs(kind: &DepotKind, i: &Instance, spec: Option<&DepotSpec>) -> String {
    let mut attrs = format!(
        " data-slot=\"{}\" data-model=\"{}\" data-x=\"{}\" data-y=\"{}\"",
        i.slot,
        escape(kind.model),
        fmt1(i.x),
        fmt1(i.y)
    );
    if let Some(s) = spec {
        attrs += &format!(" data-pool=\"{}\"", s.pool);
        if let Some(r) = s.radius {
            attrs += &format!(" data-radius=\"{r}\"");
        }
        attrs += &format!(" data-modes=\"{}\"", escape(&s.modes));
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depot(model: &str) -> Instance {
        Instance {
            slot: 7,
            model: model.into(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            fx: 0.0,
            fy: 1.0,
        }
    }

    #[test]
    fn a_road_depot_is_its_face_turned_by_its_facing() {
        let picture = Picture {
            width: 200.0,
            height: 200.0,
            north: crate::svg::North::Game,
        };
        let specs = HashMap::from([(
            "road-depots",
            DepotSpec {
                face: vec![(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)],
                boxes: vec![],
                radius: Some(400),
                pool: 12,
                modes: "BUS,TRUCK".into(),
            },
        )]);
        let rows = [
            depot("depots/road/road_depot/road_depot.mdl"),
            depot("depots/road/road_depot/road_depot_sm.mdl"),
            depot("assets/ingo_bootsschuppen/ingo_bootsschuppen.mdl"),
        ];
        let text = layers(&picture, &rows, &specs, &HashMap::new(), Cut(None)).join("\n");
        // Facing north: the building's a axis runs up the picture, its b axis to the west.
        assert!(
            text.contains("points=\"100.00,100.00 100.00,80.00 90.00,80.00 90.00,100.00\""),
            "{text}"
        );
        assert_eq!(
            text.matches("<polygon").count(),
            1,
            "the _sm twin is not drawn"
        );
        assert!(text.contains("data-modes=\"BUS,TRUCK\""));
        // The boathouse needs no spec: a dot.
        assert!(text.contains("id=\"boathouses\""));
        assert!(text.contains("<circle cx=\"100.00\" cy=\"100.00\" r=\"4\""));
        // Kinds whose script did not read are left out.
        assert!(!text.contains("tram-depots"));
    }

    #[test]
    fn a_roundhouse_is_its_pieces_and_its_turntable_or_a_dot() {
        let picture = Picture {
            width: 2000.0,
            height: 2000.0,
            north: crate::svg::North::Game,
        };
        let folder = "depots/rail/ringlokschuppen/";
        let at = |slot: usize, name: &str, x: f64, fx: f64, fy: f64| Instance {
            slot,
            model: format!("{folder}{name}.mdl"),
            x,
            y: 0.0,
            z: 0.0,
            fx,
            fy,
        };
        let rows = [
            at(1, "mav2002_rl_wanne", 0.0, 1.0, 0.0),
            at(2, "mav2002_rl_seg", 0.0, 1.0, 0.0),
            at(3, "mav2002_rl_seg_track", 0.0, 0.0, 1.0),
            at(4, "mav2002_rl_building_m1", 0.0, 1.0, 0.0),
            at(5, "mav2002_rl_building_m1", 0.0, 0.98, 0.2),
            at(6, "mav2002_rl_doors_c", 0.0, 1.0, 0.0),
            // A second roundhouse whose pieces were not read, and a shed near no centre.
            at(7, "mav2002_rl_wanne", -600.0, 1.0, 0.0),
            at(8, "mav2002_rl_building_m1", 600.0, 1.0, 0.0),
        ];
        let boxes = HashMap::from([
            (rows[1].model.clone(), (13.0, -1.7, 17.6, 1.7)),
            (rows[2].model.clone(), (13.0, -1.7, 17.6, 1.7)),
            (rows[3].model.clone(), (28.7, -6.8, 69.0, 6.8)),
        ]);
        let text = layers(&picture, &rows, &HashMap::new(), &boxes, Cut(None)).join("\n");
        assert_eq!(text.matches("id=\"roundhouses\"").count(), 1, "{text}");
        // One disc for the two ring pieces on one origin, three sheds, no door.
        assert_eq!(text.matches("class=\"depot-piece\"").count(), 4, "{text}");
        let disc = text.split("data-slot=\"2\"").nth(1).unwrap();
        let points = disc
            .split("points=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert_eq!(points.split(' ').count(), RING_SIDES);
        assert!(points.starts_with("1017.60,1000.00 "), "{points}");
        assert!(!text.contains("data-slot=\"3\"") && !text.contains("doors_c"));
        let first = text.find("id=\"depot-1\"").unwrap();
        let loose = text.find("<g class=\"depot roundhouses\">").unwrap();
        assert!(first < loose && text[loose..].contains("data-slot=\"8\""));
        // The centre with no pieces is a dot; the outline is one path.
        assert!(text.contains("id=\"depot-7\""));
        assert_eq!(text.matches("<circle").count(), 1);
        assert_eq!(text.matches("class=\"depot-outer\"").count(), 1);
        // Without boxes every centre is a dot.
        let text = layers(&picture, &rows, &HashMap::new(), &HashMap::new(), Cut(None)).join("\n");
        assert_eq!(text.matches("<circle").count(), 2);
        assert!(!text.contains("depot-piece"));
    }
}
