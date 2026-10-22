//! Interface texts by identifier; the parser lives in `acr_formats::string_table` because the
//! mission runtime (`GetTextByID`) reads the same tables.

pub use acr_formats::string_table::*;
