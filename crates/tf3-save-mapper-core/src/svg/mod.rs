//! The SVG writer.
//!
//! Coordinates inside the drawing are map metres from the picture's top-left
//! corner, with north up and east right (x is east and y is north in the game's
//! own frame, `docs/compass.md` in tf3-save-format), or with `North::Sun` x up
//! and y to the left. The `viewBox` is the map in metres (or the crop), so
//! widths are in metres too and scale with the picture. Layers are
//! groups Inkscape shows as layers, each also `class="layer"` with its id in
//! `data-layer` as in the prototype (`layer`), bottom to top: `background`, `terrain`,
//! `grid`, `coverage`, `stations`, `roads` and `rails` (each with one sublayer per street
//! or track type), the depot layers, `stations-street`, `industries`,
//! `vehicles`, `labels`, `grid-labels` and `compass`.
//!
//! `write_png` draws the map as one flat PNG instead: the raster terrain with
//! the roads and rails drawn into its pixels, as the prototype's `--png` does.

mod coverage;
mod depots;
mod grid;
mod industries;
mod labels;
mod raster;
mod rose;
mod stations;
mod terrain;
mod vehicles;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{self, Write};

use crate::edges::{Edge, Family};
use crate::header::Header;
use crate::install::{DepotSpec, Polygon, Rect};
use crate::instances::Instance;
use crate::terrain::{GRID_STEP, Terrain};

pub use grid::{GridLines, GridStep};
pub use industries::faces_with_measured;
pub use labels::{EdgeLabel, unmatched as unmatched_labels};
pub use stations::is_station_piece;

const ROAD_COLOUR: &str = "#323232";
const RAIL_COLOUR: &str = "#cd1e1e";
/// Plain paper under the network, so the dark roads read in a viewer with a dark theme.
const BACKGROUND_COLOUR: &str = "#ffffff";
/// The same paper in a PNG without terrain.
const PAPER: [u8; 3] = [255, 255, 255];
/// A bend shows as a curve only above this many metres of bulge.
const CURVE_TOLERANCE: f64 = 0.25;
/// The outline of a station, depot or industry footprint, metres.
const OUTLINE: f64 = 1.5;

#[derive(Debug, Clone)]
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
    /// Draw the terrain as one shaded picture (an embedded PNG) instead of
    /// vector bands, water and height lines.
    pub raster: bool,
    /// Where the compass rose goes, if anywhere.
    pub rose: Option<Rose>,
    /// The rose circle's diameter, metres. `None`: 250, or an eighth of the
    /// picture's shorter side when that is less. A size given is not capped.
    pub rose_size: Option<f64>,
    /// Which way is up.
    pub north: North,
    /// The grid of map coordinates.
    pub grid: GridStep,
    /// Which grid lines are drawn besides the crosses.
    pub grid_lines: GridLines,
    /// Metres between the dots of a dotted minor grid line.
    pub grid_dot_step: f64,
    /// Draw only this part of the map. The picture keeps the whole map's
    /// frame (metres from its top-left corner), and its `viewBox` is the crop.
    pub crop: Option<Crop>,
    /// The picture's width and height in pixels are multiplied by this.
    pub zoom: u32,
    /// The build that draws, for the view record's `generator-commit`
    /// (the caller knows it; the core does not). Left out when `None`.
    pub generator_commit: Option<&'static str>,
    /// Settings the caller made that this module cannot see (which layers it
    /// left out, which it added), as name and value, written to the view
    /// record after the ones found here. A value is text.
    pub settings: Vec<(&'static str, String)>,
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
            raster: false,
            rose: Some(Rose::LowerRight),
            rose_size: None,
            north: North::Game,
            grid: GridStep::Auto,
            grid_lines: GridLines::Auto,
            grid_dot_step: 25.0,
            crop: None,
            zoom: 1,
            generator_commit: None,
            settings: Vec::new(),
        }
    }
}

/// Which way the map is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum North {
    /// As the game's name compass has it: up is +y, right is +x.
    Game,
    /// From the sun's path (`docs/terrain.md` in tf3-save-format): up is +x,
    /// right is -y.
    Sun,
}

/// Where the compass rose goes: a corner of the picture, or its centre at a
/// map point (metres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rose {
    LowerRight,
    LowerLeft,
    UpperLeft,
    UpperRight,
    At(f64, f64),
}

/// A square part of the map: half-width `radius` metres round (`x`, `y`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

impl Crop {
    /// The map box the crop shows, `(x_min, x_max, y_min, y_max)` in metres:
    /// the square widened to the heightmap's 4 m grid and cut to the map.
    /// `None` when less than two grid steps of it lie on the map.
    pub fn bounds(&self, header: &Header) -> Option<(f64, f64, f64, f64)> {
        let (w, h) = (f64::from(header.map_width), f64::from(header.map_height));
        let (cols, rows) = ((w / GRID_STEP).floor(), (h / GRID_STEP).floor());
        let c0 = ((self.x - self.radius + w / 2.0) / GRID_STEP)
            .floor()
            .max(0.0);
        let c1 = ((self.x + self.radius + w / 2.0) / GRID_STEP)
            .ceil()
            .min(cols);
        let r0 = ((self.y - self.radius + h / 2.0) / GRID_STEP)
            .floor()
            .max(0.0);
        let r1 = ((self.y + self.radius + h / 2.0) / GRID_STEP)
            .ceil()
            .min(rows);
        if !(c1 - c0 >= 2.0 && r1 - r0 >= 2.0) {
            return None;
        }
        Some((
            -w / 2.0 + GRID_STEP * c0,
            -w / 2.0 + GRID_STEP * c1,
            -h / 2.0 + GRID_STEP * r0,
            -h / 2.0 + GRID_STEP * r1,
        ))
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
    /// Depots and maintenance buildings drawn as their coverage circles.
    pub coverage: Vec<Instance>,
    /// Depot footprints by layer id (`install::DEPOT_KINDS`).
    pub depot_specs: HashMap<&'static str, DepotSpec>,
    pub stations: Vec<Instance>,
    /// Station piece boxes by model path.
    pub station_boxes: HashMap<String, Rect>,
    /// Vehicle model instances (`vehicle/...`).
    pub vehicles: Vec<Instance>,
    /// Names to write beside vehicles, by instance slot.
    pub vehicle_names: HashMap<usize, String>,
    /// Edges to label.
    pub labels: Vec<EdgeLabel>,
    /// Edge entity ids by the edge's offset in the stream, where known
    /// (`strips::edge_ids`): on the edges' paths and in their labels.
    pub edge_ids: HashMap<usize, u32>,
}

/// Where map points fall in the picture: metres from the top-left corner of
/// the whole map's picture, u to the right and v down. `width` and `height`
/// are the map's extent in x and in y. Only this knows which way north is.
#[derive(Debug, Clone, Copy)]
struct Picture {
    width: f64,
    height: f64,
    north: North,
}

impl Picture {
    fn new(header: &Header, north: North) -> Picture {
        Picture {
            width: f64::from(header.map_width),
            height: f64::from(header.map_height),
            north,
        }
    }

    fn pic(&self, (x, y): (f64, f64)) -> (f64, f64) {
        match self.north {
            North::Game => (x + self.width / 2.0, self.height / 2.0 - y),
            North::Sun => (self.height / 2.0 - y, self.width / 2.0 - x),
        }
    }

    /// A map vector as a picture vector.
    fn vec(&self, (dx, dy): (f64, f64)) -> (f64, f64) {
        match self.north {
            North::Game => (dx, -dy),
            North::Sun => (-dy, -dx),
        }
    }
}

/// The part of the map the picture shows: a box in map metres, and the
/// rectangle it fills in the picture.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    /// The rectangle's top-left corner and size, picture metres.
    u0: f64,
    v0: f64,
    width: f64,
    height: f64,
}

impl View {
    /// The view of the box `(x_min, x_max, y_min, y_max)`.
    fn new(picture: &Picture, (x_min, x_max, y_min, y_max): (f64, f64, f64, f64)) -> View {
        let (a, b) = (picture.pic((x_min, y_max)), picture.pic((x_max, y_min)));
        View {
            x_min,
            x_max,
            y_min,
            y_max,
            u0: a.0.min(b.0),
            v0: a.1.min(b.1),
            width: (a.0 - b.0).abs(),
            height: (a.1 - b.1).abs(),
        }
    }

    /// The whole map.
    fn whole(picture: &Picture, header: &Header) -> View {
        let (w, h) = (f64::from(header.map_width), f64::from(header.map_height));
        View::new(picture, (-w / 2.0, w / 2.0, -h / 2.0, h / 2.0))
    }

    /// Whether something within `reach` metres of (`x`, `y`) may show.
    fn reaches(&self, x: f64, y: f64, reach: f64) -> bool {
        self.x_min - reach <= x
            && x <= self.x_max + reach
            && self.y_min - reach <= y
            && y <= self.y_max + reach
    }

    /// Whether the edge may show: the box of its curve's control points,
    /// which holds the curve, meets the view.
    fn touches(&self, e: &Edge) -> bool {
        let c = e.bezier();
        let (xs, ys) = (c.map(|p| p.0), c.map(|p| p.1));
        let max = |v: [f64; 4]| v.into_iter().fold(f64::MIN, f64::max);
        let min = |v: [f64; 4]| v.into_iter().fold(f64::MAX, f64::min);
        max(xs) >= self.x_min
            && min(xs) <= self.x_max
            && max(ys) >= self.y_min
            && min(ys) <= self.y_max
    }
}

/// The opening tag of a layer: a group Inkscape shows as one (its
/// `groupmode` and `label`), and, as the prototype writes it, `class="layer"`
/// with the id repeated in `data-layer`, so a page script can list the layers
/// with `g.layer`. A group inside a layer that holds one item (a vehicle, a
/// depot, a badge) is no layer and has none of these.
fn layer(id: &str, label: &str, attrs: &str) -> String {
    let id = escape(id);
    format!(
        "<g id=\"{id}\" class=\"layer\" data-layer=\"{id}\" inkscape:groupmode=\"layer\" \
         inkscape:label=\"{}\"{attrs}>",
        escape(label)
    )
}

/// The edges that are drawn: all of them, or on a crop those that may show
/// in it. Empty for a crop off the map.
pub fn shown_edges<'a>(header: &Header, edges: &'a [Edge], options: &SvgOptions) -> Vec<&'a Edge> {
    let picture = Picture::new(header, options.north);
    match options.crop {
        None => edges.iter().collect(),
        Some(c) => match c.bounds(header) {
            Some(b) => {
                let view = View::new(&picture, b);
                edges.iter().filter(|e| view.touches(e)).collect()
            }
            None => Vec::new(),
        },
    }
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
    let picture = Picture::new(header, options.north);
    let crop = match options.crop {
        Some(c) => Some(c.bounds(header).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "the crop lies outside the map")
        })?),
        None => None,
    };
    let view = crop.map_or_else(|| View::whole(&picture, header), |b| View::new(&picture, b));
    let (w, h) = (view.width, view.height);
    // On a crop, only what may show in it.
    let shows = |i: &Instance, reach: f64| crop.is_none() || view.reaches(i.x, i.y, reach);
    let edges: Vec<&Edge> = edges
        .iter()
        .filter(|e| crop.is_none() || view.touches(e))
        .collect();
    let edge_ids = &layers.edge_ids;
    let station_rows: Vec<Instance> = layers
        .stations
        .iter()
        .filter(|i| shows(i, stations::reach(&i.model, &layers.station_boxes)))
        .cloned()
        .collect();
    let depot_rows: Vec<Instance> = layers
        .depots
        .iter()
        .filter(|i| shows(i, depots::REACH))
        .cloned()
        .collect();
    let coverage_rows: Vec<Instance> = layers
        .coverage
        .iter()
        .filter(|i| shows(i, coverage::reach(&i.model, &layers.depot_specs)))
        .cloned()
        .collect();
    let vehicle_rows: Vec<Instance> = layers
        .vehicles
        .iter()
        .filter(|i| shows(i, 0.0))
        .cloned()
        .collect();
    let industry_rows: Vec<(Instance, String)> = layers
        .industries
        .iter()
        .filter(|(i, key)| shows(i, industries::reach(key, &layers.industry_faces)))
        .cloned()
        .collect();

    let zoom = f64::from(options.zoom.max(1));
    writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\" \
         width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">",
        (w / options.metres_per_pixel).ceil() * zoom,
        (h / options.metres_per_pixel).ceil() * zoom,
        fmt_g(view.u0),
        fmt_g(view.v0),
        fmt_g(w),
        fmt_g(h)
    )?;
    writeln!(out, "<title>{}</title>", title(layers))?;
    writeln!(out, "<desc>{}</desc>", escape(&description(header)))?;
    writeln!(
        out,
        "{}",
        view_metadata(
            header,
            (view.x_min, view.x_max, view.y_min, view.y_max),
            options
        )
    )?;
    writeln!(
        out,
        "{}<rect{} width=\"{w}\" height=\"{h}\" fill=\"{BACKGROUND_COLOUR}\"/></g>",
        layer("background", "Background", ""),
        corner_attrs(&view)
    )?;
    match &layers.terrain {
        Some(t) if options.raster => terrain::write_raster(out, t, &picture, crop, options)?,
        Some(t) => terrain::write(out, t, &picture, crop, options)?,
        None => {}
    }
    let step = match options.grid {
        GridStep::Off => None,
        GridStep::Auto => {
            let step = match options.crop {
                Some(c) => grid::auto_step(2.0 * c.radius, true),
                None => grid::auto_step(w.min(h), false),
            };
            Some((step, step))
        }
        GridStep::Every(step) => Some((step, grid::MAJOR)),
    };
    let grid_labels = match step {
        Some((step, major)) => {
            grid::check_size(&view, step)?;
            let (marks, labels) = grid::layers(
                &picture,
                &view,
                options.metres_per_pixel,
                step,
                major,
                options.grid_lines,
                options.grid_dot_step,
            );
            write_lines(out, &marks)?;
            labels
        }
        None => Vec::new(),
    };
    write_lines(
        out,
        &coverage::layers(
            &picture,
            &coverage_rows,
            &layers.depot_specs,
            options.metres_per_pixel,
        ),
    )?;
    let (stations_under, stations_over) =
        stations::layers(&picture, &station_rows, &layers.station_boxes);
    write_lines(out, &stations_under)?;
    write_network(out, &picture, &edges, edge_ids, options)?;
    write_lines(
        out,
        &depots::layers(&picture, &depot_rows, &layers.depot_specs),
    )?;
    write_lines(out, &stations_over)?;
    write_lines(
        out,
        &industries::layers(&picture, &industry_rows, &layers.industry_faces),
    )?;
    write_lines(
        out,
        &vehicles::layers(
            &picture,
            &vehicle_rows,
            &layers.vehicle_names,
            options.metres_per_pixel,
        ),
    )?;
    write_lines(
        out,
        &labels::layers(
            &picture,
            &edges,
            &layers.labels,
            edge_ids,
            options.metres_per_pixel,
        ),
    )?;
    write_lines(out, &grid_labels)?;
    if let Some(place) = options.rose {
        write_lines(
            out,
            &rose::compass(&picture, &view, place, options.rose_size),
        )?;
    }
    writeln!(out, "</svg>")
}

/// Write the map as one flat PNG: the raster terrain (plain paper without
/// one) at `options.metres_per_pixel`, the whole map or the crop, with the
/// roads and then the rails drawn into its pixels, each as wide as its lanes
/// and at least one pixel. Nothing else is drawn. A `tf3map:view` text chunk
/// records where the picture lies on the map (`view_record`).
pub fn write_png(
    out: &mut impl Write,
    header: &Header,
    edges: &[Edge],
    terrain: Option<&Terrain>,
    options: &SvgOptions,
) -> io::Result<()> {
    let crop = match options.crop {
        Some(c) => Some(c.bounds(header).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "the crop lies outside the map")
        })?),
        None => None,
    };
    let sampling = raster::Sampling::new(options.metres_per_pixel);
    let (mut img, corner) = match terrain {
        Some(t) => {
            let (columns, rows) = terrain::grid_span(t, crop);
            raster::shade(t, columns, rows, sampling, options.north)?
        }
        None => {
            let (w, h) = (f64::from(header.map_width), f64::from(header.map_height));
            let b = crop.unwrap_or((-w / 2.0, w / 2.0, -h / 2.0, h / 2.0));
            let points = |lo: f64, hi: f64| ((hi - lo) / GRID_STEP).round() as usize + 1;
            let size = sampling.size(points(b.0, b.1), points(b.2, b.3), options.north);
            (
                raster::blank(size, PAPER)?,
                raster::Corner::of_box(options.north, b),
            )
        }
    };
    let size = (img.width, img.height);
    // On a crop, the edges that may show in the picture; on the whole map all,
    // since a runway can reach past the map's edge.
    let view = View::new(
        &Picture::new(header, options.north),
        corner.bounds(size, sampling.spacing),
    );
    let edges: Vec<&Edge> = edges
        .iter()
        .filter(|e| crop.is_none() || view.touches(e))
        .collect();
    raster::draw_network(&mut img, &corner, sampling.spacing, &edges, |e| {
        edge_width(e, options)
    });
    // A flat picture has no grid or rose, so neither is a setting of it.
    let flat = SvgOptions {
        grid: GridStep::Auto,
        rose: SvgOptions::default().rose,
        ..options.clone()
    };
    let record = view_record(header, &corner, size, sampling.spacing, &flat);
    out.write_all(&raster::encode(&img, Some(("tf3map:view", record)))?)
}

/// Where a PNG lies on the map, as one JSON object with the prototype's view
/// record names (`view_fields`).
fn view_record(
    header: &Header,
    corner: &raster::Corner,
    size: (usize, usize),
    spacing: f64,
    options: &SvgOptions,
) -> String {
    let fields = view_fields(header, corner.bounds(size, spacing), options);
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v, quoted)| {
            if *quoted {
                format!("\"{k}\": \"{v}\"")
            } else {
                format!("\"{k}\": {v}")
            }
        })
        .collect();
    format!("{{{}}}", body.join(", "))
}

/// The same record as a `<metadata>` element for the SVG, `bounds` being the
/// map box the picture shows. Every value is a string.
fn view_metadata(header: &Header, bounds: (f64, f64, f64, f64), options: &SvgOptions) -> String {
    let attrs: Vec<String> = view_fields(header, bounds, options)
        .iter()
        .map(|(k, v, _)| format!("{k}=\"{}\"", escape(v)))
        .collect();
    format!(
        "<metadata><tf3map:view xmlns:tf3map=\"urn:tf3-save-format:terrain-map:1\" {}/></metadata>",
        attrs.join(" ")
    )
}

/// The view record's names and values, in the prototype's order: the map's
/// size, the metres per pixel, which way north is, the map box `bounds`
/// `(x_min, x_max, y_min, y_max)` the picture spans (and for `North::Sun` the
/// map point at its top-left corner), the crop if any, what was set away from
/// its default, and the tool that drew it (with its commit when the caller
/// gives one). The flag says the value is text. The save's file name and the
/// time are left out.
fn view_fields(
    header: &Header,
    (x_min, x_max, y_min, y_max): (f64, f64, f64, f64),
    options: &SvgOptions,
) -> Vec<(&'static str, String, bool)> {
    let (orientation, north) = match options.north {
        North::Game => ("north-up (game): image up is +y, image right is +x", "game"),
        North::Sun => ("north-up (sun): image up is +x, image right is -y", "sun"),
    };
    let mut fields = vec![
        ("map-width", header.map_width.to_string(), false),
        ("map-height", header.map_height.to_string(), false),
        ("metres-per-pixel", fmt_g(options.metres_per_pixel), false),
        ("orientation", orientation.to_owned(), true),
        ("north", north.to_owned(), true),
        ("x-min", fmt_g(x_min), false),
        ("x-max", fmt_g(x_max), false),
        ("y-min", fmt_g(y_min), false),
        ("y-max", fmt_g(y_max), false),
    ];
    if options.north == North::Sun {
        fields.extend([
            ("x-top", fmt_g(x_max), false),
            ("y-left", fmt_g(y_max), false),
        ]);
    }
    if let Some(c) = options.crop {
        fields.extend([
            ("crop-x", fmt_g(c.x), false),
            ("crop-y", fmt_g(c.y), false),
            ("crop-radius", fmt_g(c.radius), false),
        ]);
    }
    fields.extend(non_default_settings(options));
    fields.extend(
        options
            .settings
            .iter()
            .map(|(name, value)| (*name, value.clone(), true)),
    );
    fields.extend([
        ("generator", "tf3map".to_owned(), true),
        (
            "generator-version",
            env!("CARGO_PKG_VERSION").to_owned(),
            true,
        ),
    ]);
    if let Some(commit) = options.generator_commit {
        fields.push(("generator-commit", commit.to_owned(), true));
    }
    fields
}

/// What was set away from its default, in the prototype's names where it has
/// one (`contour-step`, `index-every`, `bands`, `grid`, `grid-step`,
/// `grid-lines`, `terrain`), so the record says how the map was made.
fn non_default_settings(options: &SvgOptions) -> Vec<(&'static str, String, bool)> {
    let default = SvgOptions::default();
    let mut found = Vec::new();
    let mut number = |name, value: f64, default: f64| {
        if value != default {
            found.push((name, fmt_g(value), false));
        }
    };
    number("road-width", options.road_width, default.road_width);
    number("rail-width", options.rail_width, default.rail_width);
    number("min-width", options.min_width, default.min_width);
    number("contour-step", options.contour_step, default.contour_step);
    number(
        "index-every",
        f64::from(options.index_every),
        f64::from(default.index_every),
    );
    number(
        "grid-dot-step",
        options.grid_dot_step,
        default.grid_dot_step,
    );
    number("zoom", f64::from(options.zoom), f64::from(default.zoom));
    if let Some(size) = options.rose_size {
        found.push(("rose-size", fmt_g(size), false));
    }
    if options.raster {
        found.push(("terrain", "raster".to_owned(), true));
    }
    if !options.bands {
        found.push(("bands", "false".to_owned(), true));
    }
    match options.grid {
        GridStep::Auto => {}
        GridStep::Off => found.push(("grid", "off".to_owned(), true)),
        GridStep::Every(n) => found.push(("grid-step", n.to_string(), false)),
    }
    if options.grid_lines != default.grid_lines {
        let lines = match options.grid_lines {
            GridLines::Auto => "auto",
            GridLines::None => "none",
            GridLines::Dots => "dots",
            GridLines::Faint => "faint",
        };
        found.push(("grid-lines", lines.to_owned(), true));
    }
    if options.rose != default.rose {
        let rose = match options.rose {
            None => "off".to_owned(),
            Some(Rose::LowerRight) => "lower-right".to_owned(),
            Some(Rose::LowerLeft) => "lower-left".to_owned(),
            Some(Rose::UpperLeft) => "upper-left".to_owned(),
            Some(Rose::UpperRight) => "upper-right".to_owned(),
            Some(Rose::At(x, y)) => format!("{},{}", fmt_g(x), fmt_g(y)),
        };
        found.push(("rose", rose, true));
    }
    found
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
    edges: &[&Edge],
    ids: &HashMap<usize, u32>,
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
                    &format!("{prefix}-{}", safe_id(type_name)),
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
                let id = ids
                    .get(&e.offset)
                    .map_or(String::new(), |id| format!(" data-edge=\"{id:x}\""));
                writeln!(
                    out,
                    "<path data-a=\"{:x}\" data-b=\"{:x}\"{id}{own} d=\"{d}\"/>",
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

/// ` x="..." y="..."` for an element at the view's top-left corner, or
/// nothing when that is the picture's origin.
fn corner_attrs(view: &View) -> String {
    if view.u0 == 0.0 && view.v0 == 0.0 {
        String::new()
    } else {
        format!(" x=\"{}\" y=\"{}\"", fmt_g(view.u0), fmt_g(view.v0))
    }
}

/// Six significant digits without trailing zeros, as Python's `%g` writes
/// the numbers of the prototype (no exponent: picture metres never need one).
fn fmt_g(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let exponent = v.abs().log10().floor() as i32;
    let decimals = (5 - exponent).max(0) as usize;
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        &s
    };
    if s == "-0" { "0".into() } else { s.into() }
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
            rose: None,
            ..SvgOptions::default()
        };
        write_svg(&mut out, &header(), edges, &Layers::default(), &options).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_mod_road_type_makes_a_clean_layer_id() {
        assert_eq!(safe_id("wide+road_4"), "wide_road_4");
        assert_eq!(safe_id("town_old_small"), "town_old_small");
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
        assert_eq!(fmt_g(2048.0), "2048");
        assert_eq!(fmt_g(12.5), "12.5");
        assert_eq!(fmt_g(1.0 / 3.0), "0.333333");
        assert_eq!(fmt_g(123456.78), "123457");
        assert_eq!(fmt_g(-0.25), "-0.25");
        assert_eq!(fmt_g(-0.0), "0");
    }
}
