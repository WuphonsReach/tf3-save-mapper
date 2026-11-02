//! The exit status and the stderr line a caller (a batch run over many saves)
//! tells the outcomes apart by.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "../../tf3-save-mapper-core/tests/common/mod.rs"]
mod common;

fn tf3map(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tf3map"))
        .args(args)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tf3map-exit-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The layers that read the game install, left out: tests never need one.
const NO_INSTALL: [&str; 3] = ["--no-industries", "--no-depots", "--no-stations"];

/// The core tests' hand-built save, with two roads: enough to draw a map.
fn drawable_save(dir: &Path) -> PathBuf {
    let edges = [
        common::EdgeSpec {
            a: 100,
            b: 101,
            p0: [-1000.0, 500.0, 37.5],
            p1: [-900.0, 500.0, 52.25],
            ..common::EdgeSpec::default()
        },
        common::EdgeSpec {
            a: 101,
            b: 102,
            p0: [-900.0, 500.0, 52.25],
            p1: [-900.0, 400.0, 81.75],
            ..common::EdgeSpec::default()
        },
    ];
    let save = dir.join("drawn.sav");
    std::fs::write(&save, common::sav(&common::stream(&edges))).unwrap();
    save
}

/// The files in `dir` other than `keep`: what a refused run left behind.
fn left_over(dir: &Path, keep: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p != keep)
        .collect()
}

#[test]
fn a_file_that_is_not_a_save_is_refused_with_status_3() {
    let dir = scratch("refused");
    let save = dir.join("junk.sav");
    std::fs::write(&save, b"not a save").unwrap();
    let svg = dir.join("junk.svg");
    let out = tf3map(&[save.to_str().unwrap(), "-o", svg.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr(&out).starts_with("tf3map: cannot read the save: "),
        "{}",
        stderr(&out)
    );
    assert!(!svg.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_empty_file_is_refused_with_status_3() {
    let dir = scratch("empty");
    let save = dir.join("empty.sav");
    std::fs::write(&save, b"").unwrap();
    let svg = dir.join("empty.svg");
    let out = tf3map(&[save.to_str().unwrap(), "-o", svg.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stderr(&out).starts_with("tf3map: cannot read the save: "));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn wrong_arguments_exit_with_status_2() {
    let out = tf3map(&["--no-such-option"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).starts_with("tf3map: unknown option"));
}

#[test]
fn an_output_that_names_a_save_exits_with_status_2() {
    let dir = scratch("output-sav");
    // The save need not exist: the output is refused first.
    let save = dir.join("none.sav");
    let out = tf3map(&[
        save.to_str().unwrap(),
        "-o",
        dir.join("x.sav").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).starts_with("tf3map: the output must not be a .sav file"),
        "{}",
        stderr(&out)
    );
    assert_eq!(left_over(&dir, &save), Vec::<PathBuf>::new());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_map_that_cannot_be_drawn_as_asked_exits_with_status_2() {
    let dir = scratch("drawn");
    let save = drawable_save(&dir);
    let out_path = dir.join("map.svg");
    let run = |extra: &[&str]| {
        let mut args = vec![save.to_str().unwrap(), "-o", out_path.to_str().unwrap()];
        args.extend(NO_INSTALL);
        args.extend(extra);
        tf3map(&args)
    };
    // The save draws as it is.
    let fine = run(&[]);
    assert_eq!(fine.status.code(), Some(0), "{}", stderr(&fine));
    std::fs::remove_file(&out_path).unwrap();
    for (extra, message) in [
        (&["--grid", "1"][..], "tf3map: a grid every 1 m would have "),
        (
            &["--png", "--mpp", "8192"],
            "tf3map: the picture would be 0 x 0 pixels",
        ),
        (
            &["--raster", "--mpp", "8192"],
            "tf3map: the picture would be 0 x 0 pixels",
        ),
    ] {
        let out = run(extra);
        assert_eq!(out.status.code(), Some(2), "{extra:?}: {}", stderr(&out));
        assert!(
            stderr(&out).starts_with(message),
            "{extra:?}: {}",
            stderr(&out)
        );
        // Neither the map nor its temporary file is left.
        assert_eq!(left_over(&dir, &save), Vec::<PathBuf>::new(), "{extra:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_mods_folder_is_checked_only_where_it_is_read() {
    let dir = scratch("mods");
    let save = drawable_save(&dir);
    let svg = dir.join("map.svg");
    let none = dir.join("no-such-folder");
    let run = |extra: &[&str]| {
        let mut args = vec![
            save.to_str().unwrap(),
            "-o",
            svg.to_str().unwrap(),
            "--mods",
            none.to_str().unwrap(),
        ];
        args.extend(extra);
        tf3map(&args)
    };
    let used = run(&[]);
    assert_eq!(used.status.code(), Some(1));
    assert!(stderr(&used).starts_with("tf3map: no mods folder at "));
    let unused = run(&NO_INSTALL);
    assert_eq!(unused.status.code(), Some(0), "{}", stderr(&unused));
    assert!(stderr(&unused).contains("--mods is read for the depots and --coverage: not read"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_save_fails_with_status_1() {
    let dir = scratch("missing");
    let save = dir.join("none.sav");
    let svg = dir.join("none.svg");
    let out = tf3map(&[save.to_str().unwrap(), "-o", svg.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("tf3map: cannot open the save: "));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_folder_given_as_the_save_fails_with_status_1() {
    let dir = scratch("folder");
    let save = dir.join("folder.sav");
    std::fs::create_dir(&save).unwrap();
    let svg = dir.join("folder.svg");
    let out = tf3map(&[save.to_str().unwrap(), "-o", svg.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("tf3map: cannot open the save: "),
        "{}",
        stderr(&out)
    );
    assert!(!svg.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// A save holding only the header's first fields (a 4096 x 2048 m map), as
/// one zstd frame of a raw block: enough for the crop to be checked.
fn header_only_save() -> Vec<u8> {
    let mut stream = b"tf**".to_vec();
    for v in [604u32, 1900, 4096, 2048, 0] {
        stream.extend_from_slice(&v.to_le_bytes());
    }
    let len = u8::try_from(stream.len()).unwrap();
    // Magic, a single-segment frame header with a 1-byte content size, then
    // the last block, raw, of `len` bytes.
    let mut file = vec![0x28, 0xb5, 0x2f, 0xfd, 0x20, len];
    let block = 1 | (u32::from(len) << 3);
    file.extend_from_slice(&block.to_le_bytes()[..3]);
    file.extend_from_slice(&stream);
    file
}

#[test]
fn a_crop_off_the_map_or_too_small_exits_with_status_2() {
    let dir = scratch("crop");
    let save = dir.join("small.sav");
    std::fs::write(&save, header_only_save()).unwrap();
    let svg = dir.join("small.svg");
    let run = |crop: &str| {
        tf3map(&[
            save.to_str().unwrap(),
            "-o",
            svg.to_str().unwrap(),
            "--crop",
            crop,
        ])
    };

    let off = run("5000,0,100");
    assert_eq!(off.status.code(), Some(2));
    assert!(
        stderr(&off).starts_with("tf3map: the crop lies outside the map (4096 x 2048 m"),
        "{}",
        stderr(&off)
    );
    // On the map, but under two 4 m grid steps across.
    let small = run("2,0,1");
    assert_eq!(small.status.code(), Some(2));
    assert!(
        stderr(&small).starts_with("tf3map: the crop shows less than two grid steps"),
        "{}",
        stderr(&small)
    );
    // A crop that fits gets past the check; the save has nothing more to read.
    assert_eq!(run("0,0,100").status.code(), Some(3));
    assert!(!svg.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_output_that_is_the_save_itself_is_refused() {
    let dir = scratch("same");
    // Not named .sav, so only the same-file guard can catch it.
    let save = dir.join("map.svg");
    std::fs::write(&save, header_only_save()).unwrap();
    let out = tf3map(&[save.to_str().unwrap(), "-o", save.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).starts_with("tf3map: the output must not be the save itself"),
        "{}",
        stderr(&out)
    );
    assert_eq!(std::fs::read(&save).unwrap(), header_only_save());
    std::fs::remove_dir_all(dir).unwrap();
}
