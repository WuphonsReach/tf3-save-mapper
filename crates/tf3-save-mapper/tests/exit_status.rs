//! The exit status and the stderr line a caller (a batch run over many saves)
//! tells the outcomes apart by.

use std::path::PathBuf;
use std::process::{Command, Output};

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
fn wrong_arguments_exit_with_status_2() {
    let out = tf3map(&["--no-such-option"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).starts_with("tf3map: unknown option"));
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
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("tf3map: the output must not be the save itself"),
        "{}",
        stderr(&out)
    );
    assert_eq!(std::fs::read(&save).unwrap(), header_only_save());
    std::fs::remove_dir_all(dir).unwrap();
}
