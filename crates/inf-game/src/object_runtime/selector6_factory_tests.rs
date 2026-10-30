//! Actual shared constructor6 custody and native class guards.
//! Factory domains use real resident H13 with an explicit shared H12 miss.

use super::{
    actor_factory_tests::{arena, inputs, math},
    tests::{constructed, request, resource_with_selector, retail_runtime, Commands},
    *,
};
use crate::object_runtime::*;
use inf_formats::{
    AssetRelocation, ByteSpan, InstanceCommand, InstanceField, PlacementInstanceAssets,
};
use std::sync::Arc;

#[derive(Default)]
struct Effects {
    calls: usize,
    fail: bool,
}
impl CreationListenerEffects for Effects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        _listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls += 1;
        let state = arena.object(actor)?;
        assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(state.base.dirty_word, 0);
        assert!(matches!(
            arena.actor(actor)?.extension,
            ActorExtension::Shared
        ));
        assert_eq!(arena.actor(actor)?.word_74, state.base.word_28 >> 3);
        if self.fail {
            return Err(CreationListenerError::UnsupportedInterface(
                listener.interface,
            ));
        }
        Ok(())
    }
}

#[test]
fn selector6_uses_native_shared_profile_without_derived_inputs() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 6);
    let resident = runtime.resource(13, resource as u16).unwrap();
    let region = &resident.overlay.layout().regions[resident.registration.region_index];
    let bank = inf_formats::parse_constructor_asset_bank(resident.overlay.bytes(), region).unwrap();
    let asset = &bank.assets[usize::from(resident.registration.local_index)];
    let profile = asset.dispatch.unwrap();
    assert_eq!(
        (
            profile.selector,
            profile.constructor,
            profile.vtable,
            profile.context_initializer,
            profile.payload_initializer
        ),
        (6, 0x47a7a0, 0x527c00, 0x47a610, 0x47ad70)
    );
    let inf_formats::ConstructorPayloadView::SharedActor(payload) = &asset.payload else {
        panic!("shared payload")
    };
    assert!(payload.derived.is_none());
    let math = math();
    let mut arena = arena();
    let mut listeners = CreationListeners::new();
    let mut effects = Effects::default();
    let commands = Commands::new(
        2,
        &[(8, &4321_u16.to_le_bytes()), (9, &u16::MAX.to_le_bytes())],
    );
    let object = constructed(construct_world_object(
        &mut arena,
        &runtime,
        ObjectConstructionRequest {
            actor: Some(inputs(&math, &mut listeners, &mut effects)),
            suffix: Some(commands.request()),
            ..request(resource)
        },
    ));
    let actor = arena.actor(object).unwrap();
    assert!(matches!(actor.extension, ActorExtension::Shared));
    assert_eq!(actor.health, 4321);
    assert_eq!(actor.word_7a, u16::MAX); // Command9 does not repeat payload clamp.
    assert_eq!(
        actor.word_74,
        arena.object(object).unwrap().base.word_28 >> 3
    );
    assert_eq!(arena.actor_derived_component(object).unwrap(), None);
    assert!(!arena.actor_behavior_presence_7c(object).unwrap());
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
    assert_ne!(
        arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
    arena
        .initialize_after_load(object, Default::default())
        .unwrap();
    assert_ne!(
        arena.object(object).unwrap().base.flags & AFTER_LOAD_FLAG,
        0
    );
    let before = arena.object(object).unwrap().clone();
    assert_eq!(
        arena.update(object),
        Err(ObjectError::ActorUpdateBoundary(object))
    );
    assert_eq!(arena.object(object).unwrap(), &before);
    let handle = arena.actor(object).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.is_empty());
    assert!(arena.actor_list().is_empty());
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}

#[test]
fn selector6_never_consumes_extra_derived_rng_on_success_rejection_or_listener_failure() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 6);
    let math = math();
    for mode in 0..3 {
        let mut arena = arena();
        let prior = arena.allocate(ObjectClassKind::Base);
        let mut listeners = CreationListeners::new();
        if mode == 2 {
            listeners.register(prior, CreationListenerInterface::Selector18);
        }
        let mut effects = Effects {
            calls: 0,
            fail: mode == 2,
        };
        let commands = Commands::codes(2, &[17]); // Missing next pointer and operand remain unreached.
        let mut rng = 0x1234_5678;
        let mut factory = inputs(&math, &mut listeners, &mut effects);
        factory.derived = Some(DerivedActorFactoryInputs {
            world_mask_limits: [i8::MIN, i8::MAX],
            rng: &mut rng,
        });
        let result = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                actor: Some(factory),
                suffix: (mode == 1).then(|| commands.request()),
                ..request(resource)
            },
        );
        assert_eq!(rng, 0x1234_5678);
        match mode {
            0 => {
                let object = constructed(result);
                arena
                    .destroy(crate::object_runtime::ObjectDestructionRequest {
                        object: object,
                        sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                    })
                    .unwrap();
            }
            1 => assert!(matches!(
                result,
                Ok(ObjectConstructionOutcome::Rejected {
                    command_index: 0,
                    code: 17
                })
            )),
            2 => assert!(matches!(
                result,
                Err(ObjectConstructionError::CreationListener(
                    CreationListenerError::UnsupportedInterface(
                        CreationListenerInterface::Selector18
                    )
                ))
            )),
            _ => unreachable!(),
        }
        assert_eq!(effects.calls, usize::from(mode == 2));
        assert_eq!(arena.len(), 1);
        assert!(arena.object(prior).is_ok());
        assert!(arena.actor_list().is_empty());
        assert_eq!(listeners.len(), usize::from(mode == 2));
    }
}

fn owned_health_command() -> (Arc<InstanceCommandAllocation>, PlacementInstanceAssets) {
    let mut source = vec![0_u8; 24];
    source[0..4].copy_from_slice(&1_i32.to_le_bytes());
    source[4..8].copy_from_slice(&16_u32.to_le_bytes());
    source[16..20].copy_from_slice(&8_i32.to_le_bytes());
    source[20..22].copy_from_slice(&1111_u16.to_le_bytes());
    let pointer = ByteSpan::new(4, 8);
    let owner = Arc::new(
        InstanceCommandAllocation::new(
            source.into(),
            ByteSpan::new(0, 24),
            vec![AssetRelocation {
                table_span: ByteSpan::new(0, 0),
                field_offset: 4,
                field_span: pointer,
                target_offset: 16,
            }]
            .into(),
        )
        .unwrap(),
    );
    let placement = PlacementInstanceAssets {
        placement_index: 0,
        suffix_offset: 0,
        count: InstanceField {
            span: ByteSpan::new(0, 4),
            value: 1,
        },
        command_pointer_span: pointer,
        commands: vec![InstanceCommand {
            index: 0,
            pointer_span: pointer,
            allocation_offset: 16,
            code: InstanceField {
                span: ByteSpan::new(16, 20),
                value: 8,
            },
            payload_offset: 20,
        }],
    };
    (owner, placement)
}

#[test]
fn selector6_reads_current_owned_count_and_operand_instead_of_original_source() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 6);
    let math = math();
    for current_count in [0, 1] {
        let (owner, placement) = owned_health_command();
        owner
            .runtime_allocation()
            .write_dword(0, current_count)
            .unwrap();
        owner.runtime_allocation().write_word(20, 4321).unwrap();
        let mut arena = arena();
        let mut listeners = CreationListeners::new();
        let mut effects = Effects::default();
        let object = constructed(construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                actor: Some(inputs(&math, &mut listeners, &mut effects)),
                suffix: Some(owner.commands(&placement)),
                command_allocation: Some(&owner),
                ..request(resource)
            },
        ));
        let resident = runtime.resource(13, resource as u16).unwrap();
        let region = &resident.overlay.layout().regions[resident.registration.region_index];
        let bank =
            inf_formats::parse_constructor_asset_bank(resident.overlay.bytes(), region).unwrap();
        let inf_formats::ConstructorPayloadView::SharedActor(payload) =
            &bank.assets[usize::from(resident.registration.local_index)].payload
        else {
            unreachable!()
        };
        assert_eq!(
            arena.actor(object).unwrap().health,
            if current_count == 1 {
                4321
            } else {
                payload.initial_health.value
            }
        );
        assert_eq!(
            u16::from_le_bytes(owner.source()[20..22].try_into().unwrap()),
            1111
        );
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
    }
}

#[test]
fn derived_base_allocation_keeps_its_additional_constructor_draw() {
    // This checks the actual 00425E60 allocation phase directly. The resident
    // H13 bank above contains shared6, but does not contain constructor7.
    let mut arena = arena();
    let math = math();
    let mut listeners = CreationListeners::new();
    let mut effects = Effects::default();
    let allocation = inputs(&math, &mut listeners, &mut effects).allocation;
    let mut rng = 0x1234_5678;
    let mut expected = rng;
    crate::name_entry::name_entry_rng_next(&mut expected);
    let object = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation,
                world_mask_limits: [3, 3],
            },
            &mut rng,
        )
        .unwrap();
    assert!(matches!(
        arena.actor(object).unwrap().extension,
        ActorExtension::Derived(_)
    ));
    assert_eq!(rng, expected); // Extra425E60 phase only, with an explicit H12 miss.
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
}

#[test]
fn selector6_rejects_foreign_or_opaque_owned_command_backing_and_cleans_new_actor() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 6);
    let math = math();
    for foreign in [false, true] {
        let (owner, placement) = owned_health_command();
        let equal_copy = Arc::new(
            InstanceCommandAllocation::new(
                Arc::from(owner.source()),
                owner.main_payload(),
                Arc::from(owner.relocations()),
            )
            .unwrap(),
        );
        if !foreign {
            owner
                .runtime_allocation()
                .set_runtime_pointer(
                    20,
                    RuntimePointer::Selector60Row(Selector60RowId { arena: 0, index: 0 }),
                )
                .unwrap();
        }
        let mut arena = arena();
        let prior = arena.allocate(ObjectClassKind::Base);
        let mut listeners = CreationListeners::new();
        let mut effects = Effects::default();
        let result = construct_world_object(
            &mut arena,
            &runtime,
            ObjectConstructionRequest {
                actor: Some(inputs(&math, &mut listeners, &mut effects)),
                suffix: Some(owner.commands(&placement)),
                command_allocation: Some(if foreign { &equal_copy } else { &owner }),
                ..request(resource)
            },
        );
        if foreign {
            assert!(matches!(
                result,
                Err(ObjectConstructionError::InstanceAllocation(
                    InstanceCommandAllocationError::SourceIdentity
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ObjectConstructionError::RuntimeAllocation(
                    RuntimeAllocationError::RuntimePointerScalar {
                        offset: 20,
                        field_offset: 20
                    }
                ))
            ));
        }
        assert_eq!(arena.len(), 1);
        assert!(arena.object(prior).is_ok());
        assert!(arena.actor_list().is_empty());
        assert_eq!(effects.calls, 0);
    }
}

#[test]
fn selector6_shared_network_tail_and_mode1_keep_actual_class_and_registration() {
    let runtime = retail_runtime();
    let resource = resource_with_selector(&runtime, 6);
    let math = math();
    for (placement, world_active, local_owner) in [
        (u32::MAX, false, 1),
        (7, true, 1),
        (7, false, 1),
        (7, false, 0),
    ] {
        let mut arena = arena();
        let mut listeners = CreationListeners::new();
        let mut effects = Effects::default();
        let mut factory = inputs(&math, &mut listeners, &mut effects);
        factory.network_world_active = world_active;
        factory.local_owner = local_owner;
        factory.spawn_mode_1 = true;
        let mut construction = request(resource);
        construction.context.placement_index = placement;
        construction.network_active = true;
        construction.actor = Some(factory);
        let object = constructed(construct_world_object(&mut arena, &runtime, construction));
        let handle = arena.actor(object).unwrap().handle;
        assert_ne!(handle.raw() & 0x0400_0000, 0);
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(object));
        let state = arena.object(object).unwrap();
        assert_ne!(state.base.flags & INITIALIZED_FLAG, 0);
        assert!(matches!(
            arena.actor(object).unwrap().extension,
            ActorExtension::Shared
        ));
        if local_owner != 1 {
            assert_ne!(state.base.flags & 0x8000_0000, 0);
            assert_eq!(state.base.dirty_word, 0);
        } else if placement == u32::MAX {
            assert_eq!(state.base.dirty_word, 0x80);
        } else if world_active {
            assert_eq!(state.base.dirty_word, 0);
        } else {
            assert_ne!(state.base.dirty_word & 0x8000, 0);
            assert!(arena
                .list(ObjectList::Disposal)
                .iter()
                .any(|node| node.object == object));
        }
        arena
            .destroy(crate::object_runtime::ObjectDestructionRequest {
                object: object,
                sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            })
            .unwrap();
        assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
        assert!(arena.is_empty());
    }
}

#[test]
fn complete_native_selector6_shared_construction_and_virtual_guards_hold() {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = inf_formats::FrontendExecutable::parse(&bytes).unwrap();
    for (start, end, hash) in [
        (
            0x4603b0,
            0x4603e0,
            "7c239c8736867dc7df8bd0e09774971a695c99c5e4e1602eac53e868e18084f2",
        ),
        (
            0x461a18,
            0x461a1c,
            "beefacd91984b5919a7080b12a6b7ad42a27691d524e28855e5ad51a0de43d05",
        ),
        (
            0x4604f5,
            0x46052d,
            "49b9532456fe1116bbf0f758073b100c6948279517ab3ab4811d502e268fdd06",
        ),
        (
            0x47a7a0,
            0x47aa39,
            "9f2a0cf2da98108bf6fa5d08f8441628ad921648962be06f34c22a30b799d376",
        ),
        (
            0x47a490,
            0x47a602,
            "b224054ec56a4ed7cf37593d7baf2fdcf80c08b9f0bd9681f87e992194278b62",
        ),
        (
            0x425e60,
            0x425f47,
            "bf30316b8a96753a1746060565737be661587136bd06968fe94e80224cb9bec7",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[(start - 0x400000)..(end - 0x400000)])
            ),
            hash
        );
    }
    // Canonical parser authenticates retail PE mapping. Its mapped API is for
    // data/rdata only, so complete .text spans above use authenticated raw=RVA.
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(executable.mapped(0x527c00, 0xcc).unwrap().bytes)
        ),
        "30e2287079f318b296464231064e649a054967935f45248e048adc0a241e61fb"
    );
    for (offset, expected) in [
        (0x04, 0x47ada0_u32),
        (0x08, 0x47a610),
        (0x0c, 0x47ad70),
        (0x10, 0x47a240),
        (0x14, 0x47a020),
        (0x18, 0x404610),
        (0x1c, 0x47aad0),
        (0x20, 0x401320),
        (0x24, 0x47ad30),
        (0x28, 0x401350),
        (0x2c, 0x47d910),
        (0x30, 0x47ec50),
        (0x34, 0x401310),
        (0x38, 0x401370),
        (0x40, 0x47e2e0),
        (0x58, 0x401300),
        (0x64, 0x401300),
        (0x7c, 0x401300),
    ] {
        assert_eq!(
            u32::from_le_bytes(
                executable
                    .mapped(0x527c00 + offset, 4)
                    .unwrap()
                    .bytes
                    .try_into()
                    .unwrap()
            ),
            expected
        );
    }
    // Table DWORD58 is a FUNCTION address401300; its body returns EAX0.
    // A NULL query result does not mean a zero function pointer in the table.
    assert_eq!(&bytes[0x1300..0x1303], &[0x33, 0xc0, 0xc3]);
    assert_eq!(
        u32::from_le_bytes(
            executable
                .mapped(0x527ccc, 4)
                .unwrap()
                .bytes
                .try_into()
                .unwrap()
        ),
        0xbefa_0000
    );
    assert_eq!(
        u32::from_le_bytes(
            executable
                .mapped(0x527cd0, 4)
                .unwrap()
                .bytes
                .try_into()
                .unwrap()
        ),
        0x3f7f_f000
    );
}
