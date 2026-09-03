//! Byte-exact tests against the day-one flash dump (`card-dumps/`, kept outside
//! the repo; the tests skip without it),
//! the config the card arrived with, the vendor corpus and the hand-derived
//! single-module pack. The generator must reproduce them from the spec alone.

use panelspec::PanelSpec;
use rcvbp::image::{self, Block7Builder, BootImage};
use rcvbp::spec::{self, Generated};
use rcvbp::Rcvbp;

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

struct DayOne {
    block: Vec<u8>,
    file: Vec<u8>,
    cfg: Rcvbp,
}

fn day_one() -> Option<DayOne> {
    let Ok(dump) = std::fs::read(repo("card-dumps/primary-region.bin")) else {
        eprintln!("skipped: card-dumps/primary-region.bin is not in the repo");
        return None;
    };
    let block = dump[0x7_0000..0x8_0000].to_vec();
    let n = u32::from_le_bytes(block[image::RCVBP_OFFSET..image::RCVBP_OFFSET + 4].try_into().unwrap())
        as usize;
    let file = block[image::RCVBP_OFFSET + 4..image::RCVBP_OFFSET + 4 + n].to_vec();
    let cfg = Rcvbp::from_bytes(&file).unwrap();
    Some(DayOne { block, file, cfg })
}

/// The E120's region offsets.
fn e120() -> &'static BootImage {
    &receivers::by_name("E120").unwrap().memory.boot_image
}

/// Generate with the chip library read from the repository.
fn generate(spec: &PanelSpec) -> anyhow::Result<Generated> {
    spec::generate(spec, &spec.chip_library(&panelspec::read_library)?)
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
    spec.chip.library = "config/chips/sm16269s.toml".into();
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
    let g = generate(&reference_panel()).unwrap();
    let reference = Rcvbp::load(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    assert_eq!(g.rcvbp.records.len(), reference.records.len());
    for rec in &reference.records {
        let ours = record(&g.rcvbp, rec.id());
        let diffs = differing_bytes(ours, &rec.payload);
        assert!(diffs.is_empty(), "record 0x{:02x} differs at {diffs:x?}", rec.id());
    }
}

#[test]
fn the_reference_config_reproduces_the_day_one_pack_byte_for_byte() {
    let g = generate(&reference_panel()).unwrap();
    let Some(f) = day_one() else { return };
    let diffs = differing_bytes(&g.basic_pack, &f.block[..0x100]);
    assert!(diffs.is_empty(), "pack differs at {diffs:x?}");
}

#[test]
fn our_panel_differs_from_the_reference_only_where_intended() {
    let ours = generate(&our_panel()).unwrap();
    let reference = Rcvbp::load(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    // Secondary chip id (+0x0E9/+0x205) stays clear as in their file: 0x14D
    // would declare max scan 64 on a 1/16 module (config/chips/sm16269s.toml).
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
fn the_day_one_image_rebuilds_from_erased_flash_and_its_own_parts() {
    // Same sequence as `Block7Builder::from_generated` minus the
    // phantom-position gate (the day-one left that table zero).
    let Some(f) = day_one() else { return };
    let rec01 = &f.cfg.record_01().unwrap().payload;
    let mut b = Block7Builder::erased(e120());
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
    assert!(bad.is_empty(), "pages differing from day-one: {bad:02x?}");
}

#[test]
fn the_bench_spec_displaces_the_phantom_positions() {
    // Positions width..2*width of the void-line column table are 0xFF (off the
    // chain), real columns untouched; this is what makes black LEDs-off (docs/rendering.md).
    let spec = our_panel();
    let g = generate(&spec).unwrap();
    let img = Block7Builder::from_generated(e120(), &spec, &g).unwrap().finish().image;
    let table = &img[image::VOID_LINE_COLUMNS_OFFSET..image::VOID_LINE_COLUMNS_OFFSET + 0x400];
    assert!(table[..128].iter().all(|&b| b == 0), "real columns must stay in place");
    assert!(table[128..256].iter().all(|&b| b == 0xFF), "phantom positions must be displaced");
    assert!(table[256..].iter().all(|&b| b == 0));
    // The chip page is the caller's, so the shared sequence leaves it erased.
    assert!(img[image::CHIP_PAGE_OFFSET..image::CHIP_PAGE_OFFSET + 0x100].iter().all(|&b| b == 0xFF));
}

#[test]
fn the_scan_table_is_invariant_to_the_load_width_for_this_chip() {
    let Some(f) = day_one() else { return };
    let rec01 = &f.cfg.record_01().unwrap().payload;
    let view = rcvbp::record01::View::new(rec01).unwrap();
    let want = &f.block[image::SCAN_TABLE_OFFSET..image::SCAN_TABLE_OFFSET + 0x400];
    assert_eq!(&image::scan_table::body(view, 512).unwrap()[..], want);
    assert_eq!(&image::scan_table::body(view, 256).unwrap()[..], want);
}

#[test]
fn a_single_module_screen_gets_a_module_position_table() {
    let Some(f) = day_one() else { return };
    let mut rec01 = f.cfg.record_01().unwrap().payload.clone();
    rec01[0x0C0..0x0C2].copy_from_slice(&128u16.to_le_bytes());
    rec01[0x0C2..0x0C4].copy_from_slice(&64u16.to_le_bytes());
    let mut b = Block7Builder::erased(e120());
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
    assert_eq!(spec::mapping_record(&spec), *record(&donor, 0x03));
}

#[test]
fn the_reference_mapping_is_reproduced_by_the_block_knob() {
    // The panel's own file interleaves the two row-halves every 64 columns;
    // block = 64 reproduces it. Flashing the contiguous table scrambled every column.
    let reference = Rcvbp::load(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    assert_eq!(spec::mapping_record(&our_panel()), *record(&reference, 0x03));
}

/// The embedded set, as the CLI and the site resolve a chip id.
fn embedded_chip(id: u16) -> Option<(String, String)> {
    panelspec::embedded::chip_by_family(id).map(|(p, t)| (p.to_owned(), t.to_owned()))
}

#[test]
fn the_bench_spec_survives_a_round_trip_through_its_file() {
    let spec = our_panel();
    let bytes = generate(&spec).unwrap().rcvbp.to_file_bytes().unwrap();
    let (back, unresolved) = spec::spec_from_rcvbp(&bytes, &embedded_chip).unwrap();
    // Only what the file does not carry is left over.
    assert_eq!(unresolved, ["meta", "mapping.gate_phantom_positions", "boot.arm_at_boot"]);
    assert_eq!(back.chip.library, "config/chips/sm16269s.toml");
    assert_eq!(back.module.serial_clock, Some(8));
    assert_eq!(back.module.gray_bits, Some(12));
    assert_eq!(back.mapping.block, Some(64));
    assert_eq!(back.record01_overrides.iter().collect::<Vec<_>>(), [(&0x02F, &1)]);
    assert_eq!(back.name, "128x64-16s-sm16269s");
    // The TOML the CLI writes parses back to a spec that generates the same file.
    let again = PanelSpec::parse(&back.to_toml().unwrap()).unwrap();
    assert_eq!(generate(&again).unwrap().rcvbp.to_file_bytes().unwrap(), bytes);
}

#[test]
fn the_reference_config_imports_as_the_spec_that_regenerates_it() {
    let bytes = std::fs::read(repo("third-party/configs/P2.5-32S-128X64-SM16269S-256X384I.rcvbp")).unwrap();
    let (spec, unresolved) = spec::spec_from_rcvbp(&bytes, &embedded_chip).unwrap();
    assert_eq!(unresolved, ["meta", "mapping.gate_phantom_positions", "boot.arm_at_boot"]);
    let want = reference_panel();
    assert_eq!((spec.module.width, spec.module.height, spec.module.scan), (128, 64, 16));
    assert_eq!((spec.module.line_dir, spec.module.data_groups), (0, 1));
    assert_eq!(spec.module.serial_clock, want.module.serial_clock);
    assert_eq!(spec.module.gray_bits, None, "14 bits is what the library derives");
    assert_eq!((spec.screen.width, spec.screen.height), (256, 384));
    assert_eq!((spec.color.swap, spec.color.source), (want.color.swap, want.color.source));
    assert_eq!(spec.current.gains, want.current.gains);
    assert_eq!(spec.current.percent.map(f32::to_bits), want.current.percent.map(f32::to_bits));
    assert_eq!(spec.timing.gamma.to_bits(), want.timing.gamma.to_bits());
    assert_eq!(spec.timing.refresh_hz.to_bits(), want.timing.refresh_hz.to_bits());
    assert_eq!(spec.timing.gclock, want.timing.gclock);
    assert_eq!(spec.timing.min_oe.to_bits(), want.timing.min_oe.to_bits());
    assert_eq!(spec.timing.luminance_level, want.timing.luminance_level);
    assert_eq!(spec.timing.oe_8ns, want.timing.oe_8ns);
    assert_eq!(
        (spec.mapping.reversed_groups, spec.mapping.reversed_lines, spec.mapping.block),
        (true, false, Some(64))
    );
    assert!(spec.record01_overrides.is_empty(), "+0x02F is 0 in that file, the generator's default");
    // Same family as the test's library, so the file regenerates record for record.
    let reference = Rcvbp::from_bytes(&bytes).unwrap();
    let g = generate(&spec).unwrap();
    for rec in &reference.records {
        assert_eq!(record(&g.rcvbp, rec.id()), &rec.payload[..], "record 0x{:02x}", rec.id());
    }
}

#[test]
fn an_unknown_chip_id_imports_without_a_library() {
    let bytes = generate(&our_panel()).unwrap().rcvbp.to_file_bytes().unwrap();
    let (spec, unresolved) = spec::spec_from_rcvbp(&bytes, &|_| None).unwrap();
    assert_eq!(spec.chip.library, "");
    assert_eq!(spec.name, "128x64-16s-chip-0x014c");
    assert_eq!(spec.module.gray_bits, Some(12));
    assert_eq!(spec.mapping.block, Some(64));
    assert_eq!(
        unresolved,
        [
            "meta",
            "chip.library (no library for chip id 0x014c)",
            "record01_overrides, record 0x84 (no chip library)",
            "mapping.gate_phantom_positions",
            "boot.arm_at_boot"
        ]
    );
    assert!(spec::spec_from_rcvbp(b"not a config", &|_| None).is_err());
}

#[test]
fn a_scan_that_does_not_divide_the_module_is_refused() {
    let mut spec = our_panel();
    spec.module.scan = 12;
    assert!(generate(&spec).is_err());
}
