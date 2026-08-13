use std::env;

use crate::BindingSupport;

/// Whether the standard terminal runner performs capability detection
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TerminalCapabilityDetection {
    /// Preserve caller-supplied output capabilities and legacy keyboard input
    #[default]
    Disabled,
    /// Inspect environment hints and actively query extended keyboard support
    Enabled,
}

/// Evidence available for one terminal feature
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TerminalFeatureSupport {
    /// Support could not be established
    #[default]
    Unknown,
    /// The terminal established that the feature is unavailable
    Unsupported,
    /// The terminal established that the feature is available
    Supported,
}

/// Color level advertised by the terminal environment
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TerminalColorLevel {
    /// No reliable color hint is available
    #[default]
    Unknown,
    /// The environment requests a terminal without color
    Monochrome,
    /// The environment advertises the ANSI color baseline
    Ansi16,
    /// The environment advertises the indexed 256-color palette
    Indexed256,
    /// The environment advertises 24-bit color
    TrueColor,
}

/// Keyboard protocol currently enabled by the standard terminal runner
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TerminalKeyboardProtocol {
    /// Traditional terminal keyboard input
    #[default]
    Legacy,
    /// Kitty keyboard protocol with Nagi progressive enhancements
    Kitty,
}

/// Detected terminal features and the keyboard protocol currently in use
///
/// Feature support is observational metadata. It does not grant clipboard or
/// other output permission, which remains controlled by `TerminalOptions`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TerminalCapabilityProfile {
    color_level: TerminalColorLevel,
    prefers_no_color: bool,
    hyperlinks: TerminalFeatureSupport,
    clipboard: TerminalFeatureSupport,
    extended_keyboard: TerminalFeatureSupport,
    keyboard_protocol: TerminalKeyboardProtocol,
}

impl TerminalCapabilityProfile {
    /// A profile with no detected support and legacy keyboard input
    pub const UNKNOWN: Self = Self {
        color_level: TerminalColorLevel::Unknown,
        prefers_no_color: false,
        hyperlinks: TerminalFeatureSupport::Unknown,
        clipboard: TerminalFeatureSupport::Unknown,
        extended_keyboard: TerminalFeatureSupport::Unknown,
        keyboard_protocol: TerminalKeyboardProtocol::Legacy,
    };

    /// Returns the advertised terminal color level
    #[must_use]
    pub const fn color_level(self) -> TerminalColorLevel {
        self.color_level
    }

    /// Reports a non-empty `NO_COLOR` environment preference
    #[must_use]
    pub const fn prefers_no_color(self) -> bool {
        self.prefers_no_color
    }

    /// Returns detected hyperlink support
    #[must_use]
    pub const fn hyperlinks(self) -> TerminalFeatureSupport {
        self.hyperlinks
    }

    /// Returns detected terminal clipboard support
    #[must_use]
    pub const fn clipboard(self) -> TerminalFeatureSupport {
        self.clipboard
    }

    /// Returns actively queried extended-keyboard support
    #[must_use]
    pub const fn extended_keyboard(self) -> TerminalFeatureSupport {
        self.extended_keyboard
    }

    /// Returns the keyboard protocol currently enabled by the runner
    #[must_use]
    pub const fn keyboard_protocol(self) -> TerminalKeyboardProtocol {
        self.keyboard_protocol
    }

    /// Maps extended-keyboard evidence to key-binding presentation metadata
    #[must_use]
    pub const fn modified_key_support(self) -> BindingSupport {
        match self.extended_keyboard {
            TerminalFeatureSupport::Unknown => BindingSupport::Unknown,
            TerminalFeatureSupport::Unsupported => BindingSupport::Unsupported,
            TerminalFeatureSupport::Supported => BindingSupport::Supported,
        }
    }

    /// Returns a copy with an explicit color level
    #[must_use]
    pub const fn with_color_level(mut self, level: TerminalColorLevel) -> Self {
        self.color_level = level;
        self
    }

    /// Returns a copy with an explicit no-color preference
    #[must_use]
    pub const fn with_no_color_preference(mut self, prefers_no_color: bool) -> Self {
        self.prefers_no_color = prefers_no_color;
        self
    }

    /// Returns a copy with explicit hyperlink evidence
    #[must_use]
    pub const fn with_hyperlinks(mut self, support: TerminalFeatureSupport) -> Self {
        self.hyperlinks = support;
        self
    }

    /// Returns a copy with explicit terminal clipboard evidence
    #[must_use]
    pub const fn with_clipboard(mut self, support: TerminalFeatureSupport) -> Self {
        self.clipboard = support;
        self
    }

    /// Returns a copy with explicit extended-keyboard evidence and active mode
    #[must_use]
    pub const fn with_extended_keyboard(
        mut self,
        support: TerminalFeatureSupport,
        protocol: TerminalKeyboardProtocol,
    ) -> Self {
        self.extended_keyboard = support;
        self.keyboard_protocol = protocol;
        self
    }
}

pub(crate) fn process_environment_profile() -> TerminalCapabilityProfile {
    let term = env::var_os("TERM");
    let color_term = env::var_os("COLORTERM");
    let no_color = env::var_os("NO_COLOR");
    environment_profile(
        term.as_deref().map(|value| value.to_string_lossy()),
        color_term.as_deref().map(|value| value.to_string_lossy()),
        no_color.as_deref().is_some_and(|value| !value.is_empty()),
    )
}

fn environment_profile(
    term: Option<std::borrow::Cow<'_, str>>,
    color_term: Option<std::borrow::Cow<'_, str>>,
    prefers_no_color: bool,
) -> TerminalCapabilityProfile {
    let term = term.as_deref().unwrap_or_default();
    let color_term = color_term.as_deref().unwrap_or_default();
    let lower_term = term.to_ascii_lowercase();
    let lower_color_term = color_term.to_ascii_lowercase();
    let color_level = if lower_term == "dumb" {
        TerminalColorLevel::Monochrome
    } else if matches!(lower_color_term.as_str(), "truecolor" | "24bit") {
        TerminalColorLevel::TrueColor
    } else if lower_term.contains("256color") {
        TerminalColorLevel::Indexed256
    } else if term.is_empty() {
        TerminalColorLevel::Unknown
    } else {
        TerminalColorLevel::Ansi16
    };
    let unavailable = (lower_term == "dumb").then_some(TerminalFeatureSupport::Unsupported);
    TerminalCapabilityProfile {
        color_level,
        prefers_no_color,
        hyperlinks: unavailable.unwrap_or(TerminalFeatureSupport::Unknown),
        clipboard: unavailable.unwrap_or(TerminalFeatureSupport::Unknown),
        ..TerminalCapabilityProfile::UNKNOWN
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{App, Effect, Node, Runtime, RuntimeConfig, Size, ViewContext, VirtualClock};

    use super::*;

    fn value(input: &str) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(input))
    }

    fn fixture_value(input: &str) -> Option<Cow<'_, str>> {
        (input != "-").then_some(Cow::Borrowed(input))
    }

    fn canonical_profile(profile: TerminalCapabilityProfile) -> String {
        let color = match profile.color_level() {
            TerminalColorLevel::Unknown => "unknown",
            TerminalColorLevel::Monochrome => "monochrome",
            TerminalColorLevel::Ansi16 => "ansi16",
            TerminalColorLevel::Indexed256 => "indexed256",
            TerminalColorLevel::TrueColor => "truecolor",
        };
        let feature = |support| match support {
            TerminalFeatureSupport::Unknown => "unknown",
            TerminalFeatureSupport::Unsupported => "unsupported",
            TerminalFeatureSupport::Supported => "supported",
        };
        format!(
            "{color},{},{},{}",
            profile.prefers_no_color(),
            feature(profile.hyperlinks()),
            feature(profile.clipboard())
        )
    }

    #[test]
    fn environment_profiles_match_shared_fixtures() {
        let Some(records) = crate::fixture_support::load(
            "tui/terminal-capabilities.txt",
            "terminal-capabilities",
            &["term", "colorterm", "no-color", "expected"],
        ) else {
            return;
        };
        for record in records {
            let profile = environment_profile(
                fixture_value(record.field("term")),
                fixture_value(record.field("colorterm")),
                record.field("no-color") != "-",
            );
            assert_eq!(
                canonical_profile(profile),
                record.field("expected"),
                "case {}",
                record.id
            );
        }
    }

    #[test]
    fn environment_color_hints_are_bounded_and_no_color_is_separate() {
        let true_color = environment_profile(value("xterm-256color"), value("truecolor"), true);
        assert_eq!(true_color.color_level(), TerminalColorLevel::TrueColor);
        assert!(true_color.prefers_no_color());
        assert_eq!(true_color.hyperlinks(), TerminalFeatureSupport::Unknown);
        assert_eq!(true_color.clipboard(), TerminalFeatureSupport::Unknown);

        let dumb = environment_profile(value("dumb"), None, false);
        assert_eq!(dumb.color_level(), TerminalColorLevel::Monochrome);
        assert_eq!(dumb.hyperlinks(), TerminalFeatureSupport::Unsupported);
        assert_eq!(dumb.clipboard(), TerminalFeatureSupport::Unsupported);
    }

    #[test]
    fn keyboard_profile_maps_to_binding_metadata_without_granting_output() {
        let profile = TerminalCapabilityProfile::UNKNOWN.with_extended_keyboard(
            TerminalFeatureSupport::Supported,
            TerminalKeyboardProtocol::Kitty,
        );
        assert_eq!(profile.modified_key_support(), BindingSupport::Supported);
        assert_eq!(profile.keyboard_protocol(), TerminalKeyboardProtocol::Kitty);
        assert_eq!(profile.clipboard(), TerminalFeatureSupport::Unknown);
    }

    #[test]
    fn runtime_config_exposes_a_deterministic_profile_to_view_context() {
        struct CapturingApp {
            seen: std::cell::Cell<TerminalCapabilityProfile>,
        }

        impl App for CapturingApp {
            type Message = ();

            fn update(&mut self, (): ()) -> Effect<Self::Message> {
                Effect::none()
            }

            fn view(&self, context: ViewContext) -> Node<Self::Message> {
                self.seen.set(context.terminal_capabilities);
                Node::text("profile")
            }
        }

        let profile = TerminalCapabilityProfile::UNKNOWN.with_extended_keyboard(
            TerminalFeatureSupport::Supported,
            TerminalKeyboardProtocol::Kitty,
        );
        let mut config = RuntimeConfig::new(Size::new(8, 1));
        config.terminal_capabilities = profile;
        let mut runtime = Runtime::with_clock(
            CapturingApp {
                seen: std::cell::Cell::new(TerminalCapabilityProfile::UNKNOWN),
            },
            config,
            VirtualClock::new(),
        )
        .unwrap();

        runtime.render_if_dirty().unwrap();
        assert_eq!(runtime.app_mut().seen.get(), profile);
    }
}
