//! The `grid` and `grid-labels` layers: marks at round map coordinates, and
//! the coordinates written along the picture's top and left edges.
//!
//! Where a line of constant x meets a line of constant y at a multiple of
//! the step there is a cross, small in `grid-minor` and larger in
//! `grid-major` where both lines are at a multiple of the major step (1000 m,
//! or the step itself for the default grid, which has no minor marks). The
//! grid is a set of marks, not lines that run along the roads: the major
//! lines are dotted, a round dot every half dot step, except at 1 m per pixel
//! or finer, and the minor lines are drawn only when asked for (as sparse
//! dots or faint solid lines). Labels are never moved to stay legible: a
//! left one that would sit in the top row of labels, and a top one that would
//! run off the right edge, are left out. The marks go over the terrain and
//! under the network; the labels over everything but the compass rose. The
//! lines that run up the picture are labelled along the top (constant x, or
//! constant y with `North::Sun`), those that run across it along the left.

use super::{North, Picture, View, fmt_g, layer};

/// The grid's ink: violet, which no terrain, road or rail colour is near.
const INK: &str = "#6b3fa0";
/// The closest two dots of a dotted line may be, in pixels.
const MIN_DOT_PIXELS: f64 = 10.0;
/// A label's average character width, in font sizes (a safe overestimate).
const LABEL_EM: f64 = 0.62;
/// The major step when a minor step is asked for, metres.
pub(super) const MAJOR: u32 = 1000;
/// A grid with more crosses than this is refused: `--grid 1` on a big map
/// would be gigabytes of marks.
const MAX_CROSSES: f64 = 1_000_000.0;

/// Which grid the picture gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridStep {
    /// Major marks only, every third of the picture's shorter side rounded
    /// down to 1, 2 or 5 times a power of ten (at least 1000 m on a whole map).
    Auto,
    /// No grid.
    Off,
    /// Marks every this many metres, the 1000 m ones major.
    Every(u32),
}

/// Which grid lines are drawn besides the crosses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLines {
    /// The major lines dotted, except at 1 m per pixel or finer.
    Auto,
    /// Crosses only.
    None,
    /// The major lines dotted, and the minor lines as sparse dots.
    Dots,
    /// The major lines dotted, and the minor lines as faint solid lines.
    Faint,
}

/// The default step: a third of `short_side` metres rounded down to 1, 2
/// or 5 times a power of ten, and on a whole map at least 1000 m.
pub(super) fn auto_step(short_side: f64, cropped: bool) -> u32 {
    let third = short_side / 3.0;
    let power = 10f64.powi(third.log10().floor() as i32);
    let step = [5.0, 2.0, 1.0]
        .iter()
        .map(|m| m * power)
        .find(|s| *s <= third)
        .unwrap_or(power);
    let step = step.round().max(1.0) as u32;
    if cropped { step } else { step.max(1000) }
}

/// An error when a grid every `step` metres over `view` would have more
/// than `MAX_CROSSES` crosses.
pub(super) fn check_size(view: &View, step: u32) -> std::io::Result<()> {
    let lines = |metres: f64| (metres / f64::from(step)).floor() + 1.0;
    let crosses = lines(view.width) * lines(view.height);
    if crosses > MAX_CROSSES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "a grid every {step} m would have {crosses} crosses, more than {} million: \
                 raise --grid",
                MAX_CROSSES / 1_000_000.0
            ),
        ));
    }
    Ok(())
}

/// The smallest multiple of `step` at or above `v`.
fn first_multiple(v: f64, step: i64) -> i64 {
    (v.floor() as i64).div_euclid(step) * step
        + if (v.floor() as i64).rem_euclid(step) == 0 {
            0
        } else {
            step
        }
}

/// The marks and the labels of a grid every `step` metres with the
/// multiples of `major` major, in that order: the marks go under the
/// network, the labels over it.
pub(super) fn layers(
    picture: &Picture,
    view: &View,
    mpp: f64,
    step: u32,
    major: u32,
    lines: GridLines,
    dot_step: f64,
) -> (Vec<String>, Vec<String>) {
    let (w, h) = (view.width, view.height);
    let (u0, v0) = (view.u0, view.v0);
    let stepf = f64::from(step);
    // Smaller on a close crop, no bigger for a wide step.
    let size = (7.0 * mpp)
        .min(0.03 * w.max(h))
        .max((stepf * 0.07).min(70.0));
    let ink = format!("fill=\"none\" stroke=\"{INK}\"");
    let sw = mpp; // one pixel: a small cross's arm, and every dot
    let mark = format!(
        "{ink} stroke-opacity=\"0.65\" stroke-width=\"{}\"",
        fmt_g(sw)
    );
    let big_mark = format!(
        "{ink} stroke-opacity=\"0.85\" stroke-width=\"{}\"",
        fmt_g(1.5 * sw)
    );
    let text = format!(
        "font-family=\"sans-serif\" font-size=\"{}\" fill=\"#000\" stroke=\"#fff\" stroke-width=\"{}\" \
         paint-order=\"stroke\"",
        fmt_g(size),
        fmt_g(size / 5.0)
    );
    // A whole number of dots between two crosses.
    let dot_step = dot_step.max(MIN_DOT_PIXELS * mpp);
    let dot_step = stepf / (stepf / dot_step).round_ties_even().max(1.0);
    let gap = dot_step / 2.0; // a major line's dots are twice as close, so it reads as a line
    let big_arm = 0.8 * gap; // just short of the first dot
    let arm = dot_step / 6.0;

    let step_i = i64::from(step);
    let sun = picture.north == North::Sun;
    // The lines that run up the picture: constant x (constant y for the sun,
    // falling to the right), and those that run across it, falling downwards.
    let ((up_axis, up_lo, up_hi), (across_axis, across_lo, across_hi)) = if sun {
        (("y", view.y_min, view.y_max), ("x", view.x_min, view.x_max))
    } else {
        (("x", view.x_min, view.x_max), ("y", view.y_min, view.y_max))
    };
    let first = first_multiple(up_lo, step_i);
    let mut ups: Vec<i64> = (first..=up_hi.floor() as i64)
        .step_by(step as usize)
        .collect();
    if sun {
        ups.reverse();
    }
    let last = (across_hi.floor() as i64).div_euclid(step_i) * step_i;
    let low = across_lo.ceil() as i64;
    let acrosses: Vec<i64> = std::iter::successors(Some(last), |v| Some(v - step_i))
        .take_while(|v| *v >= low)
        .collect();
    // Dots sit at multiples of the dot step in map metres, so crossing lines'
    // dots meet at the crosses. The map coordinate along a line from the top
    // edge falls downwards; along one from the left edge it rises (game) or
    // falls (sun) to the right.
    let (up_start, across_start) = if sun {
        (view.x_max, view.y_max)
    } else {
        (view.y_max, view.x_min)
    };
    let along = |start: f64, every: f64| {
        if sun {
            (-start).rem_euclid(every)
        } else {
            start.rem_euclid(every)
        }
    };
    let up_offset = (-up_start).rem_euclid(dot_step);
    let across_offset = along(across_start, dot_step);
    let up_offset_major = (-up_start).rem_euclid(gap);
    let across_offset_major = along(across_start, gap);

    let dots = format!(
        "{ink} stroke-opacity=\"0.65\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-dasharray=\"0 {}\"",
        fmt_g(sw),
        fmt_g(dot_step)
    );
    let big_dots = format!(
        "{ink} stroke-opacity=\"0.85\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-dasharray=\"0 {}\"",
        fmt_g(sw),
        fmt_g(gap)
    );
    let faint = format!(
        "{ink} stroke-opacity=\"0.3\" stroke-width=\"{}\"",
        fmt_g(sw)
    );
    let style = match lines {
        GridLines::Dots => Some(&dots),
        GridLines::Faint => Some(&faint),
        _ => None,
    };
    let dashed = lines == GridLines::Dots;
    let major_dots = lines != GridLines::None && !(lines == GridLines::Auto && mpp <= 1.0);

    let mut minor = vec![layer("grid-minor", "Grid minor", "")];
    let mut majors = vec![layer("grid-major", "Grid major", "")];
    let mut labels = vec![layer(
        "grid-labels",
        "Grid labels",
        " pointer-events=\"none\"",
    )];
    let top_band = size * 1.4; // the row of labels along the top edge
    let point = |axis: &str, val: i64| {
        if axis == "x" {
            picture.pic((val as f64, 0.0))
        } else {
            picture.pic((0.0, val as f64))
        }
    };
    let ups_u: Vec<(i64, f64)> = ups.iter().map(|&v| (v, point(up_axis, v).0)).collect();
    let acrosses_v: Vec<(i64, f64)> = acrosses
        .iter()
        .map(|&v| (v, point(across_axis, v).1))
        .collect();
    for &(val, u) in &ups_u {
        let d = format!("M{} {}V{}", fmt_g(u), fmt_g(v0), fmt_g(v0 + h));
        if val % i64::from(major) == 0 {
            if major_dots {
                majors.push(format!(
                    "<path d=\"{d}\" {big_dots} stroke-dashoffset=\"{}\"/>",
                    fmt_g(up_offset_major)
                ));
            }
        } else if let Some(style) = style {
            let extra = if dashed {
                format!(" stroke-dashoffset=\"{}\"", fmt_g(up_offset))
            } else {
                String::new()
            };
            minor.push(format!("<path d=\"{d}\" {style}{extra}/>"));
        }
        let word = format!("{up_axis} {val}");
        let x = u + size * 0.3;
        if x - u0 + word.len() as f64 * LABEL_EM * size <= w {
            labels.push(format!(
                "<text x=\"{}\" y=\"{}\" {text}>{word}</text>",
                fmt_g(x),
                fmt_g(v0 + size * 1.1)
            ));
        }
    }
    for &(val, v) in &acrosses_v {
        let d = format!("M{} {}H{}", fmt_g(u0), fmt_g(v), fmt_g(u0 + w));
        if val % i64::from(major) == 0 {
            if major_dots {
                majors.push(format!(
                    "<path d=\"{d}\" {big_dots} stroke-dashoffset=\"{}\"/>",
                    fmt_g(across_offset_major)
                ));
            }
        } else if let Some(style) = style {
            let extra = if dashed {
                format!(" stroke-dashoffset=\"{}\"", fmt_g(across_offset))
            } else {
                String::new()
            };
            minor.push(format!("<path d=\"{d}\" {style}{extra}/>"));
        }
        if v - v0 - size * 1.4 >= top_band {
            labels.push(format!(
                "<text x=\"{}\" y=\"{}\" {text}>{across_axis} {val}</text>",
                fmt_g(u0 + size * 0.3),
                fmt_g(v - size * 0.3)
            ));
        }
    }
    let (mut crosses, mut big_crosses) = (String::new(), String::new());
    for &(x, u) in &ups_u {
        for &(y, v) in &acrosses_v {
            let big = x % i64::from(major) == 0 && y % i64::from(major) == 0;
            let a = if big { big_arm } else { arm };
            let cross = format!(
                "M{} {}h{}M{} {}v{}",
                fmt_g(u - a),
                fmt_g(v),
                fmt_g(2.0 * a),
                fmt_g(u),
                fmt_g(v - a),
                fmt_g(2.0 * a)
            );
            if big {
                big_crosses += &cross;
            } else {
                crosses += &cross;
            }
        }
    }
    if !crosses.is_empty() {
        minor.push(format!("<path d=\"{crosses}\" {mark}/>"));
    }
    if !big_crosses.is_empty() {
        majors.push(format!("<path d=\"{big_crosses}\" {big_mark}/>"));
    }
    let mut marks = vec![layer(
        "grid",
        &format!("Grid {step} m"),
        " pointer-events=\"none\"",
    )];
    marks.extend(minor);
    marks.push("</g>".into());
    marks.extend(majors);
    marks.push("</g>".into());
    marks.push("</g>".into());
    labels.push("</g>".into());
    (marks, labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> Picture {
        Picture {
            width: 4096.0,
            height: 2048.0,
            north: crate::svg::North::Game,
        }
    }

    #[test]
    fn the_default_step_is_a_round_third_of_the_short_side() {
        assert_eq!(
            auto_step(2048.0, false),
            1000,
            "a whole map: 1000 m at least"
        );
        assert_eq!(auto_step(16384.0, false), 5000);
        assert_eq!(auto_step(800.0, true), 200);
        assert_eq!(auto_step(500.0, true), 100);
        assert_eq!(auto_step(3000.0, true), 1000);
        assert_eq!(auto_step(2.0, true), 1);
    }

    #[test]
    fn a_grid_of_too_many_crosses_is_refused() {
        let p = picture();
        let view = View::new(&p, (-2048.0, 2048.0, -1024.0, 1024.0));
        // 4097 x 2049 crosses at 1 m; 820 x 410 at 5 m.
        let err = check_size(&view, 1).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("raise --grid"), "{err}");
        assert!(check_size(&view, 5).is_ok());
    }

    #[test]
    fn first_multiple_rounds_up() {
        assert_eq!(first_multiple(-2048.0, 1000), -2000);
        assert_eq!(first_multiple(-2000.0, 1000), -2000);
        assert_eq!(first_multiple(1.5, 1000), 1000);
        assert_eq!(first_multiple(0.0, 1000), 0);
    }

    #[test]
    fn the_default_grid_is_major_marks_and_dotted_lines() {
        let p = picture();
        let view = View::new(&p, (-2048.0, 2048.0, -1024.0, 1024.0));
        let (marks, labels) = layers(&p, &view, 8.0, 1000, 1000, GridLines::Auto, 25.0);
        let marks = marks.join("\n");
        // x = -2000 ... 2000 and y = 1000, 0, -1000: 5 up and 3 across, all dotted.
        assert_eq!(marks.matches("stroke-dasharray").count(), 8, "{marks}");
        // x = 0 is at picture u = 2048.
        assert!(marks.contains("<path d=\"M2048 0V2048\""), "{marks}");
        // 15 crosses, all major; no minor ones.
        let big = marks
            .lines()
            .find(|l| l.contains("stroke-width=\"12\""))
            .unwrap();
        assert_eq!(big.matches('v').count(), 15, "{big}");
        assert!(!marks.split("grid-major").next().unwrap().contains("<path"));
        let labels = labels.join("\n");
        assert!(labels.contains(">x 0</text>"));
        assert!(labels.contains(">y 0</text>"));
        // y 1000 lies 24 m below the top edge, inside the row of x labels: left out.
        assert!(!labels.contains(">y 1000</text>"), "{labels}");
        // x 2000 would run off the right edge: left out.
        assert!(!labels.contains(">x 2000</text>"), "{labels}");
    }

    #[test]
    fn with_north_to_the_left_y_runs_along_the_top() {
        let p = Picture {
            north: North::Sun,
            ..picture()
        };
        // The picture is 2048 wide (y, falling to the right) and 4096 high (x, falling downwards).
        let view = View::new(&p, (-2048.0, 2048.0, -1024.0, 1024.0));
        let (marks, labels) = layers(&p, &view, 8.0, 1000, 1000, GridLines::Auto, 25.0);
        let (marks, labels) = (marks.join("\n"), labels.join("\n"));
        // y = 1000 runs down the picture at u = 1024 - 1000.
        assert!(marks.contains("<path d=\"M24 0V4096\""), "{marks}");
        // x = 2000 runs across it at v = 2048 - 2000.
        assert!(marks.contains("<path d=\"M0 48H2048\""), "{marks}");
        assert!(
            labels.contains(">y 0</text>") && labels.contains(">x -1000</text>"),
            "{labels}"
        );
        assert!(
            !labels.contains(">x 2000</text>"),
            "in the top row: left out"
        );
    }

    #[test]
    fn a_minor_step_with_faint_lines() {
        let p = picture();
        let view = View::new(&p, (-2048.0, 2048.0, -1024.0, 1024.0));
        let (marks, _) = layers(&p, &view, 8.0, 500, MAJOR, GridLines::Faint, 25.0);
        let marks = marks.join("\n");
        assert!(marks.contains("Grid 500 m"));
        // x = -1500, -500, 500, 1500 and y = -500, 500: six faint lines.
        assert_eq!(
            marks.matches("stroke-opacity=\"0.3\"").count(),
            6,
            "{marks}"
        );
        let (marks, _) = layers(&p, &view, 8.0, 500, MAJOR, GridLines::None, 25.0);
        assert!(!marks.join("").contains("dasharray"), "crosses only");
    }
}
