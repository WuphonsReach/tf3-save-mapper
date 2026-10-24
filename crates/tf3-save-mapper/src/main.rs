//! Command-line front end for `tf3-save-mapper-core`: arguments in, SVG file out.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tf3_save_mapper_core::install::{self, DEPOT_KINDS};
use tf3_save_mapper_core::svg::{
    Crop, EdgeLabel, GridLines, GridStep, North, Rose, faces_with_measured, is_station_piece,
    shown_edges, unmatched_labels,
};
use tf3_save_mapper_core::{
    Edge, Family, Header, Install, Instance, Layers, SvgOptions, container, edge_ids,
    industry_models, read_edges, read_free_ids, read_header, read_instances, read_model_table,
    read_strips, read_terrain, retain_live, write_svg,
};

const USAGE: &str = "\
Draw an SVG map of a Transport Fever 3 save. Unofficial; never writes a save.

Usage: tf3map [OPTIONS] SAVE.sav

Options:
  -o, --output FILE   where to write the SVG (default: the save's name with .svg, in the current folder)
      --crop X,Y,R    draw only the square of half-width R metres round map point X,Y
      --mpp M         metres per pixel: 1 or 2 (with --crop only), or a multiple of 4 (default 8 for
                      the whole map's vector terrain, 4 on a crop and with --raster); the terrain is
                      cut from every (M / 4)th point of the 4 m heightmap
      --raster        draw the terrain as one shaded picture embedded in the SVG (height colours, hill
                      shading, water) instead of vector bands, water and height lines
      --zoom N        show the picture N times larger (its width and height, not the viewBox)
      --game DIR      the game's install folder (the one holding base/), read for the footprints of
                      depots, stations and industries (default: the usual Steam folders)
      --contour M     metres between height lines and height bands (default 20; vector terrain)
      --no-bands      leave out the coloured height bands (vector terrain)
      --no-terrain    leave out the terrain
      --no-industries leave out the industries
      --no-depots     leave out the depots and maintenance buildings
      --no-stations   leave out the stations
      --rose WHERE    where the compass rose goes: lower-right (default), lower-left, upper-left,
                      upper-right, off, or X,Y (map metres) for its centre
      --rose-size M   the rose circle's diameter in metres (default 250, at most an eighth of the
                      picture's shorter side)
      --no-rose       leave out the compass rose (the same as --rose off)
      --north N       which way north is drawn: game (default: up is +y, right is +x, as the
                      game's name compass has it) or sun (up is +x, right is -y, from the sun's path)
      --grid N        marks every N metres of map x and y, the 1000 m ones major (default: major marks
                      only, every third of the shorter side rounded down to 1, 2 or 5 x a power of ten,
                      at least 1000 m on a whole map)
      --no-grid       leave out the grid
      --grid-lines L  auto (default: crosses and dotted major lines, crosses only at 1 m per pixel or
                      finer), none (crosses only), dots or faint (also the minor lines, as sparse dots
                      or faint solid lines)
      --grid-dot-step M  metres between the dots of --grid-lines dots (default 25, at least 10 pixels)
      --min-width M   draw no road or track narrower than M metres (default 0, the real widths)
      --road-width M  width of a road whose lanes do not read, in metres (default 16)
      --rail-width M  width of a track whose lanes do not read, in metres (default 4)
      --keep-dead     also draw edges whose end node is in the free-id list (roads the game deleted)
      --vehicles      draw the vehicles: one shape per vehicle model instance, pointing where it faces
      --name SLOT=TEXT  write TEXT beside the vehicle whose data-slot is SLOT (repeatable); a default
                      name such as \"Road Vehicle 279\" is shortened to RV279
      --label A>B[=TEXT]  name the edge between nodes A and B (hex ids, either order) along its middle,
                      with TEXT or A>B, and mark its ends (repeatable)
      --edge-ids      with --label, add the edge's entity id where the strip table gives it
  -h, --help          show this help
  -V, --version       show the version";

/// Instances this far outside the map are stray matches, not buildings.
const MAP_MARGIN: f64 = 200.0;
/// The closest height lines may be, metres: closer, a mountain is thousands of lines.
const MIN_CONTOUR: f64 = 1.0;
/// Metres per pixel on a crop: the heightmap's own spacing.
const CROP_METRES_PER_PIXEL: u32 = 4;
/// Metres per pixel of the whole map's vector terrain.
const WHOLE_METRES_PER_PIXEL: u32 = 8;
/// Metres per pixel of a raster terrain: the heightmap's own spacing.
const RASTER_METRES_PER_PIXEL: u32 = 4;

struct Args {
    save: PathBuf,
    output: PathBuf,
    options: SvgOptions,
    keep_dead: bool,
    game: Option<PathBuf>,
    terrain: bool,
    industries: bool,
    depots: bool,
    stations: bool,
    vehicles: bool,
    names: HashMap<usize, String>,
    labels: Vec<EdgeLabel>,
    edge_ids: bool,
}

enum Parsed {
    Run(Box<Args>),
    Exit(String),
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut save = None;
    let mut output = None;
    let mut options = SvgOptions::default();
    let mut keep_dead = false;
    let mut game = None;
    let (mut terrain, mut industries, mut depots, mut stations) = (true, true, true, true);
    let (mut grid, mut no_grid) = (None, false);
    let mut mpp = None;
    let mut vector_only = Vec::new();
    let (mut vehicles, mut names) = (false, HashMap::new());
    let (mut labels, mut edge_ids) = (Vec::new(), false);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Exit(USAGE.into())),
            "-V" | "--version" => {
                return Ok(Parsed::Exit(format!(
                    "tf3map {}",
                    env!("CARGO_PKG_VERSION")
                )));
            }
            "-o" | "--output" => output = Some(PathBuf::from(value("--output")?)),
            "--crop" => options.crop = Some(crop(&value("--crop")?)?),
            "--mpp" => mpp = Some(metres_per_pixel(&value("--mpp")?)?),
            "--zoom" => options.zoom = zoom(&value("--zoom")?)?,
            "--road-width" => options.road_width = width(&value("--road-width")?)?,
            "--rail-width" => options.rail_width = width(&value("--rail-width")?)?,
            "--min-width" => options.min_width = metres(&value("--min-width")?, 0.0)?,
            "--contour" => {
                options.contour_step = metres(&value("--contour")?, MIN_CONTOUR)?;
                vector_only.push("--contour");
            }
            "--game" => game = Some(PathBuf::from(value("--game")?)),
            "--no-bands" => {
                options.bands = false;
                vector_only.push("--no-bands");
            }
            "--raster" => options.raster = true,
            "--no-terrain" => terrain = false,
            "--no-industries" => industries = false,
            "--no-depots" => depots = false,
            "--no-stations" => stations = false,
            "--no-rose" => options.rose = None,
            "--rose" => options.rose = rose(&value("--rose")?)?,
            "--rose-size" => options.rose_size = Some(width(&value("--rose-size")?)?),
            "--north" => options.north = north(&value("--north")?)?,
            "--grid" => grid = Some(grid_step(&value("--grid")?)?),
            "--no-grid" => no_grid = true,
            "--grid-lines" => options.grid_lines = grid_lines(&value("--grid-lines")?)?,
            "--grid-dot-step" => options.grid_dot_step = width(&value("--grid-dot-step")?)?,
            "--keep-dead" => keep_dead = true,
            "--vehicles" => vehicles = true,
            "--label" => labels.push(label(&value("--label")?)?),
            "--edge-ids" => edge_ids = true,
            "--name" => {
                let (slot, text) = name(&value("--name")?)?;
                names.insert(slot, text);
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option {flag}"));
            }
            _ if save.is_some() => return Err("give one save".into()),
            _ => save = Some(PathBuf::from(arg)),
        }
    }
    let save = save.ok_or("no save given (try --help)")?;
    let mpp = match mpp {
        Some(m) if m < 4 && options.crop.is_none() => {
            return Err(format!("--mpp {m} needs --crop"));
        }
        Some(m) => m,
        None if options.crop.is_some() => CROP_METRES_PER_PIXEL,
        None if options.raster => RASTER_METRES_PER_PIXEL,
        None => WHOLE_METRES_PER_PIXEL,
    };
    if options.raster {
        if !terrain {
            return Err("--raster draws the terrain: not with --no-terrain".into());
        }
        if let Some(flag) = vector_only.first() {
            return Err(format!("{flag} is for the vector terrain, not --raster"));
        }
    }
    options.metres_per_pixel = f64::from(mpp);
    options.grid = match (grid, no_grid) {
        (Some(_), true) => return Err("--no-grid and --grid exclude each other".into()),
        (Some(step), false) => GridStep::Every(step),
        (None, true) if options.grid_lines != GridLines::Auto => {
            return Err("--grid-lines needs a grid".into());
        }
        (None, true) => GridStep::Off,
        (None, false) => GridStep::Auto,
    };
    // From the whole name: `Map v1.5.sav` gives `Map v1.5.svg`, not `Map v1.svg`.
    let output = output
        .unwrap_or_else(|| Path::new(save.file_name().unwrap_or_default()).with_extension("svg"));
    Ok(Parsed::Run(Box::new(Args {
        save,
        output,
        options,
        keep_dead,
        game,
        terrain,
        industries,
        depots,
        stations,
        vehicles,
        names,
        labels,
        edge_ids,
    })))
}

/// A number of metres above 0.
fn width(text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(w) if w.is_finite() && w > 0.0 => Ok(w),
        _ => Err(format!("want a number of metres above 0, not {text:?}")),
    }
}

/// A corner, `off`, or `X,Y` in map metres.
fn rose(text: &str) -> Result<Option<Rose>, String> {
    Ok(Some(match text {
        "off" => return Ok(None),
        "lower-right" => Rose::LowerRight,
        "lower-left" => Rose::LowerLeft,
        "upper-left" => Rose::UpperLeft,
        "upper-right" => Rose::UpperRight,
        _ => {
            let v: Vec<f64> = text
                .split(',')
                .map(|n| n.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .unwrap_or_default();
            match v[..] {
                [x, y] if x.is_finite() && y.is_finite() => Rose::At(x, y),
                _ => {
                    return Err(format!(
                        "--rose wants off, a corner (lower-left, lower-right, upper-left, \
                         upper-right) or X,Y in metres, not {text:?}"
                    ));
                }
            }
        }
    }))
}

fn north(text: &str) -> Result<North, String> {
    match text {
        "game" => Ok(North::Game),
        "sun" => Ok(North::Sun),
        _ => Err(format!("--north wants game or sun, not {text:?}")),
    }
}

/// `X,Y,R` in metres, R above 0.
fn crop(text: &str) -> Result<Crop, String> {
    let v: Vec<f64> = text
        .split(',')
        .map(|n| n.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("--crop wants X,Y,R in metres, not {text:?}"))?;
    match v[..] {
        [x, y, radius] if v.iter().all(|n| n.is_finite()) && radius > 0.0 => {
            Ok(Crop { x, y, radius })
        }
        _ => Err(format!(
            "--crop wants X,Y,R in metres, R above 0, not {text:?}"
        )),
    }
}

/// `A>B[=TEXT]`, A and B node ids in hex.
fn label(text: &str) -> Result<EdgeLabel, String> {
    let bad = || format!("--label wants A>B[=TEXT] with hex node ids, not {text:?}");
    let (pair, words) = text.split_once('=').unwrap_or((text, ""));
    let (a, b) = pair.split_once('>').ok_or_else(bad)?;
    let hex = |v: &str| {
        let v = v.trim();
        let v = v
            .strip_prefix("0x")
            .or_else(|| v.strip_prefix("0X"))
            .unwrap_or(v);
        u32::from_str_radix(v, 16).ok()
    };
    Ok(EdgeLabel {
        a: hex(a).ok_or_else(bad)?,
        b: hex(b).ok_or_else(bad)?,
        text: (!words.is_empty()).then(|| words.to_owned()),
    })
}

/// `SLOT=TEXT`, the slot a whole number.
fn name(text: &str) -> Result<(usize, String), String> {
    text.split_once('=')
        .and_then(|(slot, name)| Some((slot.trim().parse::<usize>().ok()?, name.to_owned())))
        .ok_or(format!(
            "--name wants SLOT=TEXT, with a number for the slot, not {text:?}"
        ))
}

/// A zoom: a whole number, 1 or more.
fn zoom(text: &str) -> Result<u32, String> {
    match text.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!(
            "--zoom wants a whole number, 1 or more, not {text:?}"
        )),
    }
}

/// Metres per pixel: 1, 2 or a multiple of 4, the heightmap's spacing.
fn metres_per_pixel(text: &str) -> Result<u32, String> {
    match text.parse::<u32>() {
        Ok(m) if m == 1 || m == 2 || (m >= 4 && m % 4 == 0) => Ok(m),
        _ => Err(format!(
            "--mpp wants 1, 2 or a multiple of 4 (the heightmap's spacing), not {text:?}"
        )),
    }
}

/// A grid step: a whole number of metres, 1 or more.
fn grid_step(text: &str) -> Result<u32, String> {
    match text.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!(
            "--grid wants a whole number of metres, 1 or more, not {text:?}"
        )),
    }
}

fn grid_lines(text: &str) -> Result<GridLines, String> {
    Ok(match text {
        "auto" => GridLines::Auto,
        "none" => GridLines::None,
        "dots" => GridLines::Dots,
        "faint" => GridLines::Faint,
        _ => {
            return Err(format!(
                "--grid-lines wants auto, none, dots or faint, not {text:?}"
            ));
        }
    })
}

/// A number of metres, at least `least`.
fn metres(text: &str, least: f64) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(m) if m.is_finite() && m >= least => Ok(m),
        _ => Err(format!(
            "want a number of metres of {least} or more, not {text:?}"
        )),
    }
}

fn run(args: Args) -> Result<(), String> {
    // A read-only tool: never let the output be a save, or the save itself.
    let same_file = match (args.save.canonicalize(), args.output.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if names_a_save(&args.output) || same_file {
        return Err("the output must not be a .sav file".into());
    }

    let o = &args.options;
    let fewest = 10.0 * o.metres_per_pixel;
    if o.grid_lines == GridLines::Dots && o.grid_dot_step < fewest {
        eprintln!(
            "--grid-dot-step {} is under 10 pixels at {} m per pixel; dots will be {fewest} m apart",
            o.grid_dot_step, o.metres_per_pixel
        );
    }

    let file = File::open(&args.save).map_err(|e| format!("cannot open the save: {e}"))?;
    let cannot_read = |e: tf3_save_mapper_core::Error| format!("cannot read the save: {e}");
    let stream = container::decompress(file).map_err(cannot_read)?;
    let header = read_header(&stream).map_err(cannot_read)?;
    if let Some(c) = args.options.crop
        && c.bounds(&header).is_none()
    {
        return Err(format!(
            "the crop lies outside the map ({} x {} m, centred on 0,0)",
            header.map_width, header.map_height
        ));
    }
    let mut edges = read_edges(&stream).map_err(cannot_read)?;

    let mut dropped = 0;
    if !args.keep_dead {
        match read_free_ids(&stream) {
            Some(free) => dropped = retain_live(&mut edges, &free),
            None => eprintln!("free-id list not found: every edge is drawn"),
        }
    }
    let tracks = edges.iter().filter(|e| e.family == Family::Track).count();
    let unread = edges.iter().filter(|e| e.width.is_none()).count();
    if unread > 0 {
        eprintln!("{unread} edges whose lanes did not read are drawn at the fallback width");
    }

    let mut layers = read_layers(&args, &stream, &header)?;
    if !args.labels.is_empty() {
        for l in unmatched_labels(&args.labels, &shown_edges(&header, &edges, &args.options)) {
            let (a, b) = (l.a.min(l.b), l.a.max(l.b));
            eprintln!("no edge between {a:x} and {b:x} in the picture");
        }
        if args.edge_ids {
            layers.edge_ids = edge_ids(&edges, &read_strips(&stream));
        }
        layers.labels = args.labels.clone();
    } else if args.edge_ids {
        eprintln!("--edge-ids works with --label: no ids written");
    }
    write_output(&args, &header, &edges, &layers)?;

    let vehicles = if args.vehicles {
        format!(", {} vehicles", layers.vehicles.len())
    } else {
        String::new()
    };
    eprintln!(
        "{} road edges, {} track edges ({dropped} deleted ones left out), {} industries, \
         {} depots, {} station pieces{vehicles}; wrote {}",
        edges.len() - tracks,
        tracks,
        layers.industries.len(),
        layers.depots.len(),
        layers.stations.len(),
        args.output.display()
    );
    Ok(())
}

/// The terrain, and the industries, depots and stations with what the game
/// install says about their shapes. A part that cannot be read is left out
/// with a note, and the map is drawn without it.
fn read_layers(args: &Args, stream: &[u8], header: &Header) -> Result<Layers, String> {
    let mut layers = Layers::default();
    if args.terrain {
        match read_terrain(stream) {
            Ok(t) => layers.terrain = Some(t),
            Err(e) => eprintln!("{e}: the map is drawn without terrain"),
        }
    }
    if !(args.industries || args.depots || args.stations || args.vehicles) {
        return Ok(layers);
    }
    let wants_install = args.industries || args.depots || args.stations;
    let found = if wants_install {
        Install::find(args.game.as_deref())
    } else {
        None
    };
    let game = match (&found, &args.game) {
        (Some(game), _) => Some(game),
        (None, _) if !wants_install => None,
        (None, Some(dir)) => {
            return Err(format!(
                "no game install in {} (want the folder holding base/content)",
                dir.display()
            ));
        }
        (None, None) => {
            eprintln!(
                "game install not found (try --game DIR): no depots or stations, \
                 and industries without footprints"
            );
            None
        }
    };
    let table = match read_model_table(stream) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}: no industries, depots, stations or vehicles");
            return Ok(layers);
        }
    };
    let mut prefixes = Vec::new();
    if args.industries {
        prefixes.push("industries/");
    }
    if args.depots && game.is_some() {
        prefixes.extend(["depots/", "assets/ingo_bootsschuppen/"]);
    }
    if args.stations && game.is_some() {
        prefixes.extend(["stations/water/", "stations/rail/", "stations/street/"]);
    }
    if args.vehicles {
        prefixes.push("vehicle/");
    }
    if prefixes.is_empty() {
        return Ok(layers);
    }
    let (half_w, half_h) = (
        f64::from(header.map_width) / 2.0 + MAP_MARGIN,
        f64::from(header.map_height) / 2.0 + MAP_MARGIN,
    );
    let instances: Vec<Instance> = read_instances(stream, &table, &prefixes)
        .into_iter()
        .filter(|i| i.x.abs() <= half_w && i.y.abs() <= half_h)
        .collect();

    if args.industries {
        let mains = industry_models(&table, &instances);
        layers.industries = instances
            .iter()
            .filter_map(|i| Some((i.clone(), mains.get(&i.model)?.clone())))
            .collect();
        layers.industry_faces =
            faces_with_measured(game.map(install::industry_faces).unwrap_or_default());
    }
    if args.vehicles {
        layers.vehicles = instances
            .iter()
            .filter(|i| i.model.starts_with("vehicle/"))
            .cloned()
            .collect();
        layers.vehicle_names = args.names.clone();
    }
    if let Some(game) = game {
        if args.depots {
            layers.depot_specs = install::depot_specs(game);
            layers.depots = instances
                .iter()
                .filter(|i| DEPOT_KINDS.iter().any(|k| k.model == i.model))
                .cloned()
                .collect();
        }
        if args.stations {
            layers.station_boxes = install::station_boxes(game);
            layers.stations = instances
                .iter()
                .filter(|i| is_station_piece(&i.model, &layers.station_boxes))
                .cloned()
                .collect();
        }
    }
    Ok(layers)
}

/// True when writing to `path` would land on a `.sav`. Windows drops trailing
/// dots and spaces from a name and reads `name:stream` as a stream of `name`,
/// so `b.sav.`, `b.sav ` and `b.sav::$DATA` all write to `b.sav` there. They
/// are refused on every system.
fn names_a_save(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy();
    let name = name.split(':').next().unwrap_or_default();
    Path::new(name.trim_end_matches(['.', ' ']))
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("sav"))
}

/// The `attempt`th name tried for the temporary file beside `output`. The
/// process id keeps a file left by a killed run from blocking the next one.
fn temp_path(output: &Path, attempt: u32) -> PathBuf {
    let mut temp = output.as_os_str().to_owned();
    temp.push(format!(".{}-{attempt}.tmp", std::process::id()));
    PathBuf::from(temp)
}

/// Write to a new file beside the output and rename it into place. The rename
/// replaces the name and never writes through it, so an output that is a hard
/// link to the save leaves the save's contents alone.
fn write_output(
    args: &Args,
    header: &Header,
    edges: &[Edge],
    layers: &Layers,
) -> Result<(), String> {
    let cannot_write = |e: std::io::Error| format!("cannot write the output: {e}");

    // A new file only (never through an existing name); skip names already taken.
    let mut attempt = 0;
    let (temp, file) = loop {
        let temp = temp_path(&args.output, attempt);
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 100 => {
                attempt += 1;
            }
            Err(e) => return Err(cannot_write(e)),
        }
    };
    let mut out = BufWriter::new(file);
    let written = write_svg(&mut out, header, edges, layers, &args.options)
        .and_then(|()| out.flush())
        .and_then(|()| std::fs::rename(&temp, &args.output));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(cannot_write(e));
    }
    Ok(())
}

fn main() -> ExitCode {
    let result = parse_args(std::env::args().skip(1)).and_then(|parsed| match parsed {
        Parsed::Run(args) => run(*args),
        Parsed::Exit(text) => {
            println!("{text}");
            Ok(())
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tf3map: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        parse_args(args.iter().map(|a| a.to_string()))
    }

    fn run_args(args: &[&str]) -> Args {
        match parse(args) {
            Ok(Parsed::Run(a)) => *a,
            _ => panic!("expected a run"),
        }
    }

    #[test]
    fn output_defaults_to_the_save_name_in_the_current_folder() {
        let a = run_args(&["some/folder/My Save.sav"]);
        assert_eq!(a.output, Path::new("My Save.svg"));
        assert!(!a.keep_dead);
    }

    #[test]
    fn output_keeps_every_dot_of_the_save_name() {
        let a = run_args(&["Map v1.5.sav"]);
        assert_eq!(a.output, Path::new("Map v1.5.svg"));
        let a = run_args(&["folder/game.2024.sav"]);
        assert_eq!(a.output, Path::new("game.2024.svg"));
    }

    #[test]
    fn an_output_that_windows_reads_as_a_save_is_refused() {
        for name in [
            "b.sav",
            "dir/B.SAV",
            "b.sav.",
            "b.sav ",
            "b.sav. .",
            "b.sav::$DATA",
            "b.sav:stream",
        ] {
            assert!(names_a_save(Path::new(name)), "{name:?}");
        }
        for name in ["b.svg", "b.sav.svg", "sav", "b.save", "dir.sav/b.svg"] {
            assert!(!names_a_save(Path::new(name)), "{name:?}");
        }
    }

    #[test]
    fn options_are_read() {
        let a = run_args(&[
            "s.sav",
            "-o",
            "out.svg",
            "--road-width",
            "12.5",
            "--keep-dead",
            "--game",
            "games/tf3",
            "--contour",
            "10",
            "--no-stations",
            "--no-rose",
        ]);
        assert_eq!(a.output, Path::new("out.svg"));
        assert_eq!(a.options.road_width, 12.5);
        assert!(a.keep_dead);
        assert_eq!(a.game.as_deref(), Some(Path::new("games/tf3")));
        assert_eq!(a.options.contour_step, 10.0);
        assert!(!a.stations && a.options.rose.is_none());
        assert!(a.terrain && a.industries && a.depots && a.options.bands);
        assert_eq!(a.options.grid, GridStep::Auto, "a grid by default");
    }

    #[test]
    fn grid_options_are_read() {
        let a = run_args(&["s.sav", "--grid", "250", "--grid-lines", "dots"]);
        assert_eq!(a.options.grid, GridStep::Every(250));
        assert_eq!(a.options.grid_lines, GridLines::Dots);
        let a = run_args(&["s.sav", "--no-grid", "--grid-dot-step", "40"]);
        assert_eq!(a.options.grid, GridStep::Off);
        assert_eq!(a.options.grid_dot_step, 40.0);
        assert!(parse(&["s.sav", "--grid", "0"]).is_err(), "too fine");
        assert!(parse(&["s.sav", "--grid", "2.5"]).is_err(), "not whole");
        assert!(parse(&["s.sav", "--grid", "100", "--no-grid"]).is_err());
        assert!(parse(&["s.sav", "--no-grid", "--grid-lines", "faint"]).is_err());
        assert!(parse(&["s.sav", "--grid-lines", "solid"]).is_err());
    }

    #[test]
    fn compass_options_are_read() {
        let a = run_args(&["s.sav"]);
        assert_eq!(a.options.rose, Some(Rose::LowerRight));
        assert_eq!(a.options.north, North::Game);
        let a = run_args(&[
            "s.sav",
            "--rose",
            "upper-left",
            "--rose-size",
            "400",
            "--north",
            "sun",
        ]);
        assert_eq!(a.options.rose, Some(Rose::UpperLeft));
        assert_eq!(a.options.rose_size, Some(400.0));
        assert_eq!(a.options.north, North::Sun);
        let a = run_args(&["s.sav", "--rose", "-120.5, 300"]);
        assert_eq!(a.options.rose, Some(Rose::At(-120.5, 300.0)));
        assert_eq!(run_args(&["s.sav", "--rose", "off"]).options.rose, None);
        for bad in ["middle", "1", "1,2,3", "a,b"] {
            assert!(parse(&["s.sav", "--rose", bad]).is_err(), "{bad}");
        }
        assert!(parse(&["s.sav", "--rose-size", "0"]).is_err());
        assert!(parse(&["s.sav", "--north", "up"]).is_err());
    }

    #[test]
    fn labels_are_read() {
        let a = run_args(&[
            "s.sav",
            "--label",
            "2a619>342aa",
            "--label",
            "0x10>FF=Main street = old",
            "--edge-ids",
        ]);
        assert_eq!(
            a.labels,
            [
                EdgeLabel {
                    a: 0x2a619,
                    b: 0x342aa,
                    text: None
                },
                EdgeLabel {
                    a: 0x10,
                    b: 0xff,
                    text: Some("Main street = old".into())
                }
            ]
        );
        assert!(a.edge_ids);
        for bad in ["2a619", "2a619>", "x>y", "1>2>3", "12345678901>1"] {
            assert!(parse(&["s.sav", "--label", bad]).is_err(), "{bad}");
        }
    }

    #[test]
    fn vehicles_and_their_names_are_read() {
        let a = run_args(&[
            "s.sav",
            "--vehicles",
            "--name",
            "1204=Road Vehicle 7",
            "--name",
            "88=a=b",
        ]);
        assert!(a.vehicles);
        assert_eq!(a.names[&1204], "Road Vehicle 7");
        assert_eq!(a.names[&88], "a=b");
        assert!(parse(&["s.sav", "--name", "bus=x"]).is_err());
        assert!(parse(&["s.sav", "--name", "12"]).is_err());
    }

    #[test]
    fn a_crop_is_drawn_at_4_m_per_pixel() {
        let a = run_args(&["s.sav", "--crop", "-120.5,300,250", "--zoom", "3"]);
        assert_eq!(
            a.options.crop,
            Some(Crop {
                x: -120.5,
                y: 300.0,
                radius: 250.0
            })
        );
        assert_eq!(a.options.metres_per_pixel, 4.0);
        assert_eq!(a.options.zoom, 3);
        assert_eq!(run_args(&["s.sav"]).options.metres_per_pixel, 8.0);
        for bad in ["1,2", "1,2,0", "1,2,-5", "a,b,c", "1,2,3,4", "1,2,inf"] {
            assert!(parse(&["s.sav", "--crop", bad]).is_err(), "{bad}");
        }
        assert!(parse(&["s.sav", "--zoom", "0"]).is_err());
        assert!(!run_args(&["s.sav"]).vehicles, "no vehicles by default");
        assert!(parse(&["s.sav", "--zoom", "1.5"]).is_err());
    }

    #[test]
    fn metres_per_pixel_are_read_whatever_the_order() {
        let mpp = |args: &[&str]| run_args(args).options.metres_per_pixel;
        assert_eq!(mpp(&["s.sav", "--mpp", "16"]), 16.0);
        assert_eq!(mpp(&["s.sav", "--mpp", "2", "--crop", "0,0,100"]), 2.0);
        assert_eq!(mpp(&["s.sav", "--crop", "0,0,100", "--mpp", "1"]), 1.0);
        assert_eq!(mpp(&["s.sav", "--crop", "0,0,100", "--mpp", "12"]), 12.0);
        for bad in ["0", "3", "6", "2.5", "-4", "four"] {
            assert!(parse(&["s.sav", "--mpp", bad]).is_err(), "{bad}");
        }
        let finer = parse(&["s.sav", "--mpp", "2"]);
        assert!(matches!(finer, Err(e) if e.contains("needs --crop")));
    }

    #[test]
    fn a_raster_terrain_is_drawn_at_4_m_per_pixel() {
        let a = run_args(&["s.sav", "--raster"]);
        assert!(a.options.raster && a.terrain);
        assert_eq!(a.options.metres_per_pixel, 4.0);
        assert_eq!(a.output, Path::new("s.svg"));
        let a = run_args(&["s.sav", "--mpp", "16", "--raster"]);
        assert_eq!(a.options.metres_per_pixel, 16.0);
        assert!(!run_args(&["s.sav"]).options.raster, "vector by default");
        for bad in [
            &["s.sav", "--raster", "--no-terrain"][..],
            &["s.sav", "--raster", "--no-bands"],
            &["s.sav", "--contour", "10", "--raster"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(parse(&[]).is_err(), "no save");
        assert!(parse(&["a.sav", "b.sav"]).is_err(), "two saves");
        assert!(parse(&["a.sav", "--nope"]).is_err(), "unknown option");
        assert!(parse(&["a.sav", "-o"]).is_err(), "missing value");
        assert!(
            parse(&["a.sav", "--road-width", "0"]).is_err(),
            "zero width"
        );
        assert!(
            parse(&["a.sav", "--rail-width", "wide"]).is_err(),
            "not a number"
        );
        assert!(
            parse(&["a.sav", "--contour", "-5"]).is_err(),
            "negative step"
        );
        assert!(
            parse(&["a.sav", "--contour", "0.5"]).is_err(),
            "too fine a step"
        );
        assert!(matches!(
            parse(&["a.sav", "--min-width", "0"]),
            Ok(Parsed::Run(_))
        ));
    }

    #[test]
    fn help_and_version_exit_early() {
        assert!(matches!(parse(&["--help"]), Ok(Parsed::Exit(t)) if t.contains("Usage")));
        assert!(matches!(parse(&["-V"]), Ok(Parsed::Exit(t)) if t.starts_with("tf3map ")));
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tf3map-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_header() -> Header {
        Header {
            version: 604,
            start_year: 1900,
            map_width: 100,
            map_height: 100,
            date: 0,
            climate: None,
            map_seed: None,
        }
    }

    fn temp_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "tmp"))
            .collect()
    }

    #[test]
    fn a_temp_file_left_by_a_killed_run_does_not_block_the_next() {
        let dir = test_dir("stale");
        let output = dir.join("map.svg");
        let stale = temp_path(&output, 0);
        std::fs::write(&stale, b"left over").unwrap();

        let args = run_args(&["absent.sav", "-o", output.to_str().unwrap()]);
        write_output(&args, &test_header(), &[], &Layers::default()).unwrap();

        assert!(
            std::fs::read_to_string(&output)
                .unwrap()
                .starts_with("<svg")
        );
        assert_eq!(std::fs::read(&stale).unwrap(), b"left over");
        assert_eq!(temp_files(&dir), [stale]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_output_that_is_a_hard_link_to_the_save_leaves_the_save_alone() {
        let dir = test_dir("link");
        let save = dir.join("save-stand-in");
        let linked = dir.join("linked.svg");
        std::fs::write(&save, b"pretend save contents").unwrap();
        std::fs::hard_link(&save, &linked).unwrap();

        let args = run_args(&[save.to_str().unwrap(), "-o", linked.to_str().unwrap()]);
        write_output(&args, &test_header(), &[], &Layers::default()).unwrap();

        assert_eq!(std::fs::read(&save).unwrap(), b"pretend save contents");
        assert!(
            std::fs::read_to_string(&linked)
                .unwrap()
                .starts_with("<svg")
        );
        assert!(temp_files(&dir).is_empty(), "no temp file is left");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
