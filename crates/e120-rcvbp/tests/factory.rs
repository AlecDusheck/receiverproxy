//! Pins against reality: the day-one flash dump (the card's factory boot
//! image and the config it was compiled from), the vendor corpus consensus,
//! and the hand-derived single-module pack. Every generator must land on
//! those bytes exactly.

use e120_rcvbp::image::{self, Block7Builder};
use e120_rcvbp::spec::PanelSpec;
use e120_rcvbp::Rcvbp;

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
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
    // Spec template paths are repo-relative, as the CLI is run from the root.
    std::env::set_current_dir(repo(".")).unwrap();
    PanelSpec::load("config/panels/p25-128x64-sm16269s.toml").unwrap()
}

fn differing_pages(a: &[u8], b: &[u8]) -> Vec<u8> {
    (0..=255u8)
        .filter(|&p| {
            let at = usize::from(p) * 0x100;
            a[at..at + 0x100] != b[at..at + 0x100]
        })
        .collect()
}

#[test]
fn the_factory_image_rebuilds_from_erased_flash_and_its_own_parts() {
    // No base image: every region is generated and the result must equal
    // the factory block except page 0xF0, which is EEPROM-backed and not
    // part of the image the vendor writes.
    let f = factory();
    let rec01 = &f.cfg.record_01().unwrap().payload;
    let mut b = Block7Builder::erased();
    b.zero_regions();
    b.basic_pack(&f.block[..0x100]).unwrap();
    b.data_swap_from(rec01).unwrap();
    b.module_positions_from(rec01).unwrap();
    b.anti_void_lines();
    b.mapping_from(&f.cfg).unwrap();
    b.scan_table_from(rec01, 512).unwrap(); // the factory pack's CardScanLen
    b.rcvbp(&f.file).unwrap();
    let (img, _, _) = b.finish();
    let bad: Vec<u8> = differing_pages(&img, &f.block).into_iter().filter(|&p| p != 0xF0).collect();
    assert!(bad.is_empty(), "pages differing from factory: {bad:02x?}");
}

#[test]
fn the_scan_table_is_invariant_to_the_load_width_for_this_chip() {
    let f = factory();
    let rec01 = &f.cfg.record_01().unwrap().payload;
    let view = e120_rcvbp::record01::View::new(rec01).unwrap();
    let want = &f.block[image::SCAN_TABLE_OFFSET..image::SCAN_TABLE_OFFSET + 0x400];
    assert_eq!(&image::scan_table::body(&view, 512).unwrap()[..], want);
    assert_eq!(&image::scan_table::body(&view, 256).unwrap()[..], want);
}

#[test]
fn a_single_module_screen_gets_a_module_position_table() {
    let f = factory();
    let mut rec01 = f.cfg.record_01().unwrap().payload.clone();
    rec01[0x0C0..0x0C2].copy_from_slice(&128u16.to_le_bytes());
    rec01[0x0C2..0x0C4].copy_from_slice(&64u16.to_le_bytes());
    let mut b = Block7Builder::erased();
    b.module_positions_from(&rec01).unwrap();
    let (img, _, _) = b.finish();
    let at = image::MODULE_POS_OFFSET;
    assert_eq!(img[at + 5], 32, "8x4 tiles of 16x16");
    // Line direction 0 walks columns right-to-left: the inner index counts
    // down from 7 while x still rises from 0.
    assert_eq!(&img[at + 0x16..at + 0x20], &[0, 7, 0, 0, 0, 0, 0, 16, 0, 16]);
    let last = at + 0x16 + 31 * 10;
    assert_eq!(&img[last..last + 10], &[3, 0, 0, 112, 0, 48, 0, 16, 0, 16]);
}

#[test]
fn the_chip_page_is_record_84_verbatim() {
    let f = factory();
    let mut b = Block7Builder::from_base(&f.block).unwrap();
    b.chip_registers_from(&f.cfg).unwrap();
    let (img, _, changed) = b.finish();
    assert_eq!(changed, vec![0x09]);
    let rec = &f.cfg.records.iter().find(|r| r.rtype[1] == 0x84).unwrap().payload;
    assert_eq!(&img[image::CHIP_PAGE_OFFSET..image::CHIP_PAGE_OFFSET + 0x100], &rec[..]);
}

#[test]
fn our_panel_reproduces_the_hand_derived_pack_from_formulas() {
    let g = our_panel().generate().unwrap();
    let expected = std::fs::read(repo("crates/e120-rcvbp/tests/fixtures/basic-pack-single-module-v2.bin")).unwrap();
    // v2 was patched by hand and kept the factory CRC; the generator recomputes it.
    let diffs: Vec<usize> = (0..0xFC).filter(|&i| g.basic_pack[i] != expected[i]).collect();
    assert!(diffs.is_empty(), "generated pack differs at {diffs:x?}");
}

#[test]
fn a_two_module_screen_reproduces_the_factory_pack() {
    let mut spec = our_panel();
    spec.screen.width = 256;
    spec.screen.height = 384;
    let g = spec.generate().unwrap();
    assert_eq!(g.basic_pack[..], factory().block[..0x100]);
}

#[test]
fn our_panel_changes_only_the_screen_size_and_sub_id_in_the_template_record() {
    let spec = our_panel();
    let template = Rcvbp::load(&repo(&spec.template.rcvbp)).unwrap();
    let g = spec.generate().unwrap();
    let before = &template.record_01().unwrap().payload;
    let after = &g.rcvbp.record_01().unwrap().payload;
    let diffs: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
    assert_eq!(diffs, vec![0x0C0, 0x0C1, 0x0C2, 0x0C3, 0x0E9, 0x205]);
    let back = Rcvbp::from_bytes(&g.rcvbp.to_file_bytes().unwrap()).unwrap();
    assert_eq!(back.records.len(), g.rcvbp.records.len());
}

#[test]
fn the_generated_mapping_is_the_vendor_consensus_table() {
    let donor = Rcvbp::load(&repo("third-party/configs/donor-P2.5-320x160-2153-consensus.rcvbp")).unwrap();
    let consensus = &donor.records.iter().find(|r| r.rtype[1] == 0x03).unwrap().payload;
    assert_eq!(our_panel().mapping_record(), *consensus);
}

#[test]
fn the_sellers_outlier_is_not_what_the_knobs_produce() {
    let seller = Rcvbp::load(&repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    let outlier = &seller.records.iter().find(|r| r.rtype[1] == 0x03).unwrap().payload;
    assert_ne!(our_panel().mapping_record(), *outlier);
}

#[test]
fn a_scan_the_reference_pack_was_not_computed_for_is_refused() {
    let mut spec = our_panel();
    spec.module.scan = 32;
    assert!(spec.generate().is_err());
}
