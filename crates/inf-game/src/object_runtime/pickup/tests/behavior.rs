use super::*;
use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    name_entry::name_entry_rng_next,
    object_runtime::{
        ActorHandle, ObjectDrawContext, ObjectModelPacket, PlayerId, TargetNullHandleSource,
    },
};

struct Birth {
    events: Vec<String>,
    object: Option<ObjectId>,
    fail_refresh: bool,
}
impl TargetNullHandleSource for Birth {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0x1234))
    }
}
impl PickupBirthHost for Birth {
    type Error = &'static str;
    fn create_pickup(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: &mut [u16; 3],
        source: ActorHandle,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error> {
        self.events.push(format!(
            "factory:{resource}:{:X}:{position:?}",
            source.raw()
        ));
        let object = arena.allocate(ObjectClassKind::TimedPickup);
        arena
            .initialize_context(
                object,
                BaseContext {
                    position_words: Some(*position),
                    placement_index: u32::MAX,
                },
            )
            .unwrap();
        *position = [10, 20, 30];
        self.object = Some(object);
        Ok(Some(object))
    }
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<(), Self::Error> {
        assert_eq!(arena.timed_pickup(object).unwrap().kind_44, Some(0x83));
        assert_eq!(arena.timed_pickup(object).unwrap().visual.model_word, 0xa8);
        assert_ne!(arena.object(object).unwrap().base.dirty_word & 0x100, 0);
        self.events.push("refresh".into());
        if self.fail_refresh {
            Err("model metadata")
        } else {
            Ok(())
        }
    }
    fn ground_height(&mut self, _: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, Self::Error> {
        self.events.push(format!("ground:{xz:?}"));
        Ok(if xz == [10, 30] { 25 } else { 40 })
    }
}
#[test]
fn pickup_birth_keeps_original_caller_position_loan_and_refresh_failure_prefix() {
    for fail in [false, true] {
        let mut arena = ObjectArena::default();
        let mut host = Birth {
            events: vec![],
            object: None,
            fail_refresh: fail,
        };
        let mut position = [1, 2, 3];
        let result = arena.birth_timed_pickup(
            PickupBirthRequest {
                position: &mut position,
                kind: 0xabcd0083,
                value: 500,
            },
            &mut host,
            &mut 7,
        );
        let object = host.object.unwrap();
        assert_eq!(position, [10, 20, 30]);
        assert_eq!(result.is_err(), fail);
        if fail {
            assert_eq!(arena.timed_pickup(object).unwrap().value_46, 1);
            assert_eq!(host.events.len(), 2);
            assert_eq!(
                arena.object(object).unwrap().base.position_words,
                Some([1, 2, 3])
            );
        } else {
            assert_eq!(arena.timed_pickup(object).unwrap().value_46, 500);
            assert_eq!(
                arena.object(object).unwrap().base.position_words,
                Some([1, 104, 3])
            );
            assert_eq!(&host.events[2..], ["ground:[10, 30]", "ground:[1, 3]"]);
        }
    }
}
#[derive(Default)]
struct Draw {
    events: Vec<&'static str>,
    packet: Option<ObjectModelPacket>,
    color: Option<u32>,
    fail_model: bool,
}
impl PickupDrawHost for Draw {
    type Error = &'static str;
    fn network_flag(&mut self) -> u32 {
        self.events.push("network");
        0
    }
    fn player_for_handle(
        &mut self,
        _: &ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<PlayerId>, Self::Error> {
        panic!("no player")
    }
    fn grant_pickup(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: PlayerId,
        _: &mut u32,
    ) -> Result<bool, Self::Error> {
        panic!("no player")
    }
    fn collection_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: PickupCollectionEffect,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        panic!("no player")
    }
    fn base_color(&mut self) -> u32 {
        self.events.push("color");
        0xa5010203
    }
    fn animation_clock(&mut self) -> u32 {
        self.events.push("clock");
        0
    }
    fn emit_trail(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        _: &MenuBasis,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push("trail");
        name_entry_rng_next(rng);
        arena.owned_mut(object).unwrap().state.base.position_words = Some([8, 9, 10]);
        arena.timed_pickup_mut(object).unwrap().visual.model_word = 777;
        Ok(())
    }
    fn model(
        &mut self,
        _: &mut ObjectArena,
        packet: ObjectModelPacket,
        color: u32,
        _: &ObjectDrawContext,
    ) -> Result<(), Self::Error> {
        self.events.push("model");
        self.packet = Some(packet);
        self.color = Some(color);
        if self.fail_model {
            Err("model")
        } else {
            Ok(())
        }
    }
}
#[test]
fn pickup_draw_captures_model_before_particle_mutation_and_never_replays_failed_prefix() {
    let bytes = raw();
    let exe = FrontendExecutable::parse(&bytes).unwrap();
    let mut process = ObjectDrawProcessState::native_startup(exe).unwrap();
    let mut context = ObjectDrawContext {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        near: 0,
        far: 1000,
        far_color: 0,
    };
    let mut arena = ObjectArena::default();
    let object = arena.allocate(ObjectClassKind::TimedPickup);
    arena
        .initialize_context(
            object,
            BaseContext {
                position_words: Some([100, 200, 300]),
                placement_index: u32::MAX,
            },
        )
        .unwrap();
    arena.write_pickup_kind(object, 0x83).unwrap();
    let mut host = Draw {
        fail_model: true,
        ..Default::default()
    };
    let mut rng = 7;
    let result = arena.draw_timed_pickup(
        object,
        PickupDrawRequest {
            context: &mut context,
            process: &mut process,
            sine_quarter: &[0; 4096],
        },
        &mut host,
        &mut rng,
    );
    assert!(result.is_err());
    assert_eq!(host.events, ["network", "color", "clock", "trail", "model"]);
    assert_eq!(host.packet.unwrap().position, [100, 200, 300]);
    assert_eq!(host.packet.unwrap().model_slot, 0xa8);
    assert_eq!(host.color, Some(0xa57f7f80));
    let prefix_rng = rng;
    let event_count = host.events.len();
    assert!(
        matches!(arena.draw_timed_pickup(object,PickupDrawRequest{context:&mut context,process:&mut process,sine_quarter:&[0;4096]},&mut host,&mut rng),Err(PickupError::PreviouslyStopped(id)) if id==object)
    );
    assert_eq!(rng, prefix_rng);
    assert_eq!(host.events.len(), event_count);
    arena.tick_timed_pickup(object, 10_000_001).unwrap();
    arena
        .destroy(ObjectDestructionRequest {
            object,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    let next = arena.allocate(ObjectClassKind::TimedPickup);
    assert!(!arena.timed_pickup(next).unwrap().draw_stopped);
}
