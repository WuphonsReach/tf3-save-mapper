//! The `coverage` layer, under the stations and the network: one dashed
//! circle per maintenance building and depot at its script's catchment
//! radius, centred on the instance's origin, a sublayer per kind
//! (`coverage-` and the depot layer's id). The circle's ink is the kind's
//! fill at half its brightness, with a faint fill of the same; its line and
//! dashes are a fixed number of pixels, so they keep their look at any scale.
//! A kind whose radius did not read, or a mod's whose folder was not found,
//! gets no circle. A ship depot's ring in the game is not always on its
//! origin (`docs/depots.md` in tf3-save-format); this draws it there.

use std::collections::HashMap;

use super::depots::data_attrs;
use super::{Picture, fmt_g, fmt2, layer};
use crate::install::{DEPOT_KINDS, DepotSpec};
use crate::instances::Instance;

/// The circle's line width and its dash and gap, in pixels.
const LINE_PX: f64 = 1.5;
const DASH_PX: (f64, f64) = (10.0, 6.0);
/// The fill's opacity.
const FILL_OPACITY: &str = "0.06";

/// Metres from an instance's origin within which its circle may show: its
/// kind's radius, or 0 for a model that gets no circle.
pub(super) fn reach(model: &str, specs: &HashMap<&'static str, DepotSpec>) -> f64 {
    DEPOT_KINDS
        .iter()
        .find(|k| k.model == model)
        .and_then(|k| specs.get(k.id)?.radius)
        .map_or(0.0, f64::from)
}

/// `#rrggbb` with each channel halved, rounded down.
fn ink(fill: &str) -> String {
    let channel = |at: usize| u8::from_str_radix(&fill[at..at + 2], 16).unwrap_or(0) / 2;
    format!("#{:02x}{:02x}{:02x}", channel(1), channel(3), channel(5))
}

pub(super) fn layers(
    picture: &Picture,
    instances: &[Instance],
    specs: &HashMap<&'static str, DepotSpec>,
    metres_per_pixel: f64,
) -> Vec<String> {
    let line = fmt_g(LINE_PX * metres_per_pixel);
    let (dash, gap) = (
        fmt_g(DASH_PX.0 * metres_per_pixel),
        fmt_g(DASH_PX.1 * metres_per_pixel),
    );
    let mut out = Vec::new();
    for kind in DEPOT_KINDS {
        let Some(spec) = specs.get(kind.id) else {
            continue;
        };
        let Some(radius) = spec.radius else {
            continue;
        };
        let rows: Vec<&Instance> = instances.iter().filter(|i| i.model == kind.model).collect();
        if rows.is_empty() {
            continue;
        }
        let ink = ink(kind.fill);
        out.push(layer(
            &format!("coverage-{}", kind.id),
            &format!("Coverage: {}", kind.label),
            "",
        ));
        for i in rows {
            let (cx, cy) = picture.pic((i.x, i.y));
            out.push(format!(
                "<g id=\"coverage-{}\" class=\"coverage {}\"{}><circle cx=\"{}\" cy=\"{}\" r=\"{radius}\" \
                 fill=\"{ink}\" fill-opacity=\"{FILL_OPACITY}\" stroke=\"{ink}\" stroke-width=\"{line}\" \
                 stroke-dasharray=\"{dash} {gap}\"/></g>",
                i.slot,
                kind.id,
                data_attrs(kind, i, Some(spec)),
                fmt2(cx),
                fmt2(cy)
            ));
        }
        out.push("</g>".into());
    }
    if out.is_empty() {
        return out;
    }
    out.insert(0, layer("coverage", "Coverage", " pointer-events=\"none\""));
    out.push("</g>".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::North;

    fn at(model: &str, slot: usize, x: f64, y: f64) -> Instance {
        Instance {
            slot,
            model: model.into(),
            x,
            y,
            fx: 1.0,
            fy: 0.0,
        }
    }

    fn spec(radius: Option<u32>) -> DepotSpec {
        DepotSpec {
            face: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
            boxes: vec![],
            radius,
            pool: 16,
            modes: "BUS,TRUCK,TRAM".into(),
        }
    }

    #[test]
    fn ink_is_the_fill_at_half_brightness() {
        assert_eq!(ink("#969696"), "#4b4b4b");
        assert_eq!(ink("#7b4a24"), "#3d2512");
        assert_eq!(ink("#f2f2f2"), "#797979");
    }

    #[test]
    fn a_circle_per_building_with_a_radius() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: North::Game,
        };
        let maint = "depots/road/road_maint_station/road_maint_station.mdl";
        let depot = "depots/road/road_depot/road_depot.mdl";
        let specs = HashMap::from([
            ("maintenance-buildings", spec(Some(600))),
            ("road-depots", spec(None)),
        ]);
        let rows = [
            at(maint, 3, 100.0, -50.0),
            at(depot, 4, 0.0, 0.0),
            at(
                "assets/ingo_bootsschuppen/ingo_bootsschuppen.mdl",
                5,
                0.0,
                0.0,
            ),
        ];
        let text = layers(&picture, &rows, &specs, 8.0).join("\n");
        assert!(text.starts_with("<g id=\"coverage\""), "{text}");
        assert!(text.contains("id=\"coverage-maintenance-buildings\""));
        assert!(
            text.contains(
                "<g id=\"coverage-3\" class=\"coverage maintenance-buildings\" data-slot=\"3\" \
                 data-model=\"depots/road/road_maint_station/road_maint_station.mdl\" \
                 data-x=\"100.0\" data-y=\"-50.0\" data-pool=\"16\" data-radius=\"600\" \
                 data-modes=\"BUS,TRUCK,TRAM\"><circle cx=\"600.00\" cy=\"550.00\" r=\"600\" \
                 fill=\"#4b4b4b\" fill-opacity=\"0.06\" stroke=\"#4b4b4b\" stroke-width=\"12\" \
                 stroke-dasharray=\"80 48\"/></g>"
            ),
            "{text}"
        );
        // No radius, or no spec (a mod's building whose folder was not found): no circle.
        assert_eq!(text.matches("<circle").count(), 1);
        assert!(!text.contains("coverage-road-depots"));
        assert!(!text.contains("coverage-boathouses"));
        assert_eq!(reach(maint, &specs), 600.0);
        assert_eq!(reach(depot, &specs), 0.0);

        // Nothing to draw: no layer at all.
        assert!(layers(&picture, &rows[1..], &specs, 8.0).is_empty());
    }
}
