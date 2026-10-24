//! The `vehicles` layer: each vehicle model instance as a five-sided shape,
//! the body with a pointed front, turned to the instance's facing. One
//! sublayer per vehicle folder (`vehicles-bus`, `vehicles-train` with the
//! wagons, ...). A vehicle gets a name only when one is asked for by its
//! slot; a default name such as `Road Vehicle 279` is shortened to `RV279`.
//! The name sits beside the vehicle and is never moved.

use std::collections::{BTreeMap, HashMap};

use super::{Picture, escape, fmt_g, fmt1, layer, safe_id};
use crate::instances::Instance;

/// (model path prefix, (length, width) in metres).
const SIZE: &[(&str, (f64, f64))] = &[
    ("vehicle/train/", (20.0, 3.0)),
    ("vehicle/waggon/", (14.0, 3.0)),
    ("vehicle/tram/", (14.0, 3.0)),
    ("vehicle/truck/", (7.0, 2.5)),
    ("vehicle/bus/", (10.0, 2.6)),
    ("vehicle/car/", (4.5, 1.8)),
    ("vehicle/ship/", (40.0, 8.0)),
];
/// Anything else.
const OTHER_SIZE: (f64, f64) = (6.0, 2.5);
const FILL: &[(&str, &str)] = &[
    ("vehicle/train/", "#d96a00"),
    ("vehicle/waggon/", "#ffc27a"),
    ("vehicle/tram/", "#8a4fc0"),
    ("vehicle/truck/", "#1f4fa8"),
    ("vehicle/bus/", "#1f7a3a"),
    ("vehicle/ship/", "#3c3c3c"),
];
/// Cars and anything else.
const GREY: &str = "#808080";
/// These get a grey outline, as the track under them is red; the others a white one.
const RAIL_BOUND: &[&str] = &["vehicle/train/", "vehicle/waggon/", "vehicle/tram/"];
const RAIL_OUTLINE: &str = "#5c5c5c";
/// The default names' first words and their short forms.
const SHORT: &[(&str, &str)] = &[
    ("Road Vehicle", "RV"),
    ("Train", "RR"),
    ("Ship", "S"),
    ("Tram", "TRM"),
    ("Aircraft", "AIR"),
];

/// A default vehicle name shortened for the picture: `Road Vehicle 279`
/// gives `RV279`, `Train 58` `RR58`, `Ship 106` `S106`, `Tram 62` `TRM62`
/// and `Aircraft 6` `AIR6`, keeping a trailing `?` (an unsure reading). Any
/// other text, such as a name the player gave, stays as it is.
fn short_name(text: &str) -> String {
    for (long, short) in SHORT {
        let Some(rest) = text.strip_prefix(long).and_then(|r| r.strip_prefix(' ')) else {
            continue;
        };
        let digits = rest.strip_suffix('?').unwrap_or(rest);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return format!("{short}{rest}");
        }
    }
    text.to_owned()
}

/// The folder a vehicle model sits in (`train`, `waggon`, `bus`, ...), or `other`.
fn class(model: &str) -> &str {
    model
        .strip_prefix("vehicle/")
        .and_then(|r| r.split_once('/'))
        .map(|(c, _)| c)
        .filter(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        .unwrap_or("other")
}

fn by_prefix<T: Copy>(table: &[(&str, T)], model: &str, other: T) -> T {
    table
        .iter()
        .find(|(p, _)| model.starts_with(p))
        .map_or(other, |(_, v)| *v)
}

pub(super) fn layers(
    picture: &Picture,
    rows: &[Instance],
    names: &HashMap<usize, String>,
    mpp: f64,
) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let font = 4.0 * mpp + 4.0; // 8 px at 1 m per pixel
    let halo = format!(
        " paint-order=\"stroke\" stroke=\"#fff\" stroke-width=\"{}\" stroke-linejoin=\"round\"",
        fmt_g(font / 4.0)
    );
    // Wagons go with the trains.
    let mut groups: BTreeMap<&str, Vec<(&str, &Instance)>> = BTreeMap::new();
    for i in rows {
        let c = class(&i.model);
        let group = if c == "waggon" { "train" } else { c };
        groups.entry(group).or_default().push((c, i));
    }
    let mut out = vec![layer(
        "vehicles",
        "Vehicles",
        &format!(" font-family=\"sans-serif\" font-size=\"{}\"", fmt_g(font)),
    )];
    for (group, members) in groups {
        out.push(layer(
            &format!("vehicles-{}", safe_id(group)),
            &format!("Vehicles: {group}"),
            "",
        ));
        for (c, i) in members {
            let (length, width) = by_prefix(SIZE, &i.model, OTHER_SIZE);
            let (px, py) = picture.pic((i.x, i.y));
            let (ux, uy) = picture.vec((i.fx, i.fy));
            let angle = uy.atan2(ux).to_degrees();
            let tip = (width * 0.9).min(length / 2.0); // the point comes out of the body's length
            let fill = by_prefix(FILL, &i.model, GREY);
            let edge = if RAIL_BOUND.iter().any(|p| i.model.starts_with(p)) {
                RAIL_OUTLINE
            } else {
                "#fff"
            };
            let (x0, x1, hw) = (-length / 2.0, length / 2.0, width / 2.0);
            let c = safe_id(c);
            out.push(format!(
                "<g id=\"vehicle-{}\" class=\"vehicle {c}\" data-slot=\"{}\" data-model=\"{}\" data-class=\"{c}\" \
                 data-x=\"{}\" data-y=\"{}\">",
                i.slot,
                i.slot,
                escape(&i.model),
                fmt1(i.x),
                fmt1(i.y)
            ));
            out.push(format!(
                "<polygon transform=\"translate({} {}) rotate({})\" points=\"{},{} {},{} {},0 {},{} {},{}\" \
                 fill=\"{fill}\" stroke=\"{edge}\" stroke-width=\"0.5\" stroke-linejoin=\"round\"/>",
                fmt1(px),
                fmt1(py),
                fmt1(angle),
                fmt_g(x0),
                fmt_g(-hw),
                fmt_g(x1 - tip),
                fmt_g(-hw),
                fmt_g(x1),
                fmt_g(x1 - tip),
                fmt_g(hw),
                fmt_g(x0),
                fmt_g(hw)
            ));
            if let Some(name) = names.get(&i.slot) {
                out.push(format!(
                    "<text x=\"{}\" y=\"{}\"{halo}>{}</text>",
                    fmt1(px + length.max(8.0) / 2.0 + 2.0),
                    fmt1(py - width / 2.0 - 1.0),
                    escape(&short_name(name))
                ));
            }
            out.push("</g>".into());
        }
        out.push("</g>".into());
    }
    out.push("</g>".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle(slot: usize, model: &str, facing: (f64, f64)) -> Instance {
        Instance {
            slot,
            model: model.into(),
            x: 10.0,
            y: 20.0,
            fx: facing.0,
            fy: facing.1,
        }
    }

    #[test]
    fn default_names_are_shortened() {
        assert_eq!(short_name("Road Vehicle 279"), "RV279");
        assert_eq!(short_name("Train 58"), "RR58");
        assert_eq!(short_name("Aircraft 6?"), "AIR6?");
        assert_eq!(short_name("Ship 1 2"), "Ship 1 2");
        assert_eq!(short_name("Trainspotter 5"), "Trainspotter 5");
        assert_eq!(short_name("Express"), "Express");
    }

    #[test]
    fn classes_come_from_the_folder() {
        assert_eq!(class("vehicle/bus/a/b.mdl"), "bus");
        assert_eq!(class("vehicle/waggon/x.mdl"), "waggon");
        assert_eq!(class("vehicle/Bus/x.mdl"), "other");
        assert_eq!(class("vehicles.mdl"), "other");
    }

    #[test]
    fn a_vehicle_points_where_it_faces() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
        };
        let rows = [
            vehicle(4, "vehicle/bus/old/bus.mdl", (0.0, 1.0)),
            vehicle(9, "vehicle/waggon/box.mdl", (1.0, 0.0)),
        ];
        let names = HashMap::from([(4, "Road Vehicle 12".to_owned())]);
        let text = layers(&picture, &rows, &names, 4.0).join("\n");
        // Facing north: up the picture, -90 degrees.
        assert!(
            text.contains("transform=\"translate(510.0 480.0) rotate(-90.0)\" points=\"-5,-1.3 2.66,-1.3 5,0 2.66,1.3 -5,1.3\" fill=\"#1f7a3a\" stroke=\"#fff\""),
            "{text}"
        );
        assert!(text.contains(">RV12</text>"));
        assert!(text.contains("x=\"517.0\" y=\"477.7\""), "{text}");
        // The wagon is drawn with the trains, in its own colour and a grey edge.
        assert!(text.contains("id=\"vehicles-train\""));
        assert!(text.contains("class=\"vehicle waggon\""));
        assert!(text.contains("fill=\"#ffc27a\" stroke=\"#5c5c5c\""));
        assert_eq!(text.matches("<text").count(), 1, "only the named one");
        assert!(text.contains("font-size=\"20\""));
    }
}
