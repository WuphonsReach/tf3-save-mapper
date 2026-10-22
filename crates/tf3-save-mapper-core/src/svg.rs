//! The SVG writer.
//!
//! Coordinates inside the drawing are map metres from the picture's top-left
//! corner, with north up and east right (x is east and y is north in the game's
//! own frame, `docs/compass.md` in tf3-save-format). The `viewBox` is the map in
//! metres, so widths are in metres too and scale with the picture. Layers are
//! groups Inkscape shows as layers: `roads` and `rails`, each with one
//! sublayer per street or track type.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};

use crate::edges::{Edge, Family};
use crate::header::Header;

const ROAD_COLOUR: &str = "#323232";
const RAIL_COLOUR: &str = "#cd1e1e";
/// Plain paper under the network, so the dark roads read in a viewer with a dark theme.
const BACKGROUND_COLOUR: &str = "#ffffff";
/// A bend shows as a curve only above this many metres of bulge.
const CURVE_TOLERANCE: f64 = 0.25;

#[derive(Debug, Clone, Copy)]
pub struct SvgOptions {
    /// Stroke width of a road, metres. The real width of a street comes from its
    /// template in the game's files; until the tool reads those, one width serves.
    pub road_width: f64,
    /// Stroke width of a track, metres.
    pub rail_width: f64,
    /// The picture's size in pixels is the map's size divided by this.
    pub metres_per_pixel: f64,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            road_width: 16.0,
            rail_width: 4.0,
            metres_per_pixel: 8.0,
        }
    }
}

/// Write the roads and rails of `edges` as an SVG of the whole map.
pub fn write_svg(
    out: &mut impl Write,
    header: &Header,
    edges: &[Edge],
    options: &SvgOptions,
) -> io::Result<()> {
    let (w, h) = (f64::from(header.map_width), f64::from(header.map_height));
    let px = |x: f64| fmt1(x + w / 2.0);
    let py = |y: f64| fmt1(h / 2.0 - y);

    // Edges grouped by family and type, in stream order within each.
    let mut groups: BTreeMap<(u8, &str), Vec<&Edge>> = BTreeMap::new();
    for e in edges {
        let family = u8::from(e.family() == Family::Track);
        groups.entry((family, e.type_name())).or_default().push(e);
    }

    writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" \
         width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        (w / options.metres_per_pixel).ceil(),
        (h / options.metres_per_pixel).ceil(),
        w,
        h
    )?;
    writeln!(out, "<title>Roads and rails</title>")?;
    writeln!(out, "<desc>{}</desc>", escape(&description(header)))?;
    writeln!(
        out,
        "<g id=\"background\" inkscape:groupmode=\"layer\" inkscape:label=\"Background\">\
         <rect width=\"{w}\" height=\"{h}\" fill=\"{BACKGROUND_COLOUR}\"/></g>"
    )?;

    for (family, id, label, colour, width) in [
        (0, "roads", "Roads", ROAD_COLOUR, options.road_width),
        (1, "rails", "Rails", RAIL_COLOUR, options.rail_width),
    ] {
        writeln!(
            out,
            "<g id=\"{id}\" inkscape:groupmode=\"layer\" inkscape:label=\"{label}\">"
        )?;
        for ((_, type_name), members) in groups.range((family, "")..(family + 1, "")) {
            let prefix = if family == 0 { "road" } else { "rail" };
            writeln!(
                out,
                "<g id=\"{prefix}-{}\" inkscape:groupmode=\"layer\" inkscape:label=\"{label}: {}\" \
                 fill=\"none\" stroke=\"{colour}\" stroke-width=\"{width}\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\">",
                escape(type_name),
                escape(type_name)
            )?;
            let mut line = String::new();
            for e in members {
                let c = e.bezier();
                line.clear();
                if e.is_curved(CURVE_TOLERANCE) {
                    let _ = write!(
                        line,
                        "M{} {}C{} {} {} {} {} {}",
                        px(c[0].0),
                        py(c[0].1),
                        px(c[1].0),
                        py(c[1].1),
                        px(c[2].0),
                        py(c[2].1),
                        px(c[3].0),
                        py(c[3].1)
                    );
                } else {
                    let _ = write!(
                        line,
                        "M{} {}L{} {}",
                        px(c[0].0),
                        py(c[0].1),
                        px(c[3].0),
                        py(c[3].1)
                    );
                }
                writeln!(
                    out,
                    "<path data-a=\"{:x}\" data-b=\"{:x}\" d=\"{line}\"/>",
                    e.a, e.b
                )?;
            }
            writeln!(out, "</g>")?;
        }
        writeln!(out, "</g>")?;
    }
    writeln!(out, "</svg>")
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

    fn edge(template: &str, p0: [f32; 3], p1: [f32; 3]) -> Edge {
        let d = [p1[0] - p0[0], p1[1] - p0[1], 0.0];
        Edge {
            a: 0xab,
            b: 0xcd,
            p0,
            p1,
            t0: d,
            t1: d,
            template: Some(template.into()),
            offset: 0,
        }
    }

    fn render(edges: &[Edge]) -> String {
        let mut out = Vec::new();
        write_svg(&mut out, &header(), edges, &SvgOptions::default()).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn flips_y_and_centres_the_origin() {
        // x east and y north on the map; the picture's origin is its top-left corner.
        let svg = render(&[edge(
            "street/town/town_old_small",
            [-500.0, 1000.0, 0.0],
            [500.0, -1000.0, 0.0],
        )]);
        assert!(svg.contains("viewBox=\"0 0 1000 2000\""));
        assert!(svg.contains("width=\"125\" height=\"250\""));
        assert!(svg.contains("d=\"M0.0 0.0L1000.0 2000.0\""), "{svg}");
        assert!(svg.contains("data-a=\"ab\" data-b=\"cd\""));
    }

    #[test]
    fn roads_and_rails_get_their_own_layers_and_colours() {
        let svg = render(&[
            edge("street/town/town_old_small", [0.0; 3], [10.0, 0.0, 0.0]),
            edge("track/standard", [0.0; 3], [10.0, 0.0, 0.0]),
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
    fn background_is_the_first_layer() {
        let svg = render(&[edge("street/a/b", [0.0; 3], [10.0, 0.0, 0.0])]);
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
    fn no_negative_zero() {
        assert_eq!(fmt1(-0.04), "0.0");
        assert_eq!(fmt1(-0.06), "-0.1");
    }
}
