//! A byte cursor over the decompressed stream, little-endian throughout.

use crate::error::{Error, Result};

/// Lua tables nest only a few levels in a save; this stops a corrupt stream
/// from recursing without end.
const MAX_LUA_DEPTH: u32 = 64;

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    /// A cursor starting at byte `pos` of `data`.
    pub(crate) fn at(data: &'a [u8], pos: usize) -> Self {
        Reader { data, pos }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or(Error::Truncated { at: self.pos })?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// A `str`: u32 length, then that many bytes.
    pub(crate) fn string(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }

    pub(crate) fn skip_string(&mut self) -> Result<()> {
        let len = self.u32()? as usize;
        self.skip(len)
    }

    /// Step over a `vec<T>` whose items `skip_item` steps over.
    pub(crate) fn skip_vec(
        &mut self,
        mut skip_item: impl FnMut(&mut Self) -> Result<()>,
    ) -> Result<()> {
        for _ in 0..self.u32()? {
            skip_item(self)?;
        }
        Ok(())
    }

    /// Step over a table body: a u32 pair count, then key and value for each.
    pub(crate) fn skip_table(&mut self) -> Result<()> {
        self.skip_table_at(0)
    }

    fn skip_table_at(&mut self, depth: u32) -> Result<()> {
        for _ in 0..self.u32()? {
            self.skip_value_at(depth)?;
            self.skip_value_at(depth)?;
        }
        Ok(())
    }

    /// Step over a Lua value: a u32 tag and its payload (`docs/lua-values.md`).
    fn skip_value_at(&mut self, depth: u32) -> Result<()> {
        let at = self.pos;
        let tag = self.u32()?;
        self.skip_payload_at(tag, at, depth)
    }

    fn skip_payload_at(&mut self, tag: u32, at: usize, depth: u32) -> Result<()> {
        match tag {
            0 => Ok(()),
            1 => self.skip(1),
            2 => self.skip(8),
            3 => self.skip_string(),
            4 if depth < MAX_LUA_DEPTH => match self.u8()? {
                0 => Ok(()),
                _ => self.skip_table_at(depth + 1),
            },
            _ => Err(Error::BadLuaValue { at }),
        }
    }

    pub(crate) fn skip_value(&mut self) -> Result<()> {
        self.skip_value_at(0)
    }

    /// A Lua value read as a number or a string; any other value is stepped
    /// over and gives [`Scalar::Other`].
    pub(crate) fn scalar(&mut self) -> Result<Scalar> {
        let at = self.pos;
        match self.u32()? {
            2 => Ok(Scalar::Number(f64::from_le_bytes(
                self.take(8)?.try_into().unwrap(),
            ))),
            3 => Ok(Scalar::Text(self.string()?)),
            tag => self.skip_payload_at(tag, at, 0).map(|()| Scalar::Other),
        }
    }

    /// The start of a Lua value that should be a table: its pair count (0 for
    /// a nil table), with the cursor on the first key. Any other value is
    /// stepped over and gives `None`.
    pub(crate) fn table_start(&mut self) -> Result<Option<u32>> {
        let at = self.pos;
        match self.u32()? {
            4 => match self.u8()? {
                0 => Ok(Some(0)),
                _ => self.u32().map(Some),
            },
            tag => self.skip_payload_at(tag, at, 0).map(|()| None),
        }
    }
}

/// A Lua key or value as [`Reader::scalar`] reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Scalar {
    Number(f64),
    Text(String),
    Other,
}
