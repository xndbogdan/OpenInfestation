use super::*;

fn executable_bytes() -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    assets.executable_bytes().to_vec()
}

/// The receipt's H6 style words and H0 strings.
fn style(slot: u32) -> u32 {
    slot.wrapping_mul(0x9E37_79B1).wrapping_add(0x7F4A_7C15)
}

/// `check_credits.py`: 0042AED0 run natively over 72 surfaces and ages,
/// with every H0 slot a synthetic string, `004A4240` answering
/// `len * mul + add` and `004A3AB0` recording each line.
#[test]
fn credits_roll_matches_native_0042aed0() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/credits_native.json"
    )))
    .unwrap();
    let bytes = executable_bytes();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let credits = FinalCredits::load(executable, b"Tester".to_vec()).unwrap();
    let lines = &credits.lines;
    assert_eq!(lines.len(), 143);
    let h0: Vec<Vec<u8>> = (0..0x400)
        .map(|slot| format!("S{slot:03X}").into_bytes())
        .collect();
    let mut checked = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let number = |value: &serde_json::Value| value.as_i64().unwrap();
        let surface = [
            number(&input["surface"][0]) as i32,
            number(&input["surface"][1]) as i32,
        ];
        let (mul, add) = (number(&input["width"][0]), number(&input["width"][1]));
        let player = input["player_line"].as_str().unwrap().as_bytes().to_vec();
        let text = |text: CreditText| -> Option<&[u8]> {
            match text {
                CreditText::H0(slot) => h0.get(usize::try_from(slot).ok()?).map(Vec::as_slice),
                CreditText::Literal(address) => credits.literal(address),
                CreditText::PlayerLine => Some(&player),
                CreditText::Blank => None,
            }
        };
        let draws = roll_credits(
            lines,
            text,
            |bytes| Ok::<_, CreditsError>((bytes.len() as i64 * mul + add) as i16),
            surface,
            number(&input["age"]) as u32,
            [style(4), style(5)],
        )
        .unwrap();
        let ops = case["ops"].as_array().unwrap();
        assert_eq!(draws.len(), ops.len(), "{input}");
        for (draw, op) in draws.iter().zip(ops) {
            let bytes = text(lines[draw.line].text).unwrap();
            let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            assert_eq!(hex, op["text"].as_str().unwrap(), "{input}");
            assert_eq!(
                [draw.x as i64, draw.y as i64, i64::from(draw.color)],
                [number(&op["x"]), number(&op["y"]), number(&op["color"])],
                "{input} line {}",
                draw.line
            );
            assert_eq!(number(&op["wrap"]), i64::from(surface[0]));
            assert_eq!(number(&op["flags"]), i64::from(TEXT_FLAGS));
            checked += 1;
        }
    }
    assert_eq!(checked, 330);
}
