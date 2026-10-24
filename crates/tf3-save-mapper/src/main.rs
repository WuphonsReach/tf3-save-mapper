//! Command-line front end for `tf3-save-mapper-core`: arguments in, SVG file out.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tf3_save_mapper_core::{
    Edge, Family, Header, SvgOptions, container, read_edges, read_free_ids, read_header,
    retain_live, write_svg,
};

const USAGE: &str = "\
Draw an SVG map of a Transport Fever 3 save. Unofficial; never writes a save.

Usage: tf3map [OPTIONS] SAVE.sav

Options:
  -o, --output FILE   where to write the SVG (default: the save's name with .svg, in the current folder)
      --road-width M  stroke width of a road in metres (default 16)
      --rail-width M  stroke width of a track in metres (default 4)
      --keep-dead     also draw edges whose end node is in the free-id list (roads the game deleted)
  -h, --help          show this help
  -V, --version       show the version";

struct Args {
    save: PathBuf,
    output: PathBuf,
    options: SvgOptions,
    keep_dead: bool,
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
            "--keep-dead" => keep_dead = true,
            flag if flag.starts_with('-') => {
                return Err(format!("unknown option {flag}"));
            }
            _ if save.is_some() => return Err("give one save".into()),
            _ => save = Some(PathBuf::from(arg)),
        }
    }
    let save = save.ok_or("no save given (try --help)")?;
    // From the whole name: `Map v1.5.sav` gives `Map v1.5.svg`, not `Map v1.svg`.
    let output = output
        .unwrap_or_else(|| Path::new(save.file_name().unwrap_or_default()).with_extension("svg"));
    Ok(Parsed::Run(Args {
        save,
        output,
        options,
        keep_dead,
    }))
}

fn width(text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(w) if w.is_finite() && w > 0.0 => Ok(w),
        _ => Err(format!(
            "a width is a number of metres above 0, not {text:?}"
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

    write_output(&args, &header, &edges)?;

    eprintln!(
        "{} road edges, {} track edges ({dropped} deleted ones left out); wrote {}",
        edges.len() - tracks,
        tracks,
        args.output.display()
    );
    Ok(())
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
fn write_output(args: &Args, header: &Header, edges: &[Edge]) -> Result<(), String> {
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
    let written = write_svg(&mut out, header, edges, &args.options)
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
        ]);
        assert_eq!(a.output, Path::new("out.svg"));
        assert_eq!(a.options.road_width, 12.5);
        assert!(a.keep_dead);
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

        let args = Args {
            save: dir.join("absent.sav"),
            output: output.clone(),
            options: SvgOptions::default(),
            keep_dead: false,
        };
        write_output(&args, &test_header(), &[]).unwrap();

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

        let args = Args {
            save: save.clone(),
            output: linked.clone(),
            options: SvgOptions::default(),
            keep_dead: false,
        };
        write_output(&args, &test_header(), &[]).unwrap();

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
