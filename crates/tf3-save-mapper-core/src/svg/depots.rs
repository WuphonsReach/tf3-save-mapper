//! The depot layers, one per kind in `install::DEPOT_KINDS`: each building's
//! ground face from its script, placed from the instance's origin and
//! facing, opaque (so it hides the ends of the road lanes under it) with the
//! stations' black outline and no marks inside. A ship depot that reaches
//! into the water is the outline of its face and collider boxes together. A
//! kind with no face (the Boathouse mod's) is a dot on its origin. Only the
//! exact model of each kind is drawn, so a depot's `_sm` twin is not.

use std::collections::HashMap;

use super::{OUTLINE, Picture, escape, fmt1, fmt2, layer, loops_d, points_attr};
use crate::geometry::{bounds, closed_outline};
use crate::install::{DEPOT_KINDS, DepotSpec};
use crate::instances::Instance;

/// Metres from a building's origin within which it may show.
pub(super) const REACH: f64 = 40.0;
/// Metres: the radius of the dot for a building with no face.
const DOT: f64 = 4.0;

pub(super) fn layers(
    picture: &Picture,
    instances: &[Instance],
    specs: &HashMap<&'static str, DepotSpec>,
) -> Vec<String> {
    let mut out = Vec::new();
    for kind in DEPOT_KINDS {
        let spec = specs.get(kind.id);
        if kind.archive.is_some() && spec.is_none() {
            continue;
        }
        let rows: Vec<&Instance> = instances.iter().filter(|i| i.model == kind.model).collect();
        if rows.is_empty() {
            continue;
        }
        out.push(layer(kind.id, kind.label, " pointer-events=\"none\""));
        for i in rows {
            let mut attrs = format!(
                "id=\"depot-{}\" class=\"depot {}\" data-slot=\"{}\" data-model=\"{}\" data-x=\"{}\" data-y=\"{}\"",
                i.slot,
                kind.id,
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
            out.push(format!("<g {attrs}>"));
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
                Some(s) => {
                    let pts: Vec<(f64, f64)> = s.face.iter().map(|&(a, b)| local(a, b)).collect();
                    out.push(format!(
                        "<polygon points=\"{}\" fill=\"{fill}\" stroke=\"#000\" stroke-width=\"{OUTLINE}\" stroke-linejoin=\"round\"/>",
                        points_attr(&pts)
                    ));
                }
                None => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn depot(model: &str) -> Instance {
        Instance {
            slot: 7,
            model: model.into(),
            x: 0.0,
            y: 0.0,
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
        let text = layers(&picture, &rows, &specs).join("\n");
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
}
