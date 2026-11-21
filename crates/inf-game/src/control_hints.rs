//! The PAL PlayStation release's (SLES-01870) pad wording for the H0 control
//! hints and shell prompts, in retail's six languages.
//!
//! PC retail's hints name a key, a mode number or a generic action ("Press
//! 'S' to save", "Press mode 2", "the turbo key"). The PlayStation release
//! carries the same H0 global IDs and replaces some of those records with pad
//! wording. This module holds its sentences for the IDs whose PC text names a
//! key, as templates whose button words are `{...}` tokens:
//!
//! | Token | Console layout | Remastered layout |
//! | --- | --- | --- |
//! | `{CONFIRM}` | Cross | A |
//! | `{SAVE}` (save and retry) | Circle | X |
//! | `{BACK}` (cancel) | Triangle | B |
//! | `{MODE}` | L1 | B |
//! | `{TURBO}` | Cross | A |
//! | `{SELECT}` (message select) | Cross | A |
//! | `{DROP}` (beam down) | Triangle | Y |
//! | `{START}` | Start | Start |
//!
//! [`pad_hint`] fills them for a layout and pad family. The Console layout is
//! the PlayStation pad the port plays under `ControllerLayout::Console`
//! (`platform_pad::CONSOLE_BUTTONS`); with an Xbox pad its names are the
//! Microsoft ones (Cross is A, Circle B, Square X, Triangle Y, L1 LB, R1 RB,
//! L2 LT, R2 RT, Start Start, Select Back), and with a PlayStation pad they
//! are the PS1's own (Cross, Circle, Square, Triangle, L1, R1, L2, R2, Start,
//! Select). The Remastered layout names its own bindings (mode B, turbo and
//! select A, drop and beam down Y, confirm A, back B, save and retry X, start
//! Start), and with a PlayStation pad those are the modern Sony names
//! (Cross, Circle, Square, Triangle, L1, R1, L2, R2, Options, Create).
//!
//! The PS1 inline button bytes `0x17..0x1A` are replaced by those names; they
//! stand for buttons, not text, so none may survive into the table.
//!
//! Record 0x43 (global 865) is absent: it rewords the factory/scientist
//! content (and names the R1-Triangle beam chord), so it is not a control
//! hint. Record 0x49 (global 871) is absent: it changes the save-failure
//! text to the memory card. All four shell prompts are present: the results
//! save `0042AAA0` and the name-entry retry `0045AEA0` are Circle, the
//! frontend/results/name-entry accepts are Cross or Start, and the frontend
//! back `004451E0` is Triangle, exactly the buttons 37..40 name.
//!
//! The icon identities come from the type-9 pad rows the PC executable
//! carries in each screen's unused slots 8/9; `ControllerLayout::Console`
//! plays them as the PlayStation pad, whose codes 1..16 are Select, L3, R3,
//! Start, Up, Right, Down, Left, L2, R2, L1, R1, Triangle, Circle, Cross and
//! Square. `0x18` is Cross: the gameplay message-select row `004697F0` and
//! the thrust consumer `0053A378` require code 15, and the frontend, results
//! and name-entry accepts take 15 (or Start 4). `0x17` is Circle: the
//! results save `0042AAA0` and the name-entry retry `0045AEA0` require code
//! 14. `0x1A` is Triangle: the frontend back `004451E0` and the gameplay
//! drop row `00469670` require code 13, and the beam-up chord `00469660`
//! takes 13 with R1 (12), matching global 865's `R1-\x1a`. `0x19` appears in
//! no string; Square is the only face button left, but nothing pins it.
//!
//! The level-1 banks put record 173 at global 865 and record 178 at 870, one
//! record per language, so records 174..178 are globals 866..870.

use std::borrow::Cow;

/// The pad family a wording names: Microsoft's positional A, B, X, Y and
/// LB, RB, LT, RT, or Sony's Cross, Circle, Square, Triangle and L1, R1, L2,
/// R2. The pad's family is live SDL state (SDL's game-controller type), so a
/// recording stores it to keep a replay off the live device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PadFamily {
    /// Microsoft's names; every pad SDL does not type as a PlayStation pad.
    #[default]
    Xbox,
    /// Sony's names, for SDL's PS3, PS4 and PS5 controller types.
    PlayStation,
}

impl PadFamily {
    /// The word a session recording writes for this family.
    pub fn word(self) -> &'static str {
        match self {
            Self::Xbox => "xbox",
            Self::PlayStation => "playstation",
        }
    }

    /// A recording's family word; unknown words are refused.
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "xbox" => Some(Self::Xbox),
            "playstation" => Some(Self::PlayStation),
            _ => None,
        }
    }
}

/// Which wording a hint or a shell prompt uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PadWording {
    /// Retail's PC text, byte for byte.
    #[default]
    Retail,
    /// The PS1 sentences, named for the Console layout's pad family.
    Console(PadFamily),
    /// The PS1 sentences, named for the Remastered layout's pad family.
    Remastered(PadFamily),
}

/// A button word a template can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Mode,
    Turbo,
    Select,
    Drop,
    Confirm,
    Save,
    Back,
    Start,
}

impl Token {
    fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"MODE" => Some(Self::Mode),
            b"TURBO" => Some(Self::Turbo),
            b"SELECT" => Some(Self::Select),
            b"DROP" => Some(Self::Drop),
            b"CONFIRM" => Some(Self::Confirm),
            b"SAVE" => Some(Self::Save),
            b"BACK" => Some(Self::Back),
            b"START" => Some(Self::Start),
            _ => None,
        }
    }
}

/// The physical button a token means on a layout. The family only renames it.
#[derive(Clone, Copy)]
enum Button {
    /// SDL's bottom face button: Cross, A.
    Bottom,
    /// SDL's right face button: Circle, B.
    Right,
    /// SDL's left face button: Square, X.
    Left,
    /// SDL's top face button: Triangle, Y.
    Top,
    /// The left shoulder: L1, LB.
    LeftShoulder,
    /// Start, or Options on a modern PlayStation pad.
    Start,
}

/// The physical button a layout binds a token to.
fn button(wording: PadWording, token: Token) -> Button {
    if token == Token::Start {
        return Button::Start;
    }
    match wording {
        // The Console layout's tables are the PS1's: mode L1, save Circle,
        // back and drop Triangle, confirm, turbo and select Cross.
        PadWording::Console(_) => match token {
            Token::Mode => Button::LeftShoulder,
            Token::Save => Button::Right,
            Token::Back | Token::Drop => Button::Top,
            _ => Button::Bottom,
        },
        // The Remastered layout binds mode and back B, save X, drop Y and the
        // rest A (`remastered_bindings`: mode `00469580` is B, code 14).
        PadWording::Remastered(_) => match token {
            Token::Save => Button::Left,
            Token::Back | Token::Mode => Button::Right,
            Token::Drop => Button::Top,
            _ => Button::Bottom,
        },
        PadWording::Retail => Button::Bottom,
    }
}

/// A button's name for the wording. A PlayStation pad under the Console
/// layout keeps the PS1's own Start and Select; under the Remastered layout
/// it uses the names on a modern pad, Options and Create.
fn name(wording: PadWording, button: Button) -> &'static [u8] {
    let remastered = matches!(wording, PadWording::Remastered(_));
    match wording {
        PadWording::Console(family) | PadWording::Remastered(family) => match family {
            PadFamily::Xbox => match button {
                Button::Bottom => b"A",
                Button::Right => b"B",
                Button::Left => b"X",
                Button::Top => b"Y",
                Button::LeftShoulder => b"LB",
                Button::Start => b"Start",
            },
            PadFamily::PlayStation => match button {
                Button::Bottom => b"Cross",
                Button::Right => b"Circle",
                Button::Left => b"Square",
                Button::Top => b"Triangle",
                Button::LeftShoulder => b"L1",
                Button::Start if remastered => b"Options",
                Button::Start => b"Start",
            },
        },
        PadWording::Retail => b"",
    }
}

/// One H0 global and its text in English, French, German, Italian, Spanish
/// and Dutch (the `settings +64` order), with `{TOKEN}` button placeholders.
struct Hint {
    global_id: u32,
    texts: [&'static [u8]; 6],
}

const HINTS: [Hint; 9] = [
    // PS1 common bank global 37: "Press \x17 to save". \x17 is Circle, and
    // the results table `00521648` puts save `0042AAA0` on code 14.
    Hint {
        global_id: 37,
        texts: [
            b"Press {SAVE} to save",
            b"{SAVE} pour enregistrer",
            b"Dr\xfccken Sie {SAVE} zum Speichern",
            b"Premi {SAVE} per salvare",
            b"Pulsa {SAVE} para guardar",
            b"Om op te slaan, druk je op {SAVE} ",
        ],
    },
    // PS1 common bank global 38: "Start or Cross to continue". The frontend
    // and results pad rows both continue on Start (code 4) or Cross (code
    // 15), so the whole prompt stands.
    Hint {
        global_id: 38,
        texts: [
            b"{START} or {CONFIRM} to continue",
            b"{START} ou {CONFIRM} pour continuer",
            b"{START} oder {CONFIRM}, um fortzufahren",
            b"START (avvio) o {CONFIRM} per continuare",
            b"{START} o {CONFIRM} para continuar",
            b"Om verder te gaan, druk je op de Starttoets of {CONFIRM}",
        ],
    },
    // PS1 common bank global 39: "Press \x18 to carry on, \x17 to retry".
    // Cross (code 15) accepts and Circle (code 14) retries: name entry's
    // `0045AE90` takes Start (4), `00445200` takes Cross (15), and the retry
    // `0045AEA0` takes Circle (14).
    Hint {
        global_id: 39,
        texts: [
            b"Press {CONFIRM} to carry on, {SAVE} to retry",
            b"{CONFIRM} pour continuer, {SAVE} pour recommencer",
            b"Dr\xfccken Sie {CONFIRM}, um fortzufahren, {SAVE} f\xfcr einen Neustart",
            b"Premi {CONFIRM} per proseguire, {SAVE} per riprovare",
            b"Pulsa {CONFIRM} para seguir, {SAVE} para volverlo a intentar",
            b"Om verder te spelen, druk je op {CONFIRM}. Om het nogmaals te proberen druk je op {SAVE}",
        ],
    },
    // PS1 common bank global 40: "Press \x18 to continue, \x1a to cancel".
    // Cross (code 15) accepts and Triangle (code 13) goes back in the
    // frontend table `00528410` (`00445200` / `004451E0`).
    Hint {
        global_id: 40,
        texts: [
            b"Press {CONFIRM} to continue, {BACK} to cancel",
            b"Appuyez sur {CONFIRM} pour continuer, {BACK} pour annuler",
            b"Dr\xfccke {CONFIRM} um fortzufahren, {BACK} um abzubrechen",
            b"Premi {CONFIRM} per continuare, {BACK} x cancellare",
            b"Pulsa {CONFIRM} para continuar, {BACK} para cancelar",
            b"Druk op {CONFIRM} om door te gaan of op {BACK} om te annuleren",
        ],
    },
    // PS1 `1l0..1l5.ovl` record 174 (global 866): "Press L1 to activate big
    // wheeled mode". L1 is the Console layout's LB (or the PS1's L1), and the
    // gameplay sub table's Mode row `00469580` (Caps Lock on the PC keyboard)
    // requires code 11 (L1); the Remastered layout binds mode to Y.
    Hint {
        global_id: 866,
        texts: [
            b"Press {MODE} to activate big wheeled mode",
            b"Appuyez sur {MODE} pour passer en configuration camion.",
            b"Dr\xfccken Sie die {MODE}-Taste, um den Truck-Modus zu aktivieren",
            b"Premi il tasto {MODE} per attivare la modalit\xe0 Bigfoot.",
            b"Pulsa {MODE} para activar el modo de ruedas grandes",
            b"Om de terreinwagen te activeren, druk je op de {MODE}-toets.",
        ],
    },
    // PS1 record 175 (global 867): "Press L1 again to return to normal buggy
    // mode". The PS1's "again" makes the mode cycle explicit.
    Hint {
        global_id: 867,
        texts: [
            b"Press {MODE} again to return to normal buggy mode",
            b"Appuyez \xe0 nouveau sur {MODE} pour revenir en mode normal (buggy)",
            b"Dr\xfccken Sie erneut die {MODE}-Taste, um zum Buggy-Modus zur\xfcckzukehren",
            b"Premi un'altra volta il tasto {MODE} per ritornare alla modalit\xe0 Buggy.",
            b"Pulsa {MODE} otra vez para volver al modo de buggy normal",
            b"Druk nogmaals op de {MODE}-toets om terug te gaan naar de buggy.",
        ],
    },
    // PS1 record 176 (global 868): "To select an item use up and down and
    // Select (Cross)". The message-select row `004697F0` requires code 15
    // (Cross), the Console layout's A and the Remastered layout's A.
    Hint {
        global_id: 868,
        texts: [
            b"To select an item use up and down and Select ({SELECT})",
            b"Pour choisir un \xe9l\xe9ment, utilisez les touches Haut et Bas, puis Select ({SELECT}).",
            b"Benutzen Sie zur Auswahl die Steuerung hoch und runter und die Select-Taste ({SELECT})",
            b"Per selezionare un oggetto, usa i tasti direzionali su e gi\xf9 e SELECT (selezione)({SELECT}).",
            b"Para seleccionar un objeto utiliza arriba, abajo y Select ({SELECT})",
            b"Om een voorwerp te selecteren, druk je op omhoog, omlaag en de Selecttoets ({SELECT})",
        ],
    },
    // PS1 record 177 (global 869): "Use Cross to activate turbo booster".
    // The thrust consumer `0053A378` requires code 15 (Cross), the Console
    // layout's A and the Remastered layout's A.
    Hint {
        global_id: 869,
        texts: [
            b"Use {TURBO} to activate turbo booster",
            b"Servez-vous de la touche {TURBO} pour enclencher le turbo.",
            b"Benutzen Sie {TURBO}, um den Turbo zu aktivieren",
            b"Usa {TURBO} per attivare il turbo.",
            b"Utiliza {TURBO} para activar la aceleraci\xf3n turbo",
            b"Om de turboversterker te activeren, druk je op {TURBO}",
        ],
    },
    // PS1 record 178 (global 870): "Drop scientists close to a factory
    // (Triangle) and they will go to work". The drop row `00469670` (D on
    // the PC keyboard, Beam Down) requires code 13 (Triangle), the Console
    // layout's Y and the Remastered layout's Y.
    Hint {
        global_id: 870,
        texts: [
            b"Drop scientists close to a factory ({DROP}) and they will go to work",
            b"D\xe9posez les savants \xe0 proximit\xe9 d'une usine (touche {DROP}) pour qu'ils se mettent au travail.",
            b"Setzen Sie Wissenschaftler bei einer Fabrik ab ({DROP}), und sie beginnen mit der Arbeit",
            b"Scarica gli scienziati vicino a uno stabilimento ({DROP}) ed essi cominceranno a lavorare.",
            b"Deja a los cient\xedficos cerca de la f\xe1brica ({DROP}) y ellos solos se ir\xe1n a trabajar",
            b"Zet de wetenschappers vlakbij de fabriek af ({DROP}) en ze gaan aan het werk.",
        ],
    },
];

/// The controller wording for H0 global `global_id` in residency language
/// `language` (0..5; others read as English), or None when the PC text stands
/// (`PadWording::Retail`).
pub fn pad_hint(wording: PadWording, global_id: u32, language: u8) -> Option<Cow<'static, [u8]>> {
    if wording == PadWording::Retail {
        return None;
    }
    let hint = HINTS.iter().find(|hint| hint.global_id == global_id)?;
    let language = if language <= 5 {
        usize::from(language)
    } else {
        0
    };
    Some(fill(hint.texts[language], wording))
}

/// Fills a template's `{TOKEN}` button words. An unknown brace group is
/// literal text; the tests assert the table has none.
fn fill(template: &'static [u8], wording: PadWording) -> Cow<'static, [u8]> {
    let mut out = Vec::with_capacity(template.len() + 8);
    let mut index = 0;
    while index < template.len() {
        let Some(open) = template[index..].iter().position(|&byte| byte == b'{') else {
            out.extend_from_slice(&template[index..]);
            break;
        };
        out.extend_from_slice(&template[index..index + open]);
        let token_start = index + open + 1;
        let Some(close) = template[token_start..]
            .iter()
            .position(|&byte| byte == b'}')
        else {
            out.extend_from_slice(&template[index + open..]);
            break;
        };
        let token_end = token_start + close;
        match Token::from_name(&template[token_start..token_end]) {
            Some(token) => out.extend_from_slice(name(wording, button(wording, token))),
            None => out.extend_from_slice(&template[index + open..=token_end]),
        }
        index = token_end + 1;
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{parse_font_bank, parse_overlay, OverlayKey, Preload};
    use std::fs;
    use std::path::Path;

    const CONTROL_IDS: [u32; 9] = [37, 38, 39, 40, 866, 867, 868, 869, 870];

    /// Every layout and family a sentence can name.
    const WORDINGS: [PadWording; 4] = [
        PadWording::Console(PadFamily::Xbox),
        PadWording::Console(PadFamily::PlayStation),
        PadWording::Remastered(PadFamily::Xbox),
        PadWording::Remastered(PadFamily::PlayStation),
    ];

    /// The table exactly as the port shipped it before the wording became
    /// per layout and pad family, in English, French, German, Italian,
    /// Spanish and Dutch. The Console layout with an Xbox pad must keep these
    /// bytes.
    const RETAIL_XBOX: [(u32, [&[u8]; 6]); 9] = [
        (
            37,
            [
                b"Press B to save",
                b"B pour enregistrer",
                b"Dr\xfccken Sie B zum Speichern",
                b"Premi B per salvare",
                b"Pulsa B para guardar",
                b"Om op te slaan, druk je op B ",
            ],
        ),
        (
            38,
            [
                b"Start or A to continue",
                b"Start ou A pour continuer",
                b"Start oder A, um fortzufahren",
                b"START (avvio) o A per continuare",
                b"Start o A para continuar",
                b"Om verder te gaan, druk je op de Starttoets of A",
            ],
        ),
        (
            39,
            [
                b"Press A to carry on, B to retry",
                b"A pour continuer, B pour recommencer",
                b"Dr\xfccken Sie A, um fortzufahren, B f\xfcr einen Neustart",
                b"Premi A per proseguire, B per riprovare",
                b"Pulsa A para seguir, B para volverlo a intentar",
                b"Om verder te spelen, druk je op A. Om het nogmaals te proberen druk je op B",
            ],
        ),
        (
            40,
            [
                b"Press A to continue, Y to cancel",
                b"Appuyez sur A pour continuer, Y pour annuler",
                b"Dr\xfccke A um fortzufahren, Y um abzubrechen",
                b"Premi A per continuare, Y x cancellare",
                b"Pulsa A para continuar, Y para cancelar",
                b"Druk op A om door te gaan of op Y om te annuleren",
            ],
        ),
        (
            866,
            [
                b"Press LB to activate big wheeled mode",
                b"Appuyez sur LB pour passer en configuration camion.",
                b"Dr\xfccken Sie die LB-Taste, um den Truck-Modus zu aktivieren",
                b"Premi il tasto LB per attivare la modalit\xe0 Bigfoot.",
                b"Pulsa LB para activar el modo de ruedas grandes",
                b"Om de terreinwagen te activeren, druk je op de LB-toets.",
            ],
        ),
        (
            867,
            [
                b"Press LB again to return to normal buggy mode",
                b"Appuyez \xe0 nouveau sur LB pour revenir en mode normal (buggy)",
                b"Dr\xfccken Sie erneut die LB-Taste, um zum Buggy-Modus zur\xfcckzukehren",
                b"Premi un'altra volta il tasto LB per ritornare alla modalit\xe0 Buggy.",
                b"Pulsa LB otra vez para volver al modo de buggy normal",
                b"Druk nogmaals op de LB-toets om terug te gaan naar de buggy.",
            ],
        ),
        (
            868,
            [
                b"To select an item use up and down and Select (A)",
                b"Pour choisir un \xe9l\xe9ment, utilisez les touches Haut et Bas, puis Select (A).",
                b"Benutzen Sie zur Auswahl die Steuerung hoch und runter und die Select-Taste (A)",
                b"Per selezionare un oggetto, usa i tasti direzionali su e gi\xf9 e SELECT (selezione)(A).",
                b"Para seleccionar un objeto utiliza arriba, abajo y Select (A)",
                b"Om een voorwerp te selecteren, druk je op omhoog, omlaag en de Selecttoets (A)",
            ],
        ),
        (
            869,
            [
                b"Use A to activate turbo booster",
                b"Servez-vous de la touche A pour enclencher le turbo.",
                b"Benutzen Sie A, um den Turbo zu aktivieren",
                b"Usa A per attivare il turbo.",
                b"Utiliza A para activar la aceleraci\xf3n turbo",
                b"Om de turboversterker te activeren, druk je op A",
            ],
        ),
        (
            870,
            [
                b"Drop scientists close to a factory (Y) and they will go to work",
                b"D\xe9posez les savants \xe0 proximit\xe9 d'une usine (touche Y) pour qu'ils se mettent au travail.",
                b"Setzen Sie Wissenschaftler bei einer Fabrik ab (Y), und sie beginnen mit der Arbeit",
                b"Scarica gli scienziati vicino a uno stabilimento (Y) ed essi cominceranno a lavorare.",
                b"Deja a los cient\xedficos cerca de la f\xe1brica (Y) y ellos solos se ir\xe1n a trabajar",
                b"Zet de wetenschappers vlakbij de fabriek af (Y) en ze gaan aan het werk.",
            ],
        ),
    ];

    /// The English the four wordings must produce, by hand: one sentence per
    /// layout and family, straight from the tables they name.
    const BY_HAND: [(PadWording, [&[u8]; 9]); 4] = [
        (
            PadWording::Console(PadFamily::Xbox),
            [
                b"Press B to save",
                b"Start or A to continue",
                b"Press A to carry on, B to retry",
                b"Press A to continue, Y to cancel",
                b"Press LB to activate big wheeled mode",
                b"Press LB again to return to normal buggy mode",
                b"To select an item use up and down and Select (A)",
                b"Use A to activate turbo booster",
                b"Drop scientists close to a factory (Y) and they will go to work",
            ],
        ),
        (
            PadWording::Console(PadFamily::PlayStation),
            [
                b"Press Circle to save",
                b"Start or Cross to continue",
                b"Press Cross to carry on, Circle to retry",
                b"Press Cross to continue, Triangle to cancel",
                b"Press L1 to activate big wheeled mode",
                b"Press L1 again to return to normal buggy mode",
                b"To select an item use up and down and Select (Cross)",
                b"Use Cross to activate turbo booster",
                b"Drop scientists close to a factory (Triangle) and they will go to work",
            ],
        ),
        (
            PadWording::Remastered(PadFamily::Xbox),
            [
                b"Press X to save",
                b"Start or A to continue",
                b"Press A to carry on, X to retry",
                b"Press A to continue, B to cancel",
                b"Press B to activate big wheeled mode",
                b"Press B again to return to normal buggy mode",
                b"To select an item use up and down and Select (A)",
                b"Use A to activate turbo booster",
                b"Drop scientists close to a factory (Y) and they will go to work",
            ],
        ),
        (
            PadWording::Remastered(PadFamily::PlayStation),
            [
                b"Press Square to save",
                b"Options or Cross to continue",
                b"Press Cross to carry on, Square to retry",
                b"Press Cross to continue, Circle to cancel",
                b"Press Circle to activate big wheeled mode",
                b"Press Circle again to return to normal buggy mode",
                b"To select an item use up and down and Select (Cross)",
                b"Use Cross to activate turbo booster",
                b"Drop scientists close to a factory (Triangle) and they will go to work",
            ],
        ),
    ];

    #[test]
    fn the_console_xbox_table_keeps_its_bytes() {
        for (global_id, texts) in RETAIL_XBOX {
            for (language, expected) in texts.into_iter().enumerate() {
                assert_eq!(
                    pad_hint(
                        PadWording::Console(PadFamily::Xbox),
                        global_id,
                        language as u8
                    )
                    .unwrap()
                    .as_ref(),
                    expected,
                    "global {global_id} language {language}"
                );
            }
        }
    }

    #[test]
    fn each_layout_and_family_names_its_own_buttons() {
        for (wording, texts) in BY_HAND {
            for (index, expected) in texts.into_iter().enumerate() {
                let global_id = CONTROL_IDS[index];
                assert_eq!(
                    pad_hint(wording, global_id, 0).unwrap().as_ref(),
                    expected,
                    "{wording:?} global {global_id}"
                );
            }
        }
    }

    #[test]
    fn retail_wording_keeps_the_pc_text() {
        for global_id in CONTROL_IDS {
            for language in 0..=5 {
                assert_eq!(pad_hint(PadWording::Retail, global_id, language), None);
            }
        }
    }

    #[test]
    fn every_control_id_has_all_six_languages() {
        for wording in WORDINGS {
            for global_id in CONTROL_IDS {
                for language in 0..=5 {
                    assert!(
                        pad_hint(wording, global_id, language).is_some(),
                        "{wording:?} global {global_id} language {language}"
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_and_non_control_ids_keep_the_pc_text() {
        for wording in WORDINGS {
            for global_id in [0, 36, 41, 692, 0x48, 865, 871, 0x3a5, u32::MAX] {
                for language in 0..=5 {
                    assert_eq!(
                        pad_hint(wording, global_id, language),
                        None,
                        "{wording:?} global {global_id} language {language}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_language_past_five_reads_as_english() {
        for wording in WORDINGS {
            for global_id in CONTROL_IDS {
                assert_eq!(
                    pad_hint(wording, global_id, 6),
                    pad_hint(wording, global_id, 0)
                );
                assert_eq!(
                    pad_hint(wording, global_id, 0xff),
                    pad_hint(wording, global_id, 0)
                );
            }
        }
    }

    #[test]
    fn every_template_token_is_known() {
        for hint in &HINTS {
            for text in hint.texts {
                let mut rest = text;
                while let Some(open) = rest.iter().position(|&byte| byte == b'{') {
                    let after = &rest[open + 1..];
                    let close = after
                        .iter()
                        .position(|&byte| byte == b'}')
                        .unwrap_or_else(|| {
                            panic!("global {} has an unclosed token", hint.global_id)
                        });
                    assert!(
                        Token::from_name(&after[..close]).is_some(),
                        "global {} has an unknown token {}",
                        hint.global_id,
                        String::from_utf8_lossy(&after[..close])
                    );
                    rest = &after[close + 1..];
                }
            }
        }
    }

    #[test]
    fn texts_are_printable_latin_1_with_no_icon_bytes() {
        for wording in WORDINGS {
            for global_id in CONTROL_IDS {
                for language in 0..=5 {
                    let text = pad_hint(wording, global_id, language).unwrap();
                    for &byte in text.iter() {
                        assert!(
                            !(0x17..=0x1a).contains(&byte),
                            "{wording:?} global {global_id} language {language} \
                             keeps icon byte {byte:#04x}"
                        );
                        assert!(
                            (0x20..=0x7e).contains(&byte) || (0xa0..=0xff).contains(&byte),
                            "{wording:?} global {global_id} language {language} \
                             has non-printable byte {byte:#04x}"
                        );
                    }
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
            for wording in WORDINGS {
                for global_id in CONTROL_IDS {
                    for language in 0..=5 {
                        let text = pad_hint(wording, global_id, language).unwrap();
                        for &byte in text.iter() {
                            assert!(
                                i32::from(byte) <= font.maximum_byte
                                    && font
                                        .sprite_words
                                        .get(usize::from(byte))
                                        .copied()
                                        .unwrap_or(0)
                                        != 0,
                                "{name} H4 font 0 has no glyph for byte {byte:#04x} \
                                 ({wording:?} global {global_id} language {language})"
                            );
                        }
                    }
                }
            }
        }
    }
}
