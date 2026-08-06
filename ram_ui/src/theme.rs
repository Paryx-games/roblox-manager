//! Semantic color tokens for the whole UI.
//!
//! Before this module every panel called `Color32::from_rgb` inline, and the
//! palette had drifted: "error red" existed as four different values, amber as
//! two, and the four presence colors were copy-pasted into three files. Nothing
//! was wrong with any single call; there was simply no place where the palette
//! was decided, so each new one was decided again.
//!
//! # Tokens are roles, not colors
//!
//! Fields are named for what a color *means* ([`Theme::danger`],
//! [`Theme::presence_in_game`]), never for what it looks like. That is the
//! whole point: a second theme is then a second set of values, not a second set
//! of decisions about which shade of red a rejected upload should be.
//!
//! The distinction that recurs most is fill versus text. A color solid enough
//! to sit behind white button text is too dark to read *as* text on the dark
//! panel background, so the loud roles come in pairs: [`Theme::danger`] and
//! [`Theme::danger_text`], [`Theme::warning`] and [`Theme::warning_text`], with
//! a third `_surface` value where a tinted banner background is needed.
//!
//! # Access
//!
//! One theme is installed into the egui context at startup and read back with
//! [`ThemeUi::theme`] (on `Ui`) or [`of`] (on `Context`). Going through the
//! context rather than a `const` is what keeps a future theme switch to one
//! call: nothing downstream holds a copy across frames, and no function
//! signature mentions the theme.
//!
//! There is deliberately exactly one theme today. No picker, no theme files, no
//! light mode.

use eframe::egui::{self, Color32};

/// Key the single installed theme is stored under in the egui context.
const THEME_KEY: &str = "rm_theme";

/// The semantic palette.
///
/// `Copy` so call sites can do `let t = ui.theme();` once per function and use
/// it freely without borrow gymnastics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    // ---- interactive accent ----
    /// Fill behind primary actions (the Launch button). Expects
    /// [`Theme::on_accent`] text on top.
    pub accent: Color32,
    /// Hover state of [`Theme::accent`].
    pub accent_hover: Color32,
    /// Accent-tinted *text* on a dark surface. Lighter than [`Theme::accent`],
    /// which is unreadable at text size against the panel.
    pub accent_text: Color32,
    /// Outline of the focused/active widget.
    pub accent_border: Color32,
    /// Neutral-positive "something is live" marker: the running-instance
    /// counter, the in-game presence dot.
    pub info: Color32,
    /// egui's own selection fill, kept here so the whole palette lives in one
    /// struct. Several primary buttons fill themselves from
    /// `visuals().selection.bg_fill`.
    pub selection: Color32,

    // ---- status ----
    /// Destructive or failed. Fill/dot strength.
    pub danger: Color32,
    /// The same meaning as text on a dark surface.
    pub danger_text: Color32,
    /// Tinted background for a danger banner.
    pub danger_surface: Color32,
    /// Risky but not yet broken: irreversible actions, stale data, retries.
    pub warning: Color32,
    /// The same meaning as text on a dark surface.
    pub warning_text: Color32,
    /// Tinted background for a warning banner.
    pub warning_surface: Color32,
    /// An account under Roblox moderation. Distinct from
    /// [`Theme::warning`] on purpose: it has to be told apart at a glance from
    /// the in-Studio presence dot it sits next to.
    pub moderated: Color32,
    /// Completed successfully. Fill/dot strength.
    pub success: Color32,
    /// The same meaning as text on a dark surface.
    pub success_text: Color32,

    // ---- presence ----
    pub presence_online: Color32,
    pub presence_in_game: Color32,
    pub presence_in_studio: Color32,
    pub presence_offline: Color32,

    // ---- neutral surfaces and text ----
    /// Placeholder blocks: an avatar that has not loaded, a chip background.
    pub surface: Color32,
    /// A neutral button that must not read as the primary action.
    pub surface_raised: Color32,
    /// Floating chrome drawn over the app, such as the drag label.
    pub overlay: Color32,
    /// Secondary text: hints and explanations under a control.
    pub text_muted: Color32,
    /// Text that should recede further still, such as a placeholder glyph.
    pub text_faint: Color32,
    /// Text and glyphs drawn on top of a saturated fill.
    pub on_accent: Color32,

    // ---- widget chrome ----
    /// Border on a resting input or button.
    pub widget_border: Color32,
    /// The same on hover.
    pub widget_border_hover: Color32,

    // ---- drag and drop ----
    /// Insertion line and highlight while reordering within a group.
    pub drop_reorder: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// The one built-in theme. Values are the ones the app already shipped;
    /// where several near-identical shades existed for one role, the most-used
    /// of them won.
    pub const fn dark() -> Self {
        Self {
            accent: Color32::from_rgb(60, 130, 220),
            accent_hover: Color32::from_rgb(80, 150, 240),
            accent_text: Color32::from_rgb(130, 180, 255),
            accent_border: Color32::from_rgb(110, 170, 230),
            info: Color32::from_rgb(30, 144, 255),
            // egui's dark-theme default, restated rather than changed: it fills
            // the selected sidebar row and the Upload/Import/Grant buttons.
            selection: Color32::from_rgb(0, 92, 128),

            danger: Color32::from_rgb(200, 60, 60),
            danger_text: Color32::from_rgb(255, 110, 110),
            danger_surface: Color32::from_rgb(80, 30, 30),
            warning: Color32::from_rgb(220, 160, 40),
            warning_text: Color32::from_rgb(240, 180, 80),
            warning_surface: Color32::from_rgb(70, 50, 20),
            moderated: Color32::from_rgb(230, 130, 40),
            success: Color32::from_rgb(60, 180, 75),
            success_text: Color32::from_rgb(130, 200, 130),

            presence_online: Color32::from_rgb(60, 180, 75),
            presence_in_game: Color32::from_rgb(30, 144, 255),
            presence_in_studio: Color32::from_rgb(255, 165, 0),
            presence_offline: Color32::from_rgb(130, 130, 130),

            surface: Color32::from_rgb(60, 60, 70),
            surface_raised: Color32::from_rgb(70, 70, 90),
            overlay: Color32::from_rgb(60, 60, 60),
            text_muted: Color32::from_rgb(180, 180, 180),
            text_faint: Color32::from_rgb(160, 160, 170),
            on_accent: Color32::WHITE,

            widget_border: Color32::from_gray(80),
            widget_border_hover: Color32::from_gray(140),

            drop_reorder: Color32::from_rgb(255, 200, 80),
        }
    }

    /// Color for a Roblox `userPresenceType`. The same four-way match used to
    /// be written out in three separate files, which is how the sidebar and the
    /// group panel came to disagree about nothing at all but could easily have
    /// come to disagree about something.
    pub fn presence(&self, presence_type: u8) -> Color32 {
        match presence_type {
            1 => self.presence_online,
            2 => self.presence_in_game,
            3 => self.presence_in_studio,
            _ => self.presence_offline,
        }
    }

    /// A translucent wash of `color`, for a drop target highlight.
    ///
    /// The channels are passed through unscaled even though the constructor is
    /// the premultiplied one. That is not an oversight: it makes the overlay
    /// read as added light rather than a dimming tint, which is what the
    /// existing drop highlights did and what actually shows up over a row that
    /// is already dark.
    pub const fn wash(color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha)
    }

    /// Push the theme into egui's own [`egui::Visuals`], so widgets RM never
    /// colors explicitly still follow it.
    ///
    /// Only the properties the app already overrode are touched. Restyling the
    /// rest of egui would be a redesign, and this is a refactor.
    pub fn apply_visuals(&self, visuals: &mut egui::Visuals) {
        // Default dark-theme widgets render with no border and a background
        // indistinguishable from `extreme_bg_color`, which made TextEdits
        // invisible next to their labels.
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.widget_border);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.widget_border_hover);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent_border);
        // Slight rounding on inputs/buttons to soften those borders.
        visuals.widgets.inactive.rounding = egui::Rounding::same(3.0);
        visuals.widgets.hovered.rounding = egui::Rounding::same(3.0);
        visuals.widgets.active.rounding = egui::Rounding::same(3.0);

        visuals.selection.bg_fill = self.selection;
    }
}

/// Install `theme` as the one this context uses, and apply it to the style.
/// Called once at startup.
pub fn install(ctx: &egui::Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(THEME_KEY), theme));
    ctx.style_mut(|style| theme.apply_visuals(&mut style.visuals));
}

/// The theme installed on `ctx`, or the built-in one if none was installed.
///
/// Falling back rather than panicking keeps unit tests and any future headless
/// path working without a startup dance.
pub fn of(ctx: &egui::Context) -> Theme {
    ctx.data(|d| d.get_temp::<Theme>(egui::Id::new(THEME_KEY)))
        .unwrap_or_default()
}

/// `ui.theme()` at any call site, with no plumbing through arguments.
pub trait ThemeUi {
    fn theme(&self) -> Theme;
}

impl ThemeUi for egui::Ui {
    fn theme(&self) -> Theme {
        of(self.ctx())
    }
}

impl ThemeUi for egui::Context {
    fn theme(&self) -> Theme {
        of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_context_returns_the_theme_that_was_installed() {
        let ctx = egui::Context::default();
        assert_eq!(of(&ctx), Theme::dark(), "should fall back before install");

        let mut custom = Theme::dark();
        custom.danger = Color32::from_rgb(1, 2, 3);
        install(&ctx, custom);
        assert_eq!(of(&ctx).danger, Color32::from_rgb(1, 2, 3));
    }

    #[test]
    fn installing_writes_through_to_visuals() {
        let ctx = egui::Context::default();
        install(&ctx, Theme::dark());
        let visuals = ctx.style().visuals.clone();
        assert_eq!(
            visuals.widgets.active.bg_stroke.color,
            Theme::dark().accent_border
        );
        assert_eq!(visuals.selection.bg_fill, Theme::dark().selection);
    }

    /// The theme must not change what egui's own selection fill looks like.
    /// Several primary buttons take their fill from it, and this refactor is
    /// not supposed to move any pixels.
    #[test]
    fn the_selection_token_matches_eguis_dark_default() {
        assert_eq!(
            Theme::dark().selection,
            egui::Visuals::dark().selection.bg_fill
        );
    }

    #[test]
    fn presence_covers_every_roblox_presence_type() {
        let t = Theme::dark();
        assert_eq!(t.presence(1), t.presence_online);
        assert_eq!(t.presence(2), t.presence_in_game);
        assert_eq!(t.presence(3), t.presence_in_studio);
        assert_eq!(t.presence(0), t.presence_offline);
        // Roblox has added presence types before; anything unknown must read as
        // offline rather than panic or render transparent.
        assert_eq!(t.presence(4), t.presence_offline);
        assert_eq!(t.presence(255), t.presence_offline);
    }

    #[test]
    fn wash_keeps_the_hue_and_only_lowers_the_alpha() {
        let base = Color32::from_rgb(255, 200, 80);
        let w = Theme::wash(base, 40);
        assert_eq!((w.r(), w.g(), w.b(), w.a()), (255, 200, 80, 40));
    }
}
