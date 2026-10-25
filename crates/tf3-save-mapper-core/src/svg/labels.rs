//! The `labels` layer: the edges asked for by their two end nodes, each
//! named along its middle (`edge-labels`), and a dot on each of their ends
//! (`nodes`). A label is the given text or the node pair, `2a619>342aa`,
//! with the edge's entity id after it where one is known; it sits on the
//! curve's middle, turned along it and kept upright, and is never moved.
//! Each labelled edge has its own light halo colour, and its end dots take
//! it; where two labelled edges meet, the half of the dot towards the first
//! one is in its colour.

use std::collections::HashMap;

use super::{Picture, bezier_point, escape, fmt_g, fmt1, layer};
use crate::edges::Edge;

/// Halo and dot colours, given to the labelled edges in turn.
const TINTS: [&str; 4] = ["#ffffff", "#ffe34d", "#5fe0ff", "#ff9ee0"];

/// An edge to label: its end nodes' ids, in either order, and the text to
/// write (the node pair when `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeLabel {
    pub a: u32,
    pub b: u32,
    pub text: Option<String>,
}

/// An unordered node pair.
fn pair(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

/// Rounded to a tenth, as the key of a node.
fn tenth(v: f64) -> i64 {
    (v * 10.0).round() as i64
}

/// The labels asked for that match none of `edges`.
pub fn unmatched<'a>(labels: &'a [EdgeLabel], edges: &[&Edge]) -> Vec<&'a EdgeLabel> {
    let have: std::collections::HashSet<(u32, u32)> =
        edges.iter().map(|e| pair(e.a, e.b)).collect();
    labels
        .iter()
        .filter(|l| !have.contains(&pair(l.a, l.b)))
        .collect()
}

pub(super) fn layers(
    picture: &Picture,
    edges: &[&Edge],
    labels: &[EdgeLabel],
    ids: &HashMap<usize, u32>,
    mpp: f64,
) -> Vec<String> {
    if labels.is_empty() {
        return Vec::new();
    }
    // A later request for the same pair replaces an earlier one.
    let texts: HashMap<(u32, u32), Option<&str>> = labels
        .iter()
        .map(|l| (pair(l.a, l.b), l.text.as_deref()))
        .collect();
    let font = 4.0 * mpp + 4.0;
    let size = 0.7 * font;
    let mut out = vec![layer(
        "labels",
        "Labels",
        &format!(
            " font-family=\"monospace\" font-size=\"{}\" text-anchor=\"middle\" fill=\"#000\"",
            fmt_g(size)
        ),
    )];
    let picked: Vec<&Edge> = edges
        .iter()
        .copied()
        .filter(|e| texts.contains_key(&pair(e.a, e.b)))
        .collect();
    let mut tint: HashMap<(u32, u32), &str> = HashMap::new();
    for e in &picked {
        let n = tint.len();
        tint.entry(pair(e.a, e.b)).or_insert(TINTS[n % TINTS.len()]);
    }
    // Each end: (its point, [(tint, the direction leaving it along the edge, the edge's name)]).
    type End<'t> = ((f64, f64), Vec<(&'t str, (f64, f64), String)>);
    let mut ends: Vec<End> = Vec::new();
    let mut end_at: HashMap<(i64, i64), usize> = HashMap::new();
    for e in &picked {
        let c = e.bezier();
        let colour = tint[&pair(e.a, e.b)];
        for ((x, y), (xo, yo)) in [(c[0], c[1]), (c[3], c[2])] {
            let (x, y) = (tenth(x) as f64 / 10.0, tenth(y) as f64 / 10.0);
            let k = *end_at.entry((tenth(x), tenth(y))).or_insert_with(|| {
                ends.push(((x, y), Vec::new()));
                ends.len() - 1
            });
            ends[k].1.push((
                colour,
                picture.vec((xo - x, yo - y)),
                format!("{:x}>{:x}", e.a, e.b),
            ));
        }
    }
    out.push(layer("edge-labels", "Edge labels", ""));
    for e in &picked {
        let key = pair(e.a, e.b);
        let c = e.bezier();
        let (m, n) = (bezier_point(&c, 0.5), bezier_point(&c, 0.51));
        let mut text = texts[&key].map_or_else(|| format!("{:x}>{:x}", e.a, e.b), str::to_owned);
        if let Some(id) = ids.get(&e.offset) {
            text += &format!(" e{id:x}");
        }
        let (ex, ey) = picture.vec((n.0 - m.0, n.1 - m.1));
        let mut angle = ey.atan2(ex).to_degrees();
        if !(-90.0..=90.0).contains(&angle) {
            angle -= if angle > 0.0 { 180.0 } else { -180.0 }; // kept upright
        }
        let (u, v) = picture.pic(m);
        out.push(format!(
            "<g class=\"edge-label\" data-a=\"{:x}\" data-b=\"{:x}\"><text transform=\"translate({} {}) rotate({})\" \
             y=\"{}\" paint-order=\"stroke\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\">{}</text></g>",
            e.a,
            e.b,
            fmt1(u),
            fmt1(v),
            fmt1(angle),
            fmt_g(size * 1.1),
            tint[&key],
            fmt_g(font / 4.0),
            escape(&text)
        ));
    }
    out.push("</g>".into());
    let r = size * 0.3;
    let ring = format!("stroke=\"#000\" stroke-width=\"{}\"", fmt_g(size * 0.08));
    out.push(layer("nodes", "Edge end nodes", ""));
    for ((x, y), parts) in &ends {
        let (cx, cy) = picture.pic((*x, *y));
        let names: Vec<&str> = parts.iter().map(|p| p.2.as_str()).collect();
        out.push(format!(
            "<g class=\"node\" data-x=\"{}\" data-y=\"{}\" data-edges=\"{}\">",
            fmt_g(*x),
            fmt_g(*y),
            names.join(" ")
        ));
        out.push(format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" {ring}/>",
            fmt1(cx),
            fmt1(cy),
            fmt_g(r),
            parts[parts.len() - 1].0
        ));
        if parts.len() > 1 {
            // A join: the half towards the first edge in its colour.
            let (dx, dy) = parts[0].1;
            let n = match dx.hypot(dy) {
                0.0 => 1.0,
                n => n,
            };
            let (px, py) = (-dy / n * r, dx / n * r);
            out.push(format!(
                "<path d=\"M{} {} A{} {} 0 0 0 {} {} Z\" fill=\"{}\" {ring}/>",
                fmt1(cx + px),
                fmt1(cy + py),
                fmt_g(r),
                fmt_g(r),
                fmt1(cx - px),
                fmt1(cy - py),
                parts[0].0
            ));
        }
        out.push("</g>".into());
    }
    out.push("</g>".into());
    out.push("</g>".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::Family;

    fn edge(a: u32, b: u32, p0: [f32; 3], p1: [f32; 3], offset: usize) -> Edge {
        let d = [p1[0] - p0[0], p1[1] - p0[1], 0.0];
        Edge {
            a,
            b,
            p0,
            p1,
            t0: d,
            t1: d,
            template: None,
            family: Family::Street,
            width: None,
            offset,
        }
    }

    #[test]
    fn labels_sit_on_the_middle_and_stay_upright() {
        let picture = Picture {
            width: 1000.0,
            height: 1000.0,
            north: crate::svg::North::Game,
        };
        // Running west: the text is turned back so it reads left to right.
        let west = edge(0x2a619, 0x342aa, [100.0, 0.0, 0.0], [0.0, 0.0, 0.0], 7);
        let north = edge(0x342aa, 0x10, [0.0, 0.0, 0.0], [0.0, 100.0, 0.0], 9);
        let other = edge(1, 2, [300.0, 0.0, 0.0], [400.0, 0.0, 0.0], 11);
        let labels = [
            EdgeLabel {
                a: 0x342aa,
                b: 0x2a619,
                text: None,
            },
            EdgeLabel {
                a: 0x10,
                b: 0x342aa,
                text: Some("main <st>".into()),
            },
            EdgeLabel {
                a: 5,
                b: 6,
                text: None,
            },
        ];
        let ids = HashMap::from([(7, 0x7de1)]);
        let edges = [&west, &north, &other];
        let text = layers(&picture, &edges, &labels, &ids, 1.0).join("\n");
        assert!(
            text.contains("translate(550.0 500.0) rotate(0.0)\" y=\"6.16\""),
            "{text}"
        );
        assert!(text.contains(">2a619&gt;342aa e7de1</text>"), "{text}");
        assert!(text.contains("rotate(-90.0)"));
        assert!(
            text.contains("stroke=\"#ffe34d\""),
            "the second edge's tint"
        );
        assert!(text.contains(">main &lt;st&gt;</text>"));
        assert!(!text.contains("data-a=\"1\""), "not asked for");
        // The shared node gets a half dot in the first edge's colour.
        assert!(text.contains("data-x=\"0\" data-y=\"0\" data-edges=\"2a619>342aa 342aa>10\""));
        assert_eq!(text.matches("<path d=\"M").count(), 1);
        assert_eq!(text.matches("<circle").count(), 3);
        let missing = unmatched(&labels, &edges);
        assert_eq!(missing.len(), 1);
        assert_eq!((missing[0].a, missing[0].b), (5, 6));
    }
}
