//! The PAL PlayStation release's (SLES-01870) pad wording for the H0 control
//! hints and shell prompts, in retail's six languages.
//!
//! PC retail's hints name a key, a mode number or a generic action ("Press
//! 'S' to save", "Press mode 2", "the turbo key"). The PlayStation release
//! carries the same H0 global IDs and replaces some of those records with pad
//! wording. This module holds its sentences for the IDs whose PC text names a
//! key, rewritten for the Xbox pad the port plays under
//! `ControllerLayout::Console` (`platform_pad::CONSOLE_BUTTONS`): Cross is A,
//! Circle B, Square X, Triangle Y, L1 LB, R1 RB, L2 LT, R2 RT, Start Start
//! and Select Back. The PS1 inline button bytes `0x17..0x1A` are replaced by
//! those names; they stand for buttons, not text, so none may survive into the
//! table.
//!
//! The level-1 banks put record 173 at global 865 and record 178 at 870, one
//! record per language, so records 174..178 are globals 866..870.
//!
//! Record 0x43 (global 865) is absent: it rewords the factory/scientist
//! content (and names the R1-Circle beam chord), so it is not a control hint.
//! Record 0x49 (global 871) is absent: it changes the save-failure text to
//! the memory card. Three shell prompts are absent because the PS1 button
//! they name is not the one the port binds to that action: global 37 names
//! Start to save, but the results page's save callback `0x0042AAA0` is on
//! Circle (button 14) and Start continues; global 39 names Start to retry,
//! but the name-entry retry callback `0x0045AEA0` is on Circle; global 40
//! names Circle to cancel, but the frontend's back callback `0x004451E0` is
//! on Triangle (button 13). Under the research note's reading of `0x17` as
//! Start those prompts would be false, so the PC text stands.
//!
//! The pad rows point the other way for the icon bytes themselves: save and
//! retry are Circle, and cancel is Triangle, so `0x17` reads as Circle and
//! `0x1A` as Triangle rather than Start and Circle. If the font check
//! confirms that, prompts 37, 39 and 40 become true as "B" and "Y" and
//! belong here, and global 870's button becomes Y.

/// One H0 global and its text in English, French, German, Italian, Spanish
/// and Dutch (the `settings +64` order).
struct Hint {
    global_id: u32,
    texts: [&'static [u8]; 6],
}

const HINTS: [Hint; 6] = [
    // PS1 `1l0..1l5.ovl` record 174 (global 866): "Press L1 to activate big
    // wheeled mode". L1 is the Console layout's LB.
    Hint {
        global_id: 866,
        texts: [
            b"Press LB to activate big wheeled mode",
            b"Appuyez sur LB pour passer en configuration camion.",
            b"Dr\xfccken Sie die LB-Taste, um den Truck-Modus zu aktivieren",
            b"Premi il tasto LB per attivare la modalit\xe0 Bigfoot.",
            b"Pulsa LB para activar el modo de ruedas grandes",
            b"Om de terreinwagen te activeren, druk je op de LB-toets.",
        ],
    },
    // PS1 record 175 (global 867): "Press L1 again to return to normal buggy
    // mode". The PS1's "again" makes the toggle explicit.
    Hint {
        global_id: 867,
        texts: [
            b"Press LB again to return to normal buggy mode",
            b"Appuyez \xe0 nouveau sur LB pour revenir en mode normal (buggy)",
            b"Dr\xfccken Sie erneut die LB-Taste, um zum Buggy-Modus zur\xfcckzukehren",
            b"Premi un'altra volta il tasto LB per ritornare alla modalit\xe0 Buggy.",
            b"Pulsa LB otra vez para volver al modo de buggy normal",
            b"Druk nogmaals op de LB-toets om terug te gaan naar de buggy.",
        ],
    },
    // PS1 record 176 (global 868): "To select an item use up and down and
    // Select (Cross)". The action's button is A.
    Hint {
        global_id: 868,
        texts: [
            b"To select an item use up and down and Select (A)",
            b"Pour choisir un \xe9l\xe9ment, utilisez les touches Haut et Bas, puis Select (A).",
            b"Benutzen Sie zur Auswahl die Steuerung hoch und runter und die Select-Taste (A)",
            b"Per selezionare un oggetto, usa i tasti direzionali su e gi\xf9 e SELECT (selezione)(A).",
            b"Para seleccionar un objeto utiliza arriba, abajo y Select (A)",
            b"Om een voorwerp te selecteren, druk je op omhoog, omlaag en de Selecttoets (A)",
        ],
    },
    // PS1 record 177 (global 869): "Use Cross to activate turbo booster".
    Hint {
        global_id: 869,
        texts: [
            b"Use A to activate turbo booster",
            b"Servez-vous de la touche A pour enclencher le turbo.",
            b"Benutzen Sie A, um den Turbo zu aktivieren",
            b"Usa A per attivare il turbo.",
            b"Utiliza A para activar la aceleraci\xf3n turbo",
            b"Om de turboversterker te activeren, druk je op A",
        ],
    },
    // PS1 record 178 (global 870): "Drop scientists close to a factory
    // (Circle) and they will go to work".
    Hint {
        global_id: 870,
        texts: [
            b"Drop scientists close to a factory (B) and they will go to work",
            b"D\xe9posez les savants \xe0 proximit\xe9 d'une usine (touche B) pour qu'ils se mettent au travail.",
            b"Setzen Sie Wissenschaftler bei einer Fabrik ab (B), und sie beginnen mit der Arbeit",
            b"Scarica gli scienziati vicino a uno stabilimento (B) ed essi cominceranno a lavorare.",
            b"Deja a los cient\xedficos cerca de la f\xe1brica (B) y ellos solos se ir\xe1n a trabajar",
            b"Zet de wetenschappers vlakbij de fabriek af (B) en ze gaan aan het werk.",
        ],
    },
    // PS1 common bank local 1 (global 38): "Start or Cross to continue". The
    // frontend and results pad rows both continue on Start (button 4) or Cross
    // (button 15), so the whole prompt stands.
    Hint {
        global_id: 38,
        texts: [
            b"Start or A to continue",
            b"Start ou A pour continuer",
            b"Start oder A, um fortzufahren",
            b"START (avvio) o A per continuare",
            b"Start o A para continuar",
            b"Om verder te gaan, druk je op de Starttoets of A",
        ],
    },
];

/// The controller wording for H0 global `global_id` in residency language
/// `language` (0..5; others read as English), or None when the PC text stands.
pub fn pad_hint(global_id: u32, language: u8) -> Option<&'static [u8]> {
    let hint = HINTS.iter().find(|hint| hint.global_id == global_id)?;
    let language = if language <= 5 {
        usize::from(language)
    } else {
        0
    };
    Some(hint.texts[language])
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{parse_font_bank, parse_overlay, OverlayKey, Preload};
    use std::fs;
    use std::path::Path;

    const CONTROL_IDS: [u32; 6] = [38, 866, 867, 868, 869, 870];

    #[test]
    fn every_control_id_has_all_six_languages() {
        for global_id in CONTROL_IDS {
            for language in 0..=5 {
                assert!(
                    pad_hint(global_id, language).is_some(),
                    "global {global_id} language {language}"
                );
            }
        }
    }

    #[test]
    fn unknown_and_non_control_ids_keep_the_pc_text() {
        for global_id in [0, 36, 37, 39, 40, 41, 692, 0x48, 865, 871, 0x3a5, u32::MAX] {
            for language in 0..=5 {
                assert_eq!(
                    pad_hint(global_id, language),
                    None,
                    "global {global_id} language {language}"
                );
            }
        }
    }

    #[test]
    fn a_language_past_five_reads_as_english() {
        for global_id in CONTROL_IDS {
            assert_eq!(pad_hint(global_id, 6), pad_hint(global_id, 0));
            assert_eq!(pad_hint(global_id, 0xff), pad_hint(global_id, 0));
        }
    }

    #[test]
    fn texts_are_printable_latin_1_with_no_icon_bytes() {
        for global_id in CONTROL_IDS {
            for language in 0..=5 {
                let text = pad_hint(global_id, language).unwrap();
                for &byte in text {
                    assert!(
                        !(0x17..=0x1a).contains(&byte),
                        "global {global_id} language {language} keeps icon byte {byte:#04x}"
                    );
                    assert!(
                        (0x20..=0x7e).contains(&byte) || (0xa0..=0xff).contains(&byte),
                        "global {global_id} language {language} has non-printable byte {byte:#04x}"
                    );
                }
            }
        }
    }

    /// The strings are drawn in H4 font 0. Every tier but the smallest has the
    /// full Latin-1 range; the 320x240 tier's font is ASCII-only in retail.
    #[test]
    fn every_byte_has_a_glyph_in_retails_h4_font_0() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let preload = Preload::parse(
            &fs::read(root.join("PreLoad.dat")).expect("retail PreLoad.dat is required"),
        )
        .unwrap();
        for tier in 1..=4 {
            let name = format!("0d{tier}.ovl");
            let source = fs::read(root.join("Overlay").join(&name))
                .unwrap_or_else(|error| panic!("retail {name} is required: {error}"));
            let key = OverlayKey::parse_filename(&name).unwrap();
            let layout = parse_overlay(
                &source,
                preload
                    .leaf(key.level)
                    .unwrap()
                    .requests(key.request_kind()),
            )
            .unwrap();
            let regions: Vec<_> = layout
                .regions
                .iter()
                .filter(|region| region.handler.get() == 4)
                .collect();
            assert_eq!(regions.len(), 1, "{name} has one H4 font bank");
            let bank = parse_font_bank(&source, regions[0]).unwrap();
            let font = &bank.fonts[0];
            for global_id in CONTROL_IDS {
                for language in 0..=5 {
                    let text = pad_hint(global_id, language).unwrap();
                    for &byte in text {
                        assert!(
                            i32::from(byte) <= font.maximum_byte
                                && font
                                    .sprite_words
                                    .get(usize::from(byte))
                                    .copied()
                                    .unwrap_or(0)
                                    != 0,
                            "{name} H4 font 0 has no glyph for byte {byte:#04x} \
                             (global {global_id} language {language})"
                        );
                    }
                }
            }
        }
    }
}
