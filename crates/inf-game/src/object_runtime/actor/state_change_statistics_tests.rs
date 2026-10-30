use super::*;
use crate::object_runtime::{ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs};
use crate::world_mission::RECORD_0C_BYTES;
use crate::world_statistics::{
    ActorRetirementAttribution, ActorStatisticCategory, WorldStatistics,
};

const NULL: ActorHandle = ActorHandle::from_raw(0xffff_ffff);

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: NULL,
    })
    .unwrap()
}

fn actor(arena: &mut ObjectArena, b4: u32, flags: u32) -> ObjectId {
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap();
    arena.actor_mut(actor).unwrap().word_b4 = b4;
    arena.owned_mut(actor).unwrap().state.base.flags = flags;
    actor
}

#[test]
fn null_and_stale_sources_use_category_four_without_borrowing_target_class() {
    let mut arena = arena();
    let target = actor(&mut arena, 18, 0);
    let source = actor(&mut arena, 1, 0x10);
    let handle = arena.actor(source).unwrap().handle;
    assert_eq!(
        arena.retirement_attribution(target, handle).unwrap(),
        ActorRetirementAttribution {
            target: ActorStatisticCategory::B4Eighteen,
            source: ActorStatisticCategory::Flag10,
        }
    );
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: source,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    for handle in [NULL, handle] {
        assert_eq!(
            arena.retirement_attribution(target, handle).unwrap(),
            ActorRetirementAttribution {
                target: ActorStatisticCategory::B4Eighteen,
                source: ActorStatisticCategory::B4ZeroOrUnresolved,
            }
        );
    }
}

struct RetirementHost<'a> {
    statistics: WorldStatistics<'a>,
    attributed: Option<ActorRetirementAttribution>,
}

impl ActorStateChangeHost for RetirementHost<'_> {
    type Error = &'static str;
    type AttributionOwner = ();

    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        match query {
            ActorStateChangeQuery::ExcludesAttribution68 | ActorStateChangeQuery::HasDerived58 => {
                Ok(false)
            }
        }
    }

    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        let attribution = arena.retirement_attribution(actor, source).unwrap();
        self.statistics.record_attribution(attribution);
        self.attributed = Some(attribution);
        Ok(())
    }

    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), Self::Error> {
        match effect {
            ActorStateChangeEffect::IncrementSelector18Statistic { .. } => {
                self.statistics.increment_selector18_retirements();
                Ok(())
            }
            ActorStateChangeEffect::StateOne { .. } => Err("unowned state-one callback"),
            _ => panic!("unexpected effect {effect:?}"),
        }
    }
}

#[test]
fn retirement_statistics_survive_the_later_required_state_one_failure() {
    let mut arena = arena();
    let target = actor(&mut arena, 18, 0);
    let mut record = [0; RECORD_0C_BYTES];
    let mut host = RetirementHost {
        statistics: WorldStatistics::from_record(&mut record),
        attributed: None,
    };
    assert!(arena.retire_actor(target, Some(NULL), &mut host).is_err());
    let attribution = host.attributed.unwrap();
    assert_eq!(
        attribution.source,
        ActorStatisticCategory::B4ZeroOrUnresolved
    );
    assert_eq!(host.statistics.attribution_count(attribution), 1);
    assert_eq!(host.statistics.selector18_retirements(), 1);
    assert_eq!(arena.actor(target).unwrap().word_c4, 1);
    assert_eq!(arena.actor(target).unwrap().health, 0);
    assert_eq!(arena.object(target).unwrap().base.flags & 0x8000, 0x8000);
}
