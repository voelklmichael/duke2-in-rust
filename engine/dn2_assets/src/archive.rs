//! NUKEM2.CMP: a directory of 200 entries of 20 bytes (name, 12 bytes
//! NUL-padded; u32 offset; u32 size), followed by the files. A name starting
//! with NUL ends the directory.

use alloc::string::String;
use alloc::vec::Vec;

use crate::AssetError;

const ENTRY_COUNT: usize = 200;
const ENTRY_SIZE: usize = 20;
const NAME_SIZE: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Upper case, as stored.
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

pub struct Archive {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
}

impl Archive {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, AssetError> {
        let bad = |reason| AssetError::BadArchive(crate::ARCHIVE_NAME, reason);
        let directory = bytes
            .get(..ENTRY_COUNT * ENTRY_SIZE)
            .ok_or(bad("shorter than its directory"))?;
        let mut entries = Vec::new();
        for raw in directory.as_chunks::<ENTRY_SIZE>().0 {
            let name = &raw[..NAME_SIZE];
            if name[0] == 0 {
                break;
            }
            let end = name.iter().position(|&b| b == 0).unwrap_or(NAME_SIZE);
            let name = core::str::from_utf8(&name[..end]).map_err(|_| bad("name is not ASCII"))?;
            let word = |at: usize| u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()) as usize;
            let (offset, size) = (word(NAME_SIZE), word(NAME_SIZE + 4));
            if offset.checked_add(size).is_none_or(|end| end > bytes.len()) {
                return Err(bad("an entry reaches past the end"));
            }
            entries.push(Entry {
                name: name.to_ascii_uppercase(),
                offset,
                size,
            });
        }
        Ok(Self { bytes, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The contents of a file; `name` is matched without regard to case.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .map(|e| &self.bytes[e.offset..e.offset + e.size])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn archive_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = vec![0; ENTRY_COUNT * ENTRY_SIZE];
        for (i, (name, data)) in files.iter().enumerate() {
            let entry = &mut bytes[i * ENTRY_SIZE..][..ENTRY_SIZE];
            entry[..name.len()].copy_from_slice(name.as_bytes());
            let offset = (ENTRY_COUNT * ENTRY_SIZE
                + files[..i].iter().map(|f| f.1.len()).sum::<usize>())
                as u32;
            entry[12..16].copy_from_slice(&offset.to_le_bytes());
            entry[16..20].copy_from_slice(&(data.len() as u32).to_le_bytes());
        }
        for (_, data) in files {
            bytes.extend_from_slice(data);
        }
        bytes
    }

    #[test]
    fn finds_files_ignoring_case() {
        let archive =
            Archive::parse(archive_with(&[("A.MNI", b"one"), ("B.MNI", b"two")])).unwrap();
        assert_eq!(archive.entries().len(), 2);
        assert_eq!(archive.get("b.mni"), Some(&b"two"[..]));
        assert_eq!(archive.get("C.MNI"), None);
    }

    #[test]
    fn rejects_entries_past_the_end() {
        let mut bytes = archive_with(&[("A.MNI", b"one")]);
        bytes.pop();
        assert!(Archive::parse(bytes).is_err());
    }
}
