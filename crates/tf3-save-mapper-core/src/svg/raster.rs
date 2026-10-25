//! The terrain as pixels, and the roads and rails drawn into them.
//!
//! The shading follows the prototype's raster picture: the land in the same
//! height ramp as the vector bands, lit from the north-west with the slopes
//! doubled, and the water from light to dark blue over its first 100 m of
//! depth, unlit. Pixel (i, j) shows point (i, j) of the grid in picture order
//! at its top-left corner, so the picture is one point fewer each way than
//! the grid and covers exactly the map or the crop; the last column and row
//! are read only for the slopes. Finer than the heightmap's 4 m the grid is
//! first filled in between its points (no new height detail).

use std::io::{self, Write};
use std::ops::RangeInclusive;

use super::terrain::{grid_corner, land_rgb, picture_grid};
use super::{CURVE_TOLERANCE, North, bezier_point};
use crate::contour::Grid;
use crate::edges::{Edge, Family};
use crate::terrain::{GRID_STEP, Terrain};

/// A bigger picture is refused: at 3 bytes a pixel it is too much to hold.
pub const MAX_PIXELS: usize = 64_000_000;
const SHALLOW: [f64; 3] = [120.0, 175.0, 220.0];
const DEEP: [f64; 3] = [30.0, 75.0, 125.0];
/// Towards the light: north-west and up.
const LIGHT: [f64; 3] = [-0.55, 0.55, 0.63];
const ROAD: [u8; 3] = [50, 50, 50];
const RAIL: [u8; 3] = [205, 30, 30];

/// RGB pixels, top row first.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Image {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// How the heightmap becomes pixels at `mpp` metres per pixel: each grid step
/// cut into `fine` (finer than 4 m), or every `step`th point kept (4 m and
/// coarser), and the metres between the pixels that gives. For the values
/// `--mpp` takes (1, 2 and multiples of 4) the spacing is `mpp`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Sampling {
    fine: usize,
    step: usize,
    pub spacing: f64,
}

impl Sampling {
    pub fn new(mpp: f64) -> Sampling {
        let (fine, step) = if mpp < GRID_STEP {
            (((GRID_STEP / mpp).round() as usize).max(1), 1)
        } else {
            (1, ((mpp / GRID_STEP) as usize).max(1))
        };
        Sampling {
            fine,
            step,
            spacing: GRID_STEP * step as f64 / fine as f64,
        }
    }

    /// The picture's width and height for `columns` x `rows` grid points.
    pub fn size(&self, columns: usize, rows: usize, north: North) -> (usize, usize) {
        let along = |n: usize| (n - 1) * self.fine / self.step;
        match north {
            North::Game => (along(columns), along(rows)),
            North::Sun => (along(rows), along(columns)),
        }
    }
}

/// The error for a picture of no pixels, or of more than `MAX_PIXELS`.
pub(super) fn check_size((w, h): (usize, usize)) -> io::Result<()> {
    if w == 0 || h == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the picture would be {w} x {h} pixels: lower --mpp or widen the crop"),
        ));
    }
    if w.saturating_mul(h) > MAX_PIXELS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "the picture would be {w} x {h} pixels, more than {} million: raise --mpp",
                MAX_PIXELS / 1_000_000
            ),
        ));
    }
    Ok(())
}

/// The map point at the picture's top-left corner and which way north is:
/// where map points fall in the picture, in metres from that corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Corner {
    pub north: North,
    pub x: f64,
    pub y: f64,
}

impl Corner {
    /// The corner of a picture of the map box `(x_min, x_max, _, y_max)`.
    pub fn of_box(north: North, (x_min, x_max, _, y_max): (f64, f64, f64, f64)) -> Corner {
        let x = match north {
            North::Game => x_min,
            North::Sun => x_max,
        };
        Corner { north, x, y: y_max }
    }

    /// Picture metres (u to the right, v down) of a map point.
    pub fn pic(&self, (x, y): (f64, f64)) -> (f64, f64) {
        match self.north {
            North::Game => (x - self.x, self.y - y),
            North::Sun => (self.y - y, self.x - x),
        }
    }

    /// The map box `(x_min, x_max, y_min, y_max)` a picture of `size` pixels
    /// `spacing` metres apart shows.
    pub fn bounds(&self, (w, h): (usize, usize), spacing: f64) -> (f64, f64, f64, f64) {
        let (w, h) = (w as f64 * spacing, h as f64 * spacing);
        match self.north {
            North::Game => (self.x, self.x + w, self.y - h, self.y),
            North::Sun => (self.x - h, self.x, self.y - w, self.y),
        }
    }
}

/// The terrain's grid points of `columns` and `rows` shaded into pixels, and
/// the map point at the picture's top-left corner.
pub(super) fn shade(
    t: &Terrain,
    columns: RangeInclusive<usize>,
    rows: RangeInclusive<usize>,
    sampling: Sampling,
    north: North,
) -> io::Result<(Image, Corner)> {
    check_size(sampling.size(columns.clone().count(), rows.clone().count(), north))?;
    let (x, y) = grid_corner(t, &columns, &rows, north);
    let grid = if sampling.fine > 1 {
        let (c0, r0) = (*columns.start(), *rows.start());
        let wide = fill_in(
            |c, r| t.at(c0 + c, r0 + r),
            columns.count(),
            rows.count(),
            sampling.fine,
        );
        let at = |c: usize, r: usize| wide.values[r * wide.width + c];
        picture_grid(at, 1, 0..=wide.width - 1, 0..=wide.height - 1, north)
    } else {
        picture_grid(|c, r| t.at(c, r), sampling.step, columns, rows, north)
    };
    Ok((
        shade_grid(&grid, t, sampling.spacing),
        Corner { north, x, y },
    ))
}

/// The grid with `f` points per step of the `width` x `height` grid `at`
/// gives (column, row, y rising), filled in along the rows first and then
/// between them; each new point is the one before it plus its share of the
/// rise, rounded down.
fn fill_in(at: impl Fn(usize, usize) -> u16, width: usize, height: usize, f: usize) -> Grid {
    let share = |a: u16, b: u16, k: usize| {
        let (a, b) = (i64::from(a), i64::from(b));
        (a + ((b - a) * k as i64).div_euclid(f as i64)) as u16
    };
    let wide_w = (width - 1) * f + 1;
    let mut wide = Vec::with_capacity(wide_w * height);
    for r in 0..height {
        for c in 0..width - 1 {
            wide.extend((0..f).map(|k| share(at(c, r), at(c + 1, r), k)));
        }
        wide.push(at(width - 1, r));
    }
    let mut values = Vec::with_capacity(wide_w * ((height - 1) * f + 1));
    for r in 0..height - 1 {
        let (lower, upper) = (
            &wide[r * wide_w..(r + 1) * wide_w],
            &wide[(r + 1) * wide_w..(r + 2) * wide_w],
        );
        for k in 0..f {
            values.extend(lower.iter().zip(upper).map(|(&a, &b)| share(a, b, k)));
        }
    }
    values.extend_from_slice(&wide[(height - 1) * wide_w..]);
    Grid {
        width: wide_w,
        height: (height - 1) * f + 1,
        values,
    }
}

/// The picture-order `grid` (points `spacing` metres apart) as shaded pixels.
fn shade_grid(grid: &Grid, t: &Terrain, spacing: f64) -> Image {
    let (gw, w, h) = (grid.width, grid.width - 1, grid.height - 1);
    let (off_z, unit) = (f64::from(t.offset_z), f64::from(t.unit));
    let water = f64::from(t.water_level);
    let water_v = (water - off_z) / unit;
    // Metres of rise per height unit of difference across two grid steps, doubled.
    let k = 2.0 * unit / (2.0 * spacing);
    let [lx, ly, lz] = LIGHT;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for j in 0..h {
        let row = &grid.values[j * gw..(j + 1) * gw];
        let up = &grid.values[j.saturating_sub(1) * gw..][..gw]; // north of it
        let down = &grid.values[(j + 1) * gw..][..gw];
        for i in 0..w {
            let v = row[i];
            let z = off_z + f64::from(v) * unit;
            if f64::from(v) < water_v {
                let depth = ((water - z) / 100.0).min(1.0);
                rgb.extend([0, 1, 2].map(|c| (SHALLOW[c] + (DEEP[c] - SHALLOW[c]) * depth) as u8));
                continue;
            }
            let colour = land_rgb(z - water);
            let rise = |a: u16, b: u16| f64::from(i32::from(a) - i32::from(b)) * k;
            let dzdx = rise(row[i + 1], row[i.saturating_sub(1)]);
            let dzdy = rise(up[i], down[i]); // rises towards the top of the picture
            let s = (-dzdx * lx - dzdy * ly + lz) / (1.0 + dzdx * dzdx + dzdy * dzdy).powf(0.5);
            let f = (0.5 + 0.75 * s).clamp(0.55, 1.25);
            rgb.extend(colour.map(|c| ((c * f) as u32).min(255) as u8));
        }
    }
    Image {
        width: w,
        height: h,
        rgb,
    }
}

/// A plain picture of one colour.
pub(super) fn blank((width, height): (usize, usize), colour: [u8; 3]) -> io::Result<Image> {
    check_size((width, height))?;
    Ok(Image {
        width,
        height,
        rgb: colour.repeat(width * height),
    })
}

/// Draw the roads into `img`, then the rails over them, each as wide as
/// `width` gives (at least one pixel). A bend is drawn as chords of about 8
/// pixels. `spacing` is the metres per pixel.
pub(super) fn draw_network(
    img: &mut Image,
    corner: &Corner,
    spacing: f64,
    edges: &[&Edge],
    width: impl Fn(&Edge) -> f64,
) {
    for (family, colour) in [(Family::Street, ROAD), (Family::Track, RAIL)] {
        for e in edges.iter().filter(|e| e.family == family) {
            let c = e.bezier();
            let steps = if e.is_curved(CURVE_TOLERANCE) {
                ((e.length() / (8.0 * spacing)).ceil() as usize).max(1)
            } else {
                1
            };
            let pts: Vec<(f64, f64)> = (0..=steps)
                .map(|i| corner.pic(bezier_point(&c, i as f64 / steps as f64)))
                .collect();
            let brush = ((width(e) / spacing).round_ties_even() as i64).max(1);
            for pair in pts.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                line(
                    img,
                    (a.0 / spacing, a.1 / spacing),
                    (b.0 / spacing, b.1 / spacing),
                    colour,
                    brush,
                );
            }
        }
    }
}

/// A line between two pixel positions, a square brush `brush` pixels wide
/// at each step; the parts outside the picture are skipped.
fn line(img: &mut Image, from: (f64, f64), to: (f64, f64), colour: [u8; 3], brush: i64) {
    let px = |v: f64| v.round_ties_even() as i64;
    let (mut x0, mut y0, x1, y1) = (px(from.0), px(from.1), px(to.0), px(to.1));
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let (lo, hi) = (-((brush - 1) / 2), brush / 2 + 1);
    let (w, h) = (img.width as i64, img.height as i64);
    loop {
        for yy in (y0 + lo).max(0)..(y0 + hi).min(h) {
            for xx in (x0 + lo).max(0)..(x0 + hi).min(w) {
                let o = 3 * (yy * w + xx) as usize;
                img.rgb[o..o + 3].copy_from_slice(&colour);
            }
        }
        if x0 == x1 && y0 == y1 {
            return;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// The picture as a PNG file, with an `iTXt` chunk (`keyword`, `text`)
/// before the pixels if given.
pub(super) fn encode(img: &Image, text: Option<(&str, String)>) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width as u32, img.height as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    if let Some((keyword, text)) = text {
        enc.add_itxt_chunk(keyword.to_owned(), text)
            .map_err(io::Error::other)?;
    }
    let mut writer = enc.write_header().map_err(io::Error::other)?;
    writer
        .write_image_data(&img.rgb)
        .map_err(io::Error::other)?;
    writer.finish().map_err(io::Error::other)?;
    Ok(out)
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `data` in base64 (RFC 4648, with padding), as a data URI holds it.
pub(super) fn write_base64(out: &mut impl Write, data: &[u8]) -> io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for k in 0..4 {
            buf.push(if k <= chunk.len() {
                BASE64[(n >> (18 - 6 * k) & 63) as usize]
            } else {
                b'='
            });
        }
        if buf.len() >= 4096 {
            out.write_all(&buf)?;
            buf.clear();
        }
    }
    out.write_all(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base64(data: &[u8]) -> String {
        let mut out = Vec::new();
        write_base64(&mut out, data).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn base64_follows_rfc_4648() {
        // The test vectors of RFC 4648, section 10.
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), coded);
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
        let long: Vec<u8> = (0..10_000u32).map(|i| (i * 7) as u8).collect();
        assert_eq!(base64(&long).len(), 4 * long.len().div_ceil(3));
    }

    #[test]
    fn sampling_follows_the_metres_per_pixel() {
        let s = Sampling::new(8.0);
        assert_eq!((s.fine, s.step, s.spacing), (1, 2, 8.0));
        // 65 points every second: 33 points, a picture of 32.
        assert_eq!(s.size(65, 9, North::Game), (32, 4));
        assert_eq!(s.size(65, 9, North::Sun), (4, 32));
        let s = Sampling::new(12.0);
        assert_eq!(s.size(65, 9, North::Game), (21, 2));
        let s = Sampling::new(1.0);
        assert_eq!((s.fine, s.step, s.spacing), (4, 1, 1.0));
        assert_eq!(s.size(65, 9, North::Game), (256, 32));
        assert!(check_size((8000, 8000)).is_ok());
        assert!(check_size((8001, 8000)).is_err());
        assert!(check_size((0, 8)).is_err(), "no pixels");
        assert!(check_size((1, 1)).is_ok());
    }

    #[test]
    fn filling_in_rounds_down_from_the_first_point() {
        // Along a row from 10 to 13 in 4 parts: 10, 10.75, 11.5, 12.25 -> 10, 10, 11, 12;
        // falling from 13 to 10: 13, 12.25, 11.5, 10.75 -> 13, 12, 11, 10.
        let g = fill_in(|c, _| [10, 13, 10][c], 3, 2, 4);
        assert_eq!((g.width, g.height), (9, 5));
        assert_eq!(&g.values[..9], &[10, 10, 11, 12, 13, 12, 11, 10, 10]);
        assert_eq!(&g.values[36..], &g.values[..9]);
    }

    fn flat(values: Vec<u16>, width: usize) -> (Grid, Terrain) {
        let height = values.len() / width;
        let t = Terrain {
            tiles_x: 1,
            tiles_y: 1,
            offset_z: -50.0,
            unit: 0.05,
            water_level: 0.0,
            heights: Vec::new(),
        };
        (
            Grid {
                width,
                height,
                values,
            },
            t,
        )
    }

    #[test]
    fn land_is_lit_from_the_north_west_and_water_is_not() {
        // 1000 units is the water level (0 m). Flat land at 0 m: the ramp's
        // first colour times 0.5 + 0.75 * 0.63 = 0.9725.
        let (g, t) = flat(vec![1000; 9], 3);
        let img = shade_grid(&g, &t, 4.0);
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(&img.rgb[..3], &[184, 199, 136]);
        // Rising to the east, so facing west, towards the light: brighter,
        // and greener as it climbs (4 and 8 m up). As the prototype draws it.
        let (g, t) = flat(vec![1000, 1080, 1160, 1000, 1080, 1160], 3);
        let img = shade_grid(&g, &t, 4.0);
        assert_eq!(img.rgb, [213, 230, 157, 196, 217, 146]);
        // 50 m under water: halfway from shallow to deep, unlit.
        let (g, t) = flat(vec![0; 4], 2);
        assert_eq!(shade_grid(&g, &t, 4.0).rgb, [75, 125, 172]);
    }

    #[test]
    fn lines_have_a_square_brush_and_are_cut_to_the_picture() {
        let mut img = blank((6, 4), [255; 3]).unwrap();
        // Two pixels wide: the brush covers the point and the one right and below.
        line(&mut img, (-2.0, 1.0), (2.5, 1.0), RAIL, 2);
        let red: Vec<(usize, usize)> = (0..24)
            .filter(|p| img.rgb[3 * p] == 205)
            .map(|p| (p % 6, p / 6))
            .collect();
        // 2.5 rounds to the even 2.
        assert_eq!(
            red,
            [
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2)
            ]
        );
    }

    #[test]
    fn a_bend_is_drawn_as_chords_of_about_8_pixels() {
        // From (5, 35) to (35, 10) on the map, leaving east and arriving
        // heading south: 39 m, so 5 chords. As the prototype draws it.
        let e = Edge {
            a: 1,
            b: 2,
            p0: [5.0, 35.0, 0.0],
            p1: [35.0, 10.0, 0.0],
            t0: [45.0, 0.0, 0.0],
            t1: [0.0, -45.0, 0.0],
            template: None,
            family: Family::Street,
            width: Some(3.0),
            offset: 0,
        };
        let mut img = blank((40, 40), [255; 3]).unwrap();
        let corner = Corner {
            north: North::Game,
            x: 0.0,
            y: 40.0,
        };
        draw_network(&mut img, &corner, 1.0, &[&e], |_| 3.0);
        let hash = img.rgb.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        });
        let road = img.rgb.chunks(3).filter(|p| *p == ROAD).count();
        assert_eq!((road, hash), (158, 0x3882_4869_c781_618d));
    }

    #[test]
    fn a_png_carries_its_text_before_the_pixels() {
        let img = Image {
            width: 3,
            height: 2,
            rgb: [1, 2, 3].repeat(6),
        };
        let png = encode(&img, Some(("tf3map:view", "{\"north\": \"game\"}".into()))).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        let at = |tag: &[u8]| png.windows(4).position(|w| w == tag).unwrap();
        assert!(at(b"iTXt") < at(b"IDAT"));
        let chunk = b"tf3map:view\0\0\0\0\0{\"north\": \"game\"}";
        assert!(png.windows(chunk.len()).any(|w| w == chunk));
        let mut dec = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .unwrap();
        let mut buf = vec![0; dec.output_buffer_size().unwrap()];
        dec.next_frame(&mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3].repeat(6));
    }
}
