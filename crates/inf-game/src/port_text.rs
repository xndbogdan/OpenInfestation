//! Text the port adds to the game's own menus, in each of retail's six
//! languages (settings `+64`: English, Français, Deutsch, Italiano,
//! Español, Nederlands, whose own strings are `0l0.ovl` to `0l5.ovl`). The
//! bytes are Latin-1, as the retail strings are, so the game's font draws
//! the accents; each follows the retail menus' capitalisation.

/// A string the port adds to a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortText {
    /// The Display row's Borderless.
    Borderless,
    /// The Filtering row (retail's Bilinear Filtering) and its values.
    Filtering,
    Point,
    Bilinear,
    Trilinear,
    Anisotropic,
    /// The Vulkan rows: Ray Tracing and its Path Tracing.
    RayTracing,
    PathTracing,
    /// The Upscaler row; its values are the upscalers' own names.
    Upscaler,
    /// The Upscaling row and its quality modes.
    Upscaling,
    NativeAa,
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
    /// The Anti-aliasing row; its values are the methods' own names (FXAA,
    /// SMAA, TAA, SSAA).
    AntiAliasing,
    /// The Lighting row and its presets.
    Lighting,
    Remaster,
    EyeCandy,
    /// The Camera Style row's third value, past retail's Normal and
    /// Cinematic: the remastered camera.
    Remastered,
    /// The Sound menus' Ambience row (the sound the port adds).
    Ambience,
    /// The Sound menus' Headphones row (the placed sounds for each ear).
    Headphones,
    /// Retail's own On and Off, as its On/Off rows say them.
    On,
    Off,
    /// A row whose feature this GPU lacks.
    Unavailable,
    /// The line shown while the renderer is made before the title.
    PreparingGraphics,
}

impl PortText {
    /// The text in `language`, retail's residency language (0 to 5; any
    /// other reads as English, as `004389B0` does).
    pub fn text(self, language: u8) -> &'static [u8] {
        let texts: [&'static [u8]; 6] = match self {
            Self::Borderless => [
                b"Borderless",
                b"Sans bordure",
                b"Randlos",
                b"Senza bordi",
                b"Sin Bordes",
                b"Randloos",
            ],
            Self::Filtering => [
                b"Filtering",
                b"Filtrage",
                b"Filterung",
                b"Filtraggio",
                b"Filtrado",
                b"Filteren",
            ],
            Self::Point => [
                b"Point",
                b"Ponctuel",
                b"Punkt",
                b"Puntuale",
                b"Puntual",
                b"Punt",
            ],
            // French writes it hyphenated, as its Bilinear Filtering row
            // ("Filtrage bi-linéaire") does.
            Self::Bilinear => [
                b"Bilinear",
                b"Bi-lin\xe9aire",
                b"Bilinear",
                b"Bilineare",
                b"Bilineal",
                b"Bilineair",
            ],
            Self::Trilinear => [
                b"Trilinear",
                b"Tri-lin\xe9aire",
                b"Trilinear",
                b"Trilineare",
                b"Trilineal",
                b"Trilineair",
            ],
            Self::Anisotropic => [
                b"Anisotropic",
                b"Anisotrope",
                b"Anisotrop",
                b"Anisotropico",
                b"Anisotr\xf3pico",
                b"Anisotroop",
            ],
            Self::RayTracing => [
                b"Ray Tracing",
                b"Lancer de rayons",
                b"Raytracing",
                b"Ray tracing",
                b"Trazado de rayos",
                b"Raytracing",
            ],
            Self::PathTracing => [
                b"Path Tracing",
                b"Trac\xe9 de chemins",
                b"Pathtracing",
                b"Path tracing",
                b"Trazado de trayectorias",
                b"Pathtracing",
            ],
            Self::Upscaler => [
                b"Upscaler",
                b"Mise \xe0 l'\xe9chelle",
                b"Hochskalierung",
                b"Upscaler",
                b"Escalado",
                b"Upscaler",
            ],
            Self::Upscaling => [
                b"Upscaling",
                b"Qualit\xe9 d'\xe9chelle",
                b"Skalierungsmodus",
                b"Modalit\xe0 upscaling",
                b"Modo de escalado",
                b"Schaalmodus",
            ],
            Self::NativeAa => [
                b"Native AA",
                b"AA natif",
                b"Natives AA",
                b"AA nativo",
                b"AA nativo",
                b"Native AA",
            ],
            Self::Quality => [
                b"Quality",
                b"Qualit\xe9",
                b"Qualit\xe4t",
                b"Qualit\xe0",
                b"Calidad",
                b"Kwaliteit",
            ],
            Self::Balanced => [
                b"Balanced",
                b"\xc9quilibr\xe9",
                b"Ausgewogen",
                b"Bilanciato",
                b"Equilibrado",
                b"Gebalanceerd",
            ],
            Self::Performance => [
                b"Performance",
                b"Performance",
                b"Leistung",
                b"Prestazioni",
                b"Rendimiento",
                b"Prestaties",
            ],
            Self::UltraPerformance => [
                b"Ultra Performance",
                b"Ultra performance",
                b"Ultra-Leistung",
                b"Ultra prestazioni",
                b"Ultra rendimiento",
                b"Ultra prestaties",
            ],
            Self::AntiAliasing => [
                b"Anti-aliasing",
                b"Anticr\xe9nelage",
                b"Kantengl\xe4ttung",
                b"Antialiasing",
                b"Antialiasing",
                b"Anti-aliasing",
            ],
            // As retail's Dynamic Lighting row words it ("Eclairage
            // dynamique", unaccented).
            Self::Lighting => [
                b"Lighting",
                b"Eclairage",
                b"Beleuchtung",
                b"Illuminazione",
                b"Iluminaci\xf3n",
                b"Belichting",
            ],
            Self::Remaster => [
                b"Remaster",
                b"Remasteris\xe9",
                b"Remaster",
                b"Rimasterizzato",
                b"Remasterizado",
                b"Remaster",
            ],
            Self::Remastered => [
                b"Remastered",
                b"Remasteris\xe9e",
                b"Remastered",
                b"Rimasterizzata",
                b"Remasterizada",
                b"Remastered",
            ],
            Self::EyeCandy => [
                b"Eye candy",
                b"Spectaculaire",
                b"Augenschmaus",
                b"Spettacolare",
                b"Espectacular",
                b"Spectaculair",
            ],
            Self::Ambience => [
                b"Ambience",
                b"Ambiance",
                b"Atmosph\xe4re",
                b"Ambiente",
                b"Ambiente",
                b"Sfeer",
            ],
            Self::Headphones => [
                b"Headphones",
                b"Casque",
                b"Kopfh\xf6rer",
                b"Cuffie",
                b"Auriculares",
                b"Koptelefoon",
            ],
            // Retail's Pause On Hints row's (global text 187).
            Self::On => [b"On", b"Oui", b"An", b"On", b"S\xed", b"Aan"],
            Self::Off => [b"Off", b"Non", b"Aus", b"Off", b"No", b"Uit"],
            Self::Unavailable => [
                b"Unavailable",
                b"Indisponible",
                b"Nicht verf\xfcgbar",
                b"Non disponibile",
                b"No disponible",
                b"Niet beschikbaar",
            ],
            Self::PreparingGraphics => [
                b"Preparing graphics...",
                b"Pr\xe9paration des graphismes...",
                b"Grafik wird vorbereitet...",
                b"Preparazione della grafica...",
                b"Preparando los gr\xe1ficos...",
                b"Graphics worden voorbereid...",
            ],
        };
        texts
            .get(usize::from(language))
            .copied()
            .unwrap_or(texts[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_port_text_has_six_languages_in_latin_1() {
        for text in [
            PortText::Borderless,
            PortText::Filtering,
            PortText::Point,
            PortText::Bilinear,
            PortText::Trilinear,
            PortText::Anisotropic,
            PortText::RayTracing,
            PortText::PathTracing,
            PortText::Upscaler,
            PortText::Upscaling,
            PortText::NativeAa,
            PortText::Quality,
            PortText::Balanced,
            PortText::Performance,
            PortText::UltraPerformance,
            PortText::AntiAliasing,
            PortText::Lighting,
            PortText::Remaster,
            PortText::Remastered,
            PortText::EyeCandy,
            PortText::Off,
            PortText::Unavailable,
            PortText::PreparingGraphics,
        ] {
            for language in 0..6 {
                assert!(!text.text(language).is_empty());
            }
            // Past the six, English.
            assert_eq!(text.text(9), text.text(0));
        }
        assert_eq!(PortText::Borderless.text(1), b"Sans bordure");
        // The accents are the retail strings' Latin-1 bytes.
        assert_eq!(PortText::Bilinear.text(1), b"Bi-lin\xe9aire");
        assert_eq!(PortText::Anisotropic.text(4), b"Anisotr\xf3pico");
    }
}
