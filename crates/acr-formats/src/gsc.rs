//! `GSCfmt` archives (`ALL.GSC`, `Patch01.GS1`, ...).
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! header   14 bytes  "GSCfmt" | u16 version (0x0200) | u8 ? (0xCD) | u8 xor_key (0x78) | u32 count
//! entries  count * 81 bytes
//!            u32  name hash, see [`name_hash`]
//!            [u8; 64] name, NUL padded, e.g. "0\AGEW_1.PAL"
//!            u32  !offset   (bitwise NOT of the offset relative to the data block)
//!            u32  size
//!            u32  reserved (always 0)
//!            u8   encrypted (1 = every byte XORed with xor_key)
//! data     starts right after the entry table
//! ```
//!
//! In the shipped data only `.GP` sprite packs are stored unencrypted.
//!
//! [`GscIndex`] parses the table without doing I/O so callers that can only read
//! asynchronously (the browser) can fetch ranges themselves; [`GscArchive`] is the
//! blocking convenience wrapper.

use std::io::{self, Read, Seek, SeekFrom};

const MAGIC: &[u8; 6] = b"GSCfmt";
pub const HEADER_LEN: usize = 14;
const ENTRY_LEN: usize = 81;
const NAME_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct GscEntry {
    /// Name as stored in the archive (original case, `\` separators).
    pub name: String,
    /// Absolute offset of the file data inside the archive.
    pub offset: u64,
    pub size: u32,
    pub encrypted: bool,
    /// Hash of the upper-case name ([`name_hash`]) the original compares before the name.
    pub name_hash: u32,
}

/// The parsed entry table of an archive.
#[derive(Debug, Clone)]
pub struct GscIndex {
    pub xor_key: u8,
    pub entries: Vec<GscEntry>,
}

impl GscIndex {
    /// Given the first [`HEADER_LEN`] bytes, returns how many bytes from the start of
    /// the archive [`GscIndex::parse`] needs.
    pub fn index_len(header: &[u8]) -> io::Result<usize> {
        let header = header.get(..HEADER_LEN).ok_or_else(|| invalid("archive header truncated"))?;
        if &header[..6] != MAGIC {
            return Err(invalid("not a GSCfmt archive"));
        }
        let count = u32::from_le_bytes(header[10..14].try_into().unwrap()) as usize;
        Ok(HEADER_LEN + count * ENTRY_LEN)
    }

    /// Parses the header and entry table (`bytes` must hold at least
    /// [`GscIndex::index_len`] bytes). `archive_len` is used to reject truncated archives.
    pub fn parse(bytes: &[u8], archive_len: u64) -> io::Result<Self> {
        let len = Self::index_len(bytes)?;
        let table = bytes.get(HEADER_LEN..len).ok_or_else(|| invalid("entry table truncated"))?;
        let data_start = len as u64;
        let entries = table
            .as_chunks::<ENTRY_LEN>()
            .0
            .iter()
            .map(|e| {
                let u32_at = |i: usize| u32::from_le_bytes(e[i..i + 4].try_into().unwrap());
                let raw_name = &e[4..4 + NAME_LEN];
                let name_len = raw_name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
                let entry = GscEntry {
                    name: decode_latin1(&raw_name[..name_len]),
                    offset: data_start + u64::from(!u32_at(68)),
                    size: u32_at(72),
                    encrypted: e[80] != 0,
                    name_hash: u32_at(0),
                };
                if entry.offset + u64::from(entry.size) > archive_len {
                    return Err(invalid(&format!("entry {} points past end of archive", entry.name)));
                }
                Ok(entry)
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self { xor_key: bytes[9], entries })
    }

    /// Decrypts raw entry bytes in place if the entry is encrypted.
    pub fn decrypt(&self, entry: &GscEntry, data: &mut [u8]) {
        if entry.encrypted {
            data.iter_mut().for_each(|b| *b ^= self.xor_key);
        }
    }
}

/// An archive read through a blocking `Read + Seek` source.
#[derive(Debug)]
pub struct GscArchive<R> {
    source: R,
    index: GscIndex,
}

impl<R: Read + Seek> GscArchive<R> {
    pub fn open(mut source: R) -> io::Result<Self> {
        let archive_len = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![0u8; HEADER_LEN];
        source.read_exact(&mut bytes)?;
        bytes.resize(GscIndex::index_len(&bytes)?, 0);
        source.read_exact(&mut bytes[HEADER_LEN..])?;
        let index = GscIndex::parse(&bytes, archive_len)?;
        Ok(Self { source, index })
    }

    pub fn index(&self) -> &GscIndex {
        &self.index
    }

    pub fn entries(&self) -> &[GscEntry] {
        &self.index.entries
    }

    /// Reads and decrypts the entry at `index`.
    pub fn read(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let entry = &self.index.entries[index];
        let mut data = vec![0u8; entry.size as usize];
        self.source.seek(SeekFrom::Start(entry.offset))?;
        self.source.read_exact(&mut data)?;
        self.index.decrypt(entry, &mut data);
        Ok(data)
    }
}

/// The hash the original stores per entry for fast lookups (`0x405098`, called from
/// `CGSCarch::GetFileHandle`): the upper-cased name in a zeroed 64-byte buffer, read as 16
/// big-endian u32 words and summed with wrap-around.
pub fn name_hash(name: &str) -> u32 {
    let mut buf = [0u8; NAME_LEN];
    for (b, c) in buf.iter_mut().zip(name.bytes()) {
        *b = c.to_ascii_uppercase();
    }
    buf.as_chunks::<4>().0.iter().fold(0u32, |sum, word| sum.wrapping_add(u32::from_be_bytes(*word)))
}

impl GscEntry {
    /// Whether the stored hash fits the name; false means a damaged entry table.
    pub fn hash_matches(&self) -> bool {
        self.name_hash == name_hash(&self.name)
    }
}

/// Archive names are ASCII in practice; map any stray high bytes 1:1 to Unicode.
fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Builds a tiny archive with one encrypted and one plain file.
    fn sample() -> Vec<u8> {
        let files: [(&str, &[u8], bool); 2] = [("0\\A.TXT", b"Hi\r\n", true), ("B.GP", b"GPAK", false)];
        let mut out = b"GSCfmt\x00\x02\xCD\x78".to_vec();
        out.extend(2u32.to_le_bytes());
        let mut offset = 0u32;
        for (name, data, enc) in files {
            out.extend(0u32.to_le_bytes());
            let mut n = [0u8; NAME_LEN];
            n[..name.len()].copy_from_slice(name.as_bytes());
            out.extend(n);
            out.extend((!offset).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.push(enc as u8);
            offset += data.len() as u32;
        }
        for (_, data, enc) in files {
            out.extend(data.iter().map(|b| if enc { b ^ 0x78 } else { *b }));
        }
        out
    }

    #[test]
    fn reads_entries_and_decrypts() {
        let mut a = GscArchive::open(Cursor::new(sample())).unwrap();
        assert_eq!(a.entries().len(), 2);
        assert_eq!(a.entries()[0].name, "0\\A.TXT");
        assert_eq!(a.read(0).unwrap(), b"Hi\r\n");
        assert_eq!(a.read(1).unwrap(), b"GPAK");
    }

    #[test]
    fn index_parses_without_io() {
        let data = sample();
        let index = GscIndex::parse(&data[..GscIndex::index_len(&data).unwrap()], data.len() as u64).unwrap();
        let e = &index.entries[0];
        let mut bytes = data[e.offset as usize..][..e.size as usize].to_vec();
        index.decrypt(e, &mut bytes);
        assert_eq!(bytes, b"Hi\r\n");
    }

    #[test]
    fn name_hash_matches_the_original() {
        // Values stored in ALL.GSC for these names.
        assert_eq!(name_hash(r"0\0TO1.XLT"), 0xcbe1_5eac);
        assert_eq!(name_hash(r"0\agew_1.pal"), 0xa403_e1c4);
    }

    #[test]
    fn rejects_truncated_archive() {
        let mut data = sample();
        data.truncate(data.len() - 1);
        assert!(GscArchive::open(Cursor::new(data)).is_err());
    }
}
