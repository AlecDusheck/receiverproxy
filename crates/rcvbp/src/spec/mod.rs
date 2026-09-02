//! A panel spec (`panelspec::PanelSpec`) to the config generated from it.
//!
//! Record 0x01 (`docs/record-0x01-fields.md`), the pixel mapping and the basic
//! pack (vendor `GetBasicParam`). Nothing is copied from a donor file: every
//! byte is a vendor default, a spec field, a chip-library value or a listed literal.

mod basic_pack;
mod generate;
mod import;
mod mapping;
mod record01;
mod records;

pub use basic_pack::verify as verify_basic_pack;
pub use generate::{generate, Generated};
pub use import::{spec_from_rcvbp, ChipLookup};
pub use mapping::record as mapping_record;
pub use panelspec::{ChipLibrary, PanelSpec};
