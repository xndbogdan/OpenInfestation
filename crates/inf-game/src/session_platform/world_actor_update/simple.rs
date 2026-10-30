//!Exact405140 list forwarding and the shared47AD30 timer of selector 6 and selector16.
//!Admission follows authenticated final classes, never a shape fallback.
use crate::object_runtime::{ActorExtension, ObjectArena, ObjectClass, ObjectError, ObjectId};

pub(crate) fn tick_simple_object_callback(
    arena: &mut ObjectArena,
    object: ObjectId,
    alternate_3c: bool,
    microseconds: u32,
) -> Result<bool, ObjectError> {
    let forwarding = match &arena.object(object)?.class {
        ObjectClass::Base
        | ObjectClass::Selector75(_)
        | ObjectClass::Visual(_)
        | ObjectClass::RotatingVisual(_)
        | ObjectClass::Selector45(_)
        | ObjectClass::TerrainLight { .. }
        | ObjectClass::RotatingTerrainLight { .. }
        | ObjectClass::RouteNode { .. }
        | ObjectClass::MapAnnotation(_) => true,
        // Their +3C is still 00405140.
        ObjectClass::Selector86(_)
        | ObjectClass::TimedPickup(_)
        | ObjectClass::Selector22(_)
        | ObjectClass::Selector65(_)
        | ObjectClass::Selector50(_)
        | ObjectClass::Selector48(_)
        | ObjectClass::Selector43(_)
        | ObjectClass::Selector81(_)
        | ObjectClass::Selector94(_)
        | ObjectClass::Selector42(_)
        | ObjectClass::Selector64(_)
        | ObjectClass::Selector58(_)
        | ObjectClass::Selector83(_)
        | ObjectClass::Selector67(_) => alternate_3c,
        // 72's +24 and +3C are 00405140; 70's and 71's +3C.
        ObjectClass::Race(race) => {
            alternate_3c || matches!(race.part, crate::object_runtime::RacePart::Gate(_))
        }
        ObjectClass::Actor(_) | ObjectClass::Selector76(_) | ObjectClass::Selector46(_) => false,
    };
    if forwarding {
        //405140 does not require the emitter/pickup's normal+24 policy.
        //479F20(false) owns dirty bits and reappend, even when already false.
        arena.set_flag1(object, false)?;
        return Ok(true);
    }
    // 004011F0, the selector-42 wake trigger's own +24.
    if !alternate_3c && arena.selector42(object).is_ok() {
        arena.tick_selector42(object)?;
        return Ok(true);
    }
    if !alternate_3c && arena.selector22(object).is_ok() {
        arena.tick_selector22(object, microseconds)?;
        return Ok(true);
    }
    // 0047AD30 is the +24 of the shared table 00527C00, which selector 6's
    // plain actors and the Remote Beamer's receiver (00526530) keep, and of
    // selector16, which inherits it. After construction only selector 6
    // keeps the shared extension.
    let shared = arena.actor(object).is_ok_and(|actor| {
        matches!(
            actor.extension,
            ActorExtension::Shared | ActorExtension::Selector56
        )
    });
    if !alternate_3c && (shared || arena.selector16(object).is_ok()) {
        arena.tick_actor_common_timer(object, microseconds)?;
        return Ok(true);
    }
    Ok(false)
}
