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
