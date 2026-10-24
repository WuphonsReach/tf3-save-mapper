//! The SVG writer.
//!
//! Coordinates inside the drawing are map metres from the picture's top-left
//! corner, with north up and east right (x is east and y is north in the game's
//! own frame, `docs/compass.md` in tf3-save-format). The `viewBox` is the map in
//! metres, so widths are in metres too and scale with the picture. Layers are
//! groups Inkscape shows as layers, bottom to top: `background`, `terrain`,
//! `stations`, `roads` and `rails` (each with one sublayer per street or
//! track type), the depot layers, `stations-street`, `industries` and
//! `compass`.

mod depots;
mod industries;
mod rose;
mod stations;
mod terrain;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{self, Write};

use crate::edges::{Edge, Family};
use crate::header::Header;
use crate::install::{DepotSpec, Polygon, Rect};
use crate::instances::Instance;
use crate::terrain::Terrain;

pub use industries::faces_with_measured;
pub use stations::is_station_piece;

const ROAD_COLOUR: &str = "#323232";
const RAIL_COLOUR: &str = "#cd1e1e";
/// Plain paper under the network, so the dark roads read in a viewer with a dark theme.
const BACKGROUND_COLOUR: &str = "#ffffff";
/// A bend shows as a curve only above this many metres of bulge.
const CURVE_TOLERANCE: f64 = 0.25;
/// The outline of a station, depot or industry footprint, metres.
const OUTLINE: f64 = 1.5;

#[derive(Debug, Clone, Copy)]
pub struct SvgOptions {
    /// Stroke width of a road whose lanes did not read, metres. Every other
    /// edge is as wide as its lanes.
    pub road_width: f64,
    /// Stroke width of a track whose lanes did not read, metres.
    pub rail_width: f64,
    /// No road or track is drawn narrower than this, metres.
    pub min_width: f64,
    /// The picture's size in pixels is the map's size divided by this. The
    /// terrain is cut from every (this / 4)th grid point, and its lines are
    /// thinned and drawn to suit.
    pub metres_per_pixel: f64,
    /// Metres between height lines and between height bands.
    pub contour_step: f64,
    /// Every this many height lines one is drawn heavier.
    pub index_every: u32,
    /// Fill the height bands with a colour ramp; without, the land is one colour.
    pub bands: bool,
    /// Draw a compass rose in the lower right corner.
    pub rose: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            road_width: 16.0,
            rail_width: 4.0,
            min_width: 0.0,
            metres_per_pixel: 8.0,
            contour_step: 20.0,
            index_every: 5,
            bands: true,
            rose: true,
        }
    }
}

/// What is drawn besides the roads and rails. Each part is left out when empty.
#[derive(Debug, Clone, Default)]
pub struct Layers {
    pub terrain: Option<Terrain>,
    /// Each industry's main model instance and its industry key.
    pub industries: Vec<(Instance, String)>,
    /// Ground faces by industry key, in the industry's own axes.
    pub industry_faces: HashMap<String, Polygon>,
    pub depots: Vec<Instance>,
    /// Depot footprints by layer id (`install::DEPOT_KINDS`).
    pub depot_specs: HashMap<&'static str, DepotSpec>,
    pub stations: Vec<Instance>,
    /// Station piece boxes by model path.
    pub station_boxes: HashMap<String, Rect>,
}

/// Where map points fall in the picture: metres from its top-left corner,
/// x to the right and y down.
#[derive(Debug, Clone, Copy)]
struct Picture {
    width: f64,
    height: f64,
}

impl Picture {
    fn pic(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x + self.width / 2.0, self.height / 2.0 - y)
    }

    /// A map vector as a picture vector.
    fn vec(&self, (dx, dy): (f64, f64)) -> (f64, f64) {
        (dx, -dy)
    }
}

/// The opening tag of a layer: a group Inkscape shows as one.
fn layer(id: &str, label: &str, attrs: &str) -> String {
    format!(
        "<g id=\"{}\" inkscape:groupmode=\"layer\" inkscape:label=\"{}\"{attrs}>",
        escape(id),
        escape(label)
    )
}

/// Write the map: the terrain and the other `layers`, and the roads and
/// rails of `edges`.
pub fn write_svg(
    out: &mut impl Write,
    header: &Header,
    edges: &[Edge],
    layers: &Layers,
    options: &SvgOptions,
) -> io::Result<()> {
    let (w, h) = (f64::from(header.map_width), f64::from(header.map_height));
    let picture = Picture {
        width: w,
        height: h,
    };

    writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" \
         width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        (w / options.metres_per_pixel).ceil(),
        (h / options.metres_per_pixel).ceil(),
        w,
        h
    )?;
    writeln!(out, "<title>{}</title>", title(layers))?;
    writeln!(out, "<desc>{}</desc>", escape(&description(header)))?;
    writeln!(
        out,
        "{}<rect width=\"{w}\" height=\"{h}\" fill=\"{BACKGROUND_COLOUR}\"/></g>",
        layer("background", "Background", "")
    )?;
    if let Some(t) = &layers.terrain {
        terrain::write(out, t, &picture, options)?;
    }
    let (stations_under, stations_over) =
        stations::layers(&picture, &layers.stations, &layers.station_boxes);
    write_lines(out, &stations_under)?;
    write_network(out, &picture, edges, options)?;
    write_lines(
        out,
        &depots::layers(&picture, &layers.depots, &layers.depot_specs),
    )?;
    write_lines(out, &stations_over)?;
    write_lines(
        out,
        &industries::layers(&picture, &layers.industries, &layers.industry_faces),
    )?;
    if options.rose {
        write_lines(out, &rose::compass(&picture))?;
    }
    writeln!(out, "</svg>")
}

fn write_lines(out: &mut impl Write, lines: &[String]) -> io::Result<()> {
    for line in lines {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

fn title(layers: &Layers) -> &'static str {
    if layers.terrain.is_some() {
        "Terrain, roads and rails"
    } else {
        "Roads and rails"
    }
}

/// The edge's stroke width: its lanes' width, or the fallback for its
/// family, and at least `min_width`.
fn edge_width(e: &Edge, options: &SvgOptions) -> f64 {
    let fallback = match e.family {
        Family::Track => options.rail_width,
        Family::Street => options.road_width,
    };
    e.width.map_or(fallback, f64::from).max(options.min_width)
}

fn write_network(
    out: &mut impl Write,
    picture: &Picture,
    edges: &[Edge],
    options: &SvgOptions,
) -> io::Result<()> {
    // Edges grouped by family and type, in stream order within each.
    let mut groups: BTreeMap<(u8, &str), Vec<&Edge>> = BTreeMap::new();
    for e in edges {
        let family = u8::from(e.family == Family::Track);
        groups.entry((family, e.type_name())).or_default().push(e);
    }
    let pt = |p: (f64, f64)| {
        let (u, v) = picture.pic(p);
        format!("{} {}", fmt1(u), fmt1(v))
    };
    for (family, id, label, colour) in [
        (0, "roads", "Roads", ROAD_COLOUR),
        (1, "rails", "Rails", RAIL_COLOUR),
    ] {
        writeln!(out, "{}", layer(id, label, ""))?;
        for ((_, type_name), members) in groups.range((family, "")..(family + 1, "")) {
            let prefix = if family == 0 { "road" } else { "rail" };
            // The group's most common width goes on the group, any other on its path.
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for e in members {
                *counts.entry(fmt_width(edge_width(e, options))).or_default() += 1;
            }
            let common = counts
                .iter()
                .max_by_key(|(_, n)| **n)
                .map(|(w, _)| w.clone())
                .unwrap_or_default();
            writeln!(
                out,
                "{}",
                layer(
                    &format!("{prefix}-{type_name}"),
                    &format!("{label}: {type_name}"),
                    &format!(
                        " fill=\"none\" stroke=\"{colour}\" stroke-width=\"{common}\" \
                         stroke-linecap=\"round\" stroke-linejoin=\"round\""
                    )
                )
            )?;
            for e in members {
                let c = e.bezier();
                let d = if e.is_curved(CURVE_TOLERANCE) {
                    format!("M{}C{} {} {}", pt(c[0]), pt(c[1]), pt(c[2]), pt(c[3]))
                } else {
                    format!("M{}L{}", pt(c[0]), pt(c[3]))
                };
                let width = fmt_width(edge_width(e, options));
                let own = if width == common {
                    String::new()
                } else {
                    format!(" stroke-width=\"{width}\"")
                };
                writeln!(
                    out,
                    "<path data-a=\"{:x}\" data-b=\"{:x}\"{own} d=\"{d}\"/>",
                    e.a, e.b
                )?;
            }
            writeln!(out, "</g>")?;
        }
        writeln!(out, "</g>")?;
    }
    Ok(())
}

fn description(header: &Header) -> String {
    let mut text = format!(
        "Transport Fever 3 save, format {}, map {} x {} m",
        header.version, header.map_width, header.map_height
    );
    if let Some(climate) = &header.climate {
        let _ = write!(text, ", climate {climate}");
    }
    if let Some(seed) = &header.map_seed {
        let _ = write!(text, ", seed {seed}");
    }
    text
}

/// One decimal, and no `-0.0`.
fn fmt1(v: f64) -> String {
    let s = format!("{v:.1}");
    if s == "-0.0" { "0.0".into() } else { s }
}

/// Two decimals, and no `-0.00`.
fn fmt2(v: f64) -> String {
    let s = format!("{v:.2}");
    if s == "-0.00" { "0.00".into() } else { s }
}

/// A width to the centimetre, without trailing zeros: `16`, `4.5`.
fn fmt_width(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Escape text for an XML attribute or element, dropping characters XML 1.0 forbids.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(' '),
            c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Anything but letters, digits, `_` and `-` replaced by `_`, for an element id.
fn safe_id(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_bad = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
            last_bad = false;
        } else if !last_bad {
            out.push('_');
            last_bad = true;
        }
    }
    out
}

/// Points as `x,y x,y ...` for a polygon, two decimals.
fn points_attr(pts: &[(f64, f64)]) -> String {
    pts.iter()
        .map(|&(u, v)| format!("{},{}", fmt2(u), fmt2(v)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Closed loops as path data, two decimals.
fn loops_d(loops: &[Vec<(f64, f64)>]) -> String {
    let mut d = String::new();
    for lp in loops {
        for (i, &(u, v)) in lp.iter().enumerate() {
            let _ = write!(
                d,
                "{}{} {}",
                if i == 0 { 'M' } else { 'L' },
                fmt2(u),
                fmt2(v)
            );
        }
        d.push('Z');
    }
    d
}

/// Open polylines as path data, two decimals.
fn runs_d(runs: &[Vec<(f64, f64)>]) -> String {
    let mut d = String::new();
    for r in runs {
        for (i, &(u, v)) in r.iter().enumerate() {
            let _ = write!(
                d,
                "{}{} {}",
                if i == 0 { 'M' } else { 'L' },
                fmt2(u),
                fmt2(v)
            );
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            version: 604,
            start_year: 1900,
            map_width: 1000,
            map_height: 2000,
            date: 0,
            climate: Some("::/climate/temperate.clima".into()),
            map_seed: Some("a<b>&\"c\u{1}".into()),
        }
    }

    fn edge(template: &str, p0: [f32; 3], p1: [f32; 3], width: Option<f32>) -> Edge {
        let d = [p1[0] - p0[0], p1[1] - p0[1], 0.0];
        Edge {
            a: 0xab,
            b: 0xcd,
            p0,
            p1,
            t0: d,
            t1: d,
            template: Some(template.into()),
            family: if template.starts_with("track/") {
                Family::Track
            } else {
                Family::Street
            },
            width,
            offset: 0,
        }
    }

    fn render(edges: &[Edge]) -> String {
        let mut out = Vec::new();
        let options = SvgOptions {
            rose: false,
            ..SvgOptions::default()
        };
        write_svg(&mut out, &header(), edges, &Layers::default(), &options).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn flips_y_and_centres_the_origin() {
        // x east and y north on the map; the picture's origin is its top-left corner.
        let svg = render(&[edge(
            "street/town/town_old_small",
            [-500.0, 1000.0, 0.0],
            [500.0, -1000.0, 0.0],
            Some(16.0),
        )]);
        assert!(svg.contains("viewBox=\"0 0 1000 2000\""));
        assert!(svg.contains("width=\"125\" height=\"250\""));
        assert!(svg.contains("d=\"M0.0 0.0L1000.0 2000.0\""), "{svg}");
        assert!(svg.contains("data-a=\"ab\" data-b=\"cd\""));
    }

    #[test]
    fn roads_and_rails_get_their_own_layers_and_colours() {
        let svg = render(&[
            edge(
                "street/town/town_old_small",
                [0.0; 3],
                [10.0, 0.0, 0.0],
                Some(16.0),
            ),
            edge("track/standard", [0.0; 3], [10.0, 0.0, 0.0], Some(4.0)),
        ]);
        assert!(svg.contains("id=\"road-town_old_small\""));
        assert!(svg.contains("id=\"rail-standard\""));
        assert!(svg.contains("stroke=\"#323232\" stroke-width=\"16\""));
        assert!(svg.contains("stroke=\"#cd1e1e\" stroke-width=\"4\""));
        assert!(
            svg.find("id=\"roads\"") < svg.find("id=\"rails\""),
            "rails draw last"
        );
    }

    #[test]
    fn edges_are_as_wide_as_their_lanes() {
        let svg = render(&[
            edge("street/a/b", [0.0; 3], [10.0, 0.0, 0.0], Some(24.0)),
            edge("street/a/b", [0.0; 3], [20.0, 0.0, 0.0], Some(24.0)),
            edge("street/a/b", [0.0; 3], [30.0, 0.0, 0.0], Some(14.5)),
            edge("street/a/b", [0.0; 3], [40.0, 0.0, 0.0], None),
        ]);
        // The common width on the group; the others on their paths, the
        // unread one at the fallback width.
        assert!(
            svg.contains("stroke=\"#323232\" stroke-width=\"24\""),
            "{svg}"
        );
        assert!(svg.contains("stroke-width=\"14.5\" d=\"M500.0 1000.0L530.0 1000.0\""));
        assert!(svg.contains("stroke-width=\"16\" d=\"M500.0 1000.0L540.0 1000.0\""));
    }

    #[test]
    fn background_is_the_first_layer() {
        let svg = render(&[edge("street/a/b", [0.0; 3], [10.0, 0.0, 0.0], None)]);
        assert!(svg.contains("<rect width=\"1000\" height=\"2000\" fill=\"#ffffff\"/>"));
        assert!(svg.find("id=\"background\"") < svg.find("id=\"roads\""));
    }

    #[test]
    fn description_is_escaped() {
        let svg = render(&[]);
        assert!(svg.contains("seed a&lt;b&gt;&amp;&quot;c</desc>"), "{svg}");
        assert!(svg.contains("climate ::/climate/temperate.clima"));
    }

    #[test]
    fn number_formats() {
        assert_eq!(fmt1(-0.04), "0.0");
        assert_eq!(fmt1(-0.06), "-0.1");
        assert_eq!(fmt_width(16.0), "16");
        assert_eq!(fmt_width(4.25), "4.25");
        assert_eq!(fmt_width(2.5), "2.5");
        assert_eq!(safe_id("a b/c..d"), "a_b_c_d");
    }
}
