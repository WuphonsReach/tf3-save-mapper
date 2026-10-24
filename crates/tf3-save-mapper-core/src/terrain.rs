//! The terrain: the map's size in tiles, the height scale, the water level
//! and the heightmap (`docs/terrain.md` in tf3-save-format).
//!
//! The terrain record is found by its resolution triple (4 m, 4 m, 0.05 m per
//! height unit). The heightmap is one `vec<u16>` of 65 x 65 points per 256 m
//! tile, tile `ty * tiles_x + tx`, neighbouring tiles sharing their edge
//! points. Grid point (c, r) of the stitched grid lies at
//! x = -width / 2 + 4c, y = -height / 2 + 4r.

use memchr::memmem;

use crate::error::{Error, Result};

/// x and y spacing of the grid (4.0, 4.0) and metres per height unit (0.05), as f32.
const RESOLUTION: [u8; 12] = [
    0x00, 0x00, 0x80, 0x40, 0x00, 0x00, 0x80, 0x40, 0xcd, 0xcc, 0x4c, 0x3d,
];
/// Points of one tile, 65 x 65.
const TILE_POINTS: u32 = 65 * 65;
/// Metres between neighbouring grid points.
pub const GRID_STEP: f64 = 4.0;
/// Metres per tile side.
const TILE_METRES: u32 = 256;

#[derive(Debug, Clone)]
pub struct Terrain {
    pub tiles_x: u32,
    pub tiles_y: u32,
    /// Height of height value 0, metres.
    pub offset_z: f32,
    /// Metres per height unit.
    pub unit: f32,
    /// The water level, metres. Ground below it is under water: lakes, sea and rivers.
    pub water_level: f32,
    /// The stitched grid, row by row: row r (y rising with r) holds
    /// `columns()` points with x rising along it.
    pub heights: Vec<u16>,
}

impl Terrain {
    /// Points along a row, x rising.
    pub fn columns(&self) -> usize {
        64 * self.tiles_x as usize + 1
    }

    /// Rows, y rising.
    pub fn rows(&self) -> usize {
        64 * self.tiles_y as usize + 1
    }

    /// The map's width and height in metres.
    pub fn size(&self) -> (u32, u32) {
        (self.tiles_x * TILE_METRES, self.tiles_y * TILE_METRES)
    }

    /// The height value at column `c`, row `r`.
    pub fn at(&self, c: usize, r: usize) -> u16 {
        self.heights[r * self.columns() + c]
    }

    /// The water level in height units.
    pub fn water_units(&self) -> f64 {
        (f64::from(self.water_level) - f64::from(self.offset_z)) / f64::from(self.unit)
    }
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

fn f32_at(data: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

/// Read the terrain record and the heightmap.
pub fn read_terrain(data: &[u8]) -> Result<Terrain> {
    let (tiles_x, tiles_y, offset_z, unit, water_level) =
        read_record(data).ok_or(Error::NoTerrain)?;
    let heights = read_heightmap(data, tiles_x, tiles_y).ok_or(Error::NoTerrain)?;
    Ok(Terrain {
        tiles_x,
        tiles_y,
        offset_z,
        unit,
        water_level,
        heights,
    })
}

/// (tiles_x, tiles_y, offset_z, unit, water level): the u32 tile counts and a
/// level count before the triple, a u32 and two f32 after it.
fn read_record(data: &[u8]) -> Option<(u32, u32, f32, f32, f32)> {
    memmem::find_iter(data, &RESOLUTION).find_map(|pos| {
        if pos < 12 || pos + 24 > data.len() {
            return None;
        }
        let (tx, ty) = (u32_at(data, pos - 12), u32_at(data, pos - 8));
        let (off_z, water) = (f32_at(data, pos + 16), f32_at(data, pos + 20));
        let plausible = (1..1000).contains(&tx)
            && (1..1000).contains(&ty)
            && (-1000.0..1000.0).contains(&off_z)
            && (-1000.0..1000.0).contains(&water);
        plausible.then(|| (tx, ty, off_z, f32_at(data, pos + 8), water))
    })
}

/// The stitched grid. Found by `0e 00 00 00`, 12 zero bytes, the tile count
/// and 4225 together; the u32 before the `0e` varies between saves, so it is
/// not part of the key. Each tile then follows as a u32 count and its points.
fn read_heightmap(data: &[u8], tiles_x: u32, tiles_y: u32) -> Option<Vec<u16>> {
    let count = tiles_x * tiles_y;
    let mut key = vec![0x0e, 0, 0, 0];
    key.extend_from_slice(&[0; 12]);
    key.extend_from_slice(&count.to_le_bytes());
    key.extend_from_slice(&TILE_POINTS.to_le_bytes());
    let mut p = memmem::find(data, &key)? + 20; // at the first tile's count
    let columns = 64 * tiles_x as usize + 1;
    let rows = 64 * tiles_y as usize + 1;
    let mut grid = vec![0u16; columns * rows];
    for i in 0..count as usize {
        if p + 4 + 2 * TILE_POINTS as usize > data.len() || u32_at(data, p) != TILE_POINTS {
            return None;
        }
        let tile = &data[p + 4..p + 4 + 2 * TILE_POINTS as usize];
        let (ty, tx) = (i / tiles_x as usize, i % tiles_x as usize);
        for r in 0..65 {
            let row = (64 * ty + r) * columns + 64 * tx;
            for c in 0..65 {
                let at = 2 * (r * 65 + c);
                grid[row + c] = u16::from_le_bytes([tile[at], tile[at + 1]]);
            }
        }
        p += 4 + 2 * TILE_POINTS as usize;
    }
    Some(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_record_is_an_error() {
        assert!(matches!(read_terrain(&[0; 64]), Err(Error::NoTerrain)));
    }
}
