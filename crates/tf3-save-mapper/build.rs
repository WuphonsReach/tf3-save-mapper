//! Puts the git commit into `--version`, so a drawing change between two
//! releases changes the version text too. Without git (a source archive) the
//! text stays the plain version, and so does a source tree that lies inside
//! some other git repository, which would otherwise lend it its commit.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What the version depends on besides git, relative to this crate.
const SOURCES: [&str; 7] = [
    "src",
    "Cargo.toml",
    "../tf3-save-mapper-core/src",
    "../tf3-save-mapper-core/icons",
    "../tf3-save-mapper-core/Cargo.toml",
    "../../Cargo.toml",
    "../../Cargo.lock",
];

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Whether git's work tree is this workspace.
fn ours() -> bool {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let top = git(&["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    top.zip(root.canonicalize().ok())
        .is_some_and(|(top, root)| top.canonicalize().is_ok_and(|top| top == root))
}

/// Tells cargo to run this again when `path` changes. Only paths that exist:
/// cargo runs a build script on every build while one it watches is missing.
fn watch(path: &str) {
    if Path::new(path).exists() {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn main() {
    let build = ours()
        .then(|| git(&["rev-parse", "--short=10", "HEAD"]))
        .flatten()
        .map(|commit| {
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
                .is_some_and(|s| !s.is_empty());
            if dirty {
                format!("{commit}-dirty")
            } else {
                commit
            }
        });
    println!(
        "cargo:rustc-env=TF3MAP_BUILD={}",
        build.as_deref().unwrap_or_default()
    );
    if build.is_none() {
        return;
    }
    // A commit moves the branch's ref, not HEAD; it may sit in packed-refs.
    let mut paths = vec![
        "HEAD".to_owned(),
        "index".to_owned(),
        "packed-refs".to_owned(),
    ];
    paths.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for path in paths {
        if let Some(path) = git(&["rev-parse", "--git-path", &path]) {
            watch(&path);
        }
    }
    for path in SOURCES {
        watch(path);
    }
}
