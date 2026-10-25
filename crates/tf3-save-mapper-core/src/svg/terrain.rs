//! The `terrain` layer: the land as height bands, the water, and height
//! lines, all vector shapes cut from the heightmap.
//!
//! Sublayers, bottom to top: `land` (one rectangle in the lowest band's
//! colour), `bands` (one path per height step, covering all ground from there
//! up, so the higher bands paint over the lower), `water` (lakes, sea and
//! rivers as one even-odd path: the game carves rivers below the water level)
//! and `contours` (`contours-minor` and `contours-index`, one path per level).
//! Heights are metres above the water. The colours are a plain height ramp,
//! not the game's own palette.
//!
//! With `SvgOptions::raster` the layer holds instead one shaded picture of
//! the terrain (`raster.rs`), embedded as a PNG.

use std::fmt::Write as _;
use std::io::{self, Write};
use std::ops::RangeInclusive;

use super::raster::{self, Sampling};
use super::{North, Picture, SvgOptions, fmt_g, layer};
use crate::contour::{Grid, Point, level_rings, simplify, split_at_border};
use crate::terrain::{GRID_STEP, Terrain};

/// The height ramp: (metres above the water, colour), interpolated.
const LAND: [(f64, [f64; 3]); 5] = [
    (0.0, [190.0, 205.0, 140.0]),
    (30.0, [130.0, 175.0, 105.0]),
    (100.0, [165.0, 155.0, 105.0]),
    (200.0, [140.0, 120.0, 100.0]),
    (300.0, [235.0, 235.0, 235.0]),
];
/// The land with no bands.
const LAND_FILL: &str = "#ece9d2";
const WATER_FILL: &str = "#78afdc";
const SHORE_LINE: &str = "#4a86b8";
const CONTOUR_MINOR: &str = "#b8996a";
const CONTOUR_INDEX: &str = "#7a5230";

/// The ramp's colour at `z` metres above the water, unrounded.
pub(super) fn land_rgb(z: f64) -> [f64; 3] {
    if z <= LAND[0].0 {
        LAND[0].1
    } else if let Some(w) = LAND.windows(2).find(|w| z <= w[1].0) {
        let ((z0, c0), (z1, c1)) = (w[0], w[1]);
        let t = (z - z0) / (z1 - z0);
        [0, 1, 2].map(|k| c0[k] + (c1[k] - c0[k]) * t)
    } else {
        LAND[LAND.len() - 1].1
    }
}

fn land_colour(z: f64) -> String {
    let [r, g, b] = land_rgb(z).map(|v| v.round_ties_even() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Path data for polylines, coordinates to a tenth of a metre; a point that
/// rounds onto the one before is dropped, and a line left with one point too.
fn path_data<'a>(polylines: impl IntoIterator<Item = &'a Vec<Point>>, closed: bool) -> String {
    let mut out = String::new();
    for pts in polylines {
        let mut seg: Vec<(i64, i64)> = Vec::new();
        for &(u, v) in pts {
            let p = ((u * 10.0).round() as i64, (v * 10.0).round() as i64);
            if seg.last() != Some(&p) {
                seg.push(p);
            }
        }
        if seg.len() < 2 {
            continue;
        }
        for (i, (u, v)) in seg.iter().enumerate() {
            let _ = write!(
                out,
                "{}{} {}",
                if i == 0 { 'M' } else { 'L' },
                tenths(*u),
                tenths(*v)
            );
        }
        if closed {
            out.push('Z');
        }
    }
    out
}

/// A number of tenths written as a decimal without a trailing `.0`.
fn tenths(t: i64) -> String {
    if t % 10 == 0 {
        format!("{}", t / 10)
    } else {
        format!(
            "{}{}.{}",
            if t < 0 { "-" } else { "" },
            t.abs() / 10,
            t.abs() % 10
        )
    }
}

/// The grid points of `columns` and `rows` in picture order, every `step`th
/// point, `at(c, r)` giving the value at column `c` (x rising) and row `r` (y
/// rising). `Game`: picture row 0 is the northernmost grid row (y largest),
/// and x rises along it from the first column. `Sun`: picture row 0 is the
/// last grid column (x largest), and y falls along it from the last row.
pub(super) fn picture_grid(
    at: impl Fn(usize, usize) -> u16,
    step: usize,
    columns: RangeInclusive<usize>,
    rows: RangeInclusive<usize>,
    north: North,
) -> Grid {
    let (outer, inner): (Vec<usize>, Vec<usize>) = match north {
        North::Game => (
            rows.rev().step_by(step).collect(),
            columns.step_by(step).collect(),
        ),
        North::Sun => (
            columns.rev().step_by(step).collect(),
            rows.rev().step_by(step).collect(),
        ),
    };
    let mut values = Vec::with_capacity(outer.len() * inner.len());
    for &o in &outer {
        values.extend(inner.iter().map(|&i| match north {
            North::Game => at(i, o),
            North::Sun => at(o, i),
        }));
    }
    Grid {
        width: inner.len(),
        height: outer.len(),
        values,
    }
}

/// The grid columns and rows a crop's map box spans (all of them without one).
pub(super) fn grid_span(
    t: &Terrain,
    crop: Option<(f64, f64, f64, f64)>,
) -> (RangeInclusive<usize>, RangeInclusive<usize>) {
    let (last_c, last_r) = (t.columns() - 1, t.rows() - 1);
    let Some((x0, x1, y0, y1)) = crop else {
        return (0..=last_c, 0..=last_r);
    };
    let (tw, th) = t.size();
    let at = |v: f64, size: u32, last: usize| {
        ((v + f64::from(size) / 2.0) / GRID_STEP)
            .round()
            .clamp(0.0, last as f64) as usize
    };
    (
        at(x0, tw, last_c)..=at(x1, tw, last_c),
        at(y0, th, last_r)..=at(y1, th, last_r),
    )
}

/// The map point at the top-left corner of the cut grid's picture: the first
/// column and last row, or with north to the left the last column and the
/// last row. Metres, from the terrain's own size, not the header's.
pub(super) fn grid_corner(
    t: &Terrain,
    columns: &RangeInclusive<usize>,
    rows: &RangeInclusive<usize>,
    north: North,
) -> (f64, f64) {
    let c = match north {
        North::Game => *columns.start(),
        North::Sun => *columns.end(),
    };
    let (tw, th) = t.size();
    (
        -f64::from(tw) / 2.0 + GRID_STEP * c as f64,
        -f64::from(th) / 2.0 + GRID_STEP * *rows.end() as f64,
    )
}

/// ` transform="translate(...)"` moving the layer to the cut grid's corner:
/// the crop's, or the map's should the header give another map size.
fn corner_shift(
    t: &Terrain,
    picture: &Picture,
    columns: &RangeInclusive<usize>,
    rows: &RangeInclusive<usize>,
    north: North,
) -> String {
    let (u0, v0) = picture.pic(grid_corner(t, columns, rows, north));
    if u0 != 0.0 || v0 != 0.0 {
        format!(" transform=\"translate({} {})\"", fmt_g(u0), fmt_g(v0))
    } else {
        String::new()
    }
}

/// The terrain as one shaded picture, a PNG in a data URI, its pixels
/// `options.metres_per_pixel` apart.
pub(super) fn write_raster(
    out: &mut impl Write,
    t: &Terrain,
    picture: &Picture,
    crop: Option<(f64, f64, f64, f64)>,
    options: &SvgOptions,
) -> io::Result<()> {
    let (columns, rows) = grid_span(t, crop);
    let shift = corner_shift(t, picture, &columns, &rows, options.north);
    let sampling = Sampling::new(options.metres_per_pixel);
    let (img, _) = raster::shade(t, columns, rows, sampling, options.north)?;
    let png = raster::encode(&img, None)?;
    write!(
        out,
        "{}<image width=\"{}\" height=\"{}\" xlink:href=\"data:image/png;base64,",
        layer("terrain", "Terrain", &shift),
        fmt_g(img.width as f64 * sampling.spacing),
        fmt_g(img.height as f64 * sampling.spacing)
    )?;
    raster::write_base64(out, &png)?;
    writeln!(out, "\"/></g>")
}

pub(super) fn write(
    out: &mut impl Write,
    t: &Terrain,
    picture: &Picture,
    crop: Option<(f64, f64, f64, f64)>,
    options: &SvgOptions,
) -> io::Result<()> {
    let mpp = options.metres_per_pixel;
    let step = ((mpp / GRID_STEP) as usize).max(1);
    let spacing = GRID_STEP * step as f64;
    let unit = f64::from(t.unit);
    let (columns, rows) = grid_span(t, crop);
    let shift = corner_shift(t, picture, &columns, &rows, options.north);
    let grid = picture_grid(|c, r| t.at(c, r), step, columns, rows, options.north);
    let step_m = options.contour_step;
    let (rings, extent) = level_rings(
        &grid,
        spacing,
        t.water_units(),
        step_m / unit,
        4.0 * spacing,
    );
    let tolerance = 0.4 * mpp;
    let rings: Vec<(u32, Vec<Vec<Point>>)> = rings
        .into_iter()
        .map(|(k, rs)| (k, rs.iter().map(|r| simplify(r, tolerance, true)).collect()))
        .collect();
    let (w, h) = (
        (grid.width - 1) as f64 * spacing,
        (grid.height - 1) as f64 * spacing,
    );
    let thin = 0.5 * mpp; // so a line stays thinner than a road
    let land = if options.bands {
        land_colour(step_m / 2.0)
    } else {
        LAND_FILL.to_owned()
    };
    writeln!(out, "{}", layer("terrain", "Terrain", &shift))?;
    writeln!(
        out,
        "{}<rect width=\"{w}\" height=\"{h}\" fill=\"{land}\"/></g>",
        layer("land", "Land", "")
    )?;
    if options.bands {
        writeln!(
            out,
            "{}",
            layer("bands", "Height bands", " stroke=\"none\"")
        )?;
        let rect = format!("M0 0H{w}V{h}H0Z");
        for (k, rs) in rings.iter().filter(|(k, _)| *k > 0) {
            writeln!(
                out,
                "<path class=\"band\" data-height=\"{}\" fill=\"{}\" fill-rule=\"evenodd\" d=\"{rect}{}\"/>",
                f64::from(*k) * step_m,
                land_colour((f64::from(*k) + 0.5) * step_m),
                path_data(rs, true)
            )?;
        }
        writeln!(out, "</g>")?;
    }
    let water: Vec<&Vec<Point>> = rings
        .iter()
        .filter(|(k, _)| *k == 0)
        .flat_map(|(_, rs)| rs)
        .collect();
    writeln!(
        out,
        "{}<path class=\"water\" fill=\"{WATER_FILL}\" fill-rule=\"evenodd\" stroke=\"{SHORE_LINE}\" \
         stroke-width=\"{thin}\" stroke-linejoin=\"round\" d=\"{}\"/></g>",
        layer("water", "Water", ""),
        path_data(water, true)
    )?;
    writeln!(out, "{}", layer("contours", "Height lines", ""))?;
    let every = options.index_every.max(1);
    for (id, label, colour, width, index) in [
        (
            "contours-minor",
            "Height lines: minor",
            CONTOUR_MINOR,
            thin,
            false,
        ),
        (
            "contours-index",
            "Height lines: index",
            CONTOUR_INDEX,
            2.0 * thin,
            true,
        ),
    ] {
        writeln!(
            out,
            "{}",
            layer(
                id,
                label,
                &format!(
                    " fill=\"none\" stroke=\"{colour}\" stroke-width=\"{width}\" \
                     stroke-linejoin=\"round\" stroke-linecap=\"round\""
                )
            )
        )?;
        for (k, rs) in &rings {
            if *k == 0 || (k % every == 0) != index {
                continue;
            }
            let pieces: Vec<(Vec<Point>, bool)> =
                rs.iter().flat_map(|r| split_at_border(r, extent)).collect();
            let closed: Vec<&Vec<Point>> = pieces.iter().filter(|p| p.1).map(|p| &p.0).collect();
            let open: Vec<&Vec<Point>> = pieces.iter().filter(|p| !p.1).map(|p| &p.0).collect();
            let d = path_data(closed, true) + &path_data(open, false);
            if !d.is_empty() {
                writeln!(
                    out,
                    "<path class=\"contour\" data-height=\"{}\" d=\"{d}\"/>",
                    f64::from(*k) * step_m
                )?;
            }
        }
        writeln!(out, "</g>")?;
    }
    writeln!(out, "</g>")?;
    writeln!(out, "</g>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_runs_from_green_to_white() {
        assert_eq!(land_colour(-5.0), "#becd8c");
        // Halfway to 30 m; 122.5 rounds to the even 122.
        assert_eq!(land_colour(15.0), "#a0be7a");
        assert_eq!(land_colour(500.0), "#ebebeb");
    }

    #[test]
    fn path_data_rounds_and_drops_repeats() {
        let lines = vec![
            vec![(0.04, 0.0), (0.0, 0.01), (10.25, 3.0)],
            vec![(1.0, 1.0), (1.01, 1.0)],
        ];
        assert_eq!(path_data(&lines, true), "M0 0L10.3 3Z");
        assert_eq!(tenths(-5), "-0.5");
        assert_eq!(tenths(-15), "-1.5");
    }

    #[test]
    fn the_grid_is_turned_north_up() {
        let t = Terrain {
            tiles_x: 1,
            tiles_y: 1,
            offset_z: 0.0,
            unit: 0.05,
            water_level: 0.0,
            heights: (0..65 * 65).map(|i| (i / 65) as u16).collect(),
        };
        let at = |c, r| t.at(c, r);
        let g = picture_grid(at, 2, 0..=64, 0..=64, North::Game);
        assert_eq!((g.width, g.height), (33, 33));
        // Row 0 of the picture is the last grid row (y largest).
        assert_eq!(g.values[0], 64);
        assert_eq!(g.values[g.width * 32], 0);
        // A part of it, turned for the sun: picture row 0 is column 40, and
        // along it the rows fall from 30.
        let g = picture_grid(at, 1, 10..=40, 20..=30, North::Sun);
        assert_eq!((g.width, g.height), (11, 31));
        assert_eq!(&g.values[..3], &[30, 29, 28]);
        assert_eq!(g.values[g.width * 30 + 10], 20);
    }
}
