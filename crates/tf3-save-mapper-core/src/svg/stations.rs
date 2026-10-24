//! The station layers: `stations` (water and rail, under the roads) and
//! `stations-street` (bus, tram and cargo stations, over the roads, so the
//! border hides the ends of the station's own lanes).
//!
//! Each placed piece is the box of its model's `boundingInfo` from the
//! install, turned by the instance's facing and moved to its origin, in a
//! sublayer per `install::STATION_KINDS` group. A group's outer edge is
//! drawn at the outline width, with no line where two boxes touch. A modular
//! street station also gets a light grey ground and a black border round the
//! union of its platform tiles and its centre, path and entrance pieces, with
//! gaps of up to 6 m closed. A water station's landing markers get an anchor,
//! small or large by the model, moved off the deck edge and turned so its
//! ring points at the deck.

use std::collections::HashMap;

use super::{OUTLINE, Picture, escape, fmt1, fmt2, layer, loops_d, points_attr, runs_d};
use crate::geometry::{clear_of_edge, closed_outline, outer_runs};
use crate::install::{Rect, STATION_KINDS, StationKind, is_street_border};
use crate::instances::Instance;

const STREET_BORDER_FILL: &str = "#e3e3e3";
/// Metres: gaps up to twice this between a street station's pieces are closed in its border.
const STREET_BORDER_CLOSE: f64 = 3.0;
const LANDING_INK: &str = "#1f1f1f";
/// Within this many metres of a deck edge a landing's anchor is moved off it.
const LANDING_REACH: f64 = 40.0;
const ANCHOR_SMALL: &str = include_str!("../../icons/anchor_small.svg");
const ANCHOR_LARGE: &str = include_str!("../../icons/anchor_large.svg");

/// A landing marker's size, from its model's name under `stations/water/models/`.
fn landing_size(model: &str) -> Option<&'static str> {
    let name = model
        .strip_prefix("stations/water/models/")?
        .strip_suffix(".mdl")?;
    match name {
        "tn_small_pier_passenger"
        | "tn_small_pier_passenger_l"
        | "tn_small_pier_passenger_r"
        | "tn_small_pier_passenger_dual"
        | "tn_small_pier_passenger_dual_l"
        | "tn_small_pier_passenger_dual_r" => Some("small"),
        "tn_medium_pier_passenger" | "tn_medium_pier_passenger_dual" => Some("large"),
        _ => None,
    }
}

/// Metres from a piece's origin within which it may show: its box's far
/// corner, or a landing's 1 m stand-in box.
pub(super) fn reach(model: &str, boxes: &HashMap<String, Rect>) -> f64 {
    let (x0, y0, x1, y1) = boxes.get(model).copied().unwrap_or((-1.0, -1.0, 1.0, 1.0));
    x0.abs().max(x1.abs()).hypot(y0.abs().max(y1.abs()))
}

/// Whether an instance is drawn on the station layers.
pub fn is_station_piece(model: &str, boxes: &HashMap<String, Rect>) -> bool {
    boxes.contains_key(model) || landing_size(model).is_some()
}

/// (metres across the icon, its centre's distance from the deck edge as a
/// share of that, the degrees its file turns the anchor).
fn landing_style(size: &str) -> (f64, f64, f64) {
    if size == "small" {
        (11.0, 0.35, 35.0)
    } else {
        (24.0, 0.5, 0.0)
    }
}

/// The piece's box in picture metres: four corners.
fn placed_box(picture: &Picture, i: &Instance, b: Rect) -> Vec<(f64, f64)> {
    let (x0, y0, x1, y1) = b;
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .iter()
        .map(|&(a, b)| picture.pic(i.place(a, b)))
        .collect()
}

/// A kind's sublayer: its pieces, and its outer edge with no line where two
/// boxes touch. Where the border stands for the modular stations, only the
/// small stops get their own outline.
fn pieces_sublayer(
    lines: &mut Vec<String>,
    kind: &StationKind,
    rows: &[(&Instance, Vec<(f64, f64)>)],
    border_here: bool,
) -> Vec<Vec<(f64, f64)>> {
    lines.push(layer(kind.id, kind.label, ""));
    // Opaque pieces in a group that is partly transparent as a whole, so
    // overlapping boxes do not darken; a same-colour edge hides the seam
    // between two boxes.
    lines.push(format!("<g opacity=\"{}\">", kind.opacity));
    for (i, c) in rows {
        lines.push(format!(
            "<g class=\"station-piece\" data-slot=\"{}\" data-model=\"{}\" data-x=\"{}\" data-y=\"{}\">\
             <polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.5\"/></g>",
            i.slot,
            escape(&i.model),
            fmt1(i.x),
            fmt1(i.y),
            points_attr(c),
            kind.fill,
            kind.fill
        ));
    }
    lines.push("</g>".into());
    let polys: Vec<Vec<(f64, f64)>> = rows.iter().map(|(_, c)| c.clone()).collect();
    let outer = if border_here {
        let stops: Vec<Vec<(f64, f64)>> = rows
            .iter()
            .filter(|(i, _)| i.model.contains("/small_stops/"))
            .map(|(_, c)| c.clone())
            .collect();
        outer_runs(&stops)
    } else {
        outer_runs(&polys)
    };
    if !outer.is_empty() {
        lines.push(format!(
            "<path class=\"station-outer\" d=\"{}\" fill=\"none\" stroke=\"#000\" stroke-width=\"{OUTLINE}\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            runs_d(&outer)
        ));
    }
    lines.push("</g>".into());
    outer
}

/// The lines of the `stations` layer and of the `stations-street` layer.
pub(super) fn layers(
    picture: &Picture,
    instances: &[Instance],
    boxes: &HashMap<String, Rect>,
) -> (Vec<String>, Vec<String>) {
    let mut under: Vec<String> = Vec::new();
    let mut over: Vec<String> = Vec::new();
    let border_polys: Vec<Vec<(f64, f64)>> = instances
        .iter()
        .filter(|i| {
            i.model
                .strip_prefix("stations/")
                .is_some_and(is_street_border)
        })
        .filter_map(|i| Some(placed_box(picture, i, *boxes.get(&i.model)?)))
        .collect();
    let border_loops = if border_polys.is_empty() {
        Vec::new()
    } else {
        closed_outline(&border_polys, STREET_BORDER_CLOSE)
    };
    // The decks' boxes and outline, which the landings' anchors keep clear of.
    let (mut deck_boxes, mut deck_outline) = (Vec::new(), Vec::new());

    for kind in STATION_KINDS {
        let mut rows: Vec<(&Instance, Vec<(f64, f64)>)> = Vec::new();
        for i in instances {
            let Some(name) = i
                .model
                .strip_prefix("stations/")
                .and_then(|m| m.strip_suffix(".mdl"))
            else {
                continue;
            };
            if kind.models.contains(&name)
                && let Some(&b) = boxes.get(&i.model)
            {
                rows.push((i, placed_box(picture, i, b)));
            }
        }
        // The street stations' ground and border are drawn with the
        // platforms, even when none of the platforms is one of the kind's.
        let platforms = kind.id == "station-street-platforms";
        let border_here = platforms && !border_loops.is_empty();
        if rows.is_empty() && !border_here {
            continue;
        }
        let street = kind.id.starts_with("station-street");
        let lines = if street { &mut over } else { &mut under };
        if lines.is_empty() {
            lines.push(if street {
                layer(
                    "stations-street",
                    "Street stations",
                    " pointer-events=\"none\"",
                )
            } else {
                layer("stations", "Stations", " pointer-events=\"none\"")
            });
        }
        if border_here {
            lines.push(layer(
                "station-street-ground",
                "Street stations: ground inside the border",
                "",
            ));
            lines.push(format!(
                "<path class=\"station-ground\" d=\"{}\" fill=\"{STREET_BORDER_FILL}\" fill-rule=\"evenodd\" \
                 stroke=\"{STREET_BORDER_FILL}\" stroke-width=\"0.5\"/></g>",
                loops_d(&border_loops)
            ));
        }
        let outer = if rows.is_empty() {
            Vec::new()
        } else {
            pieces_sublayer(lines, kind, &rows, border_here)
        };
        if border_here {
            lines.push(layer(
                "station-street-borders",
                "Street stations: border",
                "",
            ));
            lines.push(format!(
                "<path class=\"station-outer\" d=\"{}\" fill=\"none\" stroke=\"#000\" stroke-width=\"{OUTLINE}\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\"/></g>",
                loops_d(&border_loops)
            ));
        }
        if kind.id == "station-water-decks" {
            deck_boxes = rows.into_iter().map(|(_, c)| c).collect();
            deck_outline = outer;
        }
    }

    let landings: Vec<(&Instance, &str)> = instances
        .iter()
        .filter_map(|i| Some((i, landing_size(&i.model)?)))
        .collect();
    if !landings.is_empty() {
        if under.is_empty() {
            under.push(layer("stations", "Stations", " pointer-events=\"none\""));
        }
        under.push("<defs>".into());
        for (size, file) in [("large", ANCHOR_LARGE), ("small", ANCHOR_SMALL)] {
            if landings.iter().any(|(_, s)| *s == size)
                && let Some(s) = super::industries::symbol(&format!("station-anchor-{size}"), file)
            {
                under.push(s);
            }
        }
        under.push("</defs>".into());
        under.push(layer("station-landings", "Water stations: landings", ""));
        for (i, size) in landings {
            let (d, clearance, tilt) = landing_style(size);
            let ((cx, cy), edge) = clear_of_edge(
                picture.pic((i.x, i.y)),
                clearance * d + 0.75,
                &deck_boxes,
                &deck_outline,
                LANDING_REACH,
            );
            // The ring at the top of an anchor points at its deck.
            let turn = edge.map_or(String::new(), |(ex, ey)| {
                let angle = (ex - cx).atan2(cy - ey).to_degrees() - tilt;
                format!(
                    " transform=\"rotate({} {} {})\"",
                    fmt1(angle),
                    fmt2(cx),
                    fmt2(cy)
                )
            });
            under.push(format!(
                "<use class=\"station-landing\" data-slot=\"{}\" data-model=\"{}\" data-size=\"{size}\" data-x=\"{}\" \
                 data-y=\"{}\" xlink:href=\"#station-anchor-{size}\" x=\"{}\" y=\"{}\" width=\"{d}\" height=\"{d}\" \
                 color=\"{LANDING_INK}\"{turn}/>",
                i.slot,
                escape(&i.model),
                fmt1(i.x),
                fmt1(i.y),
                fmt2(cx - d / 2.0),
                fmt2(cy - d / 2.0)
            ));
        }
        under.push("</g>".into());
    }
    if !under.is_empty() {
        under.push("</g>".into());
    }
    if !over.is_empty() {
        over.push("</g>".into());
    }
    (under, over)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(model: &str, x: f64, y: f64) -> Instance {
        Instance {
            slot: 1,
            model: model.into(),
            x,
            y,
            fx: 1.0,
            fy: 0.0,
        }
    }

    #[test]
    fn rail_platforms_go_under_and_street_stops_over() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
        };
        let boxes = HashMap::from([
            (
                "stations/rail/era_a/perron_5.mdl".to_owned(),
                (-10.0, -2.0, 10.0, 2.0),
            ),
            (
                "stations/street/small_stops/small_old.mdl".to_owned(),
                (-3.0, -1.0, 3.0, 1.0),
            ),
        ]);
        let rows = [
            piece("stations/rail/era_a/perron_5.mdl", 0.0, 0.0),
            piece("stations/rail/era_a/perron_5.mdl", 20.0, 0.0),
            piece("stations/street/small_stops/small_old.mdl", 100.0, 100.0),
        ];
        let (under, over) = layers(&picture, &rows, &boxes);
        let (under, over) = (under.join("\n"), over.join("\n"));
        assert!(under.contains("id=\"station-rail-platforms\""));
        assert!(over.contains("id=\"station-street-platforms\""));
        assert!(
            !over.contains("station-street-borders"),
            "no modular station"
        );
        // The two platforms touch at x = 10: the outline runs round both as one.
        assert!(under.contains("class=\"station-outer\""));
        // A run along the shared edge would pass its middle, (510, 500).
        assert!(!under.contains("510.00 500.00"), "{under}");
    }

    #[test]
    fn a_landing_gets_an_anchor_turned_towards_the_deck() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
        };
        let boxes = HashMap::from([(
            "stations/water/models/dock_50_12.mdl".to_owned(),
            (-25.0, -6.0, 25.0, 6.0),
        )]);
        let rows = [
            piece("stations/water/models/dock_50_12.mdl", 0.0, 0.0),
            piece(
                "stations/water/models/tn_medium_pier_passenger.mdl",
                0.0,
                5.0,
            ),
        ];
        let (under, _) = layers(&picture, &rows, &boxes);
        let text = under.join("\n");
        assert!(text.contains("<symbol id=\"station-anchor-large\""));
        // Moved north off the deck edge at y = 6 (picture v = 494) by 12.75 m; its ring points south.
        assert!(text.contains("x=\"488.00\" y=\"469.25\""), "{text}");
        assert!(text.contains("rotate(180.0 500.00 481.25)"), "{text}");
    }
}
