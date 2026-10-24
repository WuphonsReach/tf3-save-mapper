//! The `compass` layer: a white circle with a black outline in the lower
//! right corner, and a black compass rose in it: north an arrow, the other
//! three arms half as long with E, S and W at their ends, the letters upright.
//! The shape is that of `tools/icons/compass_rose.svg` in tf3-save-format.

use super::{Picture, fmt1, fmt2, layer};

/// Metres across the circle, at most an eighth of the picture's short side.
const DIAMETER: f64 = 250.0;

pub(super) fn compass(picture: &Picture) -> Vec<String> {
    let (w, h) = (picture.width, picture.height);
    let r = DIAMETER.min(w.min(h) / 8.0) / 2.0;
    let gap = 0.15 * r;
    let (cx, cy) = (w - gap - r, h - gap - r);
    let k = 0.0495 * r; // the icon's unit: its arrow is 15 k long
    let north = picture.vec((0.0, 1.0));
    let east = picture.vec((1.0, 0.0));
    let south = (-north.0, -north.1);
    let west = (-east.0, -east.1);
    let at =
        |v: (f64, f64), dist: f64| format!("{} {}", fmt1(cx + v.0 * dist), fmt1(cy + v.1 * dist));
    let centre = format!("{} {}", fmt1(cx), fmt1(cy));
    let arms: String = [(north, 15.0), (south, 7.5), (east, 7.5), (west, 7.5)]
        .iter()
        .map(|&(v, n)| format!("M{centre}L{}", at(v, n * k)))
        .collect();
    let perp = (-north.1, north.0);
    let side = |s: f64| {
        format!(
            "{} {}",
            fmt1(cx + north.0 * 11.0 * k + s * perp.0 * 3.0 * k),
            fmt1(cy + north.1 * 11.0 * k + s * perp.1 * 3.0 * k)
        )
    };
    let mut out = vec![
        layer("compass", "Compass rose", ""),
        format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#ffffff\" stroke=\"#000000\" stroke-width=\"{}\"/>",
            fmt1(cx),
            fmt1(cy),
            fmt1(r),
            fmt2(0.03 * r)
        ),
        format!(
            "<path d=\"{arms}\" stroke=\"#000000\" stroke-width=\"{}\" fill=\"none\"/>",
            fmt2(2.34 / 1.8 * k)
        ),
        format!(
            "<path d=\"M{}L{}L{}z\" fill=\"#000000\"/>",
            at(north, 18.0 * k),
            side(1.0),
            side(-1.0)
        ),
    ];
    for (letter, v) in [("E", east), ("S", south), ("W", west)] {
        out.push(format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"{}\" \
             font-weight=\"bold\" fill=\"#000000\">{letter}</text>",
            fmt1(cx + v.0 * 12.6 * k),
            fmt1(cy + v.1 * 12.6 * k + 0.35 * 8.5 * k),
            fmt1(8.5 * k)
        ));
    }
    out.push("</g>".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rose_sits_in_the_lower_right_with_north_up() {
        let lines = compass(&Picture {
            width: 4096.0,
            height: 2048.0,
        });
        let text = lines.join("\n");
        // 250 m across, 18.75 m from the corner.
        assert!(
            text.contains("<circle cx=\"3952.2\" cy=\"1904.2\" r=\"125.0\""),
            "{text}"
        );
        // The arrow points up the picture: its tip is above the centre.
        assert!(text.contains("M3952.2 1904.2L3952.2 1811.4"), "{text}");
        assert!(text.contains(">W</text>"));
    }

    #[test]
    fn a_small_picture_gets_a_small_rose() {
        let text = compass(&Picture {
            width: 800.0,
            height: 400.0,
        })
        .join("");
        assert!(text.contains("r=\"25.0\""), "{text}");
    }
}
