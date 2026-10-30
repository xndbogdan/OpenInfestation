//! The Options rows both launchers list: their entries, the fix rows and
//! the upscalers this machine's GPU runs.

use inf_game::frontend_settings::PortFixes;
use inf_game::platform_pad::ControllerLayout;
use inf_game::presentation::{ClassicScaling, RendererChoice};

/// The Display row's entries, in [`DisplayMode::ALL`]'s order.
pub(super) const DISPLAY_LABELS: [&str; 3] = ["Window", "Full Screen", "Borderless"];
pub(super) const RENDERERS: [(RendererChoice, &str); 4] = [
    (RendererChoice::Auto, "Automatic"),
    (RendererChoice::OpenGl, "OpenGL (Vanilla)"),
    (RendererChoice::Vulkan, "Vulkan (Remastered)"),
    (RendererChoice::Software, "Software"),
];
/// The Camera style row's entries, settings `+6C`'s values: retail's two,
/// then the port's remastered camera.
pub(super) const CAMERA_STYLES: [&str; 3] = ["Normal", "Cinematic", "Remastered"];
pub(super) const CONTROLLER_LAYOUTS: [(ControllerLayout, &str); 2] = [
    (ControllerLayout::Pc, "PC original"),
    (ControllerLayout::Console, "Console original (PlayStation)"),
];
pub(super) const SCALINGS: [(ClassicScaling, &str); 4] = [
    (ClassicScaling::Widescreen, "Widescreen"),
    (ClassicScaling::FourThree, "Preserve 4:3"),
    (ClassicScaling::Stretched, "Stretched"),
    (ClassicScaling::Integer, "Integer"),
];
/// The Options' fix rows, in [`PortFixes`]' field order: the retail bug
/// fixes, then the Cheats and extras group's rows.
pub(super) const FIX_LABELS: [&str; FIXES] = [
    "The ACAM keeps to the ground at high frame rates",
    "No black band crosses the ACAM's roof as the hovercraft morphs",
    "The hovercraft's rear fans are not drawn inside out (black)",
    "The world keeps its speed at high frame rates",
    "The database draws every entry's model",
    "Name entry and language animations keep their speed",
    "The guided missiles' launchers fold away between forms",
    "A finished campaign in the saves gives every game its bonus",
    "Scientists, alien ships and creatures move as at 30 fps",
    "Water splashes take at most 12 of the 24 sounds",
    "A rescue completes when its last scientist boards",
    "The computer's Cheats Menu shows every cheat",
    "Pelagia: a scientist wakes the Command Centre's dead warpgate",
    "Pelagia rework: the lagoon, crystals, return gate and factories reshape world 1",
];
pub(super) const FIXES: usize = 14;
/// The rows of the retail bug fixes' group; the rest are the Cheats and
/// extras group's.
pub(super) const BUG_FIX_ROWS: usize = 11;
/// The Cheats and extras rows for Pelagia and its rework: the rework needs
/// the gate, so it goes dark and off while the gate is off.
pub(super) const FIX_PROTOTYPE_WORLD: usize = 12;
pub(super) const FIX_PELAGIA_REWORK: usize = 13;

pub(super) fn fix_values(fixes: PortFixes) -> [bool; FIXES] {
    [
        fixes.vehicle_position_carry,
        fixes.vehicle_gun_mounts,
        fixes.hovercraft_fans,
        fixes.exact_world_ticks,
        fixes.database_models,
        fixes.exact_frontend_ticks,
        fixes.guided_mounts,
        fixes.completion_bonus,
        fixes.paced_actors,
        fixes.splash_voice_cap,
        fixes.mission_completion,
        fixes.all_cheats,
        fixes.prototype_world,
        fixes.pelagia_rework,
    ]
}

pub(super) fn fixes_from(values: [bool; FIXES]) -> PortFixes {
    let [vehicle_position_carry, vehicle_gun_mounts, hovercraft_fans, exact_world_ticks, database_models, exact_frontend_ticks, guided_mounts, completion_bonus, paced_actors, splash_voice_cap, mission_completion, all_cheats, prototype_world, pelagia_rework] =
        values;
    PortFixes {
        vehicle_position_carry,
        vehicle_gun_mounts,
        hovercraft_fans,
        exact_world_ticks,
        database_models,
        exact_frontend_ticks,
        guided_mounts,
        completion_bonus,
        paced_actors,
        splash_voice_cap,
        mission_completion,
        all_cheats,
        prototype_world,
        pelagia_rework,
    }
}

/// The Vulkan group's entries: the upscalers, their quality modes and the
/// anti-aliasing methods.
pub(super) const UPSCALERS: [(inf_game::modern_settings::Upscaler, &str); 5] = {
    use inf_game::modern_settings::Upscaler;
    [
        (Upscaler::Off, "Off"),
        (Upscaler::Fsr, "AMD FSR 3.1"),
        (Upscaler::Dlss, "NVIDIA DLSS"),
        (
            Upscaler::DlssRayReconstruction,
            "NVIDIA DLSS Ray Reconstruction",
        ),
        (Upscaler::Xess, "Intel XeSS"),
    ]
};
pub(super) const UPSCALE_QUALITIES: [(inf_game::modern_settings::UpscaleQuality, &str); 5] = {
    use inf_game::modern_settings::UpscaleQuality;
    [
        (UpscaleQuality::NativeAa, "Native AA"),
        (UpscaleQuality::Quality, "Quality"),
        (UpscaleQuality::Balanced, "Balanced"),
        (UpscaleQuality::Performance, "Performance"),
        (UpscaleQuality::UltraPerformance, "Ultra Performance"),
    ]
};
pub(super) const LIGHTINGS: [(inf_game::modern_settings::Lighting, &str); 2] = {
    use inf_game::modern_settings::Lighting;
    [
        (Lighting::Remaster, "Remaster"),
        (Lighting::EyeCandy, "Eye candy"),
    ]
};
pub(super) const ANTI_ALIASINGS: [(inf_game::modern_settings::AntiAliasing, &str); 6] = {
    use inf_game::modern_settings::AntiAliasing;
    [
        (AntiAliasing::Off, "Off"),
        (AntiAliasing::Fxaa, "FXAA"),
        (AntiAliasing::Smaa, "SMAA"),
        (AntiAliasing::Taa, "TAA"),
        (AntiAliasing::Ssaa2x, "SSAA 2x"),
        (AntiAliasing::Ssaa4x, "SSAA 4x"),
    ]
};

/// The Upscaler list's entries: those the GPU runs, Ray Reconstruction
/// (which denoises traced light) only while the world is traced.
pub(super) fn listed_upscalers(
    traced: bool,
    support: inf_game::modern_settings::ModernSupport,
) -> Vec<(inf_game::modern_settings::Upscaler, &'static str)> {
    use inf_game::modern_settings::Upscaler;
    UPSCALERS
        .into_iter()
        .filter(|&(choice, _)| {
            support.runs(choice) && (traced || choice != Upscaler::DlssRayReconstruction)
        })
        .collect()
}

/// The Upscaler entry shown for the saved `choice`: the one the game runs
/// in its place (Ray Reconstruction as DLSS while untraced, one the GPU
/// lacks as the next best it has), as the Video menu's row shows it.
pub(super) fn shown_upscaler(
    traced: bool,
    choice: inf_game::modern_settings::Upscaler,
    support: inf_game::modern_settings::ModernSupport,
) -> inf_game::modern_settings::Upscaler {
    inf_game::modern_settings::ModernSettings {
        ray_tracing: traced,
        upscaler: choice,
        ..Default::default()
    }
    .effective(support)
    .upscaler
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_game::display_selection::DisplayMode;
    use inf_game::modern_settings::ModernSupport;

    #[test]
    fn the_fix_rows_cover_every_port_fix_in_order() {
        // The bug fixes and Pelagia default on; the Cheats row's option
        // defaults off.
        let defaults = PortFixes::default();
        let mut expected = [true; FIXES];
        expected[BUG_FIX_ROWS] = false;
        assert_eq!(fix_values(defaults), expected);
        assert_eq!(fixes_from(expected), defaults);
        for index in 0..FIXES {
            let mut values = [true; FIXES];
            values[index] = false;
            assert_eq!(fix_values(fixes_from(values)), values);
        }
        let fixes = fixes_from([
            false, true, true, true, true, true, true, true, true, true, true, false, true, true,
        ]);
        assert!(!fixes.vehicle_position_carry && fixes.completion_bonus);
        let fixes = fixes_from([
            true, true, true, true, true, true, true, false, true, true, true, false, true, true,
        ]);
        assert!(fixes.vehicle_position_carry && !fixes.completion_bonus);
        let fixes = fixes_from([
            true, true, true, true, true, true, true, true, true, false, true, false, true, true,
        ]);
        assert!(!fixes.splash_voice_cap && fixes.paced_actors);
        assert!(fixes_from([true; FIXES]).all_cheats);
        assert!(!fixes_from(std::array::from_fn(|index| index != FIXES - 1)).pelagia_rework);
        assert!(fixes_from(std::array::from_fn(|index| index != FIXES - 1)).prototype_world);
        // The rework row sits below the gate row it needs.
        assert!(FIX_PROTOTYPE_WORLD < FIX_PELAGIA_REWORK);
        assert!(FIX_PELAGIA_REWORK < FIXES);
        assert!(FIX_LABELS[FIX_PROTOTYPE_WORLD].contains("dead warpgate"));
        assert!(FIX_LABELS[FIX_PELAGIA_REWORK].contains("rework"));
    }

    /// Ray Reconstruction is offered only while Ray Tracing is on; the
    /// rest keep their order, Off first.
    #[test]
    fn ray_reconstruction_is_listed_only_while_traced() {
        use inf_game::modern_settings::Upscaler;
        let names = |traced| {
            listed_upscalers(traced, ModernSupport::default())
                .iter()
                .map(|(choice, _)| *choice)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(true), UPSCALERS.map(|(choice, _)| choice));
        assert_eq!(
            names(false),
            [Upscaler::Off, Upscaler::Fsr, Upscaler::Dlss, Upscaler::Xess]
        );
    }

    /// On a GPU without DLSS (an AMD card) the list drops DLSS and Ray
    /// Reconstruction, and a saved DLSS shows as FSR, the one that runs.
    #[test]
    fn the_upscaler_list_is_what_the_gpu_runs() {
        use inf_game::modern_settings::Upscaler;
        let amd = ModernSupport {
            upscalers: [true, true, false, false, true],
            ..ModernSupport::default()
        };
        let names = |traced| {
            listed_upscalers(traced, amd)
                .iter()
                .map(|(choice, _)| *choice)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(true), [Upscaler::Off, Upscaler::Fsr, Upscaler::Xess]);
        assert_eq!(names(false), names(true));
        for saved in [Upscaler::Dlss, Upscaler::DlssRayReconstruction] {
            assert_eq!(shown_upscaler(true, saved, amd), Upscaler::Fsr);
        }
        assert_eq!(shown_upscaler(true, Upscaler::Xess, amd), Upscaler::Xess);
        assert_eq!(
            shown_upscaler(
                false,
                Upscaler::DlssRayReconstruction,
                ModernSupport::default()
            ),
            Upscaler::Dlss
        );
    }

    #[test]
    fn the_option_rows_list_every_choice() {
        assert_eq!(DISPLAY_LABELS.len(), DisplayMode::ALL.len());
        for (index, mode) in DisplayMode::ALL.into_iter().enumerate() {
            assert_eq!(mode.index() as usize, index);
        }
        assert_eq!(
            RENDERERS.map(|(choice, _)| choice),
            [
                RendererChoice::Auto,
                RendererChoice::OpenGl,
                RendererChoice::Vulkan,
                RendererChoice::Software
            ]
        );
        assert_eq!(
            SCALINGS.map(|(choice, _)| choice),
            [
                ClassicScaling::Widescreen,
                ClassicScaling::FourThree,
                ClassicScaling::Stretched,
                ClassicScaling::Integer
            ]
        );
    }
}
