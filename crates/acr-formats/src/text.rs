//! Single-byte code pages used by the game's text files.
//!
//! The data was written by a Ukrainian studio and localised for Western Europe, so three
//! code pages occur (see `docs/formats/definitions.md`):
//!
//! - [`CodePage::Cp1252`]: localised texts (German in this installation), e.g.
//!   `DESCRIPTION\*.MD`, mission texts.
//! - [`CodePage::Cp866`] (DOS Cyrillic): Russian identifiers that survived in the data,
//!   e.g. the `MESSAGE` lines of unit definitions (`Копейщик`).
//! - [`CodePage::Cp1251`] (Windows Cyrillic): Russian developer comments, e.g. after
//!   `//` in unit definitions (`стоять`).
//!
//! The lower half is ASCII in all three. Bytes a code page leaves undefined map to the C1
//! control with the same value, as Windows does, so decoding never fails and is lossless.

/// A single-byte character set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodePage {
    Cp1252,
    Cp1251,
    Cp866,
}

impl CodePage {
    /// Decodes one byte.
    pub fn decode_byte(self, b: u8) -> char {
        if b < 0x80 {
            return b as char;
        }
        let i = (b - 0x80) as usize;
        match self {
            CodePage::Cp1252 if b < 0xA0 => CP1252_80[i],
            CodePage::Cp1252 => b as char,
            CodePage::Cp1251 => CP1251_HIGH[i],
            CodePage::Cp866 => CP866_HIGH[i],
        }
    }

    /// Decodes a byte string.
    pub fn decode(self, bytes: &[u8]) -> String {
        bytes.iter().map(|&b| self.decode_byte(b)).collect()
    }

    /// Encodes a string, or `None` if a character is not representable.
    pub fn encode(self, text: &str) -> Option<Vec<u8>> {
        text.chars()
            .map(|c| if c.is_ascii() { Some(c as u8) } else { (0x80..=0xFF).find(|&b| self.decode_byte(b) == c) })
            .collect()
    }
}

/// Guesses which Cyrillic code page a byte string uses, by letter frequency.
///
/// The frequent lowercase letters а–п are `0xA0..=0xAF` in CP866 but `0xE0..=0xEF` in
/// CP1251, and р–я are `0xE0..=0xEF` in CP866 but `0xF0..=0xFF` in CP1251. Returns `None`
/// for text without high bytes.
pub fn guess_cyrillic(bytes: &[u8]) -> Option<CodePage> {
    let (mut cp866, mut cp1251) = (0usize, 0usize);
    for &b in bytes {
        match b {
            0x80..=0xAF => cp866 += 1,
            0xC0..=0xDF | 0xF0..=0xFF => cp1251 += 1,
            _ => {}
        }
    }
    match (cp866, cp1251) {
        (0, 0) => None,
        (a, b) if a >= b => Some(CodePage::Cp866),
        _ => Some(CodePage::Cp1251),
    }
}

const CP1252_80: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', //
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

const CP1251_HIGH: [char; 128] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', //
    'ђ', '‘', '’', '“', '”', '•', '–', '—', '\u{98}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', //
    '\u{A0}', 'Ў', 'ў', 'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{AD}', '®', 'Ї', //
    '°', '±', 'І', 'і', 'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї', //
    'А', 'Б', 'В', 'Г', 'Д', 'Е', 'Ж', 'З', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П', //
    'Р', 'С', 'Т', 'У', 'Ф', 'Х', 'Ц', 'Ч', 'Ш', 'Щ', 'Ъ', 'Ы', 'Ь', 'Э', 'Ю', 'Я', //
    'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и', 'й', 'к', 'л', 'м', 'н', 'о', 'п', //
    'р', 'с', 'т', 'у', 'ф', 'х', 'ц', 'ч', 'ш', 'щ', 'ъ', 'ы', 'ь', 'э', 'ю', 'я',
];

const CP866_HIGH: [char; 128] = [
    'А', 'Б', 'В', 'Г', 'Д', 'Е', 'Ж', 'З', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П', //
    'Р', 'С', 'Т', 'У', 'Ф', 'Х', 'Ц', 'Ч', 'Ш', 'Щ', 'Ъ', 'Ы', 'Ь', 'Э', 'Ю', 'Я', //
    'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и', 'й', 'к', 'л', 'м', 'н', 'о', 'п', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'р', 'с', 'т', 'у', 'ф', 'х', 'ц', 'ч', 'ш', 'щ', 'ъ', 'ы', 'ь', 'э', 'ю', 'я', //
    'Ё', 'ё', 'Є', 'є', 'Ї', 'ї', 'Ў', 'ў', '°', '∙', '·', '√', '№', '¤', '■', '\u{A0}',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_each_code_page() {
        // "Копейщик" as stored in ANMBIZ.MD.
        let cp866 = [0x8A, 0xAE, 0xAF, 0xA5, 0xA9, 0xE9, 0xA8, 0xAA];
        assert_eq!(CodePage::Cp866.decode(&cp866), "Копейщик");
        // "стоять" from a comment in DELKRI.MD.
        assert_eq!(CodePage::Cp1251.decode(&[0xF1, 0xF2, 0xEE, 0xFF, 0xF2, 0xFC]), "стоять");
        assert_eq!(CodePage::Cp1252.decode(b"g\xFCnstig \x80"), "günstig €");
        assert_eq!(CodePage::Cp1252.decode(&[0x81]), "\u{81}");
    }

    #[test]
    fn encode_round_trips() {
        for cp in [CodePage::Cp1252, CodePage::Cp1251, CodePage::Cp866] {
            let all: Vec<u8> = (0..=255).collect();
            assert_eq!(cp.encode(&cp.decode(&all)).as_deref(), Some(&all[..]), "{cp:?}");
        }
        assert_eq!(CodePage::Cp1252.encode("Ж"), None);
    }

    #[test]
    fn guesses_cyrillic() {
        assert_eq!(guess_cyrillic(&[0x8A, 0xAE, 0xAF, 0xA5, 0xA9, 0xE9, 0xA8, 0xAA]), Some(CodePage::Cp866));
        assert_eq!(guess_cyrillic(&[0xF1, 0xF2, 0xEE, 0xFF, 0xF2, 0xFC]), Some(CodePage::Cp1251));
        assert_eq!(guess_cyrillic(b"ascii"), None);
    }
}
