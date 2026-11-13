//! The `industries` layer: each industry's footprint and a badge with its icon.
//!
//! The footprint is the industry's ground face from its script in the game
//! install, placed from the main model's origin and facing, drawn as the
//! game's Towns layer paints an industry (light purple, dark purple line).
//! The badge is a white disc with a grey border and the industry's icon in
//! dark grey, upright. With a footprint it is 0.4 of the polygon's shorter
//! side across and sits on the polygon's centroid (on the origin when the
//! centroid falls outside); without one it is 64 m across on the origin.
//!
//! The fields (a farm's plots, an oil well's pumpjack pads, a mine's pits)
//! are drawn under the footprint in the same colours, more translucent: the
//! slots the industry's record lists, shaped and placed by its `.con.lua`
//! ([`crate::install::industry_fields`]). A water module (a fishing raft, a
//! drilling ground) is a disc.

use std::collections::{BTreeMap, HashMap};

use super::{Cut, OUTLINE, Picture, escape, fmt1, fmt2, layer, points_attr, safe_id};
use crate::geometry::{bounds, centroid_inside};
use crate::install::{FieldShape, Polygon};
use crate::instances::Instance;

/// Metres across the badge of an industry with no footprint.
const DIAMETER: f64 = 64.0;
/// A badge is this share of its footprint's shorter side.
const FOOTPRINT_SHARE: f64 = 0.4;
const BORDER: &str = "#8c8c8c";
const INK: &str = "#3a3a3a";
const FOOTPRINT_FILL: &str = "#b44fe8";
const FOOTPRINT_LINE: &str = "#5b1a8f";
const FOOTPRINT_OPACITY: &str = "0.45";
/// The fields are more translucent than the footprint.
const FIELD_OPACITY: &str = "0.25";

/// The fields to draw: shapes by industry key, slot 1 first, and each
/// industry's slots by its main instance's `data-slot`.
#[derive(Clone, Copy)]
pub(super) struct Fields<'a> {
    pub shapes: &'a HashMap<String, Vec<Option<FieldShape>>>,
    pub slots: &'a HashMap<usize, Vec<u32>>,
}

impl Fields<'_> {
    /// The fields of one industry: slot number and shape, in slot order.
    fn of(&self, i: &Instance, key: &str) -> Vec<(u32, &FieldShape)> {
        let (Some(shapes), Some(slots)) = (self.shapes.get(key), self.slots.get(&i.slot)) else {
            return Vec::new();
        };
        let mut slots = slots.clone();
        slots.sort_unstable();
        slots.dedup();
        slots
            .into_iter()
            .filter_map(|n| {
                let shape = shapes
                    .get(usize::try_from(n).ok()?.checked_sub(1)?)?
                    .as_ref()?;
                Some((n, shape))
            })
            .collect()
    }
}

/// Outlines for the water industries, which declare no ground face: metres in
/// the industry's own axes, read off the game's Towns layer in one game
/// (good to about 2 m; `docs/models.md` in tf3-save-format). The oil
/// platform's is a stand-in for its tinted deck, with the helipad and the big
/// crane's boom, which show which way it faces.
const MEASURED_FACES: &[(&str, &[(f64, f64)])] = &[
    (
        "fishing_grounds",
        &[
            (-40.0, -28.0),
            (22.0, -28.0),
            (22.0, -12.8),
            (38.5, -12.8),
            (38.5, 20.7),
            (-30.0, 20.7),
            (-40.0, 15.5),
        ],
    ),
    (
        "sand_excavator",
        &[(-18.2, -43.5), (45.5, -43.5), (45.5, 65.0), (-18.2, 65.0)],
    ),
    (
        "oil_platform",
        &[
            (-24.5, 24.7),
            (-24.5, -29.5),
            (13.7, -29.5),
            (15.5, -31.3),
            (24.2, -31.3),
            (30.3, -25.2),
            (30.3, -16.5),
            (24.7, -10.9),
            (24.7, 24.7),
            (4.0, 24.7),
            (7.0, 52.0),
            (3.0, 52.0),
            (0.0, 24.7),
        ],
    ),
];
/// A round part, (a, b, radius) in metres in the industry's axes.
type Disc = (f64, f64, f64);
/// Round parts tinted beside a footprint: the oil platform's two docking stations.
const MEASURED_DISCS: &[(&str, &[Disc])] =
    &[("oil_platform", &[(55.0, 0.0, 5.5), (-55.0, 0.0, 5.5)])];

macro_rules! icons {
    ($($key:literal),* $(,)?) => {
        &[$(($key, include_str!(concat!("../../icons/industries/", $key, ".svg")))),*]
    };
}

/// One pictogram per base-game industry key, 64 x 64 in `currentColor`. Our
/// own drawings from tf3-save-format's `tools/icons/industries`.
const ICONS: &[(&str, &str)] = icons!(
    "brewery",
    "bricks_works",
    "cement_plant",
    "chemical_plant",
    "clay_pit",
    "coal_mine",
    "cotton_farm",
    "farm",
    "fishing_grounds",
    "food_factory",
    "forest",
    "furniture_factory",
    "glass_works",
    "iron_ore_mine",
    "livestock_farm",
    "machine_factory",
    "oil_platform",
    "oil_refinery",
    "oil_well",
    "paper_mill",
    "printing_press",
    "quarry",
    "rubber_farm",
    "sand_excavator",
    "sand_pit",
    "saw_mill",
    "steel_mill",
    "textile_factory",
    "tire_factory",
    "tool_factory",
    "vehicle_factory",
    "weaving_mill",
);

/// The drawing inside an icon file's `<svg>` element.
pub(super) fn icon_body(file: &str) -> Option<&str> {
    let open = file.find("<svg")?;
    let start = open + file[open..].find('>')? + 1;
    let end = file.rfind("</svg>")?;
    (start <= end).then(|| &file[start..end])
}

/// A `<symbol>` holding an icon, filled with `currentColor` as its file is.
pub(super) fn symbol(id: &str, file: &str) -> Option<String> {
    Some(format!(
        "<symbol id=\"{id}\" viewBox=\"0 0 64 64\"><g fill=\"currentColor\">{}</g></symbol>",
        icon_body(file)?
    ))
}

/// The footprints the install gave, with the measured ones for the industries it has none for.
pub fn faces_with_measured(install_faces: HashMap<String, Polygon>) -> HashMap<String, Polygon> {
    let mut faces: HashMap<String, Polygon> = MEASURED_FACES
        .iter()
        .map(|(k, f)| ((*k).to_owned(), f.to_vec()))
        .collect();
    faces.extend(install_faces);
    faces
}

/// The corners of what is drawn for an industry, picture metres: its
/// footprint, the boxes round its round parts and round its badge.
fn drawn(
    picture: &Picture,
    i: &Instance,
    key: &str,
    faces: &HashMap<String, Polygon>,
    fields: Fields,
) -> Vec<(f64, f64)> {
    let around = |(a, b): (f64, f64), r: f64| {
        let (u, v) = picture.pic(i.place(a, b));
        [(u - r, v - r), (u + r, v + r)]
    };
    let field_points: Vec<(f64, f64)> = fields
        .of(i, key)
        .into_iter()
        .flat_map(|(_, shape)| match shape {
            FieldShape::Faces(fs) => fs
                .iter()
                .flatten()
                .map(|&(a, b)| picture.pic(i.place(a, b)))
                .collect::<Vec<_>>(),
            FieldShape::Disc(c, r) => around(*c, *r).to_vec(),
        })
        .collect();
    let face = faces.get(key);
    let (centre, d) = face.map_or(((0.0, 0.0), DIAMETER), |f| face_badge(f));
    let discs = MEASURED_DISCS
        .iter()
        .filter(|(k, _)| *k == key)
        .flat_map(|(_, d)| d.iter())
        .flat_map(|&(a, b, r)| around((a, b), r));
    face.into_iter()
        .flatten()
        .map(|&(a, b)| picture.pic(i.place(a, b)))
        .chain(discs)
        .chain(around(centre, d / 2.0))
        .chain(field_points)
        .collect()
}

/// The badge of a footprint: its centre in the industry's axes and its diameter.
fn face_badge(face: &[(f64, f64)]) -> ((f64, f64), f64) {
    let (a0, a1, b0, b1) = bounds(face);
    (
        centroid_inside(face).unwrap_or((0.0, 0.0)),
        FOOTPRINT_SHARE * (a1 - a0).min(b1 - b0),
    )
}

pub(super) fn layers(
    picture: &Picture,
    rows: &[(Instance, String)],
    faces: &HashMap<String, Polygon>,
    fields: Fields,
    cut: Cut,
) -> Vec<String> {
    let rows: Vec<&(Instance, String)> = rows
        .iter()
        .filter(|(i, key)| cut.meets(&drawn(picture, i, key, faces, fields), OUTLINE / 2.0))
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    let mut by_key: BTreeMap<&str, Vec<&Instance>> = BTreeMap::new();
    for (i, key) in &rows {
        by_key.entry(key.as_str()).or_default().push(i);
    }
    let mut out = vec![
        layer("industries", "Industries", " pointer-events=\"none\""),
        "<defs>".into(),
    ];
    let mut have = Vec::new();
    for &key in by_key.keys() {
        if let Some(s) = ICONS
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, file)| symbol(&format!("industry-icon-{}", safe_id(key)), file))
        {
            out.push(s);
            have.push(key);
        }
    }
    out.push("</defs>".into());

    let fielded: Vec<_> = rows
        .iter()
        .map(|(i, k)| (i, k.as_str(), fields.of(i, k)))
        .filter(|(_, _, f)| !f.is_empty())
        .collect();
    if !fielded.is_empty() {
        out.push(layer("industry-fields", "Industries: fields", ""));
        for (i, key, list) in fielded {
            for (n, shape) in list {
                let head = format!(
                    "class=\"industry-field\" data-slot=\"{}\" data-key=\"{}\" data-field=\"{n}\"",
                    i.slot,
                    escape(key)
                );
                let paint = format!(
                    "fill=\"{FOOTPRINT_FILL}\" fill-opacity=\"{FIELD_OPACITY}\" stroke=\"{FOOTPRINT_LINE}\" \
                     stroke-width=\"{OUTLINE}\""
                );
                match shape {
                    FieldShape::Faces(fs) => {
                        for f in fs {
                            let pts: Vec<(f64, f64)> =
                                f.iter().map(|&(a, b)| picture.pic(i.place(a, b))).collect();
                            out.push(format!(
                                "<polygon {head} points=\"{}\" {paint} stroke-linejoin=\"round\"/>",
                                points_attr(&pts)
                            ));
                        }
                    }
                    FieldShape::Disc((a, b), r) => {
                        let (cx, cy) = picture.pic(i.place(*a, *b));
                        out.push(format!(
                            "<circle {head} cx=\"{}\" cy=\"{}\" r=\"{}\" {paint}/>",
                            fmt2(cx),
                            fmt2(cy),
                            fmt2(*r)
                        ));
                    }
                }
            }
        }
        out.push("</g>".into());
    }

    let shaped: Vec<(&Instance, &str)> = rows
        .iter()
        .filter(|(_, k)| faces.contains_key(k))
        .map(|(i, k)| (i, k.as_str()))
        .collect();
    if !shaped.is_empty() {
        out.push(layer("industry-footprints", "Industries: footprints", ""));
        for (i, key) in shaped {
            let discs = MEASURED_DISCS
                .iter()
                .filter(|(k, _)| *k == key)
                .flat_map(|(_, d)| d.iter());
            for &(a, b, r) in discs {
                let (cx, cy) = picture.pic(i.place(a, b));
                out.push(format!(
                    "<circle class=\"industry-footprint\" data-slot=\"{}\" data-key=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" \
                     fill=\"{FOOTPRINT_FILL}\" fill-opacity=\"{FOOTPRINT_OPACITY}\" stroke=\"{FOOTPRINT_LINE}\" stroke-width=\"{OUTLINE}\"/>",
                    i.slot,
                    escape(key),
                    fmt2(cx),
                    fmt2(cy),
                    fmt2(r)
                ));
            }
            let pts: Vec<(f64, f64)> = faces[key]
                .iter()
                .map(|&(a, b)| picture.pic(i.place(a, b)))
                .collect();
            out.push(format!(
                "<polygon class=\"industry-footprint\" data-slot=\"{}\" data-key=\"{}\" points=\"{}\" \
                 fill=\"{FOOTPRINT_FILL}\" fill-opacity=\"{FOOTPRINT_OPACITY}\" stroke=\"{FOOTPRINT_LINE}\" stroke-width=\"{OUTLINE}\" \
                 stroke-linejoin=\"round\"/>",
                i.slot,
                escape(key),
                points_attr(&pts)
            ));
        }
        out.push("</g>".into());
    }

    for (key, members) in &by_key {
        out.push(layer(
            &format!("industry-{}", safe_id(key)),
            &format!("Industries: {key}"),
            &format!(" data-key=\"{}\"", escape(key)),
        ));
        for i in members {
            let (centre, d) = faces
                .get(*key)
                .map_or(((0.0, 0.0), DIAMETER), |f| face_badge(f));
            let (cx, cy) = picture.pic(i.place(centre.0, centre.1));
            let (border, side) = (d / 32.0, 0.72 * d); // 2 m on a 64 m badge; the icon sits inside with a margin
            out.push(format!(
                "<g class=\"industry\" data-slot=\"{}\" data-key=\"{}\" data-model=\"{}\" data-x=\"{}\" data-y=\"{}\" \
                 data-diameter=\"{}\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#ffffff\" stroke=\"{BORDER}\" stroke-width=\"{}\"/>",
                i.slot,
                escape(key),
                escape(&i.model),
                fmt1(i.x),
                fmt1(i.y),
                fmt1(d),
                fmt2(cx),
                fmt2(cy),
                fmt2(d / 2.0 - border / 2.0),
                fmt2(border)
            ));
            if have.contains(key) {
                out.push(format!(
                    "<use xlink:href=\"#industry-icon-{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color=\"{INK}\"/>",
                    safe_id(key),
                    fmt2(cx - side / 2.0),
                    fmt2(cy - side / 2.0),
                    fmt2(side),
                    fmt2(side)
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

    fn farm(x: f64, y: f64) -> (Instance, String) {
        (
            Instance {
                slot: 3,
                model: "industries/farm/farm_sm.mdl".into(),
                x,
                y,
                z: 0.0,
                fx: 1.0,
                fy: 0.0,
            },
            "farm".into(),
        )
    }

    fn no_fields() -> Fields<'static> {
        Fields {
            shapes: Box::leak(Box::default()),
            slots: Box::leak(Box::default()),
        }
    }

    #[test]
    fn fields_are_drawn_under_the_footprint_in_slot_order() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: crate::svg::North::Game,
        };
        let faces = HashMap::from([(
            "farm".to_owned(),
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        )]);
        let square = |a: f64| vec![(a, 0.0), (a + 10.0, 0.0), (a + 10.0, 10.0), (a, 10.0)];
        let shapes = HashMap::from([(
            "farm".to_owned(),
            vec![
                Some(FieldShape::Faces(vec![square(20.0)])),
                None,
                Some(FieldShape::Disc((0.0, 50.0), 13.0)),
            ],
        )]);
        // Slot 2 has no shape and slot 9 is past the table: both left out.
        let slots = HashMap::from([(3, vec![3, 9, 2, 1])]);
        let fields = Fields {
            shapes: &shapes,
            slots: &slots,
        };
        let text = layers(&picture, &[farm(0.0, 0.0)], &faces, fields, Cut(None)).join("\n");
        let first = text.find("data-field=\"1\"").expect("slot 1");
        let third = text.find("data-field=\"3\"").expect("slot 3");
        let footprint = text.find("class=\"industry-footprint\"").unwrap();
        assert!(first < third && third < footprint, "{text}");
        assert!(!text.contains("data-field=\"2\"") && !text.contains("data-field=\"9\""));
        assert!(text.contains(
            "<polygon class=\"industry-field\" data-slot=\"3\" data-key=\"farm\" data-field=\"1\" \
             points=\"520.00,500.00 530.00,500.00 530.00,490.00 520.00,490.00\" fill=\"#b44fe8\" fill-opacity=\"0.25\""
        ), "{text}");
        assert!(text.contains("<circle class=\"industry-field\" data-slot=\"3\" data-key=\"farm\" data-field=\"3\" cx=\"500.00\" cy=\"450.00\" r=\"13.00\""));
        // A field alone reaching into a crop brings its industry in.
        let cut = Cut(Some((515.0, 535.0, 485.0, 505.0)));
        assert!(!layers(&picture, &[farm(0.0, 0.0)], &faces, fields, cut).is_empty());
        assert!(layers(&picture, &[farm(0.0, 0.0)], &faces, no_fields(), cut).is_empty());
    }

    #[test]
    fn every_icon_has_a_drawing() {
        for (key, file) in ICONS {
            assert!(icon_body(file).is_some_and(|b| b.contains('<')), "{key}");
        }
    }

    #[test]
    fn a_footprint_sets_the_badge() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: crate::svg::North::Game,
        };
        let faces = faces_with_measured(HashMap::from([(
            "farm".to_owned(),
            vec![(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)],
        )]));
        let text = layers(
            &picture,
            &[farm(10.0, 20.0)],
            &faces,
            no_fields(),
            Cut(None),
        )
        .join("\n");
        // 0.4 of the 50 m side, on the centroid (50, 25) from the origin.
        assert!(text.contains("data-diameter=\"20.0\""), "{text}");
        assert!(
            text.contains("<circle cx=\"560.00\" cy=\"455.00\""),
            "{text}"
        );
        assert!(
            text.contains("points=\"510.00,480.00 610.00,480.00 610.00,430.00 510.00,430.00\"")
        );
        assert!(text.contains("<symbol id=\"industry-icon-farm\""));
        assert!(faces.contains_key("oil_platform"));
    }

    #[test]
    fn without_a_footprint_the_badge_sits_on_the_origin() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: crate::svg::North::Game,
        };
        let text = layers(
            &picture,
            &[farm(0.0, 0.0)],
            &HashMap::new(),
            no_fields(),
            Cut(None),
        )
        .join("\n");
        assert!(text.contains("data-diameter=\"64.0\""));
        assert!(text.contains("<circle cx=\"500.00\" cy=\"500.00\""));
        assert!(!text.contains("industry-footprints"));
    }
}
