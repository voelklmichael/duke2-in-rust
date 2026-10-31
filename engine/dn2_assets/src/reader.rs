//! Little-endian reading with errors that name the file.

use alloc::string::ToString;

use crate::AssetError;

pub struct Reader<'a> {
    file: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(file: &'a str, bytes: &'a [u8]) -> Self {
        Self {
            file,
            bytes,
            pos: 0,
        }
    }

    pub fn error(&self, reason: &'static str) -> AssetError {
        AssetError::Malformed {
            file: self.file.to_string(),
            reason,
        }
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), AssetError> {
        if pos > self.bytes.len() {
            return Err(self.error("offset past the end"));
        }
        self.pos = pos;
        Ok(())
    }

    pub fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8], AssetError> {
        let bytes = self
            .bytes
            .get(self.pos..self.pos.saturating_add(count))
            .ok_or_else(|| self.error("file too short"))?;
        self.pos += count;
        Ok(bytes)
    }

    pub fn u8(&mut self) -> Result<u8, AssetError> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, AssetError> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn i16(&mut self) -> Result<i16, AssetError> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32, AssetError> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
}
