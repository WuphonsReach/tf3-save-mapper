//! Reads a Transport Fever 3 save and draws it as an SVG map.
//!
//! This crate holds the library side: container, header, street edges,
//! terrain, model instances, the facts read from the game install at run
//! time, and the SVG writer. It has no command-line code and never writes a save. The layout
//! it reads is described in the tf3-save-format notes.

pub mod container;
pub mod contour;
pub mod edges;
pub mod error;
pub mod geometry;
pub mod header;
pub mod install;
pub mod instances;
mod reader;
pub mod svg;
pub mod terrain;
mod zip;

pub use edges::{Edge, Family, read_edges, read_free_ids, retain_live};
pub use error::{Error, Result};
pub use header::{Header, read_header};
pub use install::Install;
pub use instances::{Instance, ModelTable, industry_models, read_instances, read_model_table};
pub use svg::{Layers, SvgOptions, write_svg};
pub use terrain::{Terrain, read_terrain};
