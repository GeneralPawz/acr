//! Mission text files (`columb1.txt`, `MISS7E01.TXT`, ...): the briefing and the pages a
//! mission shows with `ShowPage`.
//!
//! The mission loader (0x55afdd) reads the file named in `Missions.txt` next to the script and
//! splits it into entries:
//!
//! ```text
//! #PAGE0 Missions\Pictures\UMord 0      <- '#', id up to ' ', rest of the line up to CR
//!  AYE KOLUMBUS!                         <- body: everything up to the next '#'
//!  ...
//! ```
//!
//! The id is copied up to the first space (0x55af32 with `' '`), the header rest up to CR
//! (`'\r'`), CR LF is skipped and the body runs to the next `#`. The engine keeps the four
//! parts in parallel arrays (`SCENINF+0x68` id, `+0x6c` header, `+0x60` body, `+0x64` body
//! length). Text is CP1252.

/// One `#ID` entry of a mission text file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEntry {
    /// The identifier without `#`, e.g. `PAGE0`, `BRIEFING`.
    pub id: String,
    /// The rest of the header line, e.g. `Missions\Pictures\UMord 0` (picture and a number).
    pub header: String,
    /// The body (CP1252), with CR LF line ends as in the file.
    pub body: Vec<u8>,
}

impl TextEntry {
    /// The picture named in the header (first word), if any.
    pub fn picture(&self) -> Option<&str> {
        self.header.split_whitespace().next()
    }
}

/// All entries of a mission text file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissionTexts {
    pub entries: Vec<TextEntry>,
}

/// Copies up to (not including) `stop` like 0x55af32; returns the part and the rest after it.
fn until(data: &[u8], stop: u8) -> (&[u8], &[u8]) {
    match data.iter().position(|&b| b == stop) {
        Some(i) => (&data[..i], &data[i..]),
        None => (data, &[]),
    }
}

impl MissionTexts {
    pub fn parse(data: &[u8]) -> Self {
        let mut entries = Vec::new();
        let mut rest = data;
        while let Some(after) = rest.strip_prefix(b"#") {
            let (id, r) = until(after, b' ');
            let r = r.strip_prefix(b" ").unwrap_or(r);
            let (header, r) = until(r, b'\r');
            let r = r.strip_prefix(b"\r\n").or_else(|| r.strip_prefix(b"\r")).unwrap_or(r);
            let (body, r) = until(r, b'#');
            entries.push(TextEntry {
                id: String::from_utf8_lossy(id).trim().to_owned(),
                header: String::from_utf8_lossy(header).trim().to_owned(),
                body: body.to_vec(),
            });
            rest = r;
        }
        Self { entries }
    }

    /// Looks an entry up by id; a leading `#` is ignored and case does not matter.
    pub fn get(&self, id: &str) -> Option<&TextEntry> {
        let id = id.strip_prefix('#').unwrap_or(id);
        self.entries.iter().find(|e| e.id.eq_ignore_ascii_case(id))
    }
}

/// CP1252 bytes as a Rust string (for logs and the UI fallback font).
pub fn cp1252(text: &[u8]) -> String {
    text.iter().map(|&b| char::from_u32(u32::from(acr_script::kernel32::cp1252_to_unicode(b))).unwrap_or('?')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_entries_like_the_loader() {
        let t = MissionTexts::parse(
            b"#BRIEFING faces 0\r\n{A}Title\r\n\r\n#PAGE0 Missions\\Pictures\\UMord 0\r\n AYE!\r\n#PAGE2 Icons 24\r\nx",
        );
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[0].id, "BRIEFING");
        assert_eq!(t.entries[0].header, "faces 0");
        assert_eq!(t.entries[0].body, b"{A}Title\r\n\r\n");
        let p = t.get("#page0").unwrap();
        assert_eq!(p.picture(), Some("Missions\\Pictures\\UMord"));
        assert_eq!(p.body, b" AYE!\r\n");
        assert_eq!(t.get("PAGE2").unwrap().body, b"x");
        assert_eq!(cp1252(b"zur\xfcck"), "zurück");
    }
}
