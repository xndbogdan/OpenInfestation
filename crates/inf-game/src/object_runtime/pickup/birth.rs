//! 0046D870 preserves the original caller position through the factory and
//! subsequent terrain comparison. It never resolves an actor component.
use super::*;
use crate::object_runtime::{ActorHandle, TargetNullHandleSource};

pub struct PickupBirthRequest<'a> {
    pub position: &'a mut [u16; 3],
    pub kind: u32,
    pub value: u16,
}

pub trait PickupBirthHost: TargetNullHandleSource {
    type Error;
    /// 00460310: NULL suffix/rotation, placement -1, mode0. The callback
    /// reads this exact caller position only at virtual+C, after allocation.
    fn create_pickup(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: &mut [u16; 3],
        source: ActorHandle,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, Self::Error>;
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
    ) -> Result<(), Self::Error>;
    /// Actual terrain getter followed by 00450A30(x,z,0).
    fn ground_height(&mut self, arena: &mut ObjectArena, xz: [u16; 2]) -> Result<u16, Self::Error>;
}

impl ObjectArena {
    pub fn birth_timed_pickup<H: PickupBirthHost + ?Sized>(
        &mut self,
        request: PickupBirthRequest<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<Option<ObjectId>, PickupError<H::Error>> {
        let kind = request.kind as u16;
        let resource = if kind == 0x83 { 10 } else { 9 };
        let source = host
            .target_null_handle()
            .ok_or(PickupError::MissingProcessNull)?;
        let Some(object) = host
            .create_pickup(self, resource, request.position, source, rng)
            .map_err(|error| PickupError::Service {
                address: 0x0046_0310,
                error,
            })?
        else {
            return Ok(None);
        };
        // Native discards 46D500's false return. Known model changes retain
        // their prior type/model/dirty writes if metadata refresh stops.
        if self.write_pickup_kind(object, kind)?.is_some() {
            host.refresh_model(self, object)
                .map_err(|error| PickupError::Service {
                    address: 0x0047_a110,
                    error,
                })?;
        }
        self.timed_pickup_mut(object)?.value_46 = request.value;
        let [x, _, z] = *request.position;
        let ground = host
            .ground_height(self, [x, z])
            .map_err(|error| PickupError::Service {
                address: 0x0045_0a30,
                error,
            })?;
        if (request.position[1] as i16) < (ground as i16) {
            // 479DD0 acquires terrain afresh and queries the object's own
            // XYZ, independently of the original caller stack position.
            let position =
                self.object(object)?
                    .base
                    .position_words
                    .ok_or(PickupError::RequiredField {
                        object,
                        offset: 0x32,
                    })?;
            let ground = host
                .ground_height(self, [position[0], position[2]])
                .map_err(|error| PickupError::Service {
                    address: 0x0047_9dd0,
                    error,
                })?;
            self.owned_mut(object)?
                .state
                .base
                .position_words
                .as_mut()
                .ok_or(PickupError::RequiredField {
                    object,
                    offset: 0x32,
                })?[1] = ground;
        }
        if kind != 7 && !(0x0e..=0x10).contains(&kind) {
            let position = self
                .owned_mut(object)?
                .state
                .base
                .position_words
                .as_mut()
                .ok_or(PickupError::RequiredField {
                    object,
                    offset: 0x32,
                })?;
            position[1] = position[1].wrapping_add(64);
        }
        Ok(Some(object))
    }
}
