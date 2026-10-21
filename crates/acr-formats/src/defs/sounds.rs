//! The sound list (`SOUNDLIST.TXT`): named sound groups referenced by `.MD` files.
//!
//! ```text
//! TREE1 4                  name, number of files
//! FORGROUP 2 75 150        optional modifiers: DANGER, INGROUP n, FORGROUP a b c
//! Sound\ResDer1.wav        one path per file, relative to the game directory
//! ...
//! ```
//!
//! The engine picks one of the files at random; repeated paths raise their weight.

use super::{Issue, Tokenizer};
use crate::text::CodePage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub name: String,
    pub files: Vec<String>,
    /// `DANGER`, `INGROUP n` and `FORGROUP a b c` modifiers as written (meaning open).
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SoundList {
    pub sounds: Vec<Sound>,
    pub issues: Vec<Issue>,
}

impl SoundList {
    pub fn parse(data: &[u8]) -> Self {
        let mut list = SoundList::default();
        let mut tk = Tokenizer::new(data);
        while let Some(name) = tk.next() {
            let name_text = name.text(CodePage::Cp1252);
            let Some(count) = tk.peek().and_then(|t| t.int()).filter(|n| (0..=10_000).contains(n)) else {
                list.issues.push(Issue { line: name.line, message: format!("{name_text}: expected file count") });
                continue;
            };
            tk.next();
            let mut sound = Sound { name: name_text, files: Vec::new(), modifiers: Vec::new() };
            while let Some(t) = tk.peek() {
                let modifier = match t.bytes {
                    b"DANGER" => 1,
                    b"INGROUP" => 2,
                    b"FORGROUP" => 4,
                    _ => break,
                };
                let words: Vec<String> =
                    (0..modifier).filter_map(|_| tk.next()).map(|t| t.text(CodePage::Cp1252)).collect();
                sound.modifiers.push(words.join(" "));
            }
            for _ in 0..count {
                match tk.next() {
                    Some(t) => sound.files.push(t.text(CodePage::Cp1252)),
                    None => {
                        let message = format!("{}: file list truncated", sound.name);
                        list.issues.push(Issue { line: tk.line(), message });
                        break;
                    }
                }
            }
            list.sounds.push(sound);
        }
        list
    }

    pub fn get(&self, name: &str) -> Option<&Sound> {
        self.sounds.iter().rev().find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groups_and_modifiers() {
        let list = SoundList::parse(b"TREE1 2\r\nSound\\A.wav\r\nSound\\B.wav\r\n\r\nFIRE 1\r\nDANGER\r\nINGROUP 4\r\nFORGROUP 2 75 150\r\nSound\\C.wav \r\n");
        assert!(list.issues.is_empty());
        assert_eq!(list.sounds.len(), 2);
        assert_eq!(list.get("tree1").unwrap().files, ["Sound\\A.wav", "Sound\\B.wav"]);
        assert_eq!(list.sounds[1].modifiers, ["DANGER", "INGROUP 4", "FORGROUP 2 75 150"]);
    }

    #[test]
    fn reports_truncation() {
        let list = SoundList::parse(b"X 3\nA.wav\nY\n");
        assert_eq!(list.issues.len(), 1);
        assert_eq!(list.sounds[0].files, ["A.wav", "Y"]);
    }
}
