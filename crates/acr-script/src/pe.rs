//! Minimal reader for 32-bit Portable Executable (PE32) images.
//!
//! Covers what the scripting layer needs from the original mission/AI DLLs and
//! `dmcr.exe`: sections, imports, exports and base relocations.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeError(pub &'static str);

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for PeError {}

type Result<T> = std::result::Result<T, PeError>;

#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
    /// `IMAGE_SCN_*` flags.
    pub characteristics: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportName {
    Name(String),
    Ordinal(u16),
}

impl fmt::Display for ImportName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(n) => f.write_str(n),
            Self::Ordinal(o) => write!(f, "#{o}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub dll: String,
    pub name: ImportName,
    /// RVA of the import address table slot the loader fills in.
    pub iat_rva: u32,
}

#[derive(Debug, Clone)]
pub struct Export {
    pub name: Option<String>,
    pub ordinal: u16,
    pub rva: u32,
}

#[derive(Debug, Clone)]
pub struct PeImage {
    pub image_base: u32,
    pub entry_point_rva: u32,
    /// Size of the mapped image (`SizeOfImage`).
    pub size_of_image: u32,
    /// Size of the headers that are mapped at the image base (`SizeOfHeaders`).
    pub size_of_headers: u32,
    pub sections: Vec<Section>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    /// RVAs of the 32-bit words to adjust when the image is not loaded at `image_base`
    /// (`IMAGE_REL_BASED_HIGHLOW`; other relocation types are rejected).
    pub relocations: Vec<u32>,
}

const IMAGE_FILE_MACHINE_I386: u16 = 0x014C;
const PE32_MAGIC: u16 = 0x010B;

impl PeImage {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let r = Reader(data);
        if r.u16(0)? != 0x5A4D {
            return Err(PeError("missing MZ header"));
        }
        let pe = r.u32(0x3C)? as usize;
        if r.u32(pe)? != 0x0000_4550 {
            return Err(PeError("missing PE signature"));
        }
        let coff = pe + 4;
        if r.u16(coff)? != IMAGE_FILE_MACHINE_I386 {
            return Err(PeError("not an x86 image"));
        }
        let section_count = usize::from(r.u16(coff + 2)?);
        let optional_len = usize::from(r.u16(coff + 16)?);
        let opt = coff + 20;
        if r.u16(opt)? != PE32_MAGIC {
            return Err(PeError("not a PE32 image"));
        }
        let entry_point_rva = r.u32(opt + 16)?;
        let image_base = r.u32(opt + 28)?;
        let size_of_image = r.u32(opt + 56)?;
        let size_of_headers = r.u32(opt + 60)?;
        let dir_count = r.u32(opt + 92)? as usize;
        let dir = |i: usize| -> Result<(u32, u32)> {
            if i >= dir_count {
                return Ok((0, 0));
            }
            Ok((r.u32(opt + 96 + 8 * i)?, r.u32(opt + 100 + 8 * i)?))
        };

        let sections = (0..section_count)
            .map(|i| {
                let s = opt + optional_len + 40 * i;
                let name = r.bytes(s, 8)?;
                let name =
                    String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(8)]).into_owned();
                Ok(Section {
                    name,
                    virtual_size: r.u32(s + 8)?,
                    virtual_address: r.u32(s + 12)?,
                    raw_size: r.u32(s + 16)?,
                    raw_offset: r.u32(s + 20)?,
                    characteristics: r.u32(s + 36)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut image = Self {
            image_base,
            entry_point_rva,
            size_of_image,
            size_of_headers,
            sections,
            imports: Vec::new(),
            exports: Vec::new(),
            relocations: Vec::new(),
        };
        let (export_rva, _) = dir(0)?;
        if export_rva != 0 {
            image.exports = image.parse_exports(data, export_rva)?;
        }
        let (import_rva, _) = dir(1)?;
        if import_rva != 0 {
            image.imports = image.parse_imports(data, import_rva)?;
        }
        let (reloc_rva, reloc_size) = dir(5)?;
        if reloc_rva != 0 {
            image.relocations = image.parse_relocations(data, reloc_rva, reloc_size)?;
        }
        Ok(image)
    }

    /// Converts a relative virtual address into a file offset.
    pub fn rva_to_offset(&self, rva: u32) -> Option<usize> {
        self.sections.iter().find_map(|s| {
            let size = s.virtual_size.max(s.raw_size);
            (rva >= s.virtual_address && rva < s.virtual_address + size)
                .then(|| (rva - s.virtual_address + s.raw_offset) as usize)
        })
    }

    fn at(&self, rva: u32) -> Result<usize> {
        self.rva_to_offset(rva).ok_or(PeError("RVA outside of all sections"))
    }

    fn parse_imports(&self, data: &[u8], rva: u32) -> Result<Vec<Import>> {
        let r = Reader(data);
        let mut imports = Vec::new();
        let mut desc = self.at(rva)?;
        loop {
            let lookup_rva = r.u32(desc)?;
            let name_rva = r.u32(desc + 12)?;
            let iat_rva = r.u32(desc + 16)?;
            if name_rva == 0 {
                return Ok(imports);
            }
            let dll = r.cstr(self.at(name_rva)?)?;
            // Some linkers leave the lookup table empty; the IAT holds the same data on disk.
            let mut slot = self.at(if lookup_rva != 0 { lookup_rva } else { iat_rva })?;
            for i in 0.. {
                let v = r.u32(slot)?;
                if v == 0 {
                    break;
                }
                let name = if v & 0x8000_0000 != 0 {
                    ImportName::Ordinal(v as u16)
                } else {
                    ImportName::Name(r.cstr(self.at(v)? + 2)?)
                };
                imports.push(Import { dll: dll.clone(), name, iat_rva: iat_rva + 4 * i });
                slot += 4;
            }
            desc += 20;
        }
    }

    fn parse_exports(&self, data: &[u8], rva: u32) -> Result<Vec<Export>> {
        let r = Reader(data);
        let d = self.at(rva)?;
        let base = r.u32(d + 16)?;
        let function_count = r.u32(d + 20)? as usize;
        let name_count = r.u32(d + 24)? as usize;
        let functions = self.at(r.u32(d + 28)?)?;
        let names = if name_count > 0 { self.at(r.u32(d + 32)?)? } else { 0 };
        let ordinals = if name_count > 0 { self.at(r.u32(d + 36)?)? } else { 0 };
        let mut exports: Vec<Export> = (0..function_count)
            .map(|i| Ok(Export { name: None, ordinal: (base as usize + i) as u16, rva: r.u32(functions + 4 * i)? }))
            .collect::<Result<_>>()?;
        for i in 0..name_count {
            let index = usize::from(r.u16(ordinals + 2 * i)?);
            let name = r.cstr(self.at(r.u32(names + 4 * i)?)?)?;
            if let Some(e) = exports.get_mut(index) {
                e.name = Some(name);
            }
        }
        exports.retain(|e| e.rva != 0);
        Ok(exports)
    }
}

impl PeImage {
    fn parse_relocations(&self, data: &[u8], rva: u32, size: u32) -> Result<Vec<u32>> {
        let r = Reader(data);
        let start = self.at(rva)?;
        let mut out = Vec::new();
        let mut block = start;
        while block + 8 <= start + size as usize {
            let page = r.u32(block)?;
            let len = r.u32(block + 4)? as usize;
            if len < 8 {
                break;
            }
            for i in (8..len).step_by(2) {
                let entry = r.u16(block + i)?;
                match entry >> 12 {
                    0 => {} // padding
                    3 => out.push(page + u32::from(entry & 0xFFF)),
                    _ => return Err(PeError("unsupported relocation type")),
                }
            }
            block += len;
        }
        Ok(out)
    }

    /// Section containing `rva`.
    pub fn section_at(&self, rva: u32) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| rva >= s.virtual_address && rva - s.virtual_address < s.virtual_size.max(s.raw_size))
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&self, at: usize, len: usize) -> Result<&[u8]> {
        self.0.get(at..at + len).ok_or(PeError("read past end of image"))
    }

    fn u16(&self, at: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(at, 2)?.try_into().unwrap()))
    }

    fn u32(&self, at: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(at, 4)?.try_into().unwrap()))
    }

    fn cstr(&self, at: usize) -> Result<String> {
        let rest = self.0.get(at..).ok_or(PeError("string past end of image"))?;
        let end = rest.iter().position(|&b| b == 0).ok_or(PeError("unterminated string"))?;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal PE32 with one section holding an import of `dmcr.exe!SetResource`
    /// and an export `Init`.
    fn sample() -> Vec<u8> {
        let mut d = vec![0u8; 0x400];
        d[0..2].copy_from_slice(b"MZ");
        d[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        d[0x40..0x44].copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        d[coff..coff + 2].copy_from_slice(&IMAGE_FILE_MACHINE_I386.to_le_bytes());
        d[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        d[coff + 16..coff + 18].copy_from_slice(&224u16.to_le_bytes());
        let opt = coff + 20;
        let put = |d: &mut Vec<u8>, at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        d[opt..opt + 2].copy_from_slice(&PE32_MAGIC.to_le_bytes());
        put(&mut d, opt + 16, 0x1000);
        put(&mut d, opt + 28, 0x1000_0000);
        put(&mut d, opt + 92, 16);
        put(&mut d, opt + 96, 0x1100); // export dir
        put(&mut d, opt + 104, 0x1000); // import dir
        put(&mut d, opt + 96 + 40, 0x1190); // relocation dir
        put(&mut d, opt + 100 + 40, 12);
        put(&mut d, opt + 56, 0x2000);
        put(&mut d, opt + 60, 0x200);
        let s = opt + 224;
        d[s..s + 5].copy_from_slice(b".data");
        put(&mut d, s + 8, 0x200);
        put(&mut d, s + 12, 0x1000);
        put(&mut d, s + 16, 0x200);
        put(&mut d, s + 20, 0x200);
        // Section data at file 0x200 == RVA 0x1000.
        let f = |rva: usize| rva - 0x1000 + 0x200;
        put(&mut d, f(0x1000), 0x1040); // lookup table
        put(&mut d, f(0x100C), 0x1060); // dll name
        put(&mut d, f(0x1010), 0x1048); // IAT
        put(&mut d, f(0x1040), 0x1070); // hint/name
        put(&mut d, f(0x1048), 0x1070);
        d[f(0x1060)..f(0x1060) + 8].copy_from_slice(b"dmcr.exe");
        d[f(0x1072)..f(0x1072) + 11].copy_from_slice(b"SetResource");
        // Export directory: base 1, 1 function, 1 name.
        put(&mut d, f(0x1110), 1);
        put(&mut d, f(0x1114), 1);
        put(&mut d, f(0x1118), 1);
        put(&mut d, f(0x111C), 0x1140);
        put(&mut d, f(0x1120), 0x1144);
        put(&mut d, f(0x1124), 0x1148);
        put(&mut d, f(0x1140), 0x1180);
        put(&mut d, f(0x1144), 0x1150);
        d[f(0x1150)..f(0x1150) + 4].copy_from_slice(b"Init");
        // One relocation block for page 0x1000: a HIGHLOW fixup at 0x1184 and padding.
        put(&mut d, f(0x1190), 0x1000);
        put(&mut d, f(0x1194), 12);
        d[f(0x1198)..f(0x1198) + 2].copy_from_slice(&0x3184u16.to_le_bytes());
        d
    }

    #[test]
    fn reads_imports_and_exports() {
        let pe = PeImage::parse(&sample()).unwrap();
        assert_eq!(pe.image_base, 0x1000_0000);
        assert_eq!(pe.imports.len(), 1);
        assert_eq!(pe.imports[0].dll, "dmcr.exe");
        assert_eq!(pe.imports[0].name, ImportName::Name("SetResource".into()));
        assert_eq!(pe.imports[0].iat_rva, 0x1048);
        assert_eq!(pe.exports.len(), 1);
        assert_eq!(pe.exports[0].name.as_deref(), Some("Init"));
        assert_eq!(pe.exports[0].rva, 0x1180);
        assert_eq!(pe.relocations, [0x1184]);
        assert_eq!((pe.size_of_image, pe.size_of_headers), (0x2000, 0x200));
        assert_eq!(pe.section_at(0x1184).map(|s| s.name.as_str()), Some(".data"));
    }

    #[test]
    fn rejects_non_pe() {
        assert!(PeImage::parse(b"GPAK").is_err());
    }
}
