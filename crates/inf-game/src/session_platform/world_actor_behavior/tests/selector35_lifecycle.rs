//! Supplied descriptor scheduling over real world, particle and terrain owners.
//! This does not supply or approximate the common first +19C timer.
use super::*;
use crate::object_runtime::{ActorExtension, DerivedActorClass, Family428650Class, ObjectList};

fn actor(
    arena: &mut ObjectArena,
    host: &WorldFamilyBehaviorHost<'_, '_>,
    alternate: bool,
) -> ObjectId {
    let y = host.terrain.height_at([4096, 4096]) as u16;
    let actor = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([4096, y, 4096]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: host.sine_quarter,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 1,
        )
        .unwrap();
    arena
        .initialize_visual_context(
            actor,
            crate::object_runtime::BaseContext {
                position_words: None,
                placement_index: u32::MAX,
            },
            None,
            host.residency,
        )
        .unwrap();
    arena
        .with_supplied_family_dispatch_actor(actor, |state| {
            state.word_c4 = 1;
            let ActorExtension::Derived(derived) = &mut state.extension else {
                panic!("derived")
            };
            let DerivedActorClass::Family428650(family) = &mut derived.class else {
                panic!("family")
            };
            family.words_188_19c[0] = Some(0);
            let Family428650Class::Selector35(state) = &mut family.class else {
                panic!("selector35")
            };
            state.byte_251 = Some(u8::from(alternate));
        })
        .unwrap();
    actor
}

#[test]
fn selector35_lifecycle_production_wrappers_emit_and_retire_by_actual_b0() {
    with_host(|arena, host| {
        let profile = FamilyBehaviorProfile::Selector35;
        assert_eq!(profile.descriptor(1).unwrap().change_address, 0x43fe40);
        assert_eq!(profile.descriptor(1).unwrap().update_address, 0x435240);
        assert_eq!(host.executable.word(0x52273c + 0xb0).unwrap(), 0x43e250);
        for alternate in [false, true] {
            let actor = actor(arena, host, alternate);
            let rng = *host.process_rng;
            FamilyBehaviorHost::change(host, arena, actor, profile, 1, true).unwrap();
            assert_ne!(*host.process_rng, rng);
            let target = arena.derived_actor(actor).unwrap().target_148.unwrap();
            assert_eq!(
                arena.native_target(target).unwrap().handle_04(),
                host.factory_process.process_null_handle.unwrap()
            );
            FamilyBehaviorHost::update(host, arena, actor, profile, 1, 125000).unwrap();
            assert_eq!(
                arena
                    .native_target(target)
                    .unwrap()
                    .raw_focus_08_words()
                    .unwrap()[1],
                0xec78
            );
            // The fixture's actual Title phase selects the shared scratch
            // birth. No particle-return substitution is used by the adapter.
            let scratch = host
                .particles
                .record(crate::process_particles::ParticleId::Scratch)
                .unwrap();
            assert_ne!(scratch.read_dword(0).unwrap(), 0);
            arena
                .with_supplied_family_dispatch_actor(actor, |state| {
                    let ActorExtension::Derived(derived) = &mut state.extension else {
                        panic!("derived")
                    };
                    let DerivedActorClass::Family428650(family) = &mut derived.class else {
                        panic!("family")
                    };
                    family.words_188_19c[0] = Some(15_000_001);
                })
                .unwrap();
            FamilyBehaviorHost::update(host, arena, actor, profile, 1, 125000).unwrap();
            assert_eq!(arena.actor(actor).unwrap().word_c4, 0);
            assert_eq!(
                arena
                    .list(ObjectList::Disposal)
                    .iter()
                    .any(|node| node.object == actor),
                !alternate
            );
        }
    });
}

#[test]
fn selector35_lifecycle_production_dormant_false_entry_and_update_use_real_births() {
    with_host(|arena, host| {
        let actor = actor(arena, host, false);
        // Native bit31 bypasses only the lower radius branch. It does not
        // bypass the mandatory burst on false dormant entry.
        let flags = arena.object(actor).unwrap().base.flags;
        arena
            .apply_payload_flags(actor, flags | 0x8000_0000)
            .unwrap();
        let before = arena.family428650_actor(actor).unwrap().word_228;
        let rng = *host.process_rng;
        FamilyBehaviorHost::change(
            host,
            arena,
            actor,
            FamilyBehaviorProfile::Selector35,
            0,
            false,
        )
        .unwrap();
        assert_ne!(*host.process_rng, rng);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_228, before);
        host.particle_load_factor = 0xc000;
        FamilyBehaviorHost::update(
            host,
            arena,
            actor,
            FamilyBehaviorProfile::Selector35,
            0,
            125000,
        )
        .unwrap();
        assert_ne!(
            host.particles
                .record(crate::process_particles::ParticleId::Scratch)
                .unwrap()
                .read_dword(0)
                .unwrap(),
            0
        );
    });
}
