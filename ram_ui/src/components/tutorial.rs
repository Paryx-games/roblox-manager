//! Interactive first-launch tutorial.
//!
//! Each step highlights a specific widget (pulsing border drawn on the
//! foreground layer) and shows a small callout panel positioned next to it.
//! Steps advance automatically when the user performs the expected action
//! (adds an account, selects one, etc.) or manually via Back / Next buttons.
//!
//! # Rect reporting
//! Components write their key widget rects into [`TutorialState`] each frame
//! so the overlay always reflects the current layout:
//!   - `sidebar::show` → `state.add_btn_rect`
//!   - `main_panel::show` → `state.launch_btn_rect`
//!   - `app::show_add_dialog` → `state.browser_login_btn_rect`

use eframe::egui;

// ---------------------------------------------------------------------------
// Step enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TutorialStep {
    /// Intro — no widget highlighted, centered callout.
    Welcome,
    /// Highlight the "Add Account" sidebar button. Advances when the dialog opens.
    AddAccount,
    /// Highlight the "Log in with browser" button inside the add-account dialog.
    /// Advances when an account is successfully added (via either login method).
    EnterCookie,
    /// Highlight the sidebar account list. Advances when an account is selected.
    SelectAccount,
    /// Highlight the Launch button. "Next" button advances manually.
    LaunchGame,
    /// Final screen — no highlight, centered callout with Finish button.
    Done,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Tutorial state stored on `AppState`. Components write their key widget
/// rects into this struct each frame so the overlay is always accurate.
pub struct TutorialState {
    pub active: bool,
    pub step: TutorialStep,

    // Widget rects reported each frame by components.
    pub add_btn_rect: egui::Rect,
    pub launch_btn_rect: egui::Rect,
    pub browser_login_btn_rect: egui::Rect,
    pub sidebar_accounts_rect: egui::Rect,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self {
            active: false,
            step: TutorialStep::Welcome,
            add_btn_rect: egui::Rect::NOTHING,
            launch_btn_rect: egui::Rect::NOTHING,
            browser_login_btn_rect: egui::Rect::NOTHING,
            sidebar_accounts_rect: egui::Rect::NOTHING,
        }
    }
}

impl TutorialState {
    pub fn start() -> Self {
        Self {
            active: true,
            step: TutorialStep::Welcome,
            ..Self::default()
        }
    }

    /// Advance to the next step if we are currently on `from`.
    pub fn advance_from(&mut self, from: TutorialStep) {
        if self.active && self.step == from {
            self.step = match from {
                TutorialStep::Welcome => TutorialStep::AddAccount,
                TutorialStep::AddAccount => TutorialStep::EnterCookie,
                TutorialStep::EnterCookie => TutorialStep::SelectAccount,
                TutorialStep::SelectAccount => TutorialStep::LaunchGame,
                TutorialStep::LaunchGame => TutorialStep::Done,
                TutorialStep::Done => {
                    self.active = false;
                    TutorialStep::Done
                }
            };
        }
    }

    fn target_rect(&self) -> egui::Rect {
        match self.step {
            TutorialStep::AddAccount => self.add_btn_rect,
            TutorialStep::EnterCookie => self.browser_login_btn_rect,
            TutorialStep::SelectAccount => self.sidebar_accounts_rect,
            TutorialStep::LaunchGame => self.launch_btn_rect,
            TutorialStep::Welcome | TutorialStep::Done => egui::Rect::NOTHING,
        }
    }

    fn step_label(&self) -> &'static str {
        match self.step {
            TutorialStep::Welcome => "1 / 6",
            TutorialStep::AddAccount => "2 / 6",
            TutorialStep::EnterCookie => "3 / 6",
            TutorialStep::SelectAccount => "4 / 6",
            TutorialStep::LaunchGame => "5 / 6",
            TutorialStep::Done => "6 / 6",
        }
    }

    fn callout_content(&self) -> (&'static str, &'static str, bool, bool) {
        // (title, body, show_next_btn, show_back_btn)
        match self.step {
            TutorialStep::Welcome => (
                "Welcome to RM",
                "RM lets you manage multiple Roblox accounts, launch games, and save \
                 private servers, all from one window.\n\
                 Click Next to start the tour.",
                true,
                false,
            ),
            TutorialStep::AddAccount => (
                "Add your first account",
                "Click the highlighted button to open the Add Account dialog.",
                false,
                true,
            ),
            TutorialStep::EnterCookie => (
                "Sign in to Roblox",
                "Choose 'Log in with browser' and sign in in the popup window. \
                 RM will pick up your account and continue the tutorial automatically.",
                false,
                false,
            ),
            TutorialStep::SelectAccount => (
                "Select an account",
                "Your account now appears in the sidebar. Click it to select it.",
                false,
                false,
            ),
            TutorialStep::LaunchGame => (
                "Launch Roblox",
                "Enter a Place ID in the box and click Launch to start Roblox with \
                 this account. You can also save Favourite Places for quicker access.",
                true,
                true,
            ),
            TutorialStep::Done => (
                "You're all set!",
                "Use the Private Servers tab to save VIP server links, and the \
                 Settings tab to configure multi-instance mode and other options.\n\
                 Enjoy using RM!",
                false,
                true,
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Overlay rendering
// ---------------------------------------------------------------------------

/// Call this every frame after all other widgets are rendered.
pub fn show_overlay(ctx: &egui::Context, state: &mut TutorialState) {
    if !state.active {
        return;
    }

    let target = state.target_rect();
    let has_target = target != egui::Rect::NOTHING && target.is_finite();

    // ---- Pulsing highlight border ----
    if has_target {
        let t = (ctx.input(|i| i.time) * std::f64::consts::PI * 1.6).sin() as f32 * 0.5 + 0.5;
        let r = (220.0 + 35.0 * t) as u8;
        let g = (180.0 - 60.0 * t) as u8;
        let b = 50_u8;
        let color = egui::Color32::from_rgb(r, g, b);

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("tutorial_highlight"),
        ));
        let padded = target.expand(4.0);
        painter.rect_stroke(
            padded,
            egui::Rounding::same(6.0),
            egui::Stroke::new(2.5, color),
        );
        // Request repaint so the animation keeps running
        ctx.request_repaint();
    }

    // ---- Callout panel ----
    let (title, body, show_next, show_back) = state.callout_content();
    let step_label = state.step_label();
    let is_done = state.step == TutorialStep::Done;

    // Position: below the target rect if available; otherwise top-center so
    // the callout doesn't pile on top of a centered modal (the Add Account
    // dialog is anchored to screen center and would collide with a centered
    // callout in Welcome/Done/no-target states).
    let screen = ctx.screen_rect();
    let anchor_pos = if has_target {
        let below = egui::pos2(target.center().x, target.max.y + 12.0);
        // Clamp horizontally so the callout doesn't go off screen
        egui::pos2(
            below.x.clamp(screen.min.x + 10.0, screen.max.x - 310.0),
            below.y,
        )
    } else {
        egui::pos2(screen.center().x - 150.0, screen.min.y + 60.0)
    };

    let mut close = false;
    let mut next = false;
    let mut back = false;

    egui::Area::new(egui::Id::new("tutorial_callout"))
        .order(egui::Order::Tooltip)
        .fixed_pos(anchor_pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    ui.set_max_width(300.0);

                    // Step indicator
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(step_label)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new("x")
                                            .small()
                                            .color(ui.visuals().weak_text_color()),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Skip tutorial")
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(title).strong());
                    ui.add_space(4.0);
                    ui.label(body);
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if show_back && ui.small_button("< Back").clicked() {
                            back = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if is_done {
                                if ui.button("Finish").clicked() {
                                    close = true;
                                }
                            } else if show_next && ui.button("Next >").clicked() {
                                next = true;
                            }
                        });
                    });
                });
        });

    if close {
        state.active = false;
        state.step = TutorialStep::Welcome;
    } else if next {
        state.advance_from(state.step);
    } else if back {
        state.step = match state.step {
            TutorialStep::AddAccount => TutorialStep::Welcome,
            TutorialStep::LaunchGame => TutorialStep::SelectAccount,
            TutorialStep::Done => TutorialStep::LaunchGame,
            other => other,
        };
    }
}
