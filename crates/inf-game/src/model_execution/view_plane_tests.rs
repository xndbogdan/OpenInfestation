use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    world: [i32; 3],
    eye: [i32; 3],
    vector: [i16; 3],
    basis: [f32; 9],
    shift: u8,
    command: u16,
    offset: i16,
    turned: [i32; 3],
    dot: i32,
    next_words: i32,
}

/// 004A0CE0 with its 00490900 under identity, quarter-turn, scaled and
/// random bases: the turned vector, the dot product and the next
/// instruction (three words on, or the signed jump).
#[test]
fn view_plane_branch_matches_original_004a0ce0() {
    let receipt: Receipt =
        serde_json::from_str(include_str!("../../tests/fixtures/view_plane_native.json")).unwrap();
    let mut jumps = 0;
    for case in &receipt.cases {
        let mut basis = crate::menu::MenuBasis::INITIAL;
        basis.words[1..].copy_from_slice(&case.basis.map(f32::to_bits));
        let (turned, dot, jumped) = view_plane_decision(
            case.world,
            case.eye,
            case.vector,
            &basis,
            case.shift,
            case.command,
        );
        assert_eq!(turned, case.turned, "{} turned", case.name);
        assert_eq!(dot, case.dot, "{} dot", case.name);
        let next = if jumped { i32::from(case.offset) } else { 3 };
        assert_eq!(next, case.next_words, "{} branch", case.name);
        jumps += usize::from(jumped);
    }
    assert_eq!((receipt.cases.len(), jumps), (52, 33));
}
