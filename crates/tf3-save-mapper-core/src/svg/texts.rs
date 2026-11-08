//! The `texts` layer: words the caller places at map points (`--text`),
//! such as town names, which the save does not place. Each is white type on
//! a dark rounded box, as the game draws a town's name, centred on its
//! point and upright whichever way north is drawn, in one of three sizes.
//! A size is in map metres on an 8 km view and larger ones, so there a name
//! keeps pace with the industry badges (on a big view both want zooming in
//! to read). On a smaller view it shrinks, 1.3 times for each halving of the
//! picture's shorter side (`view_scale`), so on a close sectional it does not
//! cover what it names. The layer is drawn over everything from the save, the grade
//! overlays and the edge labels; only the grid labels, the map key and the
//! rose are over it. A text whose point lies outside the picture is left
//! out, never moved in.
//!
//! SVG cannot measure text, so the box's width comes from a table of
//! advance widths (`WIDTHS`, DejaVu Sans, one of the wider common sans
//! fonts). The type asks for DejaVu Sans or Verdana, which is about as wide;
//! a narrower font the viewer falls back to only leaves more room in the box.

use super::{Picture, escape, fmt_g, fmt1, layer};

/// A text's size, as the font size on a view of 8 km or more
/// (`view_scale` for smaller ones). There an industry badge is 64 m across without the install and
/// 40 % of its footprint's shorter side with it (about 40 to 60 m), its icon
/// 0.72 of that; a small text's capitals (about 0.7 of its size) are about
/// as tall as the icon, and a medium one reads well before the icon does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl TextSize {
    /// The font size on a view of 8 km or more, metres.
    pub fn metres(self) -> f64 {
        match self {
            TextSize::Small => 60.0,
            TextSize::Medium => 90.0,
            TextSize::Large => 140.0,
        }
    }

    /// The name as typed and as written in `data-size`.
    pub fn name(self) -> &'static str {
        match self {
            TextSize::Small => "small",
            TextSize::Medium => "medium",
            TextSize::Large => "large",
        }
    }

    /// The size typed as `name`, if it is one.
    pub fn from_name(name: &str) -> Option<TextSize> {
        [TextSize::Small, TextSize::Medium, TextSize::Large]
            .into_iter()
            .find(|s| s.name() == name)
    }
}

/// A text's look. Only one so far: white type on a dark rounded box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextStyle {
    #[default]
    Regular,
}

impl TextStyle {
    /// The name as typed and as written in `data-style`.
    pub fn name(self) -> &'static str {
        match self {
            TextStyle::Regular => "regular",
        }
    }

    /// The style typed as `name`, if it is one.
    pub fn from_name(name: &str) -> Option<TextStyle> {
        [TextStyle::Regular].into_iter().find(|s| s.name() == name)
    }
}

/// A text to write at a map point (game frame: x east, y north, whole metres).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapText {
    pub x: i32,
    pub y: i32,
    pub size: TextSize,
    pub style: TextStyle,
    pub text: String,
}

/// Type, box and box edge colours, and the box's opacity.
const INK: &str = "#ffffff";
const BOX: &str = "#1f2427";
const BOX_OPACITY: f64 = 0.85;
const EDGE: &str = "#ffffff";
const EDGE_OPACITY: f64 = 0.35;
/// As shares of the font size: how far below the point the baseline sits
/// (half a capital's height, so the capitals' middle is on the point), the
/// box's height (centred on the point, room for accents and descenders),
/// the space each side of the text, the corners' radius and the edge's width.
const BASELINE_DROP: f64 = 0.35;
const BOX_HEIGHT: f64 = 1.5;
const BOX_PAD: f64 = 0.4;
const BOX_RADIUS: f64 = 0.35;
const EDGE_WIDTH: f64 = 0.03;

/// Advance widths of DejaVu Sans in thousandths of the font size, from
/// U+0020 (space) to U+017F (the end of Latin Extended-A); U+007F to U+009F,
/// control characters, are 0.
#[rustfmt::skip]
const WIDTHS: [u16; 0x180 - 0x20] = [
    318, 401, 460, 838, 636, 950, 780, 275, 390, 390, 500, 838, 318, 361, 318, 337,
    636, 636, 636, 636, 636, 636, 636, 636, 636, 636, 337, 337, 838, 838, 838, 531,
    1000, 684, 686, 698, 770, 632, 575, 775, 752, 295, 295, 656, 557, 863, 748, 787,
    603, 787, 695, 635, 611, 732, 684, 989, 685, 611, 685, 390, 337, 390, 838, 500,
    500, 613, 635, 550, 635, 615, 352, 635, 634, 278, 278, 579, 278, 974, 634, 612,
    635, 635, 411, 521, 392, 634, 592, 818, 592, 592, 525, 636, 337, 636, 838, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    318, 401, 636, 636, 636, 636, 337, 500, 500, 1000, 471, 612, 838, 361, 1000, 500,
    500, 838, 401, 401, 500, 636, 636, 318, 500, 401, 471, 612, 969, 969, 969, 531,
    684, 684, 684, 684, 684, 684, 974, 698, 632, 632, 632, 632, 295, 295, 295, 295,
    775, 748, 787, 787, 787, 787, 787, 838, 787, 732, 732, 732, 732, 611, 605, 630,
    613, 613, 613, 613, 613, 613, 982, 550, 615, 615, 615, 615, 278, 278, 278, 278,
    612, 634, 612, 612, 612, 612, 612, 838, 612, 634, 634, 634, 634, 592, 635, 592,
    684, 613, 684, 613, 684, 613, 698, 550, 698, 550, 698, 550, 698, 550, 770, 635,
    775, 635, 632, 615, 632, 615, 632, 615, 632, 615, 632, 615, 775, 635, 775, 635,
    775, 635, 775, 635, 752, 634, 916, 695, 295, 278, 295, 278, 295, 278, 295, 278,
    295, 278, 590, 556, 295, 278, 656, 579, 579, 557, 278, 557, 278, 557, 375, 557,
    342, 562, 284, 748, 634, 748, 634, 748, 634, 813, 748, 634, 787, 612, 787, 612,
    787, 612, 1070, 1023, 695, 411, 695, 411, 695, 411, 635, 521, 635, 521, 635, 521,
    635, 521, 611, 392, 611, 392, 611, 392, 732, 634, 732, 634, 732, 634, 732, 634,
    732, 634, 732, 634, 989, 818, 611, 592, 611, 685, 525, 685, 525, 685, 525, 352,
];

/// A character's width in thousandths of the font size: from `WIDTHS`, or
/// a full square for East Asian scripts and 0.75 for anything else, both on
/// the wide side so the box holds the text.
fn char_width(c: char) -> f64 {
    let n = u32::from(c);
    match n {
        0x20..0x180 => f64::from(WIDTHS[(n - 0x20) as usize]),
        0x2e80.. => 1000.0,
        _ => 750.0,
    }
}

/// The width of `text` set at font size 1.
fn text_width(text: &str) -> f64 {
    text.chars().map(char_width).sum::<f64>() / 1000.0
}

/// Below this shorter side, metres, the type shrinks.
const REFERENCE_SIDE: f64 = 8000.0;
/// How much smaller the type is for each halving of the view.
const STEP: f64 = 1.3;

/// The factor on the sizes for a view whose picture's shorter side is
/// `side` metres: 1 at 8 km and over, 1 / 1.3 at 4 km, 1 / 1.69 at 2 km,
/// and so on.
pub(super) fn view_scale(side: f64) -> f64 {
    STEP.powf((side / REFERENCE_SIDE).log2()).min(1.0)
}

/// The texts' layer; `scale` is `view_scale` of the picture.
pub(super) fn layers(picture: &Picture, texts: &[&MapText], scale: f64) -> Vec<String> {
    if texts.is_empty() {
        return Vec::new();
    }
    let mut out = vec![layer(
        "texts",
        "Texts",
        " font-family=\"DejaVu Sans, Verdana, sans-serif\" text-anchor=\"middle\"",
    )];
    for t in texts {
        let (u, v) = picture.pic((f64::from(t.x), f64::from(t.y)));
        let size = t.size.metres() * scale;
        let (w, h) = (
            (text_width(&t.text) + 2.0 * BOX_PAD) * size,
            BOX_HEIGHT * size,
        );
        out.push(format!(
            "<g class=\"text\" data-x=\"{}\" data-y=\"{}\" data-size=\"{}\" data-style=\"{}\">\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{BOX}\" \
             fill-opacity=\"{BOX_OPACITY}\" stroke=\"{EDGE}\" stroke-opacity=\"{EDGE_OPACITY}\" \
             stroke-width=\"{}\"/><text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{INK}\">{}</text></g>",
            t.x,
            t.y,
            t.size.name(),
            t.style.name(),
            fmt1(u - w / 2.0),
            fmt1(v - h / 2.0),
            fmt1(w),
            fmt1(h),
            fmt_g(BOX_RADIUS * size),
            fmt_g(EDGE_WIDTH * size),
            fmt1(u),
            fmt1(v + BASELINE_DROP * size),
            fmt_g(size),
            escape(&t.text)
        ));
    }
    out.push("</g>".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::North;

    fn text(x: i32, y: i32, size: TextSize, words: &str) -> MapText {
        MapText {
            x,
            y,
            size,
            style: TextStyle::Regular,
            text: words.into(),
        }
    }

    #[test]
    fn texts_sit_on_their_points_in_map_metres() {
        let picture = Picture {
            width: 1000.0,
            height: 2000.0,
            north: North::Game,
        };
        let a = text(-100, 250, TextSize::Medium, "Tonopah");
        let b = text(0, 0, TextSize::Large, "R&D <east>");
        let svg = layers(&picture, &[&a, &b], 1.0).join("\n");
        assert!(svg.starts_with("<g id=\"texts\""), "{svg}");
        // (-100, 250) is 400 m from the left and 750 m from the top; the
        // baseline 0.35 x 90 m below. "Tonopah" is 4.351 wide at size 1, and
        // 0.4 more each side: 463.6 m; the box 135 m tall round the point.
        assert!(
            svg.contains(
                "<g class=\"text\" data-x=\"-100\" data-y=\"250\" data-size=\"medium\" \
                 data-style=\"regular\"><rect x=\"168.2\" y=\"682.5\" width=\"463.6\" \
                 height=\"135.0\" rx=\"31.5\""
            ),
            "{svg}"
        );
        assert!(
            svg.contains(
                "<text x=\"400.0\" y=\"781.5\" font-size=\"90\" fill=\"#ffffff\">Tonopah</text></g>"
            ),
            "{svg}"
        );
        assert!(svg.contains("font-size=\"140\""));
        assert!(svg.contains(">R&amp;D &lt;east&gt;</text>"));
        assert!(svg.ends_with("</g>"));
    }

    #[test]
    fn a_text_stays_upright_with_sun_north() {
        let picture = Picture {
            width: 1000.0,
            height: 2000.0,
            north: North::Sun,
        };
        let a = text(100, 0, TextSize::Small, "East");
        let svg = layers(&picture, &[&a], 1.0).join("\n");
        // Sun north: up is +x, so 100 m east is 100 m above the middle.
        assert!(
            svg.contains("x=\"1000.0\" y=\"421.0\" font-size=\"60\""),
            "{svg}"
        );
        assert!(!svg.contains("rotate"));
    }

    #[test]
    fn no_texts_no_layer() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: North::Game,
        };
        assert!(layers(&picture, &[], 1.0).is_empty());
    }

    #[test]
    fn sizes_and_styles_by_name() {
        for s in [TextSize::Small, TextSize::Medium, TextSize::Large] {
            assert_eq!(TextSize::from_name(s.name()), Some(s));
        }
        assert!(TextSize::metres(TextSize::Small) < TextSize::Medium.metres());
        assert_eq!(TextSize::from_name("med"), None);
        assert_eq!(TextStyle::from_name("regular"), Some(TextStyle::Regular));
        assert_eq!(TextStyle::from_name("bold"), None);
    }

    #[test]
    fn sizes_follow_the_view() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(view_scale(8000.0), 1.0));
        assert!(near(view_scale(16000.0), 1.0), "never larger");
        assert!(near(view_scale(49152.0), 1.0));
        assert!(near(view_scale(4000.0), 1.0 / 1.3));
        assert!(near(view_scale(2000.0), 1.0 / 1.69));
        // A medium name on a 4 km crop: 69.2 m type.
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: North::Game,
        };
        let a = text(0, 0, TextSize::Medium, "Deming");
        let svg = layers(&picture, &[&a], view_scale(4000.0)).join("\n");
        assert!(svg.contains("font-size=\"69.2308\""), "{svg}");
    }

    #[test]
    fn widths_come_from_the_table_and_err_wide_elsewhere() {
        assert_eq!(char_width(' '), 318.0);
        assert_eq!(char_width('A'), 684.0);
        assert_eq!(char_width('m'), 974.0);
        assert_eq!(char_width('é'), 615.0);
        assert_eq!(char_width('ſ'), 352.0); // U+017F, the table's last
        assert_eq!(char_width('Ω'), 750.0);
        assert_eq!(char_width('東'), 1000.0);
        assert!((text_width("Tonopah") - 4.351).abs() < 1e-9);
    }
}
