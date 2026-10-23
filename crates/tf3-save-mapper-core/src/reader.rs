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
        match self.u32()? {
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
}
