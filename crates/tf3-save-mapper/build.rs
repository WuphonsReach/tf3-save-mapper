//! Puts the git commit into `--version`, so a drawing change between two
//! releases changes the version text too. Without git (a source archive) the
//! text stays the plain version.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn main() {
    let build = git(&["rev-parse", "--short=10", "HEAD"]).map(|commit| {
        let dirty =
            git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
        if dirty {
            format!("{commit}-dirty")
        } else {
            commit
        }
    });
    println!("cargo:rustc-env=TF3MAP_BUILD={}", build.unwrap_or_default());
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/index");
    }
}
