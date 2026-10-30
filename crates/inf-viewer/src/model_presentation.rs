//! Evidence tier selected for one Handler-8 model-slot presentation.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ModelPresentation {
    /// Show only polygon geometry proven within the selected root entry.
    #[default]
    LocalShell,
    /// Recursively add child entries whose neutral attachment state is proven.
    NeutralComposite,
    /// Evaluate proven view-plane guards using an explicitly supplied root eye.
    ExplicitEye,
    /// Inspect one validated transient retail draw. Without an accepted bundle
    /// this tier remains unavailable and never falls back to static geometry.
    CapturedDraw,
}

impl ModelPresentation {
    pub const fn next(self) -> Self {
        match self {
            Self::LocalShell => Self::NeutralComposite,
            Self::NeutralComposite => Self::ExplicitEye,
            Self::ExplicitEye => Self::CapturedDraw,
            Self::CapturedDraw => Self::LocalShell,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::LocalShell => "LOCAL SHELL",
            Self::NeutralComposite => "NEUTRAL COMPOSITE",
            Self::ExplicitEye => "EXPLICIT EYE",
            Self::CapturedDraw => "CAPTURED DRAW",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_cycle_is_explicit_and_closed() {
        assert_eq!(
            ModelPresentation::LocalShell.next(),
            ModelPresentation::NeutralComposite
        );
        assert_eq!(
            ModelPresentation::NeutralComposite.next(),
            ModelPresentation::ExplicitEye
        );
        assert_eq!(
            ModelPresentation::ExplicitEye.next(),
            ModelPresentation::CapturedDraw
        );
        assert_eq!(
            ModelPresentation::CapturedDraw.next(),
            ModelPresentation::LocalShell
        );
        assert_eq!(ModelPresentation::CapturedDraw.label(), "CAPTURED DRAW");
    }
}
