//! Retail corpus facts that prove executable code unreachable: the model
//! commands every overlay's programs use, which models carry custom-vertex
//! selector 3, and the constructor selectors every H13 bank holds.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use inf_formats::{
    parse_constructor_asset_bank, parse_model_bank, parse_overlay, ModelInstructionKind,
    ModelProgram, OverlayKey, Preload,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

/// Every overlay with its key and bytes.
fn overlays() -> Vec<(OverlayKey, Vec<u8>)> {
    let mut paths: Vec<_> = fs::read_dir(root().join("Overlay"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            (
                OverlayKey::parse_filename(name).unwrap(),
                fs::read(&path).unwrap(),
            )
        })
        .collect()
}

fn walk(
    program: &ModelProgram,
    global: u16,
    opcodes: &mut BTreeSet<u8>,
    selector3: &mut BTreeSet<u16>,
) {
    for instruction in &program.instructions {
        let opcode = instruction.raw_command as u8;
        opcodes.insert(opcode);
        match &instruction.kind {
            ModelInstructionKind::NestedProgram(nested) => {
                walk(&nested.program, global, opcodes, selector3)
            }
            ModelInstructionKind::UnresolvedDynamic(command)
                if opcode == 0x2d && command.arguments[1] as i16 == 3 =>
            {
                selector3.insert(global);
            }
            _ => {}
        }
    }
}

#[test]
fn retail_models_use_no_line_basis_or_rotated_sprite_commands() {
    let preload = Preload::parse(&fs::read(root().join("PreLoad.dat")).unwrap()).unwrap();
    let (mut opcodes, mut selector3) = (BTreeSet::new(), BTreeSet::new());
    for (key, bytes) in overlays() {
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let request = requests.get(8).unwrap();
        if request.count == 0 {
            continue;
        }
        let layout = parse_overlay(&bytes, requests).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|r| r.handler.get() == 8)
            .unwrap();
        for entry in &parse_model_bank(&bytes, region, request, &key)
            .unwrap()
            .entries
        {
            walk(
                &entry.program,
                entry.identity.global_index,
                &mut opcodes,
                &mut selector3,
            );
        }
    }
    // Commands 02 (00495A60/00495B70), 09 (004A12E0), 18 (004A1530/004A26A0)
    // and 19 (004A1940/004A2AD0) are absent.
    assert_eq!(
        opcodes,
        BTreeSet::from([
            0x01, 0x03, 0x05, 0x07, 0x0b, 0x0e, 0x0f, 0x10, 0x15, 0x16, 0x1a, 0x1b, 0x1c, 0x22,
            0x23, 0x25, 0x26, 0x27, 0x2a, 0x2b, 0x2c, 0x2d, 0xd0, 0xd2, 0xd4, 0xd6, 0xd8, 0xda,
            0xdc, 0xde, 0xe0, 0xe2, 0xe4, 0xe6, 0xea, 0xeb, 0xee, 0xef, 0xf0, 0xf1, 0xf3, 0xf4,
            0xf5, 0xf6, 0xf7, 0xfb, 0xfc, 0xfd, 0xfe, 0xff,
        ])
    );
    // Custom-vertex selector 3 (004260C0's 00439E40 arm) only in the
    // player vehicle's form models, whose own +10 (00415D30) answers it.
    assert_eq!(selector3, BTreeSet::from([118, 119]));
}

#[test]
fn retail_constructor_banks_hold_no_selector_63_and_one_selector_13() {
    let preload = Preload::parse(&fs::read(root().join("PreLoad.dat")).unwrap()).unwrap();
    let mut selectors = BTreeMap::new();
    for (key, bytes) in overlays() {
        let requests = preload
            .leaf(key.level)
            .unwrap()
            .requests(key.request_kind());
        let Ok(layout) = parse_overlay(&bytes, requests) else {
            continue;
        };
        let Some(region) = layout.regions.iter().find(|r| r.handler.get() == 13) else {
            continue;
        };
        for asset in parse_constructor_asset_bank(&bytes, region).unwrap().assets {
            let selector = asset.dispatch.unwrap().selector;
            selectors
                .entry(selector)
                .or_insert_with(Vec::new)
                .push((key.level, asset.local_index));
        }
    }
    // Factory case 63 (the base course gate 004404F0 alone) never builds.
    assert!(!selectors.contains_key(&63));
    assert_eq!(selectors[&13], [(1, 85)]);
}
