use serde::Serialize;

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn len(self) -> usize {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Borrow this provenance span from its original byte source.
    pub fn get(self, source: &[u8]) -> Option<&[u8]> {
        source.get(self.start..self.end)
    }
}

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub(crate) const fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) const fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }

    pub(crate) fn u8(&mut self, context: &'static str) -> Result<u8> {
        Ok(self.take(1, context)?[0])
    }

    pub(crate) fn u16(&mut self, context: &'static str) -> Result<u16> {
        let bytes = self.take(2, context)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn i16(&mut self, context: &'static str) -> Result<i16> {
        let bytes = self.take(2, context)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn u32(&mut self, context: &'static str) -> Result<u32> {
        let bytes = self.take(4, context)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn span(&mut self, len: usize, context: &'static str) -> Result<ByteSpan> {
        let start = self.offset;
        self.take(len, context)?;
        Ok(ByteSpan::new(start, self.offset))
    }

    fn take(&mut self, len: usize, context: &'static str) -> Result<&'a [u8]> {
        let end = self.offset.checked_add(len).ok_or(Error::IntegerOverflow {
            offset: self.offset,
            context,
        })?;
        if end > self.data.len() {
            return Err(Error::UnexpectedEof {
                offset: self.offset,
                needed: len,
                input_len: self.data.len(),
                context,
            });
        }
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }
}
