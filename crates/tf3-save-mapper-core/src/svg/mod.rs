//! The SVG writer.
//!
//! Coordinates inside the drawing are map metres from the picture's top-left
//! corner, with north up and east right (x is east and y is north in the game's
//! own frame, `docs/compass.md` in tf3-save-format), or with `North::Sun` x up
//! and y to the left. The `viewBox` is the map in metres (or the crop), so
//! widths are in metres too and scale with the picture. Layers are
//! groups Inkscape shows as layers, each also `class="layer"` with its id in
//! `data-layer` as in the prototype (`layer`), bottom to top: `background`, `terrain`,
//! `grid`, `tunnels`, `coverage`, `stations`, `warehouses`, `roads` and `rails` (each with
//! one sublayer per street or track type), `bridges`, the depot layers, `stations-street`,
//! `industries`, `vehicles`, the overlays asked for (`grades`), `labels`, `grid-labels`,
//! the overlays' keys (`grades-key`) and `compass`.
//!
//! `write_png` draws the map as one flat PNG instead: the raster terrain with
//! the roads and rails drawn into its pixels, as the prototype's `--png` does.

mod coverage;
mod dashes;
mod depots;
mod grades;
mod grid;
mod industries;
mod labels;
mod raster;
mod rose;
mod stations;
mod terrain;
mod vehicles;
mod warehouses;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{self, Write};

use crate::edges::{Edge, Family};
use crate::header::Header;
use crate::install::{DepotSpec, Polygon, Rect, WarehouseModel};
use crate::instances::Instance;
use crate::structures::Structure;
use crate::terrain::{GRID_STEP, Terrain};

pub use grid::{GridLines, GridStep};
pub use industries::faces_with_measured;
pub use labels::{EdgeLabel, unmatched as unmatched_labels};
pub use stations::is_station_piece;
pub use warehouses::{count as warehouse_count, is_warehouse_piece};

const ROAD_COLOUR: &str = "#323232";
const RAIL_COLOUR: &str = "#cd1e1e";
/// Edges in tunnels: the group's opacity, and the dashes along them:
/// at least so many metres, so many pixels at the map's metres per pixel,
/// and so many times the edge's width (a wide road's short dashes read as
/// blotches). The gap after each dash is two thirds of it.
const TUNNEL_OPACITY: &str = "0.45";
const TUNNEL_DASH: f64 = 12.0;
const TUNNEL_DASH_PX: f64 = 6.0;
const TUNNEL_DASH_WIDTHS: f64 = 1.5;
/// Edges on bridges: a line this wide along each side, in a dark shade of
/// the edge's colour (two fifths of it: `raster::ROAD_CASING`, `RAIL_CASING`).
const CASING: f64 = 1.5;
const ROAD_CASING: &str = "#141414";
const RAIL_CASING: &str = "#520c0c";
/// Plain paper under the network, so the dark roads read in a viewer with a dark theme.
const BACKGROUND_COLOUR: &str = "#ffffff";
/// The same paper in a PNG without terrain.
const PAPER: [u8; 3] = [255, 255, 255];
/// A bend shows as a curve only above this many metres of bulge.
const CURVE_TOLERANCE: f64 = 0.25;
/// The outline of a station, depot or industry footprint, metres.
const OUTLINE: f64 = 1.5;
/// Metres round a crop from which the drawing is worked out: everything
/// whose origin lies within it, so a station's border and a landing's anchor
/// come out as on the whole map unless the station reaches further. What is
/// written is then cut to the view by each part's drawn shape (`Cut`).
const GATHER: f64 = 500.0;

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
    /// The rose circle's diameter, metres. `None`: 250, or a twelfth of the
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
    /// Colour each track by its grade, in the layer `grades` over the
    /// network (`grades::FAMILIES`).
    pub rail_grades: bool,
    /// The grade, percent, from which a track takes the last colour; the
    /// other 13 colours are spread evenly below it.
    pub rail_grade_limit: f64,
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
            rail_grades: false,
            rail_grade_limit: grades::DEFAULT_LIMIT,
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
    /// The boxes of mod depots' pieces by model path (`install::depot_piece_boxes`).
    pub depot_boxes: HashMap<String, Rect>,
    pub stations: Vec<Instance>,
    /// Station piece boxes by model path.
    pub station_boxes: HashMap<String, Rect>,
    /// Warehouse entrances and module pieces (`warehouses/...`).
    pub warehouses: Vec<Instance>,
    /// Warehouse models' boxes and cargo bays by model path
    /// (`install::warehouse_models`); without them the warehouses have no
    /// marks inside.
    pub warehouse_models: HashMap<String, WarehouseModel>,
    /// Vehicle model instances (`vehicle/...`).
    pub vehicles: Vec<Instance>,
    /// Names to write beside vehicles, by instance slot.
    pub vehicle_names: HashMap<usize, String>,
    /// Edges to label.
    pub labels: Vec<EdgeLabel>,
    /// Edge entity ids by the edge's offset in the stream, where known
    /// (`strips::edge_ids`): on the edges' paths and in their labels.
    pub edge_ids: HashMap<usize, u32>,
    /// The edges in tunnels and on bridges, by the edge's offset in the
    /// stream (`structures::find_structures`); any other edge is on the ground.
    pub structures: HashMap<usize, Structure>,
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
    /// which holds the curve, widened by `pad` metres, meets the view.
    fn touches(&self, e: &Edge, pad: f64) -> bool {
        let c = e.bezier();
        let (xs, ys) = (c.map(|p| p.0), c.map(|p| p.1));
        let max = |v: [f64; 4]| v.into_iter().fold(f64::MIN, f64::max);
        let min = |v: [f64; 4]| v.into_iter().fold(f64::MAX, f64::min);
        max(xs) + pad >= self.x_min
            && min(xs) - pad <= self.x_max
            && max(ys) + pad >= self.y_min
            && min(ys) - pad <= self.y_max
    }
}

/// What of the drawing is written: on a crop, the parts whose drawn shape
/// meets the view (its rectangle in picture metres, `(u0, u1, v0, v1)`); on
/// the whole map, everything.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cut(Option<(f64, f64, f64, f64)>);

impl Cut {
    fn new(view: &View, crop: bool) -> Cut {
        Cut(crop.then_some((
            view.u0,
            view.u0 + view.width,
            view.v0,
            view.v0 + view.height,
        )))
    }

    /// Whether a shape with these points (picture metres), widened by `pad`
    /// metres each way, meets the view.
    fn meets(&self, pts: &[(f64, f64)], pad: f64) -> bool {
        let Some((u0, u1, v0, v1)) = self.0 else {
            return true;
        };
        let (a0, a1, b0, b1) = crate::geometry::bounds(pts);
        a1 + pad >= u0 && a0 - pad <= u1 && b1 + pad >= v0 && b0 - pad <= v1
    }
}

/// The edges a crop's `view` shows, each by its drawn width and `pad` metres
/// more; on the whole map all of them, since a runway can reach past its edge.
fn cut_edges<'a>(
    edges: &'a [Edge],
    view: Option<&View>,
    options: &SvgOptions,
    pad: f64,
) -> Vec<&'a Edge> {
    edges
        .iter()
        .filter(|e| view.is_none_or(|v| v.touches(e, edge_width(e, options) / 2.0 + pad)))
        .collect()
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

/// The edges that are drawn: all of them, or on a crop those that show in
/// it. Empty for a crop off the map.
pub fn shown_edges<'a>(header: &Header, edges: &'a [Edge], options: &SvgOptions) -> Vec<&'a Edge> {
    let picture = Picture::new(header, options.north);
    match options.crop {
        None => edges.iter().collect(),
        Some(c) => match c.bounds(header) {
            Some(b) => cut_edges(edges, Some(&View::new(&picture, b)), options, 0.0),
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
    // On a crop, what lies within `GATHER` of it (a coverage circle within
    // its radius); each layer draws from that and writes what shows (`cut`).
    let cut = Cut::new(&view, crop.is_some());
    let near = |i: &Instance, reach: f64| crop.is_none() || view.reaches(i.x, i.y, reach);
    let gather = |rows: &[Instance]| -> Vec<Instance> {
        rows.iter().filter(|i| near(i, GATHER)).cloned().collect()
    };
    let dashes = tunnel_dashes(edges, &layers.structures, options);
    let levels = raster::Levels {
        structures: &layers.structures,
        dashes: &dashes,
    };
    let edges = cut_edges(edges, crop.is_some().then_some(&view), options, 0.0);
    let edge_ids = &layers.edge_ids;
    let station_rows = gather(&layers.stations);
    let depot_rows = gather(&layers.depots);
    let warehouse_rows = gather(&layers.warehouses);
    let coverage_rows: Vec<Instance> = layers
        .coverage
        .iter()
        .filter(|i| {
            near(
                i,
                GATHER.max(coverage::reach(&i.model, &layers.depot_specs)),
            )
        })
        .cloned()
        .collect();
    let vehicle_rows = gather(&layers.vehicles);
    let industry_rows: Vec<(Instance, String)> = layers
        .industries
        .iter()
        .filter(|(i, _)| near(i, GATHER))
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
    // The tunnels under everything on the ground.
    write_network(
        out,
        &picture,
        &edges,
        edge_ids,
        &levels,
        Some(Structure::Tunnel),
        options,
    )?;
    write_lines(
        out,
        &coverage::layers(
            &picture,
            &coverage_rows,
            &layers.depot_specs,
            options.metres_per_pixel,
            cut,
        ),
    )?;
    let (stations_under, stations_over) =
        stations::layers(&picture, &station_rows, &layers.station_boxes, cut);
    write_lines(out, &stations_under)?;
    write_lines(
        out,
        &warehouses::layers(
            &picture,
            &warehouse_rows,
            &layers.warehouse_models,
            options.metres_per_pixel,
            cut,
        ),
    )?;
    write_network(out, &picture, &edges, edge_ids, &levels, None, options)?;
    write_network(
        out,
        &picture,
        &edges,
        edge_ids,
        &levels,
        Some(Structure::Bridge),
        options,
    )?;
    write_lines(
        out,
        &depots::layers(
            &picture,
            &depot_rows,
            &layers.depot_specs,
            &layers.depot_boxes,
            cut,
        ),
    )?;
    write_lines(out, &stations_over)?;
    write_lines(
        out,
        &industries::layers(&picture, &industry_rows, &layers.industry_faces, cut),
    )?;
    write_lines(
        out,
        &vehicles::layers(
            &picture,
            &vehicle_rows,
            &layers.vehicle_names,
            options.metres_per_pixel,
            cut,
        ),
    )?;
    // The overlays: figures about the network over everything drawn from
    // the save, under the labels, the grid's labels and the rose. Later ones
    // (speeds) go here too.
    if grades::any(options) {
        grades::write(out, &picture, &edges, edge_ids, &levels, options)?;
    }
    write_lines(
        out,
        &labels::layers(
            &picture,
            &edges,
            &layers.labels,
            edge_ids,
            options.metres_per_pixel,
            cut,
        ),
    )?;
    write_lines(out, &grid_labels)?;
    write_lines(out, &grades::key(&view, options))?;
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
    structures: &HashMap<usize, Structure>,
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
    let dashes = tunnel_dashes(edges, structures, options);
    // On a crop, the edges that show in the picture, each brush at least a
    // pixel wide; on the whole map all.
    let view = View::new(
        &Picture::new(header, options.north),
        corner.bounds(size, sampling.spacing),
    );
    let edges = cut_edges(
        edges,
        crop.is_some().then_some(&view),
        options,
        sampling.spacing,
    );
    let levels = raster::Levels {
        structures,
        dashes: &dashes,
    };
    raster::draw_network(
        &mut img,
        &corner,
        sampling.spacing,
        &edges,
        |e| edge_width(e, options),
        &levels,
    );
    if grades::any(options) {
        raster::draw_grades(
            &mut img,
            &corner,
            sampling.spacing,
            &edges,
            |e| edge_width(e, options),
            &levels,
            |e| grades::shade(e, options),
            (grades::CASING, grades::CASING_COLOUR),
        );
    }
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
                format!("\"{k}\": \"{}\"", json_escape(v))
            } else {
                format!("\"{k}\": {v}")
            }
        })
        .collect();
    format!("{{{}}}", body.join(", "))
}

/// Text for a JSON string: quotes, backslashes and control characters escaped.
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
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
/// `grid-lines`, `terrain`), so the record says how the map was made. The
/// dot step and the rose's size count only when there is a grid or a rose.
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
    if options.grid != GridStep::Off {
        number(
            "grid-dot-step",
            options.grid_dot_step,
            default.grid_dot_step,
        );
    }
    number("zoom", f64::from(options.zoom), f64::from(default.zoom));
    if let Some(size) = options.rose_size.filter(|_| options.rose.is_some()) {
        found.push(("rose-size", fmt_g(size), false));
    }
    if options.raster {
        found.push(("terrain", "raster".to_owned(), true));
    }
    if options.rail_grades {
        found.push(("rail-grades", "on".to_owned(), true));
        if options.rail_grade_limit != default.rail_grade_limit {
            found.push(("rail-grade-limit", fmt_g(options.rail_grade_limit), false));
        }
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

/// The point at `t` (0 to 1) of the cubic with control points `c`.
fn bezier_point(c: &[(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (
        w[0] * c[0].0 + w[1] * c[1].0 + w[2] * c[2].0 + w[3] * c[3].0,
        w[0] * c[0].1 + w[1] * c[1].1 + w[2] * c[2].1 + w[3] * c[3].1,
    )
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

/// The roads, then the rails, at one level: on the ground (`None`, layers
/// `roads` and `rails`), in tunnels (layer `tunnels`, faded and dashed) or on
/// bridges (layer `bridges`, each edge cased). A sublayer per street or track
/// type: `road-TYPE` or `rail-TYPE`, with `-tunnel` or `-bridge` after it
/// for the other two levels. The tunnel and bridge layers are written only
/// when they hold an edge.
fn write_network(
    out: &mut impl Write,
    picture: &Picture,
    edges: &[&Edge],
    ids: &HashMap<usize, u32>,
    levels: &raster::Levels,
    level: Option<Structure>,
    options: &SvgOptions,
) -> io::Result<()> {
    // Edges grouped by family and type, in stream order within each.
    let mut groups: BTreeMap<(u8, &str), Vec<&Edge>> = BTreeMap::new();
    for e in edges {
        if levels.structures.get(&e.offset).copied() != level {
            continue;
        }
        let family = u8::from(e.family == Family::Track);
        groups.entry((family, e.type_name())).or_default().push(e);
    }
    let path = |e: &Edge| edge_path(picture, e);
    let (suffix, style) = match level {
        None => (
            "",
            " stroke-linecap=\"round\" stroke-linejoin=\"round\"".to_owned(),
        ),
        Some(Structure::Tunnel) => (
            "-tunnel",
            " stroke-linecap=\"butt\" stroke-linejoin=\"round\"".to_owned(),
        ),
        Some(Structure::Bridge) => (
            "-bridge",
            " stroke-linecap=\"butt\" stroke-linejoin=\"round\"".to_owned(),
        ),
    };
    // Each outer layer and the families it holds: on the ground a layer
    // per family, in a tunnel or on a bridge one layer for both.
    let outer: Vec<(String, &[u8])> = match level {
        None => vec![
            (layer("roads", "Roads", ""), &[0]),
            (layer("rails", "Rails", ""), &[1]),
        ],
        _ if groups.is_empty() => Vec::new(),
        Some(Structure::Tunnel) => vec![(
            layer(
                "tunnels",
                "Tunnels",
                &format!(" opacity=\"{TUNNEL_OPACITY}\""),
            ),
            &[0, 1],
        )],
        Some(Structure::Bridge) => vec![(layer("bridges", "Bridges", ""), &[0, 1])],
    };
    for (open, families) in outer {
        writeln!(out, "{open}")?;
        for &family in families {
            let (prefix, label, colour, casing) = if family == 0 {
                ("road", "Roads", ROAD_COLOUR, ROAD_CASING)
            } else {
                ("rail", "Rails", RAIL_COLOUR, RAIL_CASING)
            };
            for ((_, type_name), members) in groups.range((family, "")..(family + 1, "")) {
                // The group's most common width goes on the group, any other on its path.
                let mut counts: BTreeMap<String, usize> = BTreeMap::new();
                for e in members {
                    *counts.entry(fmt_width(edge_width(e, options))).or_default() += 1;
                }
                let common = most_common(counts);
                // In a tunnel the same for the dash pattern, which follows the width.
                let dasharray = |e: &Edge| {
                    levels
                        .dashes
                        .get(&e.offset)
                        .map(|d| format!("{} {}", fmt_width(d.dash), fmt_width(d.gap)))
                };
                let mut counts: BTreeMap<String, usize> = BTreeMap::new();
                for e in members {
                    if let Some(d) = dasharray(e) {
                        *counts.entry(d).or_default() += 1;
                    }
                }
                let common_dashes = match level {
                    Some(Structure::Tunnel) => Some(most_common(counts)),
                    _ => None,
                };
                let group_dashes = common_dashes
                    .as_ref()
                    .map_or(String::new(), |d| format!(" stroke-dasharray=\"{d}\""));
                let place = match level {
                    None => String::new(),
                    Some(Structure::Tunnel) => " in tunnels".into(),
                    Some(Structure::Bridge) => " on bridges".into(),
                };
                writeln!(
                    out,
                    "{}",
                    layer(
                        &format!("{prefix}-{}{suffix}", safe_id(type_name)),
                        &format!("{label}: {type_name}{place}"),
                        &format!(
                            " fill=\"none\" stroke=\"{colour}\" stroke-width=\"{common}\"{style}{group_dashes}"
                        )
                    )
                )?;
                if level == Some(Structure::Bridge) {
                    // The casing first, under every fill of the group, so the
                    // edges of one bridge join without a line between them.
                    let cased = |w: &str| fmt_width(w.parse::<f64>().unwrap_or(0.0) + 2.0 * CASING);
                    writeln!(
                        out,
                        "<g class=\"casing\" stroke=\"{casing}\" stroke-width=\"{}\">",
                        cased(&common)
                    )?;
                    for e in members {
                        let width = fmt_width(edge_width(e, options));
                        let own = if width == common {
                            String::new()
                        } else {
                            format!(" stroke-width=\"{}\"", cased(&width))
                        };
                        writeln!(out, "<path{own} d=\"{}\"/>", path(e))?;
                    }
                    writeln!(out, "</g>")?;
                }
                for e in members {
                    let width = fmt_width(edge_width(e, options));
                    let own = if width == common {
                        String::new()
                    } else {
                        format!(" stroke-width=\"{width}\"")
                    };
                    let id = ids
                        .get(&e.offset)
                        .map_or(String::new(), |id| format!(" data-edge=\"{id:x}\""));
                    let phase = match levels.dashes.get(&e.offset) {
                        Some(d) if level == Some(Structure::Tunnel) => {
                            let own = dasharray(e).filter(|d| Some(d) != common_dashes.as_ref());
                            format!(
                                "{} stroke-dashoffset=\"{}\"",
                                own.map_or(String::new(), |d| format!(" stroke-dasharray=\"{d}\"")),
                                fmt1(d.phase)
                            )
                        }
                        _ => String::new(),
                    };
                    writeln!(
                        out,
                        "<path data-a=\"{:x}\" data-b=\"{:x}\"{id}{own}{phase} d=\"{}\"/>",
                        e.a,
                        e.b,
                        path(e)
                    )?;
                }
                writeln!(out, "</g>")?;
            }
        }
        writeln!(out, "</g>")?;
    }
    Ok(())
}

/// An edge's centre line as path data: a cubic where it bends, else a line.
fn edge_path(picture: &Picture, e: &Edge) -> String {
    let pt = |p: (f64, f64)| {
        let (u, v) = picture.pic(p);
        format!("{} {}", fmt1(u), fmt1(v))
    };
    let c = e.bezier();
    if e.is_curved(CURVE_TOLERANCE) {
        format!("M{}C{} {} {}", pt(c[0]), pt(c[1]), pt(c[2]), pt(c[3]))
    } else {
        format!("M{}L{}", pt(c[0]), pt(c[3]))
    }
}

/// The most common of `counts`' keys (the last of those tied).
fn most_common(counts: BTreeMap<String, usize>) -> String {
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(k, _)| k)
        .unwrap_or_default()
}

/// The dash pattern of every edge in a tunnel, by the edge's offset: its
/// dash and gap ([`tunnel_pattern`]) and its phase (`dashes::phases`, among
/// the edges with the same pattern), from all of them, so a crop shows the
/// same dashes as the whole map.
fn tunnel_dashes(
    edges: &[Edge],
    structures: &HashMap<usize, Structure>,
    options: &SvgOptions,
) -> HashMap<usize, raster::Dashes> {
    let mut by_pattern: BTreeMap<u64, Vec<&Edge>> = BTreeMap::new();
    for e in edges
        .iter()
        .filter(|e| structures.get(&e.offset) == Some(&Structure::Tunnel))
    {
        let (dash, _) = tunnel_pattern(edge_width(e, options), options.metres_per_pixel);
        by_pattern.entry(dash.to_bits()).or_default().push(e);
    }
    let mut out = HashMap::new();
    for (bits, group) in by_pattern {
        let dash = f64::from_bits(bits);
        let gap = tunnel_gap(dash);
        for (offset, phase) in dashes::phases(&group, dash, gap) {
            out.insert(offset, raster::Dashes { phase, dash, gap });
        }
    }
    out
}

/// The dash and gap along an edge `width` metres wide in a tunnel, metres,
/// at `mpp` metres per pixel.
fn tunnel_pattern(width: f64, mpp: f64) -> (f64, f64) {
    let dash = TUNNEL_DASH
        .max(TUNNEL_DASH_PX * mpp)
        .max(TUNNEL_DASH_WIDTHS * width);
    (dash, tunnel_gap(dash))
}

fn tunnel_gap(dash: f64) -> f64 {
    dash * 2.0 / 3.0
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
    fn a_tunnel_dash_is_at_least_one_and_a_half_widths() {
        // 12 m, or 6 px, or 1.5 widths, whichever is longest; the gap two
        // thirds of the dash.
        assert_eq!(tunnel_pattern(7.25, 1.0), (12.0, 8.0));
        assert_eq!(tunnel_pattern(7.25, 8.0), (48.0, 32.0));
        assert_eq!(tunnel_pattern(32.0, 1.0), (48.0, 32.0));
        assert_eq!(tunnel_pattern(15.5, 2.0), (23.25, 15.5));
    }

    #[test]
    fn a_wider_edge_in_a_tunnel_gets_longer_dashes_and_a_rail_bridge_a_dark_red_casing() {
        // At the whole map's 8 m per pixel the dash is at least 48 m; two
        // 40 m roads take 60 m (the sublayer's), a 48 m one 72 m (its own).
        let mut edges = Vec::new();
        for (k, (w, y)) in [(40.0, 0.0), (40.0, 500.0), (48.0, 1000.0)]
            .into_iter()
            .enumerate()
        {
            let mut e = edge("street/wide", [0.0, y, 0.0], [300.0, y, 0.0], Some(w));
            e.offset = k + 1;
            edges.push(e);
        }
        let mut rail = edge(
            "track/standard",
            [0.0, -500.0, 0.0],
            [300.0, -500.0, 0.0],
            Some(4.0),
        );
        rail.offset = 4;
        edges.push(rail);
        let layers = Layers {
            structures: HashMap::from([
                (1, Structure::Tunnel),
                (2, Structure::Tunnel),
                (3, Structure::Tunnel),
                (4, Structure::Bridge),
            ]),
            ..Layers::default()
        };
        let options = SvgOptions {
            rose: None,
            ..SvgOptions::default()
        };
        let mut out = Vec::new();
        write_svg(&mut out, &header(), &edges, &layers, &options).unwrap();
        let svg = String::from_utf8(out).unwrap();
        assert!(svg.contains(
            "stroke-width=\"40\" stroke-linecap=\"butt\" stroke-linejoin=\"round\" \
             stroke-dasharray=\"60 40\""
        ));
        assert!(svg.contains(" stroke-width=\"48\" stroke-dasharray=\"72 48\" stroke-dashoffset="));
        // Once each: the group's, and the one path's own.
        assert_eq!(svg.matches("stroke-dasharray=\"60 40\"").count(), 1);
        assert_eq!(svg.matches("stroke-dasharray=\"72 48\"").count(), 1);
        assert!(svg.contains("<g class=\"casing\" stroke=\"#520c0c\" stroke-width=\"7\">"));
    }

    #[test]
    fn a_bridge_casing_is_a_dark_shade_of_its_edge_colour() {
        let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        for (colour, casing, rgb, casing_rgb) in [
            (ROAD_COLOUR, ROAD_CASING, raster::ROAD, raster::ROAD_CASING),
            (RAIL_COLOUR, RAIL_CASING, raster::RAIL, raster::RAIL_CASING),
        ] {
            assert_eq!(hex(rgb), colour);
            assert_eq!(rgb.map(|c| (u16::from(c) * 2 / 5) as u8), casing_rgb);
            assert_eq!(hex(casing_rgb), casing);
        }
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
    fn a_crop_holds_the_edges_whose_width_reaches_into_it() {
        // The crop's box is y -100 to 100; a 16 m road 4 m past it reaches
        // in, one 12 m past it does not.
        let edges = [
            edge(
                "street/town_new_medium.lua",
                [-50.0, 104.0, 0.0],
                [50.0, 104.0, 0.0],
                Some(16.0),
            ),
            edge(
                "street/town_new_medium.lua",
                [-50.0, 112.0, 0.0],
                [50.0, 112.0, 0.0],
                Some(16.0),
            ),
        ];
        let options = SvgOptions {
            crop: Some(Crop {
                x: 0.0,
                y: 0.0,
                radius: 100.0,
            }),
            ..SvgOptions::default()
        };
        let shown = shown_edges(&header(), &edges, &options);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].p0[1], 104.0);
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
    fn a_crop_needs_two_grid_steps_of_the_map_each_way() {
        let h = header(); // 1000 x 2000 m: x from -500, y from -1000
        let crop = |x, radius| Crop { x, y: 0.0, radius }.bounds(&h);
        // 0 to 8 m: two steps.
        assert_eq!(crop(4.0, 4.0), Some((0.0, 8.0, -4.0, 4.0)));
        // 1 to 7 m widens to the grid, 0 to 8 m.
        assert_eq!(crop(4.0, 3.0), Some((0.0, 8.0, -4.0, 4.0)));
        // 0 to 4 m: one step.
        assert_eq!(crop(2.0, 2.0), None);
        // Past the east edge, two steps of the map are left.
        assert_eq!(crop(500.0, 8.0), Some((492.0, 500.0, -8.0, 8.0)));
        assert_eq!(crop(505.0, 8.0), None);
    }

    #[test]
    fn view_record_strings_are_json() {
        assert_eq!(json_escape("a\"b\\c\nd"), "a\\\"b\\\\c\\u000ad");
        let options = SvgOptions {
            settings: vec![("note", "say \"hi\"".to_owned())],
            ..SvgOptions::default()
        };
        let corner = raster::Corner::of_box(North::Game, (-500.0, 500.0, -1000.0, 1000.0));
        let record = view_record(&header(), &corner, (250, 500), 4.0, &options);
        assert!(record.contains("\"note\": \"say \\\"hi\\\"\""), "{record}");
        assert!(record.contains("\"x-max\": 500,"), "{record}");
    }

    #[test]
    fn a_setting_without_its_layer_is_not_recorded() {
        let names = |options: SvgOptions| -> Vec<&str> {
            non_default_settings(&options)
                .into_iter()
                .map(|(name, _, _)| name)
                .collect()
        };
        let sized = SvgOptions {
            rose_size: Some(100.0),
            grid_dot_step: 50.0,
            ..SvgOptions::default()
        };
        assert_eq!(names(sized.clone()), ["grid-dot-step", "rose-size"]);
        let bare = SvgOptions {
            rose: None,
            grid: GridStep::Off,
            ..sized
        };
        assert_eq!(names(bare), ["grid", "rose"]);
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
