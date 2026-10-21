//! Formation shapes (`ORDERS.LST`) and formation groups (`ORD_GROUPS.LST`).
//!
//! ```text
//! #LINE15 SYM2 19 2 9 3 1 2      shape header: name, symmetry, parameters
//!    C   C   C                   grid rows: `*` soldier slot, `C` commander slot
//!                                (officer, drummer, flag bearer), space = empty
//! ***************
//! #END
//! ...
//! #EXIT
//! ```
//!
//! `ORD_GROUPS.LST` starts with the group count; each line is
//! `NAME n i*n m #SHAPE*m`, e.g. `LINE 3 0 1 2 5 #LINE15 #LINE36 #LINE72 #LINE120 #LINE196`
//! (the size variants of one formation, as named by `[OFFICERS]` in the nation files).

use super::{Issue, Tokenizer};

/// One formation shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderShape {
    /// Name with `#` (`#LINE15`).
    pub name: String,
    /// `SYMn`.
    pub symmetry: u8,
    /// The integers after `SYMn` (4 or 6 values).
    pub params: Vec<i32>,
    /// Grid rows as written, trailing whitespace removed.
    pub rows: Vec<Vec<u8>>,
}

impl OrderShape {
    /// `(column, row)` of every cell with this character, row by row.
    pub fn cells(&self, ch: u8) -> Vec<(i32, i32)> {
        let mut v = Vec::new();
        for (y, row) in self.rows.iter().enumerate() {
            for (x, &c) in row.iter().enumerate() {
                if c == ch {
                    v.push((x as i32, y as i32));
                }
            }
        }
        v
    }

    /// Soldier slots (`*`).
    pub fn soldiers(&self) -> Vec<(i32, i32)> {
        self.cells(b'*')
    }

    /// Commander slots (`C`).
    pub fn commanders(&self) -> Vec<(i32, i32)> {
        self.cells(b'C')
    }
}

#[derive(Debug, Clone, Default)]
pub struct OrderList {
    pub shapes: Vec<OrderShape>,
    pub issues: Vec<Issue>,
}

impl OrderList {
    pub fn parse(data: &[u8]) -> Self {
        let mut list = OrderList::default();
        let mut current: Option<OrderShape> = None;
        for (i, raw) in data.split(|&b| b == b'\n').enumerate() {
            let line = (i + 1) as u32;
            let text = trim_end(raw);
            if let Some(shape) = current.as_mut() {
                if text.trim_ascii_start().starts_with(b"#END") {
                    // Drop trailing blank rows; leading blank rows shift positions, keep them.
                    while shape.rows.last().is_some_and(|r| r.is_empty()) {
                        shape.rows.pop();
                    }
                    list.shapes.extend(current.take());
                } else {
                    shape.rows.push(text.to_vec());
                }
                continue;
            }
            let mut t = Tokenizer::new(text);
            let Some(first) = t.next() else { continue };
            if first.is("#EXIT") {
                break;
            }
            if !first.bytes.starts_with(b"#") {
                list.issues.push(Issue { line, message: "expected a shape header".into() });
                continue;
            }
            let name = String::from_utf8_lossy(first.bytes).into_owned();
            let symmetry = t
                .next()
                .and_then(|s| s.bytes.strip_prefix(b"SYM").and_then(|d| std::str::from_utf8(d).ok()?.parse().ok()));
            let Some(symmetry) = symmetry else {
                list.issues.push(Issue { line, message: format!("{name}: expected SYMn") });
                continue;
            };
            let mut params = Vec::new();
            while let Some(tok) = t.next() {
                match tok.int() {
                    Some(v) => params.push(v),
                    None => list.issues.push(Issue { line, message: format!("{name}: bad parameter") }),
                }
            }
            current = Some(OrderShape { name, symmetry, params, rows: Vec::new() });
        }
        if let Some(s) = current {
            list.issues.push(Issue { line: 0, message: format!("{}: missing #END", s.name) });
        }
        list
    }

    pub fn shape(&self, name: &str) -> Option<&OrderShape> {
        let name = name.strip_prefix('#').unwrap_or(name);
        self.shapes.iter().find(|s| s.name[1..].eq_ignore_ascii_case(name))
    }
}

/// One line of `ORD_GROUPS.LST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderGroup {
    pub name: String,
    pub values: Vec<i32>,
    /// Shape names with `#`, smallest first.
    pub shapes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OrderGroups {
    pub groups: Vec<OrderGroup>,
    pub issues: Vec<Issue>,
}

impl OrderGroups {
    pub fn parse(data: &[u8]) -> Self {
        let mut out = OrderGroups::default();
        let mut t = Tokenizer::new(data);
        let Some(count) = t.next().and_then(|c| c.int()) else {
            out.issues.push(Issue { line: 1, message: "expected the group count".into() });
            return out;
        };
        for _ in 0..count.max(0) {
            let line = t.line();
            let group = (|| {
                let name = String::from_utf8_lossy(t.next()?.bytes).into_owned();
                let n = t.next()?.int()?;
                let values = (0..n).map(|_| t.next()?.int()).collect::<Option<Vec<_>>>()?;
                let m = t.next()?.int()?;
                let shapes = (0..m)
                    .map(|_| Some(String::from_utf8_lossy(t.next()?.bytes).into_owned()))
                    .collect::<Option<_>>()?;
                Some(OrderGroup { name, values, shapes })
            })();
            match group {
                Some(g) => out.groups.push(g),
                None => {
                    out.issues.push(Issue { line, message: "truncated group".into() });
                    break;
                }
            }
        }
        out
    }
}

fn trim_end(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|&b| b > b' ').map_or(0, |p| p + 1);
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDERS: &[u8] = b"//Orders description file\r\n#LINE3 SYM2 0 0 0 0\r\n*  *  *\r\n#END\r\n\
        #KARE15 SYM0 9 2 9 3\r\n*****\r\n* C *\r\n** C*\r\n*****\r\n\r\n#END\r\n#EXIT\r\n";

    #[test]
    fn parses_shapes() {
        let l = OrderList::parse(ORDERS);
        assert!(l.issues.is_empty(), "{:?}", l.issues);
        assert_eq!(l.shapes.len(), 2);
        let line = l.shape("LINE3").unwrap();
        assert_eq!((line.symmetry, line.params.as_slice()), (2, &[0, 0, 0, 0][..]));
        assert_eq!(line.soldiers(), [(0, 0), (3, 0), (6, 0)]);
        let kare = l.shape("#kare15").unwrap();
        assert_eq!(kare.rows.len(), 4);
        assert_eq!(kare.soldiers().len(), 15);
        assert_eq!(kare.commanders(), [(2, 1), (3, 2)]);
    }

    #[test]
    fn parses_groups() {
        let g = OrderGroups::parse(b"2\r\nLINE   3  0 1 2  2 #LINE15 #LINE36\r\nPRUS 1 3 1 #PRUS\r\n");
        assert!(g.issues.is_empty());
        assert_eq!(g.groups[0].values, [0, 1, 2]);
        assert_eq!(g.groups[0].shapes, ["#LINE15", "#LINE36"]);
        assert_eq!(g.groups[1].name, "PRUS");
    }
}
