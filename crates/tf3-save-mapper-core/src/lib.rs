//! Reads a Transport Fever 3 save and draws it as an SVG map.
//!
//! This crate holds the library side: container, header, street edges and the
//! SVG writer. It has no command-line code and never writes a save. The layout
//! it reads is described in the tf3-save-format notes.

pub mod container;
pub mod edges;
pub mod error;
pub mod header;
mod reader;
pub mod svg;

pub use edges::{Edge, Family, read_edges, read_free_ids, retain_live};
pub use error::{Error, Result};
pub use header::{Header, read_header};
pub use svg::{SvgOptions, write_svg};
