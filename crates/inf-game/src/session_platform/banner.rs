//! Capture the Banner's announcements (`0046CC50`) with the session's
//! owners: the first player's line and, for a capture by its own unit, the
//! pickup (4, 5000).

use inf_formats::FrontendExecutable;

use super::cheats::{pickup, GrantLoan, GrantTables};
use crate::{
    grant_text::{GrantTextSource, PickupTextData},
    object_runtime::{
        h0_text::{capture_h0_text, H0TextCaptureRequest},
        ActorHandle, BannerHost, ObjectArena, ObjectId, PlayerMessageOperation,
        PlayerMessageRequest,
    },
    process_particles::factories::ParticleFactoryProcessState,
    ResidentRuntime,
};

/// `0046B2B0(4, 5000)` for the capturer.
const CAPTURE_PICKUP: (u16, i16) = (4, 5000);

pub(crate) struct WorldBannerHost<'a> {
    pub residency: &'a ResidentRuntime,
    pub executable: FrontendExecutable<'a>,
    pub process: &'a ParticleFactoryProcessState,
    pub text: Option<&'a PickupTextData<'a>>,
    /// The animation clock `005471C0`.
    pub clock: u32,
}

impl BannerHost for WorldBannerHost<'_> {
    type Error = String;

    fn announce(
        &mut self,
        arena: &mut ObjectArena,
        banner: ObjectId,
        event: u8,
        value: ActorHandle,
    ) -> Result<(), String> {
        let Some((line, captured)) = arena
            .banner_line(banner, event, value)
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let player = arena
            .first_player()
            .ok_or("0046CC50 reaches no first player")?;
        // 00405540: H0 past its count gives no text.
        let source = capture_h0_text(H0TextCaptureRequest {
            index_word: line,
            resident: Some(self.residency),
        })
        .map_err(|error| format!("0046CC50's H0 {line:#x}: {error:?}"))?;
        let text = match source {
            GrantTextSource::Record(Some(text)) => Some(text),
            _ => None,
        };
        let recipient = arena
            .player_message(player)
            .map_err(|error| error.to_string())?
            .identity();
        arena
            .apply_player_message(PlayerMessageRequest {
                recipient,
                operation: PlayerMessageOperation::Send { text, flag: 0 },
            })
            .map_err(|error| error.to_string())?;
        if captured {
            let tables = GrantTables::parse(self.executable)?;
            let text = self
                .text
                .ok_or("the capture's pickup needs the pickup text")?;
            let (id, amount) = CAPTURE_PICKUP;
            pickup(
                arena,
                player,
                id,
                amount,
                &tables,
                GrantLoan {
                    executable: self.executable,
                    residency: self.residency,
                    process: self.process,
                    text,
                    clock: self.clock,
                },
            )?;
        }
        Ok(())
    }
}
