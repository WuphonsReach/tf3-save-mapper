//! Command-line front end for `tf3-save-mapper-core`: arguments in, SVG file out.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!(
        "tf3map {}: nothing to draw yet, the map code is not written",
        env!("CARGO_PKG_VERSION")
    );
    ExitCode::FAILURE
}
