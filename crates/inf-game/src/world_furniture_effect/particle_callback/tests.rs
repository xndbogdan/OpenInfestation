use super::*;
use serde_json::{json, Value};

struct Host {
    stop: bool,
    call: Option<Value>,
}
impl FurnitureParticleCallbackHost for Host {
    type Error = &'static str;
    fn emit(
        &mut self,
        effect: FurnitureParticleEffect,
        inputs: FurnitureParticleCallbackInputs<'_>,
    ) -> Result<(), Self::Error> {
        self.call = Some(match effect {
            FurnitureParticleEffect::Burst(b) => {
                json!({"kind":"burst","count":b.debris_count,"position":b.position,"radius":b.placement_radius,"central":b.central_type,"debris":b.debris_types,"speed":b.speed,"flags":b.flags})
            }
            FurnitureParticleEffect::Central { selector } => {
                json!({"kind":"central","selector":selector,"position":inputs.context.position_words,"flags":0,"same_position":true})
            }
            FurnitureParticleEffect::Emitter { selector } => {
                json!({"kind":"emitter","selector":selector,"position":inputs.context.position_words,"flags":0,"same_position":true})
            }
            FurnitureParticleEffect::TerrainFlash { selector } => {
                json!({"kind":"terrain_flash","selector":selector,"position":inputs.context.position_words,"flags":0,"same_position":true})
            }
        });
        // The actual context remains loaned even though the burst argument
        // was copied. Lower-service writes survive both success and failure.
        inputs.context.position_words = [8, 9, 10];
        *inputs.rng = 55;
        if self.stop {
            Err("lower service")
        } else {
            Ok(())
        }
    }
}

#[test]
fn furniture_particle_callback_matches_twelve_native_dispatch_and_return_segments() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/furniture_particle_callback_native.json"
    ))
    .unwrap();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut context = FurnitureCallbackContext {
            position_words: std::array::from_fn(|axis| {
                input["position"][axis].as_u64().unwrap() as u16
            }),
        };
        let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
        let mut rng = 1;
        let mut host = Host {
            stop: input["stop"].as_bool().unwrap(),
            call: None,
        };
        let result = run_furniture_particle_callback(
            input["selector"].as_u64().unwrap() as u32,
            &mut host,
            FurnitureParticleCallbackInputs {
                context: &mut context,
                sounds: &mut sounds,
                rng: &mut rng,
            },
        );
        assert_eq!(result.is_ok(), case["normal_return"].as_bool().unwrap());
        if let Ok(value) = result {
            assert!(value);
        }
        assert_eq!(host.call.unwrap(), case["call"]);
        assert_eq!(context.position_words, [8, 9, 10]);
        assert_eq!(rng, 55);
    }
}
