//! Byte-exact tests against the day-one flash dump (`card-dumps/primary-region.bin`),
//! the config the card arrived with, the vendor corpus and the hand-derived
//! single-module pack. The generator must reproduce them from the spec alone.

use e120_rcvbp::image::{self, Block7Builder};
use e120_rcvbp::spec::PanelSpec;
use e120_rcvbp::Rcvbp;

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

struct Factory {
    block: Vec<u8>,
    file: Vec<u8>,
    cfg: Rcvbp,
}

fn factory() -> Factory {
    let dump = std::fs::read(repo("card-dumps/primary-region.bin")).expect("factory dump");
    let block = dump[0x7_0000..0x8_0000].to_vec();
    let n = u32::from_le_bytes(block[image::RCVBP_OFFSET..image::RCVBP_OFFSET + 4].try_into().unwrap())
        as usize;
    let file = block[image::RCVBP_OFFSET + 4..image::RCVBP_OFFSET + 4 + n].to_vec();
    let cfg = Rcvbp::from_bytes(&file).unwrap();
    Factory { block, file, cfg }
}

fn our_panel() -> PanelSpec {
    // Spec library paths are repo-relative, as the CLI is run from the root.
    std::env::set_current_dir(repo(".")).unwrap();
    PanelSpec::load("config/panels/p25-128x64-sm16269s.toml").unwrap()
}

/// The config the card arrived with, as a spec: same module, the SM16169SH
/// register set, a 256x384 wall.
fn reference_panel() -> PanelSpec {
    let mut spec = our_panel();
    // That file says 14-bit grey and +0x02F = 0. Ours keeps 12 (12-16 render
    // alike, docs/rendering.md) and +0x02F = 1, without which nothing displays.
    spec.module.gray_bits = None;
    spec.record01_overrides.remove(&0x02F);
    spec.chip.library = "config/chips/sm16169sh.toml".into();
    spec.screen.width = 256;
    spec.screen.height = 384;
    spec
}

fn record(cfg: &Rcvbp, id: u8) -> &[u8] {
    &cfg.find_by_id(id).unwrap().payload
}

fn differing_pages(a: &[u8], b: &[u8]) -> Vec<u8> {
    (0..=255u8)
        .filter(|&p| {
            let at = usize::from(p) * 0x100;
            a[at..at + 0x100] != b[at..at + 0x100]
        })
        .collect()
}

fn differing_bytes(a: &[u8], b: &[u8]) -> Vec<usize> {
    assert_eq!(a.len(), b.len(), "length mismatch");
    (0..a.len()).filter(|&i| a[i] != b[i]).collect()
}

#[test]
fn the_reference_config_is_regenerated_record_for_record() {
    let g = reference_panel().generate().unwrap();
    let reference = Rcvbp::load(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    assert_eq!(g.rcvbp.records.len(), reference.records.len());
    for rec in &reference.records {
        let ours = record(&g.rcvbp, rec.id());
        let diffs = differing_bytes(ours, &rec.payload);
        assert!(diffs.is_empty(), "record 0x{:02x} differs at {diffs:x?}", rec.id());
    }
}

#[test]
fn the_reference_config_reproduces_the_factory_pack_byte_for_byte() {
    let g = reference_panel().generate().unwrap();
    let diffs = differing_bytes(&g.basic_pack, &factory().block[..0x100]);
    assert!(diffs.is_empty(), "pack differs at {diffs:x?}");
}

#[test]
fn our_panel_differs_from_the_reference_only_where_intended() {
    let ours = our_panel().generate().unwrap();
    let reference = Rcvbp::load(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    // Secondary chip id (+0x0E9/+0x205) stays clear as in their file: 0x14D
    // would declare max scan 64 on a 1/16 module (config/chips/sm16269s-factory.toml).
    let d = differing_bytes(record(&ours.rcvbp, 0x01), record(&reference, 0x01));
    // +0x023 grey 12 (theirs 14; 12-16 render alike), +0x02F = 1 (theirs 0;
    // required to display), then the single-module screen size.
    assert_eq!(d, vec![0x023, 0x02F, 0x0C0, 0x0C1, 0x0C2, 0x0C3]);
    assert!(differing_bytes(record(&ours.rcvbp, 0x03), record(&reference, 0x03)).is_empty());
    assert!(differing_bytes(record(&ours.rcvbp, 0x84), record(&reference, 0x84)).is_empty());
    let reference = Rcvbp::load(fixture("p25-128x64-fixed.rcvbp")).unwrap();
    // Secondary parameters: the screen size the vendor mirrors there.
    let d = differing_bytes(record(&ours.rcvbp, 0x8a), record(&reference, 0x8a));
    assert_eq!(d, vec![0x10, 0x11, 0x12, 0x13]);
    // The hand-derived pack patched four fields and left the rest at the
    // wall's values; the generated one also derives these from the layout.
    let v2 = std::fs::read(fixture("basic-pack-single-module-v2.bin")).unwrap();
    // +0x08 (grey 12) and +0x19 (record +0x02F = 1) mirror the record 0x01 diffs above.
    let layout_derived = [0x08, 0x19, 0x25, 0x2A, 0x39, 0x3A, 0x3B, 0x3C, 0xE3, 0xE4, 0xE5, 0xE6];
    let d: Vec<usize> = (0..0xFC)
        .filter(|i| !layout_derived.contains(i))
        .filter(|&i| ours.basic_pack[i] != v2[i])
        .collect();
    assert!(d.is_empty(), "pack differs at {d:x?}");
    let p = &ours.basic_pack;
    assert_eq!(p[0x25], 1, "modules / split");
    assert_eq!(p[0x2A], 1, "module count");
    assert_eq!(&p[0x39..0x3D], &[1, 0, 0, 128], "CardScanLen 256, extent 128");
    assert_eq!(&p[0xE3..0xE7], &[1, 0, 1, 0], "MaxPsc 256 x2");
}

#[test]
fn the_factory_image_rebuilds_from_erased_flash_and_its_own_parts() {
    // Same sequence as `Block7Builder::from_generated` minus the
    // phantom-position gate (the factory left that table zero).
    let f = factory();
    let rec01 = &f.cfg.record_01().unwrap().payload;
    let mut b = Block7Builder::erased();
    b.zero_regions();
    b.basic_pack(&f.block[..0x100]).unwrap();
    b.data_swap_from(rec01).unwrap();
    b.module_positions_from(rec01).unwrap();
    b.anti_void_lines();
    b.mapping_from(&f.cfg).unwrap();
    b.scan_table_from(rec01, 512).unwrap();
    b.rcvbp(&f.file).unwrap();
    let img = b.finish().image;
    let bad: Vec<u8> = differing_pages(&img, &f.block).into_iter().filter(|&p| p != 0xF0).collect();
    assert!(bad.is_empty(), "pages differing from factory: {bad:02x?}");
}

#[test]
fn the_bench_spec_displaces_the_phantom_positions() {
    // Positions width..2*width of the void-line column table are 0xFF (off the
    // chain), real columns untouched; this is what makes black LEDs-off (docs/rendering.md).
    let spec = our_panel();
    let g = spec.generate().unwrap();
    let img = Block7Builder::from_generated(&spec, &g).unwrap().finish().image;
    let table = &img[image::VOID_LINE_COLUMNS_OFFSET..image::VOID_LINE_COLUMNS_OFFSET + 0x400];
    assert!(table[..128].iter().all(|&b| b == 0), "real columns must stay in place");
    assert!(table[128..256].iter().all(|&b| b == 0xFF), "phantom positions must be displaced");
    assert!(table[256..].iter().all(|&b| b == 0));
    // The chip page is the caller's, so the shared sequence leaves it erased.
    assert!(img[image::CHIP_PAGE_OFFSET..image::CHIP_PAGE_OFFSET + 0x100].iter().all(|&b| b == 0xFF));
}

#[test]
fn the_scan_table_is_invariant_to_the_load_width_for_this_chip() {
    let f = factory();
    let rec01 = &f.cfg.record_01().unwrap().payload;
    let view = e120_rcvbp::record01::View::new(rec01).unwrap();
    let want = &f.block[image::SCAN_TABLE_OFFSET..image::SCAN_TABLE_OFFSET + 0x400];
    assert_eq!(&image::scan_table::body(view, 512).unwrap()[..], want);
    assert_eq!(&image::scan_table::body(view, 256).unwrap()[..], want);
}

#[test]
fn a_single_module_screen_gets_a_module_position_table() {
    let f = factory();
    let mut rec01 = f.cfg.record_01().unwrap().payload.clone();
    rec01[0x0C0..0x0C2].copy_from_slice(&128u16.to_le_bytes());
    rec01[0x0C2..0x0C4].copy_from_slice(&64u16.to_le_bytes());
    let mut b = Block7Builder::erased();
    b.module_positions_from(&rec01).unwrap();
    let img = b.finish().image;
    let at = image::MODULE_POS_OFFSET;
    assert_eq!(img[at + 5], 32, "8x4 tiles of 16x16");
    assert_eq!(&img[at + 0x16..at + 0x20], &[0, 7, 0, 0, 0, 0, 0, 16, 0, 16]);
    let last = at + 0x16 + 31 * 10;
    assert_eq!(&img[last..last + 10], &[3, 0, 0, 112, 0, 48, 0, 16, 0, 16]);
}

#[test]
fn the_default_block_gives_the_vendor_consensus_table() {
    // No `block`: each data group takes one contiguous half of the chain, the
    // majority wiring in the vendor corpus. Not the wiring of the bench panel.
    let donor = Rcvbp::load(repo("third-party/configs/donor-P2.5-320x160-2153-consensus.rcvbp")).unwrap();
    let mut spec = our_panel();
    spec.mapping.block = None;
    assert_eq!(spec.mapping_record(), *record(&donor, 0x03));
}

#[test]
fn the_reference_mapping_is_reproduced_by_the_block_knob() {
    // The panel's own file interleaves the two row-halves every 64 columns;
    // block = 64 reproduces it. Flashing the contiguous table scrambled every column.
    let reference = Rcvbp::load(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    assert_eq!(our_panel().mapping_record(), *record(&reference, 0x03));
}

#[test]
fn a_scan_that_does_not_divide_the_module_is_refused() {
    let mut spec = our_panel();
    spec.module.scan = 12;
    assert!(spec.generate().is_err());
}
