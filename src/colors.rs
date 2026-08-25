use ratatui::style::Color;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub overlay: Color,
    pub text: Color,
    pub subtext: Color,
    pub muted: Color,
    pub accent: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub border: Color,
    /// Per-lane colors for the commit graph, in the same red/green/yellow/
    /// blue/magenta/cyan order `git log --graph --color` cycles through
    /// (`log.graphColors`'s default) — indexed by `(ansi_code - 31) % 6`, so
    /// a lane keeps the same color across every row for as long as it lives,
    /// matching what git's own graph layout algorithm already computed.
    pub graph_colors: [Color; 6],
}

/// Display name for [`Palette::mocha`], also the default of `Preferences::theme`.
pub const THEME_MOCHA: &str = "Catppuccin Mocha";
/// Display name for [`Palette::latte`].
pub const THEME_LATTE: &str = "Catppuccin Latte";

impl Palette {
    pub fn mocha() -> Self {
        Self {
            bg: Color::Rgb(30, 30, 46),
            surface: Color::Rgb(49, 50, 68),
            overlay: Color::Rgb(69, 71, 90),
            text: Color::Rgb(205, 214, 244),
            subtext: Color::Rgb(166, 173, 200),
            muted: Color::Rgb(108, 112, 134),
            accent: Color::Rgb(137, 180, 250),
            green: Color::Rgb(166, 227, 161),
            yellow: Color::Rgb(249, 226, 175),
            red: Color::Rgb(243, 139, 168),
            border: Color::Rgb(69, 71, 90),
            graph_colors: [
                Color::Rgb(243, 139, 168), // Red
                Color::Rgb(166, 227, 161), // Green
                Color::Rgb(249, 226, 175), // Yellow
                Color::Rgb(137, 180, 250), // Blue
                Color::Rgb(203, 166, 247), // Mauve
                Color::Rgb(148, 226, 213), // Teal
            ],
        }
    }

    /// Catppuccin's official light counterpart to Mocha, same role mapping
    /// (bg=Base, surface=Mantle one step darker, accent=Blue, ...).
    pub fn latte() -> Self {
        Self {
            bg: Color::Rgb(239, 241, 245),
            surface: Color::Rgb(230, 233, 239),
            overlay: Color::Rgb(204, 208, 218),
            text: Color::Rgb(76, 79, 105),
            subtext: Color::Rgb(92, 95, 119),
            muted: Color::Rgb(156, 160, 176),
            accent: Color::Rgb(30, 102, 245),
            green: Color::Rgb(64, 160, 43),
            yellow: Color::Rgb(223, 142, 29),
            red: Color::Rgb(210, 15, 57),
            border: Color::Rgb(204, 208, 218),
            graph_colors: [
                Color::Rgb(210, 15, 57),  // Red
                Color::Rgb(64, 160, 43),  // Green
                Color::Rgb(223, 142, 29), // Yellow
                Color::Rgb(30, 102, 245), // Blue
                Color::Rgb(136, 57, 239), // Mauve
                Color::Rgb(23, 146, 153), // Teal
            ],
        }
    }

    /// Resolve a `Preferences::theme` string to a palette. Unrecognised or
    /// empty values (including prefs.json written before this option
    /// existed) fall back to Mocha rather than erroring.
    pub fn for_name(name: &str) -> Self {
        match name {
            THEME_LATTE => Self::latte(),
            _ => Self::mocha(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_name_resolves_known_themes_and_falls_back_to_mocha() {
        let mocha = Palette::for_name(THEME_MOCHA);
        let latte = Palette::for_name(THEME_LATTE);
        assert_eq!(mocha.bg, Palette::mocha().bg);
        assert_eq!(latte.bg, Palette::latte().bg);
        assert_ne!(mocha.bg, latte.bg);

        // Empty (never configured) and garbage (future/unknown theme name in
        // a shared prefs.json) must not panic or produce a blank palette.
        assert_eq!(Palette::for_name("").bg, Palette::mocha().bg);
        assert_eq!(
            Palette::for_name("nonexistent-theme").bg,
            Palette::mocha().bg
        );
    }
}
