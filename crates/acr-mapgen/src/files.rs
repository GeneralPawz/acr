//! Readers for the generator's data files, all text formats parsed the way the engine's
//! `fscanf`-style reader (0x404FB1) does: whitespace-separated tokens.

/// Whitespace-separated tokens (CP1252 bytes mapped 1:1 to chars).
pub fn tokens(data: &[u8]) -> Vec<String> {
    data.split(|b| b.is_ascii_whitespace()).filter(|t| !t.is_empty()).map(latin1).collect()
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Lines without line breaks and trailing blanks (`Data\Start%d.dat` reader, 0x569B0C).
fn lines(data: &[u8]) -> Vec<String> {
    data.split(|&b| b == b'\n').map(|l| latin1(l).trim_end_matches(['\r', ' ']).to_string()).collect()
}

fn int(token: Option<&String>) -> Option<i32> {
    token?.parse().ok()
}

/// `Terrain\Res.dat`: the choices of the random map menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResDat {
    /// Mine density ("Mineralien"): name and three mine counts per start area.
    pub minerals: Vec<(String, [i32; 3])>,
    /// Start resources: name and up to 8 amounts in `RESU` order.
    pub resources: Vec<(String, [u32; 8])>,
    /// Relief type names ("Bodentyp").
    pub reliefs: Vec<String>,
    /// Landscape styles: name and `(players, land shape file)` choices.
    pub styles: Vec<(String, Vec<(i32, String)>)>,
}

impl ResDat {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let lines = lines(data);
        let mut rows = lines.iter().map(|l| tokens(l.as_bytes())).filter(|t| !t.is_empty());
        let mut res = Self::default();
        let section = |rows: &mut dyn Iterator<Item = Vec<String>>| -> Option<Vec<Vec<String>>> {
            let head = rows.next()?;
            let n = int(head.first())?;
            (0..n).map(|_| rows.next()).collect()
        };
        for row in section(&mut rows)? {
            let v = |i| int(row.get(i)).unwrap_or(0);
            res.minerals.push((row[0].clone(), [v(1), v(2), v(3)]));
        }
        for row in section(&mut rows)? {
            let n = int(row.get(1)).unwrap_or(0).clamp(0, 8) as usize;
            let mut amounts = [0; 8];
            for (i, a) in amounts.iter_mut().enumerate().take(n) {
                *a = int(row.get(2 + i)).unwrap_or(0) as u32;
            }
            res.resources.push((row[0].clone(), amounts));
        }
        for row in section(&mut rows)? {
            res.reliefs.push(row[0].clone());
        }
        for row in section(&mut rows)? {
            let n = int(row.get(3)).unwrap_or(0).max(0) as usize;
            let maps = (0..n).filter_map(|i| Some((int(row.get(4 + 2 * i))?, row.get(5 + 2 * i)?.clone()))).collect();
            res.styles.push((row.get(1)?.clone(), maps));
        }
        Some(res)
    }
}

/// One land shape of a `.MSP` file (header "COST"): a 256 × 256 land/water mask with
/// start points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandShape {
    /// Start points in use (`nsp`) and number of player groups.
    pub points: usize,
    pub groups: usize,
    /// Start point positions on the 256 grid: `(x, y)`.
    pub positions: [(u8, u8); 8],
    /// Group of each start point.
    pub point_group: [u8; 8],
    /// 0 = water, 255 = land, row-major 256 × 256, plus one guard byte.
    pub mask: Vec<u8>,
}

/// Number of shape variants in an `.MSP` file.
pub fn msp_variants(data: &[u8]) -> Option<u32> {
    (data.get(..4)? == b"COST").then(|| u32::from_le_bytes(data[4..8].try_into().unwrap())).filter(|&n| n > 0)
}

/// Decodes variant `index` (0x45CDF2): run lengths alternating water and land.
pub fn msp_shape(data: &[u8], index: u32) -> Option<LandShape> {
    let at = |i: usize| -> Option<usize> { Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?) as usize) };
    let (start, end) = (at(8 + 4 * index as usize)?, at(12 + 4 * index as usize)?);
    let blob = data.get(start..end)?;
    let header = blob.get(..0x22)?;
    let mut positions = [(0, 0); 8];
    for (i, p) in positions.iter_mut().enumerate() {
        *p = (header[6 + 2 * i], header[7 + 2 * i]);
    }
    let runs = u32::from_le_bytes(header[0x1E..0x22].try_into().ok()?) as usize;
    let mut mask = vec![0u8; 0x10001];
    mask[0x10000] = 0xE3;
    mask[0xFFFF] = 0xE3;
    let (mut pos, mut fill) = (0usize, 0u8);
    for &run in blob.get(0x22..0x22 + runs)? {
        let run = run as usize;
        if run != 0 {
            let end = (pos + run).min(mask.len());
            mask[pos.min(end)..end].fill(fill);
        }
        fill = !fill;
        pos += run;
    }
    Some(LandShape {
        points: header[4] as usize,
        groups: header[5] as usize,
        positions,
        point_group: header[0x16..0x1E].try_into().ok()?,
        mask,
    })
}

/// `Terrain\cost%d.dat`: shore textures by distance to the coast.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostTable {
    pub step: i32,
    pub levels: Vec<Vec<u8>>,
}

impl CostTable {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let t = tokens(data);
        let mut it = t.iter();
        let n = int(it.next())?;
        let step = int(it.next())?;
        let mut levels = Vec::new();
        for _ in 0..n {
            let k = int(it.next())?;
            levels.push((0..k).map(|_| int(it.next()).map(|v| v as u8)).collect::<Option<Vec<_>>>()?);
        }
        Some(Self { step, levels })
    }
}

/// A `.terr` file: piece lists with attempt counts and start links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrLine {
    pub list: String,
    pub count: i32,
    /// `(link kind, chain length)` choices.
    pub starts: Vec<(i32, i32)>,
}

pub fn parse_terr(data: &[u8]) -> Vec<TerrLine> {
    let t = tokens(data);
    let mut it = t.iter();
    let mut out = Vec::new();
    while let (Some(list), Some(count), Some(n)) = (it.next(), int(it.next()), int(it.next())) {
        let mut starts = Vec::new();
        for _ in 0..n.max(0) {
            starts.push((int(it.next()).unwrap_or(0), int(it.next()).unwrap_or(0)));
        }
        out.push(TerrLine { list: list.clone(), count, starts });
    }
    out
}

/// `Terrain\feature%d%d.dat`: `(list, count, minimum distance from the start)` triples.
pub fn parse_features(data: &[u8]) -> Vec<(String, i32, i32)> {
    let t = tokens(data);
    let mut out = Vec::new();
    let mut it = t.iter();
    while let (Some(list), Some(count), Some(dist)) = (it.next(), int(it.next()), int(it.next())) {
        out.push((list.clone(), count, dist));
    }
    out
}

/// `AI\AI.DAT`: the nations of the random map menu and their start unit (peasant).
pub fn parse_ai_nations(data: &[u8]) -> Vec<(String, String)> {
    let lines = lines(data);
    let mut rows = lines.iter().map(|l| tokens(l.as_bytes())).filter(|t| !t.is_empty());
    let n = rows.next().and_then(|h| int(h.first())).unwrap_or(0).max(0) as usize;
    rows.take(n).filter(|r| r.len() >= 3).map(|r| (r[0].clone(), r[2].clone())).collect()
}

/// One nation's block of `Data\Start%d.dat`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartUnits {
    /// Extra resources in file order: wood, food, stone, gold, iron, coal.
    pub resources: [i32; 6],
    /// Placed units relative to the start point, in 32-pixel steps: `(dx, dy, unit id)`.
    pub units: Vec<(i32, i32, String)>,
}

/// Finds the block `#<peasant>` and reads it like 0x452080.
pub fn parse_start_units(data: &[u8], peasant: &str) -> Option<StartUnits> {
    let lines = lines(data);
    let mut it = lines.iter();
    it.find(|l| l.strip_prefix('#') == Some(peasant))?;
    let res = tokens(it.next()?.as_bytes());
    if res.len() < 6 {
        return None;
    }
    let mut out = StartUnits::default();
    for (i, r) in out.resources.iter_mut().enumerate() {
        *r = int(res.get(i))?;
    }
    let n = int(tokens(it.next()?.as_bytes()).first())?;
    let mut types: Vec<(char, String)> = Vec::new();
    for _ in 0..n.max(0) {
        let t = tokens(it.next()?.as_bytes());
        types.push((t.get(1)?.chars().next()?, t.first()?.clone()));
    }
    let center = tokens(it.next()?.as_bytes());
    let (cx, cy) = (int(center.first())?, int(center.get(1))?);
    for (row, line) in it.take_while(|l| !l.starts_with('#')).enumerate() {
        for (col, c) in line.chars().enumerate() {
            if c == ' ' {
                continue;
            }
            if let Some((_, id)) = types.iter().find(|(l, _)| *l == c) {
                out.units.push((col as i32 - cx, row as i32 - cy, id.clone()));
            }
        }
    }
    Some(out)
}

/// `Terrain\factures_set%d.dat`: per texture four ground paint variants and a brightness.
pub fn parse_factures(data: &[u8]) -> Vec<[u8; 5]> {
    let mut table = vec![[0xFF; 5]; 256];
    let t = tokens(data);
    for c in t.as_chunks::<6>().0 {
        let v: Option<Vec<i32>> = c.iter().map(|s| s.parse().ok()).collect();
        let Some(v) = v else { break };
        if let Some(e) = table.get_mut(v[0] as usize) {
            *e = [v[1] as u8, v[2] as u8, v[3] as u8, v[4] as u8, v[5] as u8];
        }
    }
    table
}

/// `Terrain\fract_set%d.txt`: the 8 inland textures by noise level.
pub fn parse_fract(data: &[u8]) -> Option<[u8; 8]> {
    let t = tokens(data);
    let mut out = [0; 8];
    for (i, o) in out.iter_mut().enumerate() {
        *o = int(t.get(i))? as u8;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_res_dat() {
        let res = ResDat::parse(
            b"2 1\r\nRA_Lean 1 1 1\r\nRA_Rich 4 3 3\r\n1 0\r\nRA_Normal 6 1 2 3 4 5 6\r\n1 0\r\nRT_Plane\r\n\
              1 1\r\n#0 LT_Land 000.STY 2 2 a2.msp 3 a3.msp\r\n",
        )
        .unwrap();
        assert_eq!(res.minerals[1], ("RA_Rich".into(), [4, 3, 3]));
        assert_eq!(res.resources[0].1, [1, 2, 3, 4, 5, 6, 0, 0]);
        assert_eq!(res.reliefs, ["RT_Plane"]);
        assert_eq!(res.styles[0], ("LT_Land".into(), vec![(2, "a2.msp".into()), (3, "a3.msp".into())]));
    }

    #[test]
    fn reads_start_units() {
        let file = b"#A(X)\r\n0 1 0 0 0 0\r\n0\r\n3 0\r\nX\r\n#B(X)\r\n0 1500 0 0 0 0\r\n2\r\nP(X) P\r\nK(X) X\r\n\
                     2 1\r\n P \r\n  X\r\n#C(X)\r\n";
        let s = parse_start_units(file, "B(X)").unwrap();
        assert_eq!(s.resources, [0, 1500, 0, 0, 0, 0]);
        assert_eq!(s.units, vec![(-1, -1, "P(X)".to_string()), (0, 0, "K(X)".to_string())]);
    }
}
