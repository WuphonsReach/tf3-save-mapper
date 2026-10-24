//! The `compass` layer: a white circle with a black outline in a corner of
//! the picture (the lower right by default) or round a map point, and a
//! black compass rose in it: north an arrow, the other three arms half as
//! long with E, S and W at their ends, the letters upright. The arms point
//! where north and east lie in the picture, so with `North::Sun` north is
//! to the left. The shape is that of `tools/icons/compass_rose.svg` in
//! tf3-save-format.

use super::{Picture, Rose, View, fmt1, fmt2, layer};

/// Metres across the circle by default, at most an eighth of the picture's short side.
const DIAMETER: f64 = 250.0;

pub(super) fn compass(
    picture: &Picture,
    view: &View,
    place: Rose,
    diameter: Option<f64>,
) -> Vec<String> {
    let (w, h) = (view.width, view.height);
    let r = diameter.unwrap_or_else(|| DIAMETER.min(w.min(h) / 8.0)) / 2.0;
    let gap = 0.15 * r;
    let (left, top) = (view.u0 + gap + r, view.v0 + gap + r);
    let (right, bottom) = (view.u0 + w - gap - r, view.v0 + h - gap - r);
    let (cx, cy) = match place {
        Rose::LowerRight => (right, bottom),
        Rose::LowerLeft => (left, bottom),
        Rose::UpperLeft => (left, top),
        Rose::UpperRight => (right, top),
        Rose::At(x, y) => picture.pic((x, y)),
    };
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
        let p = Picture {
            width: 4096.0,
            height: 2048.0,
            north: crate::svg::North::Game,
        };
        let lines = compass(
            &p,
            &View::new(&p, (-2048.0, 2048.0, -1024.0, 1024.0)),
            Rose::LowerRight,
            None,
        );
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
        let p = Picture {
            width: 800.0,
            height: 400.0,
            north: crate::svg::North::Game,
        };
        let view = View::new(&p, (-400.0, 400.0, -200.0, 200.0));
        let text = compass(&p, &view, Rose::LowerRight, None).join("");
        assert!(text.contains("r=\"25.0\""), "{text}");
        // A size asked for is not capped.
        let text = compass(&p, &view, Rose::UpperLeft, Some(300.0)).join("");
        assert!(
            text.contains("<circle cx=\"172.5\" cy=\"172.5\" r=\"150.0\""),
            "{text}"
        );
    }

    #[test]
    fn a_rose_round_a_map_point_and_turned_for_the_sun() {
        let p = Picture {
            width: 800.0,
            height: 400.0,
            north: crate::svg::North::Sun,
        };
        // Sun: the picture is 400 wide (y) and 800 high (x); up is +x.
        let view = View::new(&p, (-400.0, 400.0, -200.0, 200.0));
        assert_eq!((view.width, view.height), (400.0, 800.0));
        let text = compass(&p, &view, Rose::At(100.0, -50.0), Some(100.0)).join("\n");
        // (100, -50) is at u = 200 + 50, v = 400 - 100.
        assert!(
            text.contains("<circle cx=\"250.0\" cy=\"300.0\" r=\"50.0\""),
            "{text}"
        );
        // North (+y) points left: the arrow's tip is west of the centre.
        let k = 0.0495 * 50.0;
        assert!(
            text.contains(&format!("M250.0 300.0L{} 300.0", fmt1(250.0 - 15.0 * k))),
            "{text}"
        );
    }
}
