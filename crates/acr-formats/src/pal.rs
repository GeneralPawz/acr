//! 256-color palettes (`*.PAL`): 768 bytes of 8-bit RGB triples.

#[derive(Clone, PartialEq, Eq)]
pub struct Palette(pub [[u8; 3]; 256]);

impl Palette {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut colors = [[0u8; 3]; 256];
        let (triples, _) = bytes.get(..768)?.as_chunks::<3>();
        colors.copy_from_slice(triples);
        Some(Self(colors))
    }

    pub fn rgb(&self, index: u8) -> [u8; 3] {
        self.0[usize::from(index)]
    }
}

impl std::fmt::Debug for Palette {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Palette({:?}..)", &self.0[..4])
    }
}
