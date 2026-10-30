//! Portable consumption of the 22 original 004375A0 adapter receipts.
//! The lower 004887D0 service is supplied. Native world-pointer values,
//! unaligned pointer representation and unused upper EAX are outside the Rust
//! interface; the original mutable context identity and AL result are checked.

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing},
};
use inf_formats::{
    parse_terrain_bank, ByteSpan, FrontendExecutable, HandlerFraming, HandlerId, HandlerRegion,
    TERRAIN_CELL_COUNT, TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};

fn assets() -> FrontendAssets {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    FrontendAssets::open(&root).unwrap()
}

fn empty_terrain() -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = (-1_i8) as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn number(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

fn position(value: &Value) -> [u16; 3] {
    serde_json::from_value(value.clone()).unwrap()
}

fn portable_state(native: &Value) -> Value {
    let mut state = native.clone();
    state.as_object_mut().unwrap().remove("current_g");
    state
}

struct Host<'a> {
    input: &'a Value,
    source: Cell<Option<ActorHandle>>,
    order: RefCell<Vec<&'static str>>,
    events: Vec<Value>,
    original_context: *const FurnitureCallbackContext,
    forbid_lower: bool,
}

impl Host<'_> {
    fn snapshot(&self, context: &FurnitureCallbackContext, rng: u32) -> Value {
        json!({
            "position":context.position_words,
            "source_null":self.source.get().map(ActorHandle::raw),
            "rng":rng
        })
    }
}

impl TargetNullHandleSource for Host<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        self.order.borrow_mut().push("source");
        self.source.get()
    }
}

impl FurnitureRadiusCallbackHost for Host<'_> {
    type Error = &'static str;

    fn apply_radius(
        &mut self,
        inputs: FurnitureRadiusCallbackInputs<'_>,
        source: ActorHandle,
        replicate: bool,
    ) -> Result<(), Self::Error> {
        assert!(
            !self.forbid_lower,
            "missing source must precede the lower service"
        );
        assert_eq!(self.order.borrow().as_slice(), ["source"]);
        self.order.borrow_mut().push("radius");
        let same_position = std::ptr::eq(&*inputs.context, self.original_context);
        self.events.push(json!({
            "kind":"world_radius", "effect_block":inputs.block_address,
            "position_pointer_is_input":same_position,
            "position":inputs.context.position_words,
            "source":source.raw(), "replicate":u32::from(replicate),
            "state":self.snapshot(inputs.context,*inputs.rng)
        }));
        if self.input["stop"].as_str() == Some("before_lower") {
            return Err("before_lower");
        }
        for (field, value) in self.input["after_lower"].as_object().unwrap() {
            match field.as_str() {
                "position" => inputs.context.position_words = position(value),
                "source_null" => self.source.set(Some(ActorHandle::from_raw(number(value)))),
                "rng" => *inputs.rng = number(value),
                // The adapter borrows world services, rather than exposing a
                // writable native process-global pointer to those services.
                "current_g" => {}
                other => panic!("unowned supplied field {other}"),
            }
        }
        if self.input["stop"].as_str() == Some("after_lower_mutation") {
            return Err("after_lower_mutation");
        }
        Ok(())
    }
}

#[test]
fn furniture_radius_callback_matches_22_native_adapter_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/furniture_radius_callback_native.json"
    )))
    .unwrap();
    assert_eq!(fixture["coverage"]["cases"], 22);
    assert_eq!(fixture["provenance"]["fpcw"], 0x027f);
    assert_eq!(fixture["provenance"]["code_patched"], false);
    assert_eq!(fixture["provenance"]["hooks"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let assets = assets();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let mut queue = FurnitureEffectQueue::default();
        let mut terrain = empty_terrain();
        let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
        let mut context = FurnitureCallbackContext {
            position_words: position(&input["position"]),
        };
        let mut rng = number(&input["rng"]);
        let mut host = Host {
            input,
            source: Cell::new(Some(ActorHandle::from_raw(number(&input["source_null"])))),
            order: RefCell::new(Vec::new()),
            events: Vec::new(),
            original_context: &context,
            forbid_lower: false,
        };
        assert_eq!(
            host.snapshot(&context, rng),
            portable_state(&case["before"])
        );
        let result = run_furniture_radius_callback(
            &mut host,
            FurnitureRadiusCallbackInputs {
                queue: &mut queue,
                terrain: &mut terrain,
                context: &mut context,
                block_address: number(&input["effect_block"]),
                sounds: &mut sounds,
                rng: &mut rng,
            },
        );
        let expected = match input["stop"].as_str() {
            None => Ok(true),
            Some("before_lower") => Err(FurnitureRadiusCallbackError::Radius("before_lower")),
            Some("after_lower_mutation") => {
                Err(FurnitureRadiusCallbackError::Radius("after_lower_mutation"))
            }
            other => panic!("unowned native stop {other:?}"),
        };
        assert_eq!(result, expected, "case {index} result");
        assert_eq!(result.is_ok(), case["normal_return"].as_bool().unwrap());
        // Only AL is the callback's result. Upper EAX remains arbitrary lower
        // return residue in the receipt, and has no portable Boolean meaning.
        assert_eq!(
            result.as_ref().ok().copied(),
            case["return_al"].as_u64().map(|value| value != 0),
            "case {index} AL"
        );
        let mut events = case["events"].as_array().unwrap().clone();
        for event in &mut events {
            event.as_object_mut().unwrap().remove("address");
            event.as_object_mut().unwrap().remove("current_g");
            event["state"] = portable_state(&event["state"]);
        }
        assert_eq!(
            host.events, events,
            "case {index} arguments and captured source"
        );
        assert_eq!(host.order.borrow().as_slice(), ["source", "radius"]);
        assert_eq!(
            host.snapshot(&context, rng),
            portable_state(&case["output"]),
            "case {index} retained prefix"
        );
        assert!(queue.records().is_empty());
        assert!(sounds.is_empty());
    }
}

#[test]
fn furniture_radius_callback_missing_source_precedes_lower_reads_but_raw_zero_is_valid() {
    let assets = assets();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut queue = FurnitureEffectQueue::default();
    let mut terrain = empty_terrain();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let mut context = FurnitureCallbackContext {
        position_words: [0xffff, 0x8000, 0x7fff],
    };
    let original = context;
    let original_cell = terrain.cell_at([0, 0]);
    let mut rng = 0xdead_beef;
    let input = json!({"stop":null,"after_lower":{}});
    let mut host = Host {
        input: &input,
        source: Cell::new(None),
        order: RefCell::new(Vec::new()),
        events: Vec::new(),
        original_context: &context,
        forbid_lower: true,
    };
    let result = run_furniture_radius_callback(
        &mut host,
        FurnitureRadiusCallbackInputs {
            queue: &mut queue,
            terrain: &mut terrain,
            context: &mut context,
            block_address: u32::MAX,
            sounds: &mut sounds,
            rng: &mut rng,
        },
    );
    assert_eq!(
        result,
        Err(FurnitureRadiusCallbackError::MissingProcessNull)
    );
    assert_eq!(host.order.borrow().as_slice(), ["source"]);
    assert!(host.events.is_empty());
    assert_eq!(context, original);
    assert_eq!(terrain.cell_at([0, 0]), original_cell);
    assert_eq!(rng, 0xdead_beef);
    assert!(queue.records().is_empty());
    assert!(sounds.is_empty());

    host.source.set(Some(ActorHandle::from_raw(0)));
    host.forbid_lower = false;
    host.order.borrow_mut().clear();
    assert_eq!(
        run_furniture_radius_callback(
            &mut host,
            FurnitureRadiusCallbackInputs {
                queue: &mut queue,
                terrain: &mut terrain,
                context: &mut context,
                block_address: u32::MAX,
                sounds: &mut sounds,
                rng: &mut rng,
            },
        ),
        Ok(true)
    );
    assert_eq!(host.events[0]["source"], 0);
    assert_eq!(host.events[0]["effect_block"], u32::MAX);
    assert_eq!(host.order.borrow().as_slice(), ["source", "radius"]);
}

#[test]
fn furniture_radius_callback_complete_original_body_is_guarded() {
    use sha2::{Digest, Sha256};

    let assets = assets();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    // Full 34-byte original: source read, stack argument order, current world
    // load, 004887D0 call, MOV AL,1 and RET8. Text raw offset is VA-00400000.
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes[0x0003_75a0..0x0003_75c2])),
        "abb50cd7df2154fb95f962fdb68dad6e0896d2dfce392c4bd9bcdff510b8a9a5"
    );
}
