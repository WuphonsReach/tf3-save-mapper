//! The `.sav` container: zstd frames holding one data stream.
//!
//! The game writes the whole stream as one frame and then an empty frame
//! (`docs/container.md` in tf3-save-format). Decoding across frames gives the
//! stream; nothing in it records where it ends.

use std::io::Read;

use crate::error::{Error, Result};

/// Largest decompressed stream accepted. The biggest saves seen are about a
/// gigabyte.
pub const MAX_STREAM_BYTES: u64 = 4 << 30;

/// Decompress a save from `source`. The frame header's content size, when
/// present, sizes the buffer.
pub fn decompress(mut source: impl Read) -> Result<Vec<u8>> {
    // Look at the frame header first so a large stream is allocated once.
    let mut head = [0u8; 32];
    let got = read_up_to(&mut source, &mut head)?;
    let hint = zstd::zstd_safe::get_frame_content_size(&head[..got])
        .ok()
        .flatten()
        .filter(|&n| n <= MAX_STREAM_BYTES);

    let chained = (&head[..got]).chain(source);
    let mut decoder = zstd::stream::read::Decoder::new(chained)?;
    let mut out = Vec::with_capacity(hint.unwrap_or(0) as usize);
    // One byte more than the limit, so an oversize stream is told apart from one that fits exactly.
    decoder
        .by_ref()
        .take(MAX_STREAM_BYTES + 1)
        .read_to_end(&mut out)?;
    if out.len() as u64 > MAX_STREAM_BYTES {
        return Err(Error::TooLarge {
            limit: MAX_STREAM_BYTES,
        });
    }
    Ok(out)
}

fn read_up_to(source: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match source.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's trailing empty frame.
    const EMPTY_FRAME: [u8; 9] = [0x28, 0xb5, 0x2f, 0xfd, 0x20, 0x00, 0x01, 0x00, 0x00];

    #[test]
    fn reads_across_the_trailing_empty_frame() {
        let stream = b"tf**plus some payload".repeat(100);
        let mut file = zstd::encode_all(&stream[..], 3).unwrap();
        file.extend_from_slice(&EMPTY_FRAME);
        assert_eq!(decompress(&file[..]).unwrap(), stream);
    }

    #[test]
    fn rejects_bytes_that_are_not_zstd() {
        assert!(matches!(
            decompress(&b"not a save at all, not a save at all"[..]),
            Err(Error::Io(_))
        ));
    }
}
