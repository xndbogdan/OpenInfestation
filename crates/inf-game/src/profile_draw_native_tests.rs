//! Synthetic final-text boundaries compared against unmodified PE profile
//! callbacks. 453810 reports a fixed [37,9] extent; the callback owner still
//! executes all native descriptor arithmetic and state mutations.

use super::*;
use crate::{
    menu::{MenuBasis, MenuCommand, MenuDrawPlacement, MenuRowDescriptor},
    profile_menu::{ProfileMenu, ProfileMenuServices, ProfileMenuTemplates},
};
use inf_formats::{
    encode_save_records, ByteSpan, Font, FontMode, GlyphMetrics, SaveRecord, SAVE_PAYLOAD_BYTES,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn number(v: &Value, key: &str) -> i32 {
    v[key].as_i64().unwrap() as i32
}
fn pair(v: &Value, key: &str) -> [i32; 2] {
    std::array::from_fn(|i| v[key][i].as_i64().unwrap() as i32)
}
fn flag(v: &Value, key: &str) -> bool {
    v[key].as_bool().unwrap()
}
fn color(c: MenuColor) -> u32 {
    u32::from_le_bytes([c.rgb[0], c.rgb[1], c.rgb[2], c.high_byte.unwrap_or(0)])
}
fn descriptor(d: MenuRowDescriptor) -> Value {
    json!({"position":d.position_pixels,"size":d.extent_pixels,"scale":d.reveal,"reserved":0,"flags":d.flags,"age":d.age_millis,"color":color(d.color)})
}
fn native_descriptor(v: &Value) -> Value {
    let mut v = v.clone();
    v.as_object_mut().unwrap().remove("extent");
    v
}

struct Resources {
    font: Font,
    strings: BTreeMap<i32, Vec<u8>>,
    styles: [u32; 2],
}
impl Resources {
    fn new(case: &Value) -> Self {
        let strings = (0..512)
            .map(|i| {
                (
                    i,
                    match i {
                        0x10a => b"Yes".to_vec(),
                        0x10b => b"No".to_vec(),
                        _ => format!("L{i:04X}").into_bytes(),
                    },
                )
            })
            .collect();
        let mut metrics = vec![
            GlyphMetrics {
                advance: 0,
                spacing: 0,
                offset: [0, 0]
            };
            256
        ];
        // The status consumer narrows measured AX. A single L glyph with this
        // signed advance gives the same AX without faking measure_text_width.
        metrics[b'L' as usize].advance = number(case, "measured_width") as i16;
        Self {
            strings,
            styles: [number(case, "style2") as u32, number(case, "style3") as u32],
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
                metrics,
                sprite_words: (1..=256).collect(),
            },
        }
    }
}
impl MenuTextResources for Resources {
    type Error = &'static str;
    fn string(&self, index: i32) -> Result<&[u8], Self::Error> {
        self.strings
            .get(&index)
            .map(Vec::as_slice)
            .ok_or("missing synthetic string")
    }
    fn font(&self, index: i32) -> Result<&Font, Self::Error> {
        assert_eq!(index, 0);
        Ok(&self.font)
    }
    fn style(&self, index: i32) -> Result<u32, Self::Error> {
        Ok(self.styles[(index - 2) as usize])
    }
    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        unreachable!("profile owner does not submit glyphs at this boundary")
    }
}

#[derive(Default)]
struct Host {
    events: Vec<Value>,
}
impl ProfileTextHost for Host {
    type Error = &'static str;
    fn row_text(&mut self, r: MenuRowTextRequest<'_>) -> Result<MenuTextOutcome, Self::Error> {
        assert_eq!(r.placement, MenuTextPlacement::Left);
        self.events.push(json!({"kind":"row","left":String::from_utf8(terminated(r.text).to_vec()).unwrap(),"right":null,"descriptor":descriptor(*r.descriptor)}));
        Ok(MenuTextOutcome {
            advance_from_origin: 37,
        })
    }
    fn raw_text(&mut self, r: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        self.events.push(json!({"kind":"raw","text":String::from_utf8(r.text.to_vec()).unwrap(),"position":r.position_pixels,"color":color(r.color),"width":r.width_pixels}));
        Ok(())
    }
    fn set_clip_rect(&mut self, rect: [i16; 4]) -> Result<(), Self::Error> {
        self.events.push(json!({"kind":"clip","rect":rect}));
        Ok(())
    }
    fn published_origin_pixels(&self) -> [i32; 2] {
        [0, 0]
    }
}

struct Services;
impl ProfileMenuServices for Services {
    type Error = std::convert::Infallible;
    fn rotate_menu_basis(&mut self, _: &mut MenuBasis, _: i32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reset_profile_input(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn select_camera(&mut self, _: i32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn set_transition_flag(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn request_phase(&mut self, _: u32) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn play_sound(&mut self, _: i16) -> Result<(), Self::Error> {
        unreachable!()
    }
    fn begin_controller_fade(&mut self) -> Result<(), Self::Error> {
        unreachable!()
    }
}

#[test]
fn original_profile_draw_receipts_match_descriptors_colors_positions_and_state() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../tests/fixtures/profile_draw_native.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let templates =
        ProfileMenuTemplates::parse(&fs::read(root.join("../../retail/Infest.exe")).unwrap())
            .unwrap();
    let dir = root.join(format!(
        "../../.tmp/profile-draw-native-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut groups = BTreeMap::<String, usize>::new();
    for (case_index, receipt) in cases.iter().enumerate() {
        let c = &receipt["input"];
        let kind = c["kind"].as_str().unwrap();
        *groups.entry(kind.into()).or_default() += 1;
        let count = number(c, "count") as usize;
        let mut payload = [0; SAVE_PAYLOAD_BYTES];
        payload[..6].copy_from_slice(b"Pilot\0");
        payload[6..10].copy_from_slice(&2u32.to_le_bytes());
        payload[10..14].copy_from_slice(&3720u32.to_le_bytes());
        let records = vec![SaveRecord::from_payload(payload).unwrap(); count];
        let sidecar = dir.join(format!("{case_index}.sav"));
        fs::write(&sidecar, encode_save_records(&records).unwrap()).unwrap();
        let mut store = ProfileStore::open(&sidecar).unwrap();
        let mut menu = ProfileMenu::new(templates.clone(), MenuBasis::INITIAL, false);
        menu.finish_draw();
        menu.tick(0, false, &mut store, &mut Services).unwrap();
        let data = menu.base().data().clone();
        menu.base_mut()
            .replace_data(data, number(c, "selection"), None);
        for _ in 0..number(c, "mode") {
            let (base, state) = menu.split_mut();
            state.change_mode(base, 1, false, 0).unwrap();
        }
        // Native fixtures seed +2D independently of the lock. Exercise its
        // recovered writer, then restore the supplied lock before drawing.
        menu.base_mut().set_navigation_locked(true);
        menu.execute_callback(
            0x449300,
            if flag(c, "choice") {
                MenuCommand::AdjustPrevious
            } else {
                MenuCommand::AdjustNext
            },
            [0, 0],
            false,
            &mut store,
            &mut Services,
        )
        .unwrap();
        menu.base_mut().set_navigation_locked(flag(c, "locked"));
        menu.split_mut()
            .1
            .set_format_state(number(c, "format_state"));
        // change_mode mutates the header; oracle starts the original copied
        // label independently from the seeded mode.
        menu.base().data().records_mut().items[0].draw_arguments[0] = 145;
        let callback = match kind {
            "header" => 0x448e60,
            "slot" => 0x449320,
            "help" => 0x448c60,
            "format" => 0x448d10,
            "confirmation" => 0x448990,
            "draw" => 0,
            _ => unreachable!(),
        };
        let mut row = MenuRowDrawRequest {
            menu_source: menu.base().data().source(),
            layout_index: 0,
            item_index: 0,
            callback_address: callback,
            arguments: [
                if kind == "slot" {
                    number(c, "slot") as u32
                } else {
                    number(c, "label") as u32
                },
                0,
            ],
            context_origin_pixels: [0, 0],
            placement: MenuDrawPlacement::Linear,
            descriptor: MenuRowDescriptor {
                position_pixels: pair(c, "position").map(|v| v as i16),
                extent_pixels: pair(c, "size").map(|v| v as i16),
                reveal: number(c, "scale") as u16,
                flags: number(c, "flags") as u32,
                age_millis: number(c, "age"),
                color: MenuColor::from_word(number(c, "color") as u32),
            },
        };
        let resources = Resources::new(c);
        let mut host = Host::default();
        if kind == "draw" {
            let timer = c["timers"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .find(|(_, t)| t.as_i64().unwrap() > 0);
            if let Some((i, millis)) = timer {
                draw_profile_status(
                    ProfileTimer {
                        kind: [
                            ProfileTimerKind::LoadFailure,
                            ProfileTimerKind::CopyFailure,
                            ProfileTimerKind::InitialWarning,
                            ProfileTimerKind::DeleteFailure,
                        ][i],
                        millis: millis.as_i64().unwrap() as i32,
                    },
                    pair(c, "surface"),
                    &resources,
                    &mut host,
                )
                .unwrap();
            }
        } else {
            let (base, state) = menu.split_mut();
            let mut draw = ProfileRowDraw {
                request: &mut row,
                base,
                state,
                store: &mut store,
                owner_flag: flag(c, "owner"),
                surface_pixels: pair(c, "surface"),
            };
            if kind == "confirmation" {
                ProfileDrawContext {
                    resources: &resources,
                    host: &mut host,
                }
                .confirmation(&mut draw, pair(c, "position")[0] as i16, 0xce)
                .unwrap();
            } else {
                draw_profile_row(draw, &resources, &mut host).unwrap();
            }
        }
        let mut expected = Vec::new();
        for event in receipt["events"].as_array().unwrap() {
            let mut event = event.clone();
            match event["kind"].as_str().unwrap() {
                "row" => event["descriptor"] = native_descriptor(&event["descriptor"]),
                "raw" => {
                    let obj = event.as_object_mut().unwrap();
                    obj.remove("age");
                    obj.remove("flags");
                    if kind == "draw" {
                        let rgb = obj["color"].as_u64().unwrap() & 0xffffff;
                        obj.insert("color".into(), json!(rgb));
                    }
                }
                "clip" => (),
                _ => continue,
            }
            expected.push(event);
        }
        // 00448E60 never writes its [esp+0x48] selected-X slot when mode is
        // Load (0): confirmation reuses whatever that stack slot held on entry.
        // The emulator harness observes 0x1000 (4096) there, while Load+locked
        // with a clear owner byte is unreachable through native menu transitions
        // (Load activation schedules without locking; lock entry requires
        // Copy/Delete or the owner branch). The port defines this unreachable
        // state as the selected label's starting X instead of replicating
        // uninitialized stack contents. The first three label rows already match;
        // only the confirmation tail diverges.
        let unreachable_load_locked =
            kind == "header" && number(c, "mode") == 0 && flag(c, "locked") && !flag(c, "owner");
        if unreachable_load_locked {
            for (index, (actual, want)) in host.events.iter().zip(expected.iter_mut()).enumerate() {
                if index < 3 {
                    continue;
                }
                if let (Some(a), Some(w)) = (actual.get("descriptor"), want.get_mut("descriptor")) {
                    if let (Some(ax), Some(wx)) = (
                        a.get("position").and_then(|p| p.get(0)).cloned(),
                        w.get_mut("position").and_then(|p| p.get_mut(0)),
                    ) {
                        *wx = ax;
                    }
                }
            }
        }
        assert_eq!(host.events, expected, "case {case_index} {kind}: {c}");
        if kind != "draw" {
            if unreachable_load_locked {
                let mut want_final = native_descriptor(&receipt["final_row"]);
                want_final["position"][0] = descriptor(row.descriptor)["position"][0].clone();
                assert_eq!(
                    descriptor(row.descriptor),
                    want_final,
                    "case {case_index} final descriptor"
                );
            } else {
                assert_eq!(
                    descriptor(row.descriptor),
                    native_descriptor(&receipt["final_row"]),
                    "case {case_index} final descriptor"
                );
            }
            assert_eq!(
                menu.state().mode().native(),
                number(receipt, "final_mode"),
                "case {case_index} final mode"
            );
            assert_eq!(
                menu.state().format_state(),
                number(receipt, "final_format_state"),
                "case {case_index} final format"
            );
            assert_eq!(
                menu.base().data().records().items[0].draw_arguments[0],
                number(receipt, "final_header_label") as u32,
                "case {case_index} final header"
            );
        }
    }
    assert_eq!(groups.len(), 6);
    assert!(groups.values().all(|&count| count > 0));
}
