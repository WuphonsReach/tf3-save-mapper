//! Grades: each edge of a kind asked for (`FAMILIES`: roads with
//! `SvgOptions::road_grades`, tram lines with `tram_grades`, tracks with
//! `rail_grades`) coloured by its grade ([`Edge::grade`]) in the
//! layer `grades`, one of the overlays drawn over everything taken from the
//! save, the tunnels' and bridges' edges too.
//!
//! The colours are 14 bands, from dark green on the level through yellow to
//! dark red, and magenta at or over the family's limit: each band is a
//! thirteenth of the limit wide, so at the tracks' default 13 % (the
//! steepest the game's track types allow, `maxSlope`) one per whole percent.
//! Roads and tram lines default to 20 %: their types set no limit, but the
//! saves show the game builds them up to that. Another limit spreads the
//! same 14 colours under it. Each coloured line has a thin
//! near-black casing, so the colours stand out on any terrain. An edge in a
//! tunnel keeps its dashes, so the layer still says what runs underground;
//! one on a bridge is as wide as the bridge's fill, so the bridge's own
//! casing still shows outside it.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};

use super::raster::Levels;
use super::{
    Picture, SvgOptions, View, edge_path, edge_width, fmt_width, fmt1, fmt2, layer, most_common,
};
use crate::edges::{Edge, Family};
use crate::structures::Structure;

/// The colours by band, the last for grades at or over the limit. Evenly
/// spaced in OKLab along ColorBrewer's RdYlGn (green first, its pale centre
/// swapped for a yellow); the magenta stands apart from the ramp.
pub(super) const PALETTE: [[u8; 3]; 14] = [
    [0, 104, 55],
    [15, 132, 70],
    [49, 160, 84],
    [97, 186, 97],
    [149, 209, 104],
    [198, 231, 97],
    [247, 222, 59],
    [254, 191, 83],
    [251, 157, 89],
    [245, 117, 70],
    [228, 77, 51],
    [201, 37, 39],
    [165, 0, 38],
    [197, 27, 138],
];
/// The casing along each side of a coloured line, metres, and its colour.
pub(super) const CASING: f64 = 0.75;
pub(super) const CASING_COLOUR: [u8; 3] = [30, 30, 30];
/// The limits the colours are spread over when none is given, percent: the
/// tracks', and the roads' and tram lines'.
pub(super) const RAIL_DEFAULT_LIMIT: f64 = 13.0;
pub(super) const STREET_DEFAULT_LIMIT: f64 = 20.0;

/// A kind of edge whose grades can be coloured.
pub(super) struct GradeFamily {
    /// In the sublayers' ids (`grades-rail`) and labels.
    id: &'static str,
    label: &'static str,
    /// The row's name in the key.
    key_label: &'static str,
    is: fn(&Edge) -> bool,
    asked: fn(&SvgOptions) -> bool,
    /// The grade, percent, at which the last colour starts.
    limit: fn(&SvgOptions) -> f64,
}

fn is_track(e: &Edge) -> bool {
    e.family == Family::Track
}

fn road_grades(options: &SvgOptions) -> bool {
    options.road_grades
}

fn road_limit(options: &SvgOptions) -> f64 {
    options.road_grade_limit
}

fn tram_grades(options: &SvgOptions) -> bool {
    options.tram_grades
}

fn tram_limit(options: &SvgOptions) -> f64 {
    options.tram_grade_limit
}

fn rail_grades(options: &SvgOptions) -> bool {
    options.rail_grades
}

fn rail_limit(options: &SvgOptions) -> f64 {
    options.rail_grade_limit
}

/// Bottom to top: roads (a street with tram lanes among them), then tram
/// lines of their own, then rails. No edge is in two.
pub(super) const FAMILIES: &[GradeFamily] = &[
    GradeFamily {
        id: "road",
        label: "Roads",
        key_label: "Road Grade",
        is: Edge::is_road,
        asked: road_grades,
        limit: road_limit,
    },
    GradeFamily {
        id: "tram",
        label: "Tram lines",
        key_label: "Tram Grade",
        is: Edge::is_tram_only,
        asked: tram_grades,
        limit: tram_limit,
    },
    GradeFamily {
        id: "rail",
        label: "Rails",
        key_label: "Rail Grade",
        is: is_track,
        asked: rail_grades,
        limit: rail_limit,
    },
];

/// Whether any grades are asked for.
pub(super) fn any(options: &SvgOptions) -> bool {
    FAMILIES.iter().any(|f| (f.asked)(options))
}

/// The grade as written in `data-grade`: percent, two decimals.
fn shown(e: &Edge) -> f64 {
    (e.grade() * 100.0).round() / 100.0
}

/// The band of a grade, percent, under `limit`: a thirteenth of it each,
/// the last from the limit on.
fn band(grade: f64, limit: f64) -> usize {
    let last = PALETTE.len() - 1;
    ((grade * last as f64 / limit).floor().max(0.0) as usize).min(last)
}

/// The place among `FAMILIES` of the kind asked for that `e` is, and its
/// colour; `None` for an edge whose grade is not drawn. The colour is the
/// band of the grade as written, so the two agree.
pub(super) fn shade(e: &Edge, options: &SvgOptions) -> Option<(usize, [u8; 3])> {
    FAMILIES
        .iter()
        .enumerate()
        .find(|(_, f)| (f.asked)(options) && (f.is)(e))
        .map(|(k, f)| (k, PALETTE[band(shown(e), (f.limit)(options))]))
}

pub(super) fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The layer `grades`, written whenever grades are asked for: per family,
/// bottom to top as `FAMILIES` has them, a sublayer per level, the tunnels'
/// (`grades-rail-tunnel`, dashed as the tunnel under it), the ground's
/// (`grades-rail`) and the bridges' (`grades-rail-bridge`), each written only
/// when it holds an edge. In each the casings come first, then the colours.
/// Each edge's path has its grade in percent, two decimals, in `data-grade`.
pub(super) fn write(
    out: &mut impl Write,
    picture: &Picture,
    edges: &[&Edge],
    ids: &HashMap<usize, u32>,
    levels: &Levels,
    options: &SvgOptions,
) -> io::Result<()> {
    writeln!(
        out,
        "{}",
        layer(
            "grades",
            "Grades",
            " fill=\"none\" stroke-linejoin=\"round\""
        )
    )?;
    let levels_in_order = [
        (Some(Structure::Tunnel), "-tunnel", " in tunnels", "butt"),
        (None, "", "", "round"),
        (Some(Structure::Bridge), "-bridge", " on bridges", "butt"),
    ];
    for family in FAMILIES.iter().filter(|f| (f.asked)(options)) {
        let limit = (family.limit)(options);
        for &(level, suffix, place, cap) in &levels_in_order {
            let members: Vec<&Edge> = edges
                .iter()
                .copied()
                .filter(|e| (family.is)(e) && levels.structures.get(&e.offset).copied() == level)
                .collect();
            if members.is_empty() {
                continue;
            }
            let dashed = level == Some(Structure::Tunnel);
            // The commonest width and dashes on the group, any other on its path.
            let mut widths: BTreeMap<String, usize> = BTreeMap::new();
            let mut patterns: BTreeMap<String, usize> = BTreeMap::new();
            let pattern = |e: &Edge| {
                levels
                    .dashes
                    .get(&e.offset)
                    .filter(|_| dashed)
                    .map(|d| format!("{} {}", fmt_width(d.dash), fmt_width(d.gap)))
            };
            for e in &members {
                *widths.entry(fmt_width(edge_width(e, options))).or_default() += 1;
                if let Some(p) = pattern(e) {
                    *patterns.entry(p).or_default() += 1;
                }
            }
            let width = most_common(widths);
            let cased = |w: &str| fmt_width(w.parse::<f64>().unwrap_or(0.0) + 2.0 * CASING);
            let common_pattern = (!patterns.is_empty()).then(|| most_common(patterns));
            let group_dashes = common_pattern
                .as_ref()
                .map_or(String::new(), |p| format!(" stroke-dasharray=\"{p}\""));
            let dashes = |e: &Edge| match levels.dashes.get(&e.offset) {
                Some(d) if dashed => {
                    let own = pattern(e).filter(|p| Some(p) != common_pattern.as_ref());
                    format!(
                        "{} stroke-dashoffset=\"{}\"",
                        own.map_or(String::new(), |p| format!(" stroke-dasharray=\"{p}\"")),
                        fmt1(d.phase)
                    )
                }
                _ => String::new(),
            };
            writeln!(
                out,
                "{}",
                layer(
                    &format!("grades-{}{suffix}", family.id),
                    &format!("Grades: {}{place}", family.label),
                    &format!(
                        " stroke-width=\"{width}\" stroke-linecap=\"{cap}\"{group_dashes} \
                         data-limit=\"{}\"",
                        fmt_width(limit)
                    )
                )
            )?;
            writeln!(
                out,
                "<g class=\"casing\" stroke=\"{}\" stroke-width=\"{}\">",
                hex(CASING_COLOUR),
                cased(&width)
            )?;
            for e in &members {
                let own = fmt_width(edge_width(e, options));
                let own = if own == width {
                    String::new()
                } else {
                    format!(" stroke-width=\"{}\"", cased(&own))
                };
                writeln!(
                    out,
                    "<path{own}{} d=\"{}\"/>",
                    dashes(e),
                    edge_path(picture, e)
                )?;
            }
            writeln!(out, "</g>")?;
            for e in members {
                let grade = shown(e);
                let own = fmt_width(edge_width(e, options));
                let own = if own == width {
                    String::new()
                } else {
                    format!(" stroke-width=\"{own}\"")
                };
                let id = ids
                    .get(&e.offset)
                    .map_or(String::new(), |id| format!(" data-edge=\"{id:x}\""));
                writeln!(
                    out,
                    "<path data-a=\"{:x}\" data-b=\"{:x}\"{id} data-grade=\"{grade:.2}\" stroke=\"{}\"{own}{} d=\"{}\"/>",
                    e.a,
                    e.b,
                    hex(PALETTE[band(grade, limit)]),
                    dashes(e),
                    edge_path(picture, e)
                )?;
            }
            writeln!(out, "</g>")?;
        }
    }
    writeln!(out, "</g>")
}

/// The key's width as a share of the picture's, and its parts in units of
/// one swatch's pitch: the strip's height, the gap between swatches, the
/// numbers' size, the panel's padding and its distance from the bottom edge.
const KEY_SHARE: f64 = 0.4;
const KEY_STRIP: f64 = 0.5;
const KEY_GAP: f64 = 0.1;
const KEY_FONT: f64 = 0.5;
const KEY_PAD: f64 = 0.35;
const KEY_MARGIN: f64 = 0.4;
/// Between rows, below the numbers.
const KEY_ROW_GAP: f64 = 0.3;
/// A number's average character width, in font sizes (a safe overestimate,
/// as the grid's labels take it).
const KEY_EM: f64 = 0.62;
const KEY_INK: &str = "#323232";

/// The grades at a row's 14 boundaries under `limit`, percent, all in one
/// format: whole numbers when every one is whole at one decimal (`0` to
/// `13`), else every one with one decimal (`0.0`, `0.8` to `10.0`).
fn key_numbers(limit: f64) -> Vec<String> {
    let last = PALETTE.len() - 1;
    let numbers: Vec<String> = (0..=last)
        .map(|k| format!("{:.1}", limit * k as f64 / last as f64))
        .collect();
    if numbers.iter().all(|n| n.ends_with(".0")) {
        numbers
            .iter()
            .map(|n| n.trim_end_matches(".0").to_owned())
            .collect()
    } else {
        numbers
    }
}

/// Which of the 14 boundaries of a row get their number: the first, the
/// limit (the last), and every `step`th between, where `step` keeps the
/// widest number at `font` clear of its neighbours `pitch` apart; one that
/// would crowd the limit is left out.
fn key_marks(numbers: &[String], font: f64, pitch: f64) -> Vec<usize> {
    let widest = numbers
        .iter()
        .map(|n| n.len() as f64 * KEY_EM * font)
        .fold(0.0, f64::max);
    let step = ((widest / (0.9 * pitch)).ceil() as usize).max(1);
    let last = numbers.len() - 1;
    (0..=last)
        .filter(|&k| k == last || (k % step == 0 && last - k >= step))
        .collect()
}

/// The layer `grades-key`: a white panel along the bottom of the picture,
/// in the middle, with a row per family drawn (top to bottom as `FAMILIES`
/// has them: roads, tram lines, rails): its name, its 14 colours with a gap
/// between each two, and `%`; under them the grade at each gap, the first
/// at the strip's start; the last colour runs from the limit up, so the
/// last number is the limit. The key is 40 % of the picture's width.
pub(super) fn key(view: &View, options: &SvgOptions) -> Vec<String> {
    let families: Vec<&GradeFamily> = FAMILIES.iter().filter(|f| (f.asked)(options)).collect();
    if families.is_empty() {
        return Vec::new();
    }
    let colours = PALETTE.len() as f64;
    let pitch = KEY_SHARE * view.width / colours;
    let (strip, gap, font, pad) = (
        KEY_STRIP * pitch,
        KEY_GAP * pitch,
        KEY_FONT * pitch,
        KEY_PAD * pitch,
    );
    let row = strip + 0.25 * font + font;
    let rows = families.len() as f64;
    let names = families
        .iter()
        .map(|f| f.key_label.len() as f64 * KEY_EM * font)
        .fold(0.0, f64::max)
        + 0.5 * font;
    // The name, a space, the strip, a space and the unit; the first number
    // reaches into the space before the strip.
    let panel_w =
        2.0 * pad + names + 0.5 * font + colours * pitch - gap + 0.25 * font + KEY_EM * font;
    let panel_h = 2.0 * pad + rows * row + (rows - 1.0) * KEY_ROW_GAP * pitch;
    let left = view.u0 + (view.width - panel_w) / 2.0;
    let top = view.v0 + view.height - KEY_MARGIN * pitch - panel_h;
    let text = |x: f64, y: f64, anchor: &str, body: &str| {
        format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\">{body}</text>",
            fmt1(x),
            fmt1(y)
        )
    };
    let mut out = vec![
        layer(
            "grades-key",
            "Grades key",
            &format!(
                " font-family=\"sans-serif\" font-size=\"{}\" fill=\"{KEY_INK}\"",
                fmt2(font)
            ),
        ),
        format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" \
             stroke=\"#000000\" stroke-width=\"{}\"/>",
            fmt1(left),
            fmt1(top),
            fmt1(panel_w),
            fmt1(panel_h),
            fmt2(0.04 * pitch)
        ),
    ];
    for (i, family) in families.iter().enumerate() {
        let limit = (family.limit)(options);
        let y = top + pad + i as f64 * (row + KEY_ROW_GAP * pitch);
        let x0 = left + pad + names + 0.5 * font;
        out.push(format!(
            "<g class=\"key-row\" data-family=\"{}\" data-limit=\"{}\">",
            family.id,
            fmt_width(limit)
        ));
        out.push(text(
            x0 - 0.5 * font,
            y + strip / 2.0 + 0.35 * font,
            "end",
            family.key_label,
        ));
        for (k, colour) in PALETTE.iter().enumerate() {
            out.push(format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" \
                 stroke-width=\"{}\"/>",
                fmt2(x0 + k as f64 * pitch),
                fmt1(y),
                fmt2(pitch - gap),
                fmt1(strip),
                hex(*colour),
                hex(CASING_COLOUR),
                fmt2(0.04 * pitch)
            ));
        }
        // The boundaries: the strip's start, then each gap.
        let numbers = key_numbers(limit);
        let baseline = y + strip + 0.25 * font + 0.8 * font;
        for k in key_marks(&numbers, font, pitch) {
            let x = if k == 0 {
                x0
            } else {
                x0 + k as f64 * pitch - gap / 2.0
            };
            out.push(text(x, baseline, "middle", &numbers[k]));
        }
        out.push(text(
            x0 + colours * pitch - gap + 0.25 * font,
            y + strip / 2.0 + 0.35 * font,
            "start",
            "%",
        ));
        out.push("</g>".to_owned());
    }
    out.push("</g>".to_owned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_per_whole_percent_at_the_default_limit() {
        let b = |g| band(g, RAIL_DEFAULT_LIMIT);
        assert_eq!(
            [b(0.0), b(0.99), b(1.0), b(2.5), b(3.0), b(3.99)],
            [0, 0, 1, 2, 3, 3]
        );
        assert_eq!([b(12.99), b(13.0), b(25.0)], [12, 13, 13]);
    }

    #[test]
    fn a_lower_limit_squeezes_the_bands() {
        // 10 % over 13 bands: 0.77 % each.
        let b = |g| band(g, 10.0);
        assert_eq!(
            [b(0.76), b(0.77), b(3.0), b(9.99), b(10.0)],
            [0, 1, 3, 12, 13]
        );
    }

    #[test]
    fn the_ramp_runs_green_to_red_and_magenta_over_the_limit() {
        assert_eq!(hex(PALETTE[0]), "#006837");
        assert_eq!(hex(PALETTE[12]), "#a50026");
        assert_eq!(hex(PALETTE[13]), "#c51b8a");
    }

    #[test]
    fn the_key_numbers_the_gaps_in_one_format_and_thins_crowded_ones() {
        let whole = key_numbers(13.0);
        assert_eq!(whole[..3], ["0", "1", "2"]);
        assert_eq!(whole[13], "13");
        // One number needs a decimal, so all have one.
        let tenths = key_numbers(10.0);
        assert_eq!(tenths[..4], ["0.0", "0.8", "1.5", "2.3"]);
        assert_eq!(tenths[13], "10.0");
        let halves = key_numbers(6.5);
        assert_eq!(halves[..3], ["0.0", "0.5", "1.0"]);
        // Two characters at half a pitch fit at every gap.
        assert_eq!(key_marks(&whole, 0.5, 1.0), (0..=13).collect::<Vec<_>>());
        // Three or four do not: every second, and 12 left out beside 13.
        assert_eq!(key_marks(&tenths, 0.5, 1.0), [0, 2, 4, 6, 8, 10, 13]);
    }

    #[test]
    fn only_what_is_asked_for_is_shaded() {
        let options = SvgOptions::default();
        assert!(!any(&options));
        let options = SvgOptions {
            rail_grades: true,
            ..options
        };
        assert!(any(&options));
    }
}
