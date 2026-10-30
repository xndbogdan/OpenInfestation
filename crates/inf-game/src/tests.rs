use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use inf_formats::{HandlerFraming, HandlerRequest, RequestKind, ResourceSegment, HANDLER_COUNT};

use crate::{Error, HandlerResources, Installation, Runtime, SceneContext};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(leaves: &[(u8, [Vec<(usize, HandlerRequest)>; 4])]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-game-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut bytes = vec![3, 2];
        for _ in 0..HANDLER_COUNT {
            bytes.extend_from_slice(&8u16.to_le_bytes());
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.push(1); // node allocation count (not host memory ownership)
        bytes.push(leaves.len() as u8);
        for (level, tables) in leaves {
            bytes.push(0); // leaf discriminator
            bytes.push(*level);
            for table in tables {
                for handler in 0..HANDLER_COUNT {
                    let request = table.iter().find(|(id, _)| *id == handler);
                    if let Some((_, request)) = request {
                        bytes.extend_from_slice(&request.count.to_le_bytes());
                        bytes.extend_from_slice(&(request.segments.len() as u16).to_le_bytes());
                        for segment in &request.segments {
                            bytes.extend_from_slice(&segment.packed().to_le_bytes());
                        }
                    } else {
                        bytes.extend_from_slice(&[0; 4]);
                    }
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), bytes).unwrap();
        Self { root }
    }

    fn overlay(&self, name: &str, bytes: &[u8]) {
        fs::write(self.root.join("Overlay").join(name), bytes).unwrap();
    }

    fn installation(&self) -> Installation {
        Installation::open(&self.root).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn request(count: i16, segments: &[(u16, i16)]) -> HandlerRequest {
    HandlerRequest {
        count,
        segments: segments
            .iter()
            .map(|&(local_start, global_delta)| ResourceSegment {
                local_start,
                global_delta,
            })
            .collect(),
    }
}

fn standard(payload: &[u8]) -> Vec<u8> {
    let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn only_active_phases_are_required_or_read() {
    let fixture = Fixture::new(&[(
        7,
        [vec![(0, request(1, &[(0, 0)]))], vec![], vec![], vec![]],
    )]);
    fixture.overlay("7.ovl", &standard(b"base\0"));
    // An inactive phase may be absent or contain arbitrary bytes. It is not
    // opened and cannot accidentally register resources from another context.
    fixture.overlay("7l9.ovl", b"not an overlay");
    let scene = fixture
        .installation()
        .prepare_scene(SceneContext::new(7, 8, 9))
        .unwrap();
    assert_eq!(scene.overlays().len(), 1);
    assert_eq!(scene.overlays()[0].key().request_kind(), RequestKind::Base);
    assert_eq!(scene.summary().registrations, 1);
    assert_eq!(scene.summary().active_slots, 1);
    assert!(scene.resource(0, 1).is_none());
    assert!(scene.resource(16, 0).is_none());
    assert!(scene.resource(0, u16::MAX).is_none());
}

#[test]
fn unknown_scene_is_distinct_from_a_missing_required_file() {
    let fixture = Fixture::new(&[(
        7,
        [vec![(0, request(1, &[(0, 0)]))], vec![], vec![], vec![]],
    )]);
    let installation = fixture.installation();
    assert!(matches!(
        installation.prepare_scene(SceneContext::new(44, 0, 0)),
        Err(Error::UnknownScene { level: 44 })
    ));
    let error = installation
        .prepare_scene(SceneContext::new(7, 0, 0))
        .unwrap_err();
    let Error::Read { path, source } = error else {
        panic!("expected missing required file");
    };
    assert!(path.ends_with("7.ovl"));
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn aliases_and_later_phases_preserve_the_complete_overwrite_chain() {
    let fixture = Fixture::new(&[(
        7,
        [
            vec![(0, request(3, &[(0, 0), (2, -2)]))],
            vec![(0, request(1, &[(0, 0)]))],
            vec![(0, request(1, &[(0, 0)]))],
            vec![(0, request(1, &[(0, 0)]))],
        ],
    )]);
    fixture.overlay("7.ovl", &standard(b"first\0unreplaced\0alias\0"));
    fixture.overlay("7d3.ovl", &standard(b"difficulty\0"));
    fixture.overlay("7l2.ovl", &standard(b"language\0"));
    fixture.overlay("7d3l2.ovl", &standard(b"combined\0"));
    let scene = fixture
        .installation()
        .prepare_scene(SceneContext::new(7, 3, 2))
        .unwrap();
    assert_eq!(
        scene
            .overlays()
            .iter()
            .map(|ovl| ovl.key().request_kind())
            .collect::<Vec<_>>(),
        [
            RequestKind::Base,
            RequestKind::Difficulty,
            RequestKind::Language,
            RequestKind::DifficultyLanguage
        ]
    );
    assert_eq!(
        scene
            .registrations()
            .iter()
            .map(|entry| entry.replaces)
            .collect::<Vec<_>>(),
        [None, None, Some(0), Some(2), Some(3), Some(4)]
    );
    let winner = scene.resource(0, 0).unwrap();
    assert_eq!(winner.overlay_index, 3);
    assert_eq!(winner.local_index, 0);
    assert_eq!(scene.resource(0, 1).unwrap().overlay_index, 0);
    let overlay = &scene.overlays()[winner.overlay_index];
    let (_, HandlerResources::Strings(bank)) = overlay.handler(0).unwrap() else {
        panic!("canonical string bank required");
    };
    assert_eq!(
        bank.entries[winner.local_index as usize].raw(overlay.bytes()),
        Some(b"combined".as_slice())
    );
    let summary = scene.summary();
    assert_eq!(summary.registrations, 6);
    assert_eq!(summary.active_slots, 2);
    assert_eq!(summary.handlers[0].registrations, 6);
    assert_eq!(summary.handlers[0].active_slots, 2);
    assert_eq!(
        serde_json::to_value(summary).unwrap()["scope"],
        "independent_scene"
    );
}

#[test]
fn failures_in_later_phases_leave_the_previous_scene_intact() {
    let fixture = Fixture::new(&[
        (
            7,
            [vec![(0, request(1, &[(0, 0)]))], vec![], vec![], vec![]],
        ),
        (
            8,
            [
                vec![(0, request(1, &[(0, 1)]))],
                vec![],
                vec![(0, request(1, &[(0, 1)]))],
                vec![],
            ],
        ),
    ]);
    fixture.overlay("7.ovl", &standard(b"previous\0"));
    fixture.overlay("8.ovl", &standard(b"prepared\0"));
    let mut runtime = Runtime::new(fixture.installation());
    assert!(runtime.scene().is_none());
    runtime.load_scene(SceneContext::new(7, 0, 0)).unwrap();
    let previous = runtime.scene().unwrap().summary();
    assert!(runtime.load_scene(SceneContext::new(8, 0, 0)).is_err());
    assert_eq!(runtime.scene().unwrap().summary(), previous);
    fixture.overlay("8l0.ovl", &standard(b"unterminated"));
    assert!(matches!(
        runtime.load_scene(SceneContext::new(8, 0, 0)),
        Err(Error::Handler { handler: 0, .. })
    ));
    assert_eq!(runtime.scene().unwrap().summary(), previous);
    fixture.overlay("8l0.ovl", &standard(b"committed\0"));
    runtime.load_scene(SceneContext::new(8, 0, 0)).unwrap();
    assert_eq!(runtime.scene().unwrap().context().level, 8);
    assert!(runtime.scene().unwrap().resource(0, 0).is_none());
    assert!(runtime.scene().unwrap().resource(0, 1).is_some());
}

#[test]
fn installation_rejects_invalid_registration_plans() {
    for bad in [
        request(-1, &[(0, 0)]),
        request(1, &[]),
        request(1, &[(1, 0)]),
        request(1, &[(0, -1)]),
        request(1, &[(0, 8)]),
    ] {
        let fixture = Fixture::new(&[(7, [vec![(0, bad)], vec![], vec![], vec![]])]);
        assert!(matches!(
            Installation::open(&fixture.root),
            Err(Error::NegativeCount { .. }
                | Error::InvalidMapping { .. }
                | Error::Capacity { .. })
        ));
    }
    let fixture = Fixture::new(&[
        (7, [vec![], vec![], vec![], vec![]]),
        (7, [vec![], vec![], vec![], vec![]]),
    ]);
    assert!(matches!(
        Installation::open(&fixture.root),
        Err(Error::DuplicateScene { level: 7 })
    ));
}

#[test]
fn corrupt_preload_framing_and_typed_payloads_fail_closed() {
    let fixture = Fixture::new(&[(
        7,
        [vec![(1, request(1, &[(0, 0)]))], vec![], vec![], vec![]],
    )]);
    let installation = fixture.installation();
    fixture.overlay("7.ovl", &standard(&[1, 2]));
    assert!(matches!(
        installation.prepare_scene(SceneContext::new(7, 0, 0)),
        Err(Error::Handler { handler: 1, .. })
    ));
    fixture.overlay("7.ovl", &[4, 0, 0, 0, 1]);
    assert!(matches!(
        installation.prepare_scene(SceneContext::new(7, 0, 0)),
        Err(Error::Format { .. })
    ));
    let mut trailing = standard(&[1, 2, 3, 4]);
    trailing.push(9);
    fixture.overlay("7.ovl", &trailing);
    assert!(matches!(
        installation.prepare_scene(SceneContext::new(7, 0, 0)),
        Err(Error::Format { .. })
    ));
    fs::write(fixture.root.join("PreLoad.dat"), [0]).unwrap();
    assert!(matches!(
        Installation::open(&fixture.root),
        Err(Error::Format { .. })
    ));
}

#[test]
fn opaque_handler_bytes_and_relocation_spans_remain_unmodified() {
    let fixture = Fixture::new(&[(
        7,
        [
            vec![(13, request(1, &[(0, 0)])), (14, request(1, &[(0, 0)]))],
            vec![],
            vec![],
            vec![],
        ],
    )]);
    let mut bytes = Vec::new();
    for _ in 0..2 {
        bytes.extend(standard(&4u32.to_le_bytes())); // raw relative word, not host pointer
        bytes.extend_from_slice(&1u32.to_le_bytes()); // one relocation
        bytes.extend_from_slice(&0u32.to_le_bytes()); // relocation position
        bytes.extend_from_slice(&0u32.to_le_bytes()); // one entry offset
    }
    fixture.overlay("7.ovl", &bytes);
    let scene = fixture
        .installation()
        .prepare_scene(SceneContext::new(7, 0, 0))
        .unwrap();
    let overlay = &scene.overlays()[0];
    assert_eq!(overlay.bytes(), bytes);
    assert_eq!(overlay.requests().0[13].count, 1);
    assert_eq!(overlay.layout().covered_bytes(), bytes.len());
    for handler in [13, 14] {
        let (region, data) = overlay.handler(handler).unwrap();
        assert!(data.is_opaque());
        assert_eq!(region.requested_count, 1);
        assert!(scene.summary().handlers[handler as usize].opaque);
    }
    let (region, _) = overlay.handler(13).unwrap();
    let HandlerFraming::RelocationTail(tail) = &region.framing else {
        panic!("relocation framing");
    };
    assert_eq!(tail.relocation_count, 1);
    assert_eq!(
        tail.main_payload.get(overlay.bytes()).unwrap(),
        4u32.to_le_bytes()
    );
}
