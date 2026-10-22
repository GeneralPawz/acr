//! Interface texts looked up by identifier (`GetTextByID`, 0x60a020).
//!
//! The original reads `Comment.txt`, `Missions\miss_*.txt`, every `Text\*.txt` and
//! `moretx.dat` (0x609f40). Each line is `ID text`: the identifier is the first
//! whitespace-delimited word, the text is the rest of the line after one separator
//! character. The first definition of an identifier wins; unknown identifiers are shown
//! as themselves. Texts stay CP1252 bytes because the fonts are indexed by byte.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Texts {
    map: HashMap<Vec<u8>, Vec<u8>>,
}

impl Texts {
    /// Text files in the order the original reads them, given all game paths (normalized).
    pub fn sources(paths: &[String]) -> Vec<String> {
        let mut missions: Vec<&String> = paths
            .iter()
            .filter(|p| p.starts_with("MISSIONS/MISS_") && p.ends_with(".TXT") && p.matches('/').count() == 1)
            .collect();
        let mut texts: Vec<&String> = paths
            .iter()
            .filter(|p| p.starts_with("TEXT/") && p.ends_with(".TXT") && p.matches('/').count() == 1)
            .collect();
        missions.sort();
        texts.sort();
        let mut order = vec!["COMMENT.TXT".to_owned()];
        order.extend(missions.into_iter().cloned());
        order.extend(texts.into_iter().cloned());
        order.push("MORETX.DAT".to_owned());
        order.retain(|p| paths.contains(p));
        order
    }

    /// Adds the definitions of one file; earlier definitions take precedence.
    pub fn add(&mut self, data: &[u8]) {
        let mut rest = data;
        loop {
            // `fscanf("%s")` skips whitespace (including empty lines) before the identifier.
            let start = rest.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(rest.len());
            rest = &rest[start..];
            if rest.is_empty() {
                break;
            }
            let id_len = rest.iter().position(|b| b.is_ascii_whitespace()).unwrap_or(rest.len());
            let id = &rest[..id_len];
            rest = &rest[id_len..];
            // One separator character, then everything up to the end of the line.
            if rest.first().is_some_and(|&b| b != b'\n') {
                rest = &rest[1..];
            }
            let line_len = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
            let mut text = &rest[..line_len];
            rest = &rest[(line_len + 1).min(rest.len())..];
            while let [head @ .., b'\r'] = text {
                text = head;
            }
            self.map.entry(id.to_vec()).or_insert_with(|| text.to_vec());
        }
    }

    /// The text for `id`, or `id` itself if it is not defined.
    pub fn get<'a>(&'a self, id: &'a str) -> &'a [u8] {
        self.map.get(id.as_bytes()).map_or(id.as_bytes(), Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_first_wins() {
        let mut t = Texts::default();
        t.add(b"// comment line\r\n\r\nINTF_MAIN_0 Einzelspieler\r\nINTF_MAIN_B Zur\xfcck\r\nEMPTY\r\nLAST x  y");
        t.add(b"INTF_MAIN_0 Single Player\n");
        assert_eq!(t.get("INTF_MAIN_0"), b"Einzelspieler");
        assert_eq!(t.get("INTF_MAIN_B"), b"Zur\xfcck");
        assert_eq!(t.get("EMPTY"), b"");
        assert_eq!(t.get("LAST"), b"x  y");
        assert_eq!(t.get("UNKNOWN"), b"UNKNOWN");
    }

    #[test]
    fn source_order() {
        let paths: Vec<String> =
            ["TEXT/B.TXT", "TEXT/A.TXT", "MISSIONS/MISS_1.TXT", "MORETX.DAT", "TEXT/SUB/C.TXT", "TEXT/X.NDS"]
                .map(String::from)
                .to_vec();
        assert_eq!(Texts::sources(&paths), ["MISSIONS/MISS_1.TXT", "TEXT/A.TXT", "TEXT/B.TXT", "MORETX.DAT"]);
    }
}
