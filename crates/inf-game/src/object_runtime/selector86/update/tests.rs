use super::*;
use crate::object_runtime::{BaseContext, ObjectClassKind};
use inf_formats::{native_basis::spacing_sample, ByteSpan};
use serde_json::{json, Value};

fn number(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/selector86_update_native.json"
    ))
    .unwrap()
}

struct Host<'a> {
    input: &'a Value,
    sine: &'a [u16; 4096],
    events: Vec<Value>,
    trails: usize,
}
impl Selector86UpdateHost for Host<'_> {
    type Error = &'static str;
    fn cached_camera_position(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<[u16; 3], Self::Error> {
        Ok(serde_json::from_value(self.input["camera"].clone()).unwrap())
    }
    fn surface_height(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        xz: [u16; 2],
    ) -> Result<i16, Self::Error> {
        // No RNG consumer lies between sampling and the two service boundaries.
        // Native expected height event RNG is verified alongside its trail,
        // and stopped-height final RNG is checked by the enclosing test.
        self.events.push(json!({"phase":"height","xz":xz,"elapsed":arena.selector86(object).unwrap().elapsed_44}));
        if self.input["stop"] == "height" {
            return Err("height");
        }
        let height = self.input["height"].as_i64().unwrap() as i16;
        let water = spacing_sample(
            number(&self.input["plane"]),
            u32::from(xz[0]),
            u32::from(xz[1]),
            0,
            number(&self.input["world_flags"]) & 64,
            i32::from(height) as u32,
            number(&self.input["shift"]),
            self.sine,
        ) as u16 as i16;
        Ok(height.max(water))
    }
    fn emit_trail(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        trail: Selector86Trail,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.last_mut().unwrap()["rng"] = json!(*rng);
        self.events.push(json!({"phase":"trail","kind":trail.particle_type,"position":trail.position,
            "velocity":[0,100,0],"flags":0x800,"elapsed":arena.selector86(object).unwrap().elapsed_44,"rng":*rng}));
        if self.input["stop"] == "trail" {
            return Err("trail");
        }
        self.trails += 1;
        if self.trails == 1 && !self.input["mutation"].is_null() {
            let m = &self.input["mutation"];
            arena.owned_mut(object).unwrap().state.base.position_words =
                Some(serde_json::from_value(m["position"].clone()).unwrap());
            let data = arena.selector86(object).unwrap().data_40.as_ref().unwrap();
            for (index, key) in [(0, "radius"), (2, "kind"), (3, "period")] {
                data.allocation
                    .write_word(data.allocation_offset + index * 2, number(&m[key]) as u16)
                    .unwrap();
            }
            *rng = number(&m["rng"]);
        }
        Ok(())
    }
}

#[test]
fn selector86_update_matches_complete_native_control_rng_and_water_segments() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let fixture = fixture();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 30);
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut arena = ObjectArena::new();
        let object = arena.allocate(ObjectClassKind::Selector86);
        arena
            .initialize_context(
                object,
                BaseContext {
                    position_words: serde_json::from_value(input["position"].clone()).unwrap(),
                    placement_index: u32::MAX,
                },
            )
            .unwrap();
        arena.owned_mut(object).unwrap().state.base.flags = number(&input["flags"]);
        let bytes: Vec<u8> = [
            number(&input["radius"]) as u16,
            500,
            number(&input["kind"]) as u16,
            number(&input["period"]) as u16,
        ]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
        let allocation =
            Arc::new(RuntimeAllocation::new(bytes.into(), ByteSpan::new(0, 8)).unwrap());
        arena.selector86_mut(object).unwrap().data_40 = Some(RetainedSelector86Data {
            allocation,
            allocation_offset: 0,
        });
        arena.selector86_mut(object).unwrap().elapsed_44 = number(&input["elapsed"]);
        arena.selector86_mut(object).unwrap().distance_squared_48 =
            number(&input["distance_squared"]);
        let mut rng = number(&input["rng"]);
        let mut host = Host {
            input,
            sine: &assets.sine_table().values,
            events: vec![],
            trails: 0,
        };
        let result = arena.tick_selector86(
            object,
            number(&input["delta"]),
            executable,
            &mut host,
            &mut rng,
        );
        if input["stop"] == "height" {
            host.events.last_mut().unwrap()["rng"] = json!(rng);
        }
        assert_eq!(
            result.is_ok(),
            case["normal_return"].as_bool().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(json!(host.events), case["events"], "{}", case["name"]);
        assert_eq!(rng, number(&case["output"]["rng"]), "{}", case["name"]);
        assert_eq!(
            arena.selector86(object).unwrap().elapsed_44,
            number(&case["output"]["elapsed"]),
            "{}",
            case["name"]
        );
        assert_eq!(
            json!(arena.object(object).unwrap().base.position_words.unwrap()),
            case["output"]["position"]
        );
        if result.is_err() {
            let before = arena.object(object).unwrap().clone();
            let events = host.events.clone();
            let before_rng = rng;
            assert!(matches!(
                arena.tick_selector86(
                    object,
                    number(&input["delta"]),
                    executable,
                    &mut host,
                    &mut rng
                ),
                Err(Selector86UpdateError::PreviouslyStopped(_))
            ));
            assert_eq!(arena.object(object).unwrap(), &before);
            assert_eq!(rng, before_rng);
            assert_eq!(host.events, events);
        }
    }
}

#[test]
fn selector86_update_native_bodies_vtable_and_sine_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let bytes = assets.executable_bytes();
    let f = fixture();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        f["retail_sha256"].as_str().unwrap()
    );
    for guard in f["guards"].as_array().unwrap() {
        let at = number(&guard["address"]) as usize - 0x400000;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[at..at + number(&guard["length"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
    let exe = FrontendExecutable::parse(bytes).unwrap();
    assert_eq!(exe.word(0x5218cc + 0x24).unwrap(), 0x42df40);
    assert_eq!(exe.word(0x5218cc + 0x3c).unwrap(), 0x405140);
    assert_eq!(
        exe.mapped(0x53df00, 8192).unwrap().bytes,
        assets
            .sine_table()
            .values
            .iter()
            .copied()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    );
}
