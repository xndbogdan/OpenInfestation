use super::*;
use crate::frontend_scene::SceneView;
use crate::menu::MenuBasis;
use crate::model_context::{ModelContext, ModelDispatch, ModelProjection};
use crate::object_runtime::ActorHandle;
use inf_formats::FrontendExecutable;
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    context_position: [i32; 3],
    eye: [i32; 3],
    view_basis: [f32; 9],
    focal: [i32; 2],
    viewport: [i32; 2],
    center: [i32; 2],
    fog_factor: i32,
    fade: [i32; 2],
    coordinate_shift: u8,
    far_color: u32,
    sorting: bool,
    selector: u8,
    texture_flags: u16,
    word_44: u16,
    dword_54: u32,
    nodes: Vec<[u16; 4]>,
    packets: Vec<Packet>,
}
#[derive(Deserialize)]
struct Packet {
    key: u32,
    callback: u32,
    texture_slot: i32,
    payload: Vec<Option<u8>>,
}

fn basis(values: [f32; 9]) -> MenuBasis {
    let mut result = MenuBasis::INITIAL;
    result.words[1..].copy_from_slice(&values.map(f32::to_bits));
    result
}

/// check_selector67_draw.py's distinct descriptor per selectable slot.
fn descriptor_address(slot: i32) -> u32 {
    match slot {
        0x29d => 0x0075_6000,
        0x38b => 0x0075_7000,
        1 => 0x0075_8000,
        other => panic!("{other:X}"),
    }
}

/// ribbon_sort_word over the supplied descriptor's selector and flags.
fn sorted(case: &Case, depth: u32, slot: i32) -> u32 {
    if !case.sorting {
        return depth;
    }
    ribbon_sort_word::<std::convert::Infallible>(
        depth,
        case.selector,
        |_| Ok(case.texture_flags),
        || {
            Ok(TextureSortIdentity::NativeEntryAddress(descriptor_address(
                slot,
            )))
        },
    )
    .unwrap()
}

/// 00486B30 against the unchanged executable: every packet's kind, sort
/// key, texture slot and payload byte, over straight, lateral, receding,
/// behind-the-eye, capped, degenerate and sorted trails.
#[test]
fn selector67_draw_matches_native_ribbon_packets() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .expect("required original retail executable");
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mode = executable.word(RIBBON_MODE_0053C568).unwrap();
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector67_draw_native.json"
    ))
    .unwrap();
    let mut packets = 0;
    for case in &receipt.cases {
        let mut arena = ObjectArena::new();
        let ribbon = arena.allocate_selector67(ActorHandle::from_raw(0));
        {
            let state = arena.selector67_mut(ribbon).unwrap();
            state.word_44 = case.word_44;
            state.dword_54 = case.dword_54;
            state.nodes = case
                .nodes
                .iter()
                .map(|&[x, y, z, length]| super::super::RibbonNode {
                    point: [x, y, z],
                    length,
                })
                .collect();
        }
        let view = SceneView {
            eye: case.eye,
            basis: basis(case.view_basis),
        };
        // 0048FE60 receives the context position's sign-extended low WORDs.
        let origin = case.context_position.map(|value| value as u16);
        let mut input = RibbonDrawInput {
            projector: ModelProjectionBackend {
                transform: ModelContext::new(
                    origin.map(|value| i32::from(value as i16)),
                    MenuBasis::INITIAL,
                )
                .publish(view, case.coordinate_shift),
                view,
                projection: ModelProjection {
                    focal: case.focal,
                    viewport: case.viewport,
                    center: case.center,
                    fade: case.fade,
                },
                dispatch: ModelDispatch::Far,
                coordinate_shift: case.coordinate_shift,
                fog_factor: case.fog_factor,
            },
            origin,
            focal_x: case.focal[0],
        };
        let quads = arena.selector67_draw_quads(ribbon, &mut input).unwrap();
        let slot = arena.selector67_texture_slot(ribbon).unwrap();
        assert_eq!(quads.len(), case.packets.len(), "{} packets", case.name);
        for (index, (quad, native)) in quads.iter().zip(&case.packets).enumerate() {
            let label = format!("{} packet {index}", case.name);
            let command = quad.command(sorted(case, quad.depth, slot), mode, None, case.far_color);
            assert_eq!(
                command.callback.native_address(),
                native.callback,
                "{label}"
            );
            assert_eq!(command.depth, native.key, "{label} key");
            assert_eq!(slot, native.texture_slot, "{label} slot");
            assert_eq!(command.payload_blocks, 4, "{label}");
            for (offset, expected) in native.payload.iter().enumerate() {
                if (0x24..0x28).contains(&offset) {
                    continue;
                }
                assert_eq!(command.payload[offset], *expected, "{label} +{offset:02X}");
            }
            packets += 1;
        }
    }
    assert_eq!((receipt.cases.len(), packets), (27, 41));
}
