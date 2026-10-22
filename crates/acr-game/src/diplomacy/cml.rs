//! The CML markup of intExplorer pages, as far as the diplomacy pages use it:
//! `#tag[%NAME](arg, arg, ...)`. Arguments are split at top-level commas; `[...]`, `{...}`,
//! `(...)` and `"..."` nest. Text stays CP1252 bytes.

/// One tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// `txt`, `ctxt`, `font`, ... (lower case, without `#`).
    pub name: String,
    /// The element name after `[%`, if any.
    pub id: Option<Vec<u8>>,
    pub args: Vec<Vec<u8>>,
}

/// The tags of a page. Like intExplorer's parser (0x1000a9c0) it scans for `#` anywhere, so
/// a tag may follow another on the same line (DipServer's masks end without a newline before
/// the first offer); `//` comments run to the end of the line, other text is skipped.
pub fn parse(page: &[u8]) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < page.len() {
        match page[i] {
            b'#' => {
                let (tag, next) = tag(page, i + 1);
                out.extend(tag);
                i = next.max(i + 1);
            }
            b'/' if page.get(i + 1) == Some(&b'/') => {
                while i < page.len() && !matches!(page[i], b'\n' | b'\r') {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// A tag starting after its `#` at `start`; returns it and the position after it.
fn tag(page: &[u8], start: usize) -> (Option<Tag>, usize) {
    let mut i = start;
    while i < page.len() && !matches!(page[i], b'(' | b'[' | b'\r' | b'\n') {
        i += 1;
    }
    let name = String::from_utf8_lossy(&page[start..i]).trim().to_ascii_lowercase();
    let mut id = None;
    if page.get(i) == Some(&b'[') {
        let Some(close) = page[i..].iter().position(|&b| b == b']') else { return (None, i) };
        let inner = &page[i + 1..i + close];
        id = Some(inner.strip_prefix(b"%").unwrap_or(inner).to_vec());
        i += close + 1;
    }
    let mut args = Vec::new();
    if page.get(i) == Some(&b'(') {
        let open = i + 1;
        let mut depth = 0i32;
        let mut quoted = false;
        i = open;
        while i < page.len() {
            match page[i] {
                b'"' => quoted = !quoted,
                b'[' | b'{' | b'(' if !quoted => depth += 1,
                b')' if !quoted && depth == 0 => break,
                b']' | b'}' | b')' if !quoted => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        args = split_args(&page[open..i.min(page.len())]);
        i += 1;
    }
    (Some(Tag { name, id, args }), i)
}

fn trim(mut l: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = l
        && first.is_ascii_whitespace()
    {
        l = rest;
    }
    while let [rest @ .., last] = l
        && last.is_ascii_whitespace()
    {
        l = rest;
    }
    l
}

/// Splits at commas outside brackets and quotes.
pub fn split_args(s: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut start = 0;
    for (i, &b) in s.iter().enumerate() {
        match b {
            b'"' => quoted = !quoted,
            b'[' | b'{' | b'(' if !quoted => depth += 1,
            b']' | b'}' | b')' if !quoted => depth -= 1,
            b',' if !quoted && depth <= 0 => {
                out.push(trim(&s[start..i]).to_vec());
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() || !out.is_empty() {
        out.push(trim(&s[start..]).to_vec());
    }
    out
}

/// A quoted argument without its quotes. intExplorer copies it raw (`\\` stays two bytes;
/// Windows collapses them in paths), so this only strips the quotes.
pub fn unquote(a: &[u8]) -> Vec<u8> {
    a.strip_prefix(b"\"").and_then(|a| a.strip_suffix(b"\"")).unwrap_or(a).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        let page = b"#font(YF,YF,WF)\r\n<FRM1>\n#txt[%TX2](%WH[x:20,y:%TX1,w:100%-40,h:100%],{DS|perform&0&1&0},\"{* 1000 Gold - Saboteure}\")\n#cpix[%PX](%EBX[x:0,y:0,w:100%,h:30%],{},\"Dip\\\\lic.gp\",0,0,0,0)";
        let tags = parse(page);
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].name, "font");
        assert_eq!(tags[0].args, vec![b"YF".to_vec(), b"YF".to_vec(), b"WF".to_vec()]);
        assert_eq!(tags[1].id.as_deref(), Some(&b"TX2"[..]));
        assert_eq!(tags[1].args.len(), 3);
        assert_eq!(tags[1].args[0], b"%WH[x:20,y:%TX1,w:100%-40,h:100%]");
        assert_eq!(tags[1].args[1], b"{DS|perform&0&1&0}");
        assert_eq!(unquote(&tags[2].args[2]), b"Dip\\\\lic.gp");
        assert_eq!(tags[2].args.len(), 7);
        let joined = parse(b"#font(Fn16,Fn16y,Fn16)#txt[%TX1](%WH[x:20,y:%TX0],{DS|a},\"{* b}\")\n// #x()\n");
        assert_eq!(joined.len(), 2);
        assert_eq!(joined[1].id.as_deref(), Some(&b"TX1"[..]));
        let unquoted = parse(b"#ctxt[%TX0](%EBX[x:10,y:%PX+8],{},Liste der St\xe4mme:)\n");
        assert_eq!(unquoted[0].args[2], b"Liste der St\xe4mme:");
    }
}
