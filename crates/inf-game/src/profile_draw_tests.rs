use super::*;
use crate::{
    menu::{MenuBasis, MenuDrawPlacement, MenuRowDescriptor, MenuSource},
    profile_menu::{ProfileMenu, ProfileMenuTemplates},
};
use inf_formats::{
    encode_save_records, ByteSpan, Font, FontMode, GlyphMetrics, SaveRecord, SAVE_PAYLOAD_BYTES,
};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

struct Resources {
    font: Font,
    strings: BTreeMap<i32, Vec<u8>>,
}
impl Resources {
    fn new() -> Self {
        let mut strings = BTreeMap::new();
        for label in 0..=0x110 {
            strings.insert(label, format!("label-{label:x}").into_bytes());
        }
        strings.insert(0x10a, vec![0xe9, b'x']);
        strings.insert(0x10b, b"No".to_vec());
        Self {
            strings,
            font: Font {
                local_index: 0,
                header_span: ByteSpan::new(0, 64),
                raw_header: [0; 16],
                mode: FontMode::Direct,
                coordinate_divisor: 1,
                line_step_terms: [10, 2],
                maximum_byte: 255,
                fallback_byte: b'?',
                metric_span: ByteSpan::new(64, 2112),
                sprite_span: ByteSpan::new(2112, 3136),
                metrics: vec![
                    GlyphMetrics {
                        advance: 6,
                        spacing: 0,
                        offset: [0, 0]
                    };
                    256
                ],
                sprite_words: (1..=256).collect(),
            },
        }
    }
}
impl MenuTextResources for Resources {
    type Error = &'static str;
    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.strings
            .get(&argument)
            .map(Vec::as_slice)
            .ok_or("H0 missing")
    }
    fn font(&self, _: i32) -> Result<&Font, Self::Error> {
        Ok(&self.font)
    }
    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        Ok(if argument == 3 {
            0xee60_4020
        } else {
            0xdd20_6080
        })
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        unreachable!()
    }
}
#[derive(Default)]
struct Host {
    rows: Vec<(Vec<u8>, MenuRowDescriptor)>,
    raw: Vec<(Vec<u8>, [i32; 2], i32, MenuColor, [i32; 2])>,
    clips: Vec<[i16; 4]>,
}
impl ProfileTextHost for Host {
    type Error = &'static str;
    fn row_text(
        &mut self,
        request: MenuRowTextRequest<'_>,
    ) -> Result<MenuTextOutcome, Self::Error> {
        assert_eq!(request.placement, MenuTextPlacement::Left);
        self.rows
            .push((terminated(request.text).to_vec(), *request.descriptor));
        // Nontrivial narrow advance and Y write test callback chaining.
        request.descriptor.position_pixels[1] =
            request.descriptor.position_pixels[1].wrapping_add(2);
        Ok(MenuTextOutcome {
            advance_from_origin: 0x10007,
        })
    }
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        assert_eq!(request.emission, MenuTextEmission::Draw);
        assert_eq!(request.reveal, MenuTextReveal::Complete);
        self.raw.push((
            request.text.to_vec(),
            request.position_pixels,
            request.width_pixels,
            request.color,
            request.context_origin_pixels,
        ));
        Ok(())
    }
    fn set_clip_rect(&mut self, clip: [i16; 4]) -> Result<(), Self::Error> {
        self.clips.push(clip);
        Ok(())
    }
    fn published_origin_pixels(&self) -> [i32; 2] {
        [123, -456]
    }
}

fn owners() -> (ProfileMenu, ProfileStore) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let templates =
        ProfileMenuTemplates::parse(&fs::read(root.join("../../retail/Infest.exe")).unwrap())
            .unwrap();
    let dir = root.join(format!(
        "../../.tmp/profile-draw-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut payload = [0; SAVE_PAYLOAD_BYTES];
    payload[..4].copy_from_slice(&[b'A', 0xfe, b'Z', 0]);
    payload[4..8].copy_from_slice(&2u32.to_le_bytes());
    payload[8..12].copy_from_slice(&7384u32.to_le_bytes());
    let bytes = encode_save_records(&[SaveRecord::from_payload(payload).unwrap()]).unwrap();
    fs::write(dir.join("sidecar.sav"), bytes).unwrap();
    (
        ProfileMenu::new(templates, MenuBasis::INITIAL, false),
        ProfileStore::open(dir.join("sidecar.sav")).unwrap(),
    )
}
fn request(callback: u32) -> MenuRowDrawRequest {
    MenuRowDrawRequest {
        menu_source: MenuSource::Derived {
            constructor: 0x448220,
            vtable: 0x524f28,
        },
        layout_index: 0,
        item_index: 0,
        callback_address: callback,
        arguments: [0, 0],
        context_origin_pixels: [320, 240],
        placement: MenuDrawPlacement::Linear,
        descriptor: MenuRowDescriptor {
            position_pixels: [10, 20],
            extent_pixels: [300, 96],
            reveal: u16::MAX,
            flags: 0x40,
            age_millis: 1200,
            color: MenuColor::from_rgb([1, 2, 3]),
        },
    }
}
fn draw(
    menu: &mut ProfileMenu,
    store: &mut ProfileStore,
    row: &mut MenuRowDrawRequest,
    owner: bool,
    resources: &Resources,
    host: &mut Host,
) {
    let (base, state) = menu.split_mut();
    draw_profile_row(
        ProfileRowDraw {
            request: row,
            base,
            state,
            store,
            owner_flag: owner,
            surface_pixels: [640, 480],
        },
        resources,
        host,
    )
    .unwrap();
}

#[test]
fn slot_formats_saved_bytes_mission_and_elapsed_hours_minutes() {
    let (mut menu, mut store) = owners();
    let resources = Resources::new();
    let mut host = Host::default();
    let mut row = request(0x449320);
    draw(
        &mut menu, &mut store, &mut row, false, &resources, &mut host,
    );
    assert_eq!(host.rows[0].0, b"1. A\xfeZ - label-e2 02:03");
    assert_eq!(host.rows[0].1.color, MenuColor::from_rgb([1, 2, 3]));
    row.arguments[0] = 13;
    draw(
        &mut menu, &mut store, &mut row, false, &resources, &mut host,
    );
    assert_eq!(host.rows[1].0, b"14. label-d1");
}

#[test]
fn header_mutates_descriptor_then_confirmation_chains_signed_word_advances() {
    let (mut menu, mut store) = owners();
    menu.base_mut().set_navigation_locked(true);
    let resources = Resources::new();
    let mut host = Host::default();
    let mut row = request(0x448e60);
    draw(
        &mut menu, &mut store, &mut row, false, &resources, &mut host,
    );
    assert_eq!(
        host.rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        vec![
            b"label-cd".to_vec(),
            b"label-d0".to_vec(),
            b"label-cf".to_vec(),
            b"label-ce".to_vec(),
            b" (".to_vec(),
            vec![0xe9],
            b"/".to_vec(),
            b"N".to_vec(),
            b")".to_vec()
        ]
    );
    assert_eq!(
        host.rows[..3]
            .iter()
            .map(|r| r.1.position_pixels[0])
            .collect::<Vec<_>>(),
        [10, 110, 200]
    );
    assert_eq!(
        host.rows[..3]
            .iter()
            .map(|r| r.1.age_millis)
            .collect::<Vec<_>>(),
        [1200, 900, 600]
    );
    assert_eq!(host.rows[3].1.position_pixels, [10, 47]);
    assert_eq!(
        host.rows[4..]
            .iter()
            .map(|r| r.1.position_pixels[0])
            .collect::<Vec<_>>(),
        [17, 24, 31, 38, 45]
    );
    assert_eq!(host.rows[5].1.color, MenuColor::from_word(0xdd206080));
    assert_eq!(host.rows[7].1.color, MenuColor::from_word(0xee604020));
}

#[test]
fn format_initial_state_writes_three_but_delays_help_until_next_draw() {
    let (mut menu, mut store) = owners();
    let resources = Resources::new();
    let mut host = Host::default();
    let mut row = request(0x448d10);
    draw(
        &mut menu, &mut store, &mut row, false, &resources, &mut host,
    );
    assert_eq!(menu.state().format_state(), 3);
    assert_eq!(host.rows.len(), 1);
    assert_eq!(host.rows[0].0, b"label-9d");
    draw(
        &mut menu, &mut store, &mut row, false, &resources, &mut host,
    );
    assert_eq!(host.rows.len(), 3);
    assert_eq!(host.rows[2].0, b"label-28");
    assert_eq!(host.rows[2].1.position_pixels, [10, 48]);
}

#[test]
fn owner_footer_is_raw_clipped_and_present_even_without_confirmation() {
    let (mut menu, mut store) = owners();
    let resources = Resources::new();
    let mut host = Host::default();
    let mut row = request(0x448e60);
    draw(&mut menu, &mut store, &mut row, true, &resources, &mut host);
    assert_eq!(host.rows.len(), 1);
    assert_eq!(host.rows[0].0, b"label-90");
    assert_eq!(host.clips, [[0, 0, 640, 480]]);
    assert_eq!(host.raw[0].0, b"label-28");
    assert_eq!(host.raw[0].1, [64, 420]);
    assert_eq!(host.raw[0].2, 512);
    assert_eq!(host.raw[0].4, [123, -456]);
}

#[test]
fn status_timer_measures_signed_word_width_and_preserves_unknown_high_color() {
    let mut resources = Resources::new();
    resources
        .font
        .metrics
        .iter_mut()
        .for_each(|m| m.advance = 10_000);
    for (kind, label) in [
        (ProfileTimerKind::LoadFailure, 0xc6),
        (ProfileTimerKind::CopyFailure, 0xc8),
        (ProfileTimerKind::InitialWarning, 0xc7),
        (ProfileTimerKind::DeleteFailure, 0xc9),
    ] {
        let mut host = Host::default();
        draw_profile_status(
            ProfileTimer { kind, millis: 200 },
            [640, 480],
            &resources,
            &mut host,
        )
        .unwrap();
        assert!(host.rows.is_empty());
        assert_eq!(host.raw[0].0, resources.strings[&label]);
        assert_eq!(
            host.raw[0].1,
            [(640 - i32::from(80_000i32 as i16)) / 2, 331]
        );
        assert_eq!(host.raw[0].3, MenuColor::from_rgb([80, 80, 64]));
        assert_eq!(host.raw[0].4, [123, -456]);
    }
}
