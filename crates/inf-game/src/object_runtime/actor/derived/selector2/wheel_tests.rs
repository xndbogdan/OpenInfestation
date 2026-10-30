//! The vehicle's wheel vertex (00415D30, exact selector 3) against every
//! wheel the Infest01 recording drew on Level 1's fine terrain.

use std::sync::Arc;

use serde_json::Value;

use super::test_support::loaded;
use crate::{
    model_vertices::{FurnitureCustomTrail, ModelCustomVertexRequest, ModelVertices},
    object_runtime::{
        ObjectCustomVertexEffects, ObjectCustomVertexRequest, ObjectLegServices,
        Selector10CustomServices, Selector10LegGround,
    },
    terrain_runtime::{TerrainRuntime, TerrainSpacing, TerrainWaterSurfaceRequest},
};

/// The ground and RNG 00415D30 reaches; births are recorded in the
/// fixture's [kind, type, position, velocity, flags, shadow] shape.
struct Wheel<'a> {
    terrain: &'a TerrainRuntime,
    sine: Arc<[u16; 4096]>,
    executable: inf_formats::FrontendExecutable<'a>,
    clock: u32,
    world_flags: u32,
    update_byte: u8,
    rng: u32,
    draws: u32,
    births: Vec<Vec<u64>>,
}

impl ObjectCustomVertexEffects for Wheel<'_> {
    fn load_factor(&mut self) -> Result<i32, String> {
        Err("the wheel reads no load factor".into())
    }

    fn random(&mut self) -> Result<u32, String> {
        Err("the wheel draws through its leg services".into())
    }

    fn birth_trail(&mut self, _: FurnitureCustomTrail) -> Result<(), String> {
        Err("the wheel births no trail".into())
    }

    fn legs(&mut self) -> Result<ObjectLegServices<'_>, String> {
        let sine = Arc::clone(&self.sine);
        let executable = self.executable;
        Ok(ObjectLegServices {
            services: self,
            executable,
            sine,
        })
    }

    fn birth_material_sprite(
        &mut self,
        position: [u16; 3],
        velocity: [u16; 3],
    ) -> Result<(), String> {
        let mut birth = vec![0, 0];
        birth.extend(position.map(u64::from));
        birth.extend(velocity.map(u64::from));
        birth.extend([0, 0]);
        self.births.push(birth);
        Ok(())
    }

    fn birth_impact(&mut self, position: [u16; 3]) -> Result<(), String> {
        let mut birth = vec![1, 0];
        birth.extend(position.map(u64::from));
        birth.extend([0; 5]);
        self.births.push(birth);
        Ok(())
    }
}

impl Selector10LegGround for Wheel<'_> {
    fn height(&mut self, x: u16, z: u16, _: u32) -> i16 {
        self.terrain.height_at([x, z])
    }

    fn water(&mut self, x: u16, z: u16, clock: u32, flags: u32, terrain: i16) -> i16 {
        self.terrain.water_surface_height(
            TerrainWaterSurfaceRequest {
                xz: [x, z],
                wave_clock: clock,
                world_flags: flags,
                terrain_height: terrain,
            },
            &self.sine,
        )
    }
}

impl Selector10CustomServices for Wheel<'_> {
    fn clock(&self) -> u32 {
        self.clock
    }

    fn world_flags(&self) -> u32 {
        self.world_flags
    }

    fn update_byte(&self) -> u8 {
        self.update_byte
    }

    fn random(&mut self) -> u32 {
        self.draws += 1;
        crate::name_entry::name_entry_rng_next(&mut self.rng)
    }

    fn birth_trail(&mut self, _: FurnitureCustomTrail) -> Result<(), String> {
        Err("the wheel births no trail".into())
    }
}

/// Each call starts from its captured entry state. A call whose RNG moved
/// drew the roll; one without births must leave the RNG where native did.
#[test]
fn wheel_vertex_matches_every_infest01_level1_wheel() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_wheels_infest01.json"
    ))
    .unwrap();
    let (_resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = Arc::new(assets.sine_table().values);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let calls = fixture["calls"].as_array().unwrap();
    let mut failures = Vec::new();
    let (mut sprites, mut impacts, mut truck) = (0, 0, 0);
    for (index, call) in calls.iter().enumerate() {
        let call = call.as_array().unwrap();
        let word = |at: usize| call[at].as_u64().unwrap() as u32;
        let extras = [word(0) as u16, word(1) as u16];
        {
            let base = &mut world.arena.owned_mut(vehicle).unwrap().state.base;
            base.flags = (base.flags & !0x4000_0000) | (word(5) & 0x4000_0000);
        }
        for axis in 0..3 {
            world.arena.actor_mut(vehicle).unwrap().rotating.basis.words[4 + axis] = word(6 + axis);
        }
        let state = world.arena.selector2_actor_mut(vehicle).unwrap();
        state.dwords_1e8_1f4[0] = word(9);
        state.dword_228 = Some(word(10));
        state.drive.words_22c_236 = Some(std::array::from_fn(|i| word(11 + i) as u16));
        truck += usize::from(word(9) == 2);
        let mut vertices = ModelVertices::new_invocation(extras[0] + 1);
        vertices.seed_world(
            usize::from(extras[0]),
            [word(2) as i32, word(3) as i32, word(4) as i32],
        );
        let mut effects = Wheel {
            terrain: &world.terrain,
            sine: Arc::clone(&sine),
            executable,
            clock: word(19),
            world_flags: world.world_flags,
            update_byte: word(17) as u8,
            rng: word(18),
            draws: 0,
            births: Vec::new(),
        };
        let mut output = None;
        let result = world
            .arena
            .compute_model_custom_vertex(ObjectCustomVertexRequest {
                source: vehicle,
                compute: ModelCustomVertexRequest {
                    selector: 3,
                    extras: &extras,
                },
                vertices: &vertices,
                output: &mut output,
                effects: Some(&mut effects),
            });
        if let Err(error) = result {
            failures.push(format!("call {index}: {error}"));
            continue;
        }
        let native_output = [word(20), word(21), word(22)];
        let port_output = output.map(|axes| axes.map(|axis| axis as u32));
        let mut found = Vec::new();
        if port_output != Some(native_output) {
            found.push(format!(
                "output port {port_output:x?} native {native_output:x?}"
            ));
        }
        let native_births: Vec<Vec<u64>> = call[24]
            .as_array()
            .unwrap()
            .iter()
            .map(|birth| {
                birth
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_u64().unwrap())
                    .collect()
            })
            .collect();
        if effects.births != native_births {
            found.push(format!(
                "births port {:x?} native {native_births:x?}",
                effects.births
            ));
        }
        let (entry, exit) = (word(18), word(23));
        let drew = u32::from(entry != exit);
        if effects.draws != drew || (native_births.is_empty() && effects.rng != exit) {
            found.push(format!(
                "rng port {} draws to {:x}, native {entry:x} -> {exit:x}",
                effects.draws, effects.rng
            ));
        }
        for birth in &native_births {
            if birth[0] == 0 {
                sprites += 1;
            } else {
                impacts += 1;
            }
        }
        if !found.is_empty() {
            failures.push(format!("call {index}: {}", found.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} wheels differ:\n{}",
        failures.len(),
        calls.len(),
        failures
            .iter()
            .take(20)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    );
    // The recording reaches both births and the truck's longer reach.
    assert!(
        sprites > 0 && impacts > 0 && truck > 0,
        "{sprites} {impacts} {truck}"
    );
}
