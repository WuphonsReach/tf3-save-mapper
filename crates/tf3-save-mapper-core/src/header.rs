//! The header at the start of the stream (`docs/header.md` in tf3-save-format).
//!
//! Only what the map needs is kept: the format version, start year, map size
//! and date, and, when the rest of the header reads, the climate and the map
//! seed. Company names, mods and the preview picture are stepped over and not
//! stored, so nothing about the player ends up in the output.

use crate::error::{Error, Result};
use crate::reader::Reader;

const MAGIC: &[u8; 4] = b"tf**";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Format version, for example 604.
    pub version: u32,
    /// Year the game was started.
    pub start_year: u32,
    /// Map size in metres.
    pub map_width: u32,
    pub map_height: u32,
    /// The date shown in the game, as a Julian day number.
    pub date: u32,
    /// The `climate` resource path, such as `::/climate/temperate.clima`. `None`
    /// when the tail of the header does not read (older formats differ after
    /// the preview) or the resource is missing.
    pub climate: Option<String>,
    /// The text of the Seed box in the New Game dialog. `None` with the tail.
    pub map_seed: Option<String>,
}

/// Read the header of a decompressed stream. The first fields are enough for
/// the map's size; if the later ones do not read, the result still carries
/// those and leaves `climate` and `map_seed` empty.
pub fn read_header(stream: &[u8]) -> Result<Header> {
    let mut r = Reader::new(stream);
    if r.take(4)? != MAGIC {
        return Err(Error::NotASave);
    }
    let version = r.u32()?;
    let start_year = r.u32()?;
    let map_width = r.u32()?;
    let map_height = r.u32()?;
    let date = r.u32()?;
    let mut header = Header {
        version,
        start_year,
        map_width,
        map_height,
        date,
        climate: None,
        map_seed: None,
    };
    if let Ok((climate, seed)) = read_rest(&mut r) {
        header.climate = climate;
        header.map_seed = Some(seed);
    }
    Ok(header)
}

/// From the money to the seed. Returns the climate resource and the seed.
fn read_rest(r: &mut Reader) -> Result<(Option<String>, String)> {
    r.skip(8 + 4)?; // money i64, counter u32
    r.skip_table()?; // info
    r.skip_vec(|r| {
        // A mod: id, source, path, name, extra, then flags.
        for _ in 0..5 {
            r.skip_string()?;
        }
        r.skip(4)
    })?;
    r.skip(1)?; // flag before the preview
    r.skip(8)?; // preview width and height
    r.skip_string()?; // the preview picture, held as a str
    r.skip(6 * 8)?; // stats: 6 x i64
    r.skip(11 * 4)?; // 11 x i32
    r.skip_vec(|r| r.skip(8))?; // (u32, u32) pairs
    r.skip(2 * 4)?; // 2 x u32
    r.skip(9 * 8)?; // 9 x i64
    r.skip(2 * 4)?; // 2 x u32
    r.skip_vec(|r| {
        r.skip_string()?;
        r.skip(4)
    })?; // labels
    r.skip(4 + 8 + 8 + 4)?; // u32, i64, i64, u32
    r.skip_vec(|r| r.skip_string())?; // config mods
    let mut climate = None;
    for _ in 0..r.u32()? {
        let key = r.string()?;
        let value = r.string()?;
        if key == "climate" {
            climate = Some(value);
        }
    }
    r.skip_vec(|r| {
        r.skip_string()?;
        r.skip_table()
    })?; // config params
    r.skip_string()?; // mission
    r.skip_string()?; // kind
    r.skip(1 + 4)?; // flag, first version
    let seed = r.string()?;
    Ok((climate, seed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(version: u32) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        for v in [version, 1950, 20480, 40960, 2_433_000] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn size_survives_an_unreadable_tail() {
        let mut stream = fixed(568);
        stream.extend_from_slice(&[0xff; 10]); // not a header tail
        let h = read_header(&stream).unwrap();
        assert_eq!((h.version, h.map_width, h.map_height), (568, 20480, 40960));
        assert_eq!(h.start_year, 1950);
        assert_eq!(h.map_seed, None);
        assert_eq!(h.climate, None);
    }

    #[test]
    fn wrong_magic_is_not_a_save() {
        assert!(matches!(
            read_header(b"RIFF....................."),
            Err(Error::NotASave)
        ));
    }

    #[test]
    fn short_stream_is_truncated() {
        assert!(matches!(
            read_header(&fixed(604)[..10]),
            Err(Error::Truncated { .. })
        ));
    }
}
