//! Command-line front end for `tf3-save-mapper-core`: arguments in, SVG file out.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tf3_save_mapper_core::install::{self, DEPOT_KINDS};
use tf3_save_mapper_core::svg::{GridLines, GridStep, faces_with_measured, is_station_piece};
use tf3_save_mapper_core::{
    Edge, Family, Header, Install, Instance, Layers, SvgOptions, container, industry_models,
    read_edges, read_free_ids, read_header, read_instances, read_model_table, read_terrain,
    retain_live, write_svg,
};

const USAGE: &str = "\
Draw an SVG map of a Transport Fever 3 save. Unofficial; never writes a save.

Usage: tf3map [OPTIONS] SAVE.sav

Options:
  -o, --output FILE   where to write the SVG (default: the save's name with .svg, in the current folder)
      --game DIR      the game's install folder (the one holding base/), read for the footprints of
                      depots, stations and industries (default: the usual Steam folders)
      --contour M     metres between height lines and height bands (default 20)
      --no-bands      leave out the coloured height bands
      --no-terrain    leave out the terrain
      --no-industries leave out the industries
      --no-depots     leave out the depots and maintenance buildings
      --no-stations   leave out the stations
      --no-rose       leave out the compass rose
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
  -h, --help          show this help
  -V, --version       show the version";

/// Instances this far outside the map are stray matches, not buildings.
const MAP_MARGIN: f64 = 200.0;
/// The closest height lines may be, metres: closer, a mountain is thousands of lines.
const MIN_CONTOUR: f64 = 1.0;

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
}

enum Parsed {
    Run(Args),
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
            "--road-width" => options.road_width = width(&value("--road-width")?)?,
            "--rail-width" => options.rail_width = width(&value("--rail-width")?)?,
            "--min-width" => options.min_width = metres(&value("--min-width")?, 0.0)?,
            "--contour" => options.contour_step = metres(&value("--contour")?, MIN_CONTOUR)?,
            "--game" => game = Some(PathBuf::from(value("--game")?)),
            "--no-bands" => options.bands = false,
            "--no-terrain" => terrain = false,
            "--no-industries" => industries = false,
            "--no-depots" => depots = false,
            "--no-stations" => stations = false,
            "--no-rose" => options.rose = false,
            "--grid" => grid = Some(grid_step(&value("--grid")?)?),
            "--no-grid" => no_grid = true,
            "--grid-lines" => options.grid_lines = grid_lines(&value("--grid-lines")?)?,
            "--grid-dot-step" => options.grid_dot_step = width(&value("--grid-dot-step")?)?,
            "--keep-dead" => keep_dead = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option {flag}"));
            }
            _ if save.is_some() => return Err("give one save".into()),
            _ => save = Some(PathBuf::from(arg)),
        }
    }
    let save = save.ok_or("no save given (try --help)")?;
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
    Ok(Parsed::Run(Args {
        save,
        output,
        options,
        keep_dead,
        game,
        terrain,
        industries,
        depots,
        stations,
    }))
}

/// A number of metres above 0.
fn width(text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(w) if w.is_finite() && w > 0.0 => Ok(w),
        _ => Err(format!("want a number of metres above 0, not {text:?}")),
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

    let layers = read_layers(&args, &stream, &header)?;
    write_output(&args, &header, &edges, &layers)?;

    eprintln!(
        "{} road edges, {} track edges ({dropped} deleted ones left out), {} industries, \
         {} depots, {} station pieces; wrote {}",
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
    if !(args.industries || args.depots || args.stations) {
        return Ok(layers);
    }
    let found = Install::find(args.game.as_deref());
    let game = match (&found, &args.game) {
        (Some(game), _) => Some(game),
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
            eprintln!("{e}: no industries, depots or stations");
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
        Parsed::Run(args) => run(args),
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
            Ok(Parsed::Run(a)) => a,
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
        assert!(!a.stations && !a.options.rose);
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
