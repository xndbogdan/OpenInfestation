use super::*;
use std::path::Path;

use inf_formats::{
    parse_frontend_sine_table, parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming,
    HandlerId, HandlerRegion, FRONTEND_SINE_TABLE_ADDRESS, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};

use crate::{
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
    world_furniture_effect::{FurnitureEffectProgram, FurnitureQueueRequest},
};

fn receipts() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/furniture_draw_native.json"
    ))
    .unwrap()
}

fn math() -> NativeBasisMath {
    let bytes =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
            .unwrap();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    NativeBasisMath::new(
        parse_frontend_sine_table(executable.data(), FRONTEND_SINE_TABLE_ADDRESS)
            .unwrap()
            .values,
    )
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn words<const N: usize>(value: &Value) -> [u32; N] {
    std::array::from_fn(|index| number(&value[index]))
}

fn signed<const N: usize>(value: &Value) -> [i32; N] {
    std::array::from_fn(|index| value[index].as_i64().unwrap() as i32)
}

fn terrain(state: u8, lookup: u8, spacing: TerrainSpacing) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    // Canonical H12 marked cell and its actual four-byte sparse record.
    bytes[TERRAIN_PREFIX_BYTES + 1] = 4;
    let offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2;
    bytes[offset..offset + 2].copy_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, state, lookup]);
    let span = ByteSpan::new(0, bytes.len());
    let source = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap()
    .entries
    .remove(0);
    TerrainRuntime::new(
        &source,
        TerrainSampling {
            spacing,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn context_json(context: &FurnitureDrawContext) -> Value {
    json!({"fade": context.fade, "far_color": context.far_color})
}

struct Host<'a> {
    supplied: &'a Value,
    events: Vec<Value>,
    fades: usize,
    models: Vec<FurnitureModelRequest>,
    no_draw: bool,
    model_mutation: Option<(FurnitureDrawContext, u32)>,
}

impl<'a> Host<'a> {
    fn new(supplied: &'a Value) -> Self {
        Self {
            supplied,
            events: Vec::new(),
            fades: 0,
            models: Vec::new(),
            no_draw: false,
            model_mutation: None,
        }
    }

    fn fail(&self, effect: &'static str) -> Result<(), &'static str> {
        if self.supplied["fail"].as_str() == Some(effect) {
            Err(effect)
        } else {
            Ok(())
        }
    }
}

impl FurnitureDrawHost for Host<'_> {
    type Error = &'static str;

    fn bind_h1(&mut self, lookup: TerrainResourceLookup) -> Result<u16, Self::Error> {
        let slot = number(&self.supplied["slot"]) as u16;
        self.events.push(json!(["bind", lookup.raw(), slot]));
        self.fail("bind")?;
        Ok(slot)
    }

    fn model_extent(&mut self, slot: u16) -> Result<u32, Self::Error> {
        assert_eq!(slot, number(&self.supplied["slot"]) as u16);
        let extent = number(&self.supplied["extent"]);
        self.events.push(json!(["extent", extent]));
        self.fail("extent")?;
        Ok(extent)
    }

    fn far_color32(&mut self) -> Result<u32, Self::Error> {
        let color = number(&self.supplied["color32"]);
        self.events.push(json!(["color", color]));
        self.fail("color")?;
        Ok(color)
    }

    fn publish_fade(&mut self, context: &FurnitureDrawContext) -> Result<(), Self::Error> {
        self.events.push(json!(["fade", context_json(context)]));
        self.fail(if self.fades == 0 { "fade" } else { "restore" })?;
        self.fades += 1;
        Ok(())
    }

    fn birth_trail(
        &mut self,
        request: FurnitureTrailRequest,
        _rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events
            .push(json!(["trail", {"particle_type": request.particle_type,
            "position": request.position, "velocity": request.velocity, "flags": request.flags}]));
        self.fail("trail")
    }

    fn draw_h1(
        &mut self,
        terrain: &TerrainRuntime,
        context: &mut FurnitureDrawContext,
        request: FurnitureModelRequest,
        rng: &mut u32,
    ) -> Result<FurnitureModelOutcome, Self::Error> {
        let state = terrain.buckets()[usize::from(request.record.bucket)].records
            [request.record.index]
            .state;
        self.events
            .push(json!(["model", context_json(context), state]));
        self.models.push(request);
        self.fail("model")?;
        if let Some((changed_context, changed_rng)) = self.model_mutation {
            *context = changed_context;
            *rng = changed_rng;
        }
        Ok(if self.no_draw {
            FurnitureModelOutcome::NoDraw
        } else {
            FurnitureModelOutcome::Submitted
        })
    }
}

struct Inputs {
    terrain: TerrainRuntime,
    pending: FurnitureEffectQueue,
    context: FurnitureDrawContext,
    state: FurnitureDrawState,
    rng: u32,
}

impl Inputs {
    fn new(row: &Value, math: &NativeBasisMath) -> Self {
        let spacing = match number(&row["mask"]) {
            127 => TerrainSpacing::Units128,
            255 => TerrainSpacing::Units256,
            other => panic!("unsupported native supplied spacing mask {other}"),
        };
        let terrain = terrain(
            number(&row["state"]) as u8,
            number(&row["lookup"]) as u8,
            spacing,
        );
        let mut pending = FurnitureEffectQueue::default();
        if let Some(remaining) = row["pending"].as_u64() {
            let position = signed::<3>(&row["position"]);
            let mut xz = [position[0] as u16, position[2] as u16];
            if row["wrong_key"].as_bool() == Some(true) {
                xz[0] ^= spacing.units();
            }
            pending
                .enqueue(
                    &terrain,
                    FurnitureQueueRequest {
                        xz,
                        program: FurnitureEffectProgram::new(0x00531000).unwrap(),
                    },
                )
                .unwrap();
            pending.record_mut(0).unwrap().remaining_microseconds = remaining as u32;
        }
        let mut state = FurnitureDrawState::new(math);
        state.current_basis.words[0] = number(&row["counter"]);
        state.rebuild(math);
        Self {
            terrain,
            pending,
            context: FurnitureDrawContext {
                view: SceneView {
                    eye: signed(&row["eye"]),
                    basis: NativeBasis {
                        words: words(&row["view_basis"]),
                    },
                },
                fade: signed(&row["fade"]),
                far_color: number(&row["far_color"]),
            },
            state,
            rng: number(&row["rng"]),
        }
    }

    fn draw(
        &mut self,
        row: &Value,
        host: &mut Host<'_>,
    ) -> Result<FurnitureDrawOutcome, FurnitureDrawError<&'static str>> {
        draw_furniture_item(
            FurnitureDrawRequest {
                terrain: &mut self.terrain,
                record: TerrainSparseRecordId {
                    bucket: 0,
                    index: 0,
                },
                position: signed(&row["position"]),
                context: &mut self.context,
                pending: &self.pending,
                load_factor: row["load_factor"].as_i64().unwrap() as i32,
                clock: number(&row["clock"]),
                state: &mut self.state,
            },
            host,
            &mut self.rng,
        )
    }
}

#[test]
fn original_furniture_item_receipts_preserve_services_prefixes_and_packets() {
    let fixture = receipts();
    let math = math();
    let rows = fixture["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 51);
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let mut inputs = Inputs::new(row, &math);
        let original_queue = inputs.pending.records().to_vec();
        let mut host = Host::new(row);
        let result = inputs.draw(row, &mut host);
        let error = result.as_ref().err().map(|error| match error {
            FurnitureDrawError::Host { error, .. } => *error,
            FurnitureDrawError::Record(_) => panic!("unexpected record error for {name}"),
        });
        assert_eq!(error, row["expected"]["error"].as_str(), "{name}");
        // Basis publication has no host callback. Its exact ten-word final
        // state independently pins the source event at435A3F, including faults.
        let events: Vec<_> = row["expected"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event[0] != "basis")
            .cloned()
            .collect();
        assert_eq!(host.events, events, "{name}");
        assert_eq!(
            inputs.state.current_basis.words,
            words::<10>(&row["expected"]["current"]),
            "{name}"
        );
        assert_eq!(
            context_json(&inputs.context),
            row["expected"]["context"],
            "{name}"
        );
        assert_eq!(inputs.rng, number(&row["expected"]["rng"]), "{name}");
        assert_eq!(
            inputs.terrain.buckets()[0].records[0].state,
            number(&row["expected"]["state"]) as u8,
            "{name}"
        );
        assert_eq!(
            inputs.pending.records(),
            original_queue,
            "draw does not unlink queue: {name}"
        );
        for request in host.models {
            assert_eq!(
                request.record,
                TerrainSparseRecordId {
                    bucket: 0,
                    index: 0
                }
            );
            assert_eq!(request.h1_slot, number(&row["slot"]) as u16, "{name}");
            assert_eq!(request.position, signed::<3>(&row["position"]), "{name}");
            assert_eq!(request.clock, number(&row["clock"]), "{name}");
            assert_eq!(
                request.basis.words,
                words::<10>(&row["expected"]["current"]),
                "{name}"
            );
        }
    }
}

#[test]
fn original_furniture_acquisition_bases_preserve_counter_and_rotations() {
    let fixture = receipts();
    let math = math();
    let rows = fixture["basis"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    for row in rows {
        let mut state = FurnitureDrawState::new(&math);
        state.current_basis.words = [0xdead_beef; 10];
        state.current_basis.words[0] = number(&row["counter"]);
        state.rebuild(&math);
        assert_eq!(state.current_basis.words, words::<10>(&row["current"]));
        for index in 0..16 {
            assert_eq!(
                state.basis((index as u8) << 4).words,
                words::<10>(&row["bases"][index])
            );
        }
        assert_eq!(row["events"], json!([["queue_registration"]]));
        assert_eq!(row["queue"], json!([0x00546e0c, 0, 0x00546e08]));
    }
}

#[test]
fn original_packed_callback_receipts_cover_every_state_byte_and_selector_policy() {
    let fixture = receipts();
    let rows = fixture["packed"].as_array().unwrap();
    assert_eq!(rows.len(), 1280);
    for row in rows {
        let record = TerrainRecord {
            x: 13,
            state: number(&row["state"]) as u8,
            resource_lookup: TerrainResourceLookup::from_raw(7),
        };
        assert_eq!(
            furniture_packed_value(
                row["selector"].as_i64().unwrap() as i32,
                number(&row["clock"]),
                record
            ),
            number(&row["expected"]) as u16
        );
    }
}

#[test]
fn required_bind_failure_precedes_basis_mutation_and_all_later_services() {
    let mut row = receipts()["cases"][1].clone();
    row["fail"] = json!("bind");
    let mut inputs = Inputs::new(&row, &math());
    let original_basis = inputs.state.current_basis;
    let original_context = inputs.context;
    let mut host = Host::new(&row);
    assert_eq!(
        inputs.draw(&row, &mut host),
        Err(FurnitureDrawError::Host {
            effect: "H11 bind",
            error: "bind"
        })
    );
    assert_eq!(host.events, vec![json!(["bind", 7, 9])]);
    assert_eq!(inputs.state.current_basis, original_basis);
    assert_eq!(inputs.context, original_context);
    assert_eq!(inputs.rng, number(&row["rng"]));
}

#[test]
fn genuine_null_model_restores_override_and_real_model_effects_keep_reached_custody() {
    let fixture = receipts();
    let row = &fixture["cases"][5]; // Native pending-fade case.
    let mut inputs = Inputs::new(row, &math());
    let inherited = inputs.context;
    let mut host = Host::new(row);
    host.no_draw = true;
    assert_eq!(
        inputs.draw(row, &mut host),
        Ok(FurnitureDrawOutcome::Drawn(FurnitureModelOutcome::NoDraw))
    );
    assert_eq!(inputs.context, inherited);
    assert_eq!(host.fades, 2);

    // A real H1 host can mutate renderer context/RNG. Native restoration owns
    // only the temporary fade/color, so camera changes and RNG remain live.
    let mut inputs = Inputs::new(row, &math());
    let mut host = Host::new(row);
    let changed = FurnitureDrawContext {
        view: SceneView {
            eye: [1, 2, 3],
            basis: NativeBasis::INITIAL,
        },
        fade: [-50, 50],
        far_color: 9,
    };
    host.model_mutation = Some((changed, 0x87654321));
    inputs.draw(row, &mut host).unwrap();
    assert_eq!(inputs.context.view, changed.view);
    assert_eq!(inputs.context.fade, inherited.fade);
    assert_eq!(inputs.context.far_color, inherited.far_color);
    assert_eq!(inputs.rng, 0x87654321);
}
