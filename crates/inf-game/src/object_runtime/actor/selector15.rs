//! Selector 15, the factory: 0042E250 over 0040D800 over the shared actor
//! (final 00521940). Construction, the 0042EDD0 payload, commands 0042F100,
//! postinitialization 0042E330, the family after-load 0040DFC0, the 0042E550
//! model callback, the idle arm of light 00431DC0 and the link codes of
//! 0042F380 are owned here; the +24 update 0042EE30 (docking and
//! production) is in [`factory`].

use inf_formats::{
    GrantTable, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
    GRANT_MASK_WORDS,
};

use super::{
    derived::Selector12GrantOwner,
    family40d800::{Actor40d800State, FamilyLinkChange},
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorConstructorNull, ActorExtension, ActorInitializationRequest,
};
use crate::object_runtime::{ActorHandle, ObjectArena, ObjectError, ObjectId};
use crate::ResidentRuntime;
use inf_formats::native_basis::NativeBasisMath;

mod broadcast;
mod factory;
mod shop;
mod staff;
pub use broadcast::{UnitBroadcast, UnitBroadcastCode, UnitBroadcastOutcome};
pub use factory::{FactoryError, FactoryHost, FactoryStaffHost, StaffReleaseHost};
pub(super) use staff::FACTORY_NEUTRALISED_TEXT;

/// The grant owner type both selector 12 (+24C) and selector 15 (+198) embed.
const GRANT_OWNER_VTABLE: u32 = 0x0052_7818;
/// 0042E330 replaces the model with H8 0xBD when +15C is 1.
const SELECTOR15_ALTERNATE_MODEL: u16 = 0xbd;

/// 0042E250's writes after 0040D800.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector15State {
    pub intermediate: Actor40d800State,
    /// The product's build time in microseconds (00431E50); the payload
    /// initializer 0042EE00 zeroes it, not the constructor.
    pub word_158: Option<u32>,
    /// Payload DWORD +50, or command 22. One selects the alternate model
    /// and the "Enemy Factory" shop.
    pub word_15c: u32,
    /// The shop state (00431CE0): 0 Options, 1 Facilities, 2 Energy
    /// Distribution, 3 build, 4 fitting, 5 grant, 6 recovery. The light pass
    /// and network codecs read its low bits.
    pub word_160: u32,
    /// Postinit copies the health WORD +A0.
    pub word_164: Option<u32>,
    /// The recovery timer +168, the build progress +16C, the fitting timer
    /// +170 and the grant timer +174, zeroed by the constructor.
    pub words_168_174: [u32; 4],
    /// Postinit WORDs: +178/+17A low WORDs of +140/+13C, then +17C..+182 zero.
    /// The model callback returns them for selectors 3..8; production
    /// writes its progress at +17E and +180.
    pub words_178_182: Option<[u16; 6]>,
    /// The product's kind (00431E50).
    pub word_184: u32,
    /// The docked vehicle's handle; the process NULL at construction.
    pub handle_188: ActorHandle,
    /// The product's ID and amount, unwritten until 00431E50.
    pub product_18c: Option<u16>,
    pub amount_18e: Option<u16>,
    /// The dock point +190..+194, which the model's custom selector 6
    /// (`0042E506`) captures on every draw. The constructor leaves it
    /// unwritten.
    pub dock_190: Option<[u16; 3]>,
    pub grants_198: Selector12GrantOwner,
    /// The bound shop menu: nonzero while one exists. The port stores 1,
    /// not the native pointer.
    pub word_1b8: u32,
    pub byte_1bc: u8,
    pub byte_1bd: u8,
    pub byte_1be: u8,
    pub byte_1bf: u8,
    pub word_1c0: u32,
    pub word_1c4: u32,
}

impl Selector15State {
    fn new(null: ActorHandle) -> Self {
        Self {
            intermediate: Actor40d800State::new(),
            word_158: None,
            word_15c: 0,
            word_160: 0,
            word_164: None,
            words_168_174: [0; 4],
            words_178_182: None,
            word_184: 0,
            handle_188: null,
            product_18c: None,
            amount_18e: None,
            dock_190: None,
            grants_198: Selector12GrantOwner {
                vtable_00: GRANT_OWNER_VTABLE,
                mask_04: [0; GRANT_MASK_WORDS],
                notification_owner_1c: 0,
            },
            word_1b8: 0,
            byte_1bc: 0,
            byte_1bd: 1,
            byte_1be: 0,
            byte_1bf: 1,
            word_1c0: 0,
            word_1c4: 0,
        }
    }
}

impl ObjectArena {
    pub fn allocate_selector15(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let constructor_null = request.constructor_null;
        let object = self.allocate_actor(request)?;
        // 0042E2A5 rereads 0054E894, the value 0047A7A0 already used.
        let null = match constructor_null {
            ActorConstructorNull::Process(process) => process
                .process_null_handle
                .ok_or(ActorConstructionError::MissingProcessNull)?,
            ActorConstructorNull::RegistrySnapshotApproximation => self
                .actor_handles()
                .ok_or(ActorConstructionError::MissingRegistry)?
                .null_handle(),
        };
        self.actor_mut(object)?.extension =
            ActorExtension::Selector15(Box::new(Selector15State::new(null)));
        Ok(object)
    }

    pub fn selector15(&self, object: ObjectId) -> Result<&Selector15State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector15(state) => Ok(state),
            _ => Err(ObjectError::NotSelector15(object)),
        }
    }

    pub(in crate::object_runtime) fn selector15_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector15State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector15(state) => Ok(state),
            _ => Err(ObjectError::NotSelector15(object)),
        }
    }

    /// Pelagia's egg: the factory takes `product` into its grant mask `+198`,
    /// as retail's commands 23 to 25 do. With `own_product` it also holds the
    /// product as its own `+7A`. Single bits: neither 0xA7 (Hovercraft mode)
    /// nor 0x14 (Headlights) reaches another ID through the grant closure, so
    /// a direct set equals retail's grant.
    pub(crate) fn prepare_pelagia_factory(
        &mut self,
        factory: ObjectId,
        product: u16,
        own_product: bool,
    ) -> Result<(), ObjectError> {
        self.selector15(factory)
            .map_err(|_| ObjectError::NotActor(factory))?;
        if own_product {
            self.actor_mut(factory)
                .map_err(|_| ObjectError::NotActor(factory))?
                .word_7a = product;
        }
        let mask = &mut self
            .selector15_mut(factory)
            .map_err(|_| ObjectError::NotActor(factory))?
            .grants_198
            .mask_04;
        let index = usize::from(product >> 5);
        if let Some(word) = mask.get_mut(index) {
            *word |= 1 << (product & 31);
            Ok(())
        } else {
            Err(ObjectError::NotActor(factory))
        }
    }

    /// 0042EDD0: the shared context, then 0042EE00 over a nonzero payload:
    /// the family 0040D950, +158 = 0 and +15C = payload +50.
    pub(crate) fn initialize_selector15_actor(
        &mut self,
        object: ObjectId,
        request: ActorInitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), ObjectError> {
        self.selector15(object)?;
        let family = request
            .payload
            .family40d950
            .clone()
            .ok_or(ObjectError::MissingFamily40d950Payload(object))?;
        let dword_50 = request
            .payload
            .selector15
            .as_ref()
            .ok_or(ObjectError::MissingFamily40d950Payload(object))?
            .dword_50
            .value;
        self.initialize_actor(object, request, runtime, math)?;
        self.apply_family40d950_payload(object, &family)?;
        let state = self.selector15_mut(object)?;
        state.word_158 = Some(0);
        state.word_15c = dword_50;
        Ok(())
    }

    /// 0042F100: shared commands through 15, 0040D9C0 for 16..20, then code
    /// 21 (accepted, no read), 22 (+15C) and 23..25 (grant into +198).
    pub fn apply_selector15_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
        grants: Option<&GrantTable>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(15) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector15(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector15_mut(object)?;
        if let Some(result) = state.intermediate.apply_command(code, &command.payload) {
            let dirty_0200 = result.map_err(ActorCommandError::Payload)?;
            if dirty_0200 {
                self.owned_mut(object)?.state.base.dirty_word |= 0x0200;
            }
            return Ok(true);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (21, NoPayloadRead) => {}
            (22, Selector15Dword15c(value)) => state.word_15c = value.value,
            (23..=25, Selector15GrantWord(value)) => {
                // The owner's notification binding is null, so 004750C0 is
                // the pure ownership closure.
                let table = grants.ok_or(ActorCommandError::MissingGrantTable(code))?;
                inf_formats::grant_into(
                    &mut state.grants_198.mask_04,
                    table,
                    u32::from(value.value),
                )?;
            }
            (21..=25, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 0042E330, selector 15's +1C. It copies health and the store words,
    /// grants its own +7A into +198 and, when +15C is 1, switches to model
    /// 0xBD and joins team 0x12 if the store holds anything. The family
    /// 0040E000 follows.
    pub(crate) fn finish_selector15_initialization(
        &mut self,
        object: ObjectId,
        runtime: &ResidentRuntime,
        grants: &GrantTable,
    ) -> Result<(), ActorConstructionError> {
        let health = self.actor(object)?.health;
        let id = self.actor(object)?.word_7a;
        let state = self.selector15_mut(object)?;
        state.word_164 = Some(u32::from(health));
        state.words_178_182 = Some([
            state.intermediate.word_140 as u16,
            state.intermediate.word_13c as u16,
            0,
            0,
            0,
            0,
        ]);
        inf_formats::grant_into(&mut state.grants_198.mask_04, grants, u32::from(id))?;
        if state.word_15c == 1 {
            let stored = state.intermediate.word_140;
            self.owned_mut(object)?.state.base.dirty_word |= 0x0100;
            self.actor_mut(object)?.rotating.visual.model_word = SELECTOR15_ALTERNATE_MODEL;
            self.refresh_visual_model(object, runtime)?;
            if stored != 0 {
                self.owned_mut(object)?.state.base.dirty_word |= 2;
                self.actor_mut(object)?.word_b4 = 0x12;
            }
        }
        Ok(self.finish_family40d800_initialization(object)?)
    }

    /// 0042F380, selector 15's +98, for the link codes, which reach the
    /// family's 0040DE70. Codes 6, 0xD and 0x18 have their own arms.
    pub(super) fn selector15_message(
        &mut self,
        object: ObjectId,
        code: i32,
    ) -> Result<FamilyLinkChange, ObjectError> {
        if matches!(code, 6 | 0xd | 0x18) {
            return Err(ObjectError::LinkMessageBoundary(object));
        }
        self.family40d800_message(object, code)
    }
}

/// 0042E550, selector 15's +14: selectors 3..8 read the postinit WORDs at
/// +178..+182; every other selector returns itself.
pub(crate) fn selector15_value(state: &Selector15State, selector: u32) -> Option<u16> {
    if (3..9).contains(&(selector as i32)) {
        return state
            .words_178_182
            .map(|words| words[selector as usize - 3]);
    }
    Some(selector as u16)
}

/// 00431DC0, selector 15's +28. Production states other than 4, 5 and 6
/// return without a light; those three draw RNG-scaled terrain lights.
pub(crate) fn selector15_light_is_idle(state: &Selector15State) -> bool {
    !matches!(state.word_160, 4..=6)
}

#[cfg(test)]
#[path = "selector15_tests.rs"]
mod tests;
