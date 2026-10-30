use inf_formats::{
    native_basis::NativeBasis,
    observed_model::{
        load_healthsmall_bundle, HealthSmallObservation, ObservationError, TtdPosition,
        HEALTHSMALL_RETAIL_SHA256, HEALTHSMALL_SOURCE_SHA256,
    },
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

const NAMES: [(&str, usize, usize); 8] = [
    ("model-entry-before.bin", 4696, 0),
    ("model-entry.bin", 4696, 7),
    ("f5-context.bin", 428, 1),
    ("f5-slot0.bin", 24, 1),
    ("callback-return-context.bin", 428, 3),
    ("post05-context.bin", 428, 6),
    ("post05-vertex-slots.bin", 4080, 6),
    ("post05-projection-globals.bin", 144, 6),
];
fn source() -> Option<Vec<u8>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Overlay/1.ovl");
    if !path.exists() {
        eprintln!("retail 1.ovl missing; copy the game into retail/");
        return None;
    }
    Some(fs::read(path).unwrap()[0x20eb8..0x22110].to_vec())
}
fn put(bytes: &mut [u8], offset: usize, word: u32) {
    bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
}
struct Fixture {
    path: PathBuf,
    source: Vec<u8>,
    dumps: BTreeMap<String, Vec<u8>>,
    metadata: Value,
    transcript: String,
}
impl Fixture {
    fn new() -> Option<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let source = source()?;
        let path = std::env::temp_dir().join(format!(
            "inf-healthsmall-observation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let mut context = vec![0; 428];
        put(&mut context, 0, 0x300000);
        put(&mut context, 4, 0x300ff0);
        for (i, word) in [100, 200, -300i32].iter().enumerate() {
            put(&mut context, 8 + i * 4, *word as u32);
        }
        for (i, word) in NativeBasis::INITIAL.words.iter().enumerate() {
            put(&mut context, 0x14 + i * 4, *word);
        }
        put(&mut context, 0x44, 0x400000);
        put(&mut context, 0x1a8, 0x200000);
        context[0x60] = 1;
        let mut post = context.clone();
        post[0x72..0x74].copy_from_slice(&0x6000u16.to_le_bytes());
        let mut slot0 = vec![0xcc; 24];
        slot0[..12].copy_from_slice(&context[8..20]);
        let mut slots = vec![0xcc; 4080];
        slots[..24].copy_from_slice(&slot0);
        for slot in (2..170).step_by(2) {
            put(
                &mut slots,
                slot * 24 + 12,
                slot as u32 | ((slot as u32 + 1) << 16),
            );
            put(&mut slots, slot * 24 + 16, 1000);
            slots[slot * 24 + 20] = 0;
            slots[slot * 24 + 21] = 7;
        }
        let mut globals = vec![0; 144];
        globals[..52].copy_from_slice(&post[8..60]);
        let mut before = source.clone();
        before[0x100..0x200].fill(0);
        let dumps = BTreeMap::from([
            ("model-entry-before.bin".to_owned(), before),
            ("model-entry.bin".to_owned(), source.clone()),
            ("f5-context.bin".to_owned(), context.clone()),
            ("callback-return-context.bin".to_owned(), context),
            ("post05-context.bin".to_owned(), post),
            ("f5-slot0.bin".to_owned(), slot0),
            ("post05-vertex-slots.bin".to_owned(), slots),
            ("post05-projection-globals.bin".to_owned(), globals),
        ]);
        let mut result = Self {
            path,
            source,
            dumps,
            metadata: Value::Null,
            transcript: String::new(),
        };
        result.rebuild();
        Some(result)
    }
    fn rebuild(&mut self) {
        let stages = [
            "entry", "f5", "call", "return", "write", "op05", "post05", "end",
        ];
        let mut events = Vec::new();
        self.transcript =
            "0:000> .echo INF_TTD_TARGET_PE_VALIDATED\nINF_TTD_TARGET_PE_VALIDATED\n".into();
        for (index, stage) in stages.into_iter().enumerate() {
            let args = match index {
                0 | 7 => 0,
                5 | 6 => 0x200420,
                _ => 0x200418,
            };
            let extra = match index {
                0 => " parent=00000000 caller=004a00c6", 1|6 => " near_far=1 vertices=00300000",
                2 => " object=00400000 vtable=00520a44 target=004789b0 selector=0 f5_args=00200418 return_site=004a03b0",
                3 => " value=06b0", 4 => " register1=6000", _=>"",
            };
            let fields = format!(
                "stage={stage} thread=2a ctx=00001000 entry=00200000 args={args:08x}{extra}"
            );
            self.transcript.push_str(&format!(
                "INF_HEALTH_EVENT {fields}  \nCurrent position: A:{}\n",
                index + 1
            ));
            let details: BTreeMap<_, _> = fields
                .split_whitespace()
                .map(|field| {
                    let (k, v) = field.split_once('=').unwrap();
                    (k.to_owned(), v.to_owned())
                })
                .collect();
            let mut event = json!({"stage":stage,"position":format!("A:{}",index+1),"thread":42,"ctx":4096,"entry":0x200000,"args":args,"details":details});
            for key in match index {
                2 => vec![
                    "object",
                    "vtable",
                    "target",
                    "selector",
                    "f5_args",
                    "return_site",
                ],
                3 => vec!["value"],
                4 => vec!["register1"],
                _ => vec![],
            } {
                event[key] = json!(u32::from_str_radix(details[key].as_str(), 16).unwrap());
            }
            events.push(event);
            for (name, length, producing_event) in NAMES {
                if producing_event != index {
                    continue;
                }
                let address = address(name);
                self.transcript.push_str(&format!(
                    "INF_HEALTH_DUMP name={name} address={address:08x} bytes={length} \n"
                ));
                for (row, chunk) in self.dumps[name].chunks(16).enumerate() {
                    self.transcript
                        .push_str(&format!("{:08x}  ", address + row as u32 * 16));
                    for column in 0..16 {
                        if let Some(byte) = chunk.get(column) {
                            self.transcript.push_str(&format!("{byte:02x}"));
                        } else {
                            self.transcript.push_str("  ");
                        }
                        self.transcript.push(if column == 7 && chunk.len() > 8 {
                            '-'
                        } else {
                            ' '
                        });
                    }
                    self.transcript.push_str(" de ad be ef ????\n");
                }
                self.transcript.push_str("INF_HEALTH_DUMP_END\n");
            }
        }
        self.transcript.push_str("INF_HEALTH_COMPLETE\nINF_HEALTH_STOP scans=1 matches=1 stage=8\nINF_TTD_QUERY_COMPLETE\n");
        let dump_lengths: BTreeMap<_, _> = NAMES
            .into_iter()
            .map(|(name, len, _)| (name, len))
            .collect();
        self.metadata = json!({"schema_version":1,"preset":"HealthSmallDraw","status":"completed",
            "trace":"C:/evidence/trace.run","trace_bytes":100,"index_bytes":20,"retail_exe":"C:/evidence/Infest.exe","retail_sha256":HEALTHSMALL_RETAIL_SHA256,
            "start_position":"9:FFFF","max_hits":1,"timeout_seconds":45,"hit_limit_reached":false,
            "observation":{"complete":true,"scans":1,"matches":1,"stage":8,"events":events},
            "subject":{"OverlayOffset":0x20eb8,"EntryBytes":4696,"Sha256":HEALTHSMALL_SOURCE_SHA256,"F5ArgumentsOffset":0x418,"Op05ArgumentsOffset":0x420,"VertexSlots":170,"Dumps":dump_lengths},
            "stop_reason":"correlated_draw_end","entry_sha256":HEALTHSMALL_SOURCE_SHA256,"entry_matches_source":true,"identity_status":"full_source_match","observed_hits":1,"last_observed_position":"A:8"});
    }
    fn write(&self) {
        fs::write(
            self.path.join("invocation.json"),
            serde_json::to_vec(&self.metadata).unwrap(),
        )
        .unwrap();
        fs::write(self.path.join("transcript.txt"), &self.transcript).unwrap();
        for (name, bytes) in &self.dumps {
            fs::write(self.path.join(name), bytes).unwrap();
        }
    }
    fn load(&self) -> Result<HealthSmallObservation, ObservationError> {
        self.write();
        load_healthsmall_bundle(&self.path, &self.source)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.path.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("inf-healthsmall-observation-"));
        fs::remove_dir_all(&self.path).unwrap();
    }
}
fn address(name: &str) -> u32 {
    match name {
        "model-entry.bin" | "model-entry-before.bin" => 0x200000,
        "f5-slot0.bin" | "post05-vertex-slots.bin" => 0x300000,
        "post05-projection-globals.bin" => 0x55f1c4,
        _ => 0x1000,
    }
}

#[test]
fn imports_source_bound_geometry_without_promoting_unwritten_world_fields() {
    let Some(mut f) = Fixture::new() else { return };
    f.transcript = f.transcript.replace(
        "INF_HEALTH_DUMP name=post05-context.bin",
        "Time Travel Position: A:7\nINF_HEALTH_DUMP name=post05-context.bin",
    );
    let observed = f.load().unwrap();
    assert_eq!(observed.source_model.identity.local_index, 166);
    assert_eq!(observed.source_model.identity.global_index, 168);
    assert_eq!(observed.source_model.span.start, 0);
    assert_eq!(observed.provenance.source_span.start, 0x20eb8);
    assert_eq!(observed.projection_slots.len(), 84);
    assert_eq!(observed.projection_slots[0].slot, 2);
    assert_eq!(observed.projection_slots[83].slot, 168);
    assert_eq!(observed.projection_slots[0].raw[..12], [0xcc; 12]);
    assert_eq!(observed.projection_slots[0].fog, 7);
    assert_eq!(observed.slot0_world, [100, 200, -300]);
    assert_eq!(observed.callback.result, 0x06b0);
    assert_eq!(observed.callback.register1, 0x6000);
    assert_ne!(
        observed.snapshots.model_entry_before,
        observed.snapshots.model_entry
    );
}
#[test]
fn metadata_completion_is_not_trusted() {
    let Some(mut f) = Fixture::new() else { return };
    for (pointer, value) in [
        ("/status", json!("partial")),
        ("/observation/complete", json!(false)),
        ("/identity_status", json!("deferred_until_draw_completion")),
        ("/retail_sha256", json!("bad")),
        ("/observed_hits", json!(2)),
        ("/observation/events/2/target", json!(0)),
        ("/observation/events/2/details/target", json!("00400000")),
        ("/subject/EntryBytes", json!(4695)),
    ] {
        f.rebuild();
        *f.metadata.pointer_mut(pointer).unwrap() = value;
        assert!(f.load().is_err(), "{pointer}");
    }
}
#[test]
fn rejects_incomplete_reordered_reused_or_unpositioned_transcripts() {
    let Some(mut f) = Fixture::new() else { return };
    for (from, to) in [
        ("stage=end thread", "stage=post05 thread"),
        ("Current position: A:8", "Current position: A:7"),
        ("stage=call thread=2a", "stage=call thread=2b"),
        ("return_site=004a03b0", "return_site=004a11ac"),
        ("value=06b0", "value=06b1"),
        ("register1=6000", "register1=1234"),
        ("Current position: A:4", "not a position"),
        ("INF_HEALTH_COMPLETE", "INF_HEALTH_INTERRUPTED"),
        ("INF_TTD_QUERY_COMPLETE", ""),
        ("INF_HEALTH_EVENT", " INF_HEALTH_EVENT"),
        ("INF_HEALTH_DUMP ", " INF_HEALTH_DUMP "),
        ("INF_HEALTH_STOP ", " INF_HEALTH_STOP "),
        (
            "INF_HEALTH_DUMP name=post05-context.bin",
            "Current position: A:FF\nINF_HEALTH_DUMP name=post05-context.bin",
        ),
        (
            "INF_HEALTH_DUMP name=post05-context.bin",
            "Time Travel Position: A:FF\nINF_HEALTH_DUMP name=post05-context.bin",
        ),
        (
            "INF_HEALTH_DUMP name=post05-context.bin",
            " Current position: A:FF\nINF_HEALTH_DUMP name=post05-context.bin",
        ),
        (
            "INF_HEALTH_DUMP name=post05-context.bin",
            " Time Travel Position: A:FF\nINF_HEALTH_DUMP name=post05-context.bin",
        ),
        ("Current position: A:4", "Time Travel Position: A:4"),
        (
            "INF_TTD_QUERY_COMPLETE",
            "Current position: B:1\nINF_TTD_QUERY_COMPLETE",
        ),
    ] {
        f.rebuild();
        f.transcript = f.transcript.replace(from, to);
        assert!(f.load().is_err(), "{from}");
    }
}
#[test]
fn rejects_malformed_missing_and_unbound_raw_dumps() {
    let Some(mut f) = Fixture::new() else { return };
    for (from, to) in [
        ("address=00300000 bytes=24", "address=00300001 bytes=24"),
        ("name=f5-slot0.bin", "name=unknown.bin"),
        ("bytes=24", "bytes=25"),
        ("00300010  ", "00300011  "),
        ("c8 00 00 00", "?? 00 00 00"),
        ("INF_HEALTH_DUMP_END", "not an end"),
    ] {
        f.rebuild();
        f.transcript = f.transcript.replacen(from, to, 1);
        assert!(f.load().is_err(), "{from}");
    }
    f.rebuild();
    f.write();
    fs::remove_file(f.path.join("f5-slot0.bin")).unwrap();
    assert!(load_healthsmall_bundle(&f.path, &f.source).is_err());
    f.write();
    fs::write(f.path.join("f5-slot0.bin"), [0; 24]).unwrap();
    assert!(load_healthsmall_bundle(&f.path, &f.source).is_err());
}
#[test]
fn rejects_literal_but_invalid_context_or_projection_data() {
    let Some(mut f) = Fixture::new() else { return };
    for (name, offset, value) in [
        ("f5-context.bin", 0x18, 0x7fc00000),
        ("post05-context.bin", 0x18, 0x7f800000),
        ("callback-return-context.bin", 0x1a8, 0),
        ("post05-context.bin", 0, 0),
        ("f5-slot0.bin", 0, 101),
        ("post05-projection-globals.bin", 0, 101),
        ("post05-vertex-slots.bin", 2 * 24 + 16, 63),
        ("post05-vertex-slots.bin", 2 * 24 + 16, 0xffffffff),
    ] {
        let old = f.dumps[name].clone();
        put(f.dumps.get_mut(name).unwrap(), offset, value);
        f.rebuild();
        assert!(f.load().is_err(), "{name}+{offset:x}");
        f.dumps.insert(name.to_owned(), old);
    }
}
#[test]
fn source_and_file_limits_are_strict() {
    let Some(mut f) = Fixture::new() else { return };
    f.write();
    let mut bad = f.source.clone();
    bad[0x500] ^= 1;
    assert!(load_healthsmall_bundle(&f.path, &bad).is_err());
    fs::write(f.path.join("model-entry.bin"), vec![0; 4697]).unwrap();
    assert!(matches!(
        load_healthsmall_bundle(&f.path, &f.source),
        Err(ObservationError::TooLarge { .. })
    ));
    f.write();
    fs::write(f.path.join("invocation.json"), vec![b' '; 256 * 1024 + 1]).unwrap();
    assert!(matches!(
        load_healthsmall_bundle(&f.path, &f.source),
        Err(ObservationError::TooLarge { .. })
    ));
    f.rebuild();
    f.transcript.push_str(&" ".repeat(2 * 1024 * 1024));
    assert!(matches!(f.load(), Err(ObservationError::TooLarge { .. })));
}
#[test]
fn numerical_positions_are_unsigned_and_strict() {
    assert!("9:FFFFFFFFFFFFFFFF".parse::<TtdPosition>().unwrap() < "A:0".parse().unwrap());
    assert_eq!(
        "FFFFFFFFFFFFFFFF:1"
            .parse::<TtdPosition>()
            .unwrap()
            .to_string(),
        "FFFFFFFFFFFFFFFF:1"
    );
    for bad in ["0", "0x10:1", "1:", "1:2:3", "10000000000000000:0"] {
        assert!(bad.parse::<TtdPosition>().is_err());
    }
}
#[test]
fn accepted_local_bundles_remain_distinct_observations() {
    let Some(source) = source() else { return };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/infestation-ttd");
    let paths = [
        root.join("20260923-102834-171-HealthSmallDraw"),
        root.join("20260923-103318-490-HealthSmallDraw"),
    ];
    if paths.iter().any(|p| !p.exists()) {
        eprintln!("local accepted TTD bundles absent; synthetic corpus-backed importer fixtures still run");
        return;
    }
    let first = load_healthsmall_bundle(&paths[0], &source).unwrap();
    let second = load_healthsmall_bundle(&paths[1], &source).unwrap();
    assert_eq!(
        (first.callback.result, second.callback.result),
        (0x6b0, 0x6b6)
    );
    assert_eq!(
        (first.callback.register1, second.callback.register1),
        (0x6000, 0x6c00)
    );
    assert_ne!(first.nested.basis, second.nested.basis);
    assert!(first.events[7].position < second.events[0].position);
    assert!(first
        .projection_slots
        .iter()
        .zip(second.projection_slots)
        .all(|(a, b)| a.raw[12..22] != b.raw[12..22]));
}
