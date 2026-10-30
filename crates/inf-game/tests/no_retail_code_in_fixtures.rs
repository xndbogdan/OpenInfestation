//! The fixtures are public: a code guard (bytes beside a sha256) keeps
//! its length and hash, never more than a short instruction's retail bytes.

#[test]
fn no_fixture_carries_retail_code_bytes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let long_guard = |text: &str| {
        text.match_indices("\"bytes\":").any(|(at, key)| {
            let rest = text[at + key.len()..].trim_start();
            let Some(value) = rest.strip_prefix('"') else {
                return false;
            };
            let hex = value.bytes().take_while(u8::is_ascii_hexdigit).count();
            let after = value[hex..].strip_prefix('"').unwrap_or("").trim_start();
            let after = after.strip_prefix(',').unwrap_or("").trim_start();
            hex > 32 && after.starts_with("\"sha256\"")
        })
    };
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json")
            && long_guard(&std::fs::read_to_string(&path).unwrap())
        {
            found.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        found.is_empty(),
        "retail code bytes in public fixtures: {found:?}"
    );
}
