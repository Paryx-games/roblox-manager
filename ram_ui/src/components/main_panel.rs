//! Main content panel — selected account details, avatar, launch controls.

use eframe::egui;
use ram_core::models::{Account, LaunchPreset};

use crate::theme::ThemeUi;

/// Actions the main panel can request.
pub enum MainPanelAction {
    LaunchGame { place_id: u64, job_id: Option<String> },
    RemoveAccount(u64),
    UpdateAlias { user_id: u64, alias: String },
    /// Save the current Place ID / Job ID inputs as a named launch preset.
    SavePreset {
        name: String,
        place_id: u64,
        job_id: Option<String>,
    },
    KillAll,
    /// Open a webview pre-logged in as this account.
    OpenBrowserAs(u64),
}

/// Persistent input state for the main panel.
#[derive(Default)]
pub struct MainPanelState {
    pub place_id_input: String,
    pub job_id_input: String,
    pub alias_input: String,
    /// Track which account the alias input belongs to.
    alias_for_user: Option<u64>,
    /// Name buffer for the "Save as preset" inline form.
    pub preset_name_input: String,
    /// True while the "save as preset" popover is open.
    pub show_save_form: bool,
    /// Set the frame the save popover opens so we request focus exactly once.
    save_form_needs_focus: bool,
}

/// Result returned by the main panel.
pub struct MainPanelResult {
    pub action: Option<MainPanelAction>,
    /// Screen rect of the Launch button (for tutorial highlighting).
    pub launch_btn_rect: egui::Rect,
}

/// Draw the main panel for a selected account.
pub fn show(
    ui: &mut egui::Ui,
    account: &Account,
    state: &mut MainPanelState,
    roblox_running: bool,
    avatar_bytes: Option<&Vec<u8>>,
    presets: &[LaunchPreset],
    anonymize: bool,
) -> MainPanelResult {
    let theme = ui.theme();
    let mut action: Option<MainPanelAction> = None;
    let mut launch_btn_rect = egui::Rect::NOTHING;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            let section_frame = egui::Frame::default()
                .inner_margin(egui::Margin::same(12.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(ui.visuals().extreme_bg_color);

            // -------------------------------------------------------------
            // Header — avatar, name, presence chip, kebab menu (⋮) on right.
            // -------------------------------------------------------------
            section_frame.show(ui, |ui: &mut egui::Ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    draw_avatar(ui, account.user_id, avatar_bytes, 80.0, anonymize);
                    ui.add_space(8.0);

                    ui.vertical(|ui| {
                        if anonymize {
                            ui.heading("Account");
                        } else {
                            ui.heading(&account.display_name);
                            ui.label(
                                egui::RichText::new(format!("@{}", account.username))
                                    .color(ui.visuals().weak_text_color()),
                            );
                            ui.label(
                                egui::RichText::new(format!("ID: {}", account.user_id))
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        ui.add_space(2.0);
                        draw_presence_chip(ui, &account.last_presence);
                    });

                    // Kebab menu on the right
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Min),
                        |ui| {
                            egui::menu::menu_button(ui, "...", |ui| {
                                ui.set_min_width(160.0);
                                if ui
                                    .button(
                                        egui::RichText::new("\u{1f5d1}  Remove account")
                                            .color(theme.danger_text),
                                    )
                                    .clicked()
                                {
                                    action = Some(MainPanelAction::RemoveAccount(account.user_id));
                                    ui.close_menu();
                                }
                            })
                            .response
                            .on_hover_text("More actions");
                        },
                    );
                });
            });
            ui.add_space(8.0);

            // -------------------------------------------------------------
            // Moderation banner — most urgent info, surfaced before launch.
            // -------------------------------------------------------------
            if let Some(info) = account
                .moderation
                .as_ref()
                .filter(|m| m.is_active())
            {
                let banned = info.is_banned;
                let bg = if banned {
                    theme.danger_surface
                } else {
                    theme.warning_surface
                };
                let fg = if banned {
                    theme.danger_text
                } else {
                    theme.warning_text
                };
                egui::Frame::default()
                    .fill(bg)
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                fg,
                                egui::RichText::new(if banned {
                                    "\u{26a0} Account terminated"
                                } else {
                                    "\u{26a0} Account moderated"
                                })
                                .strong()
                                .size(15.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button("\u{1f310} Open browser as")
                                        .on_hover_text(
                                            "Sign in via webview to view the full moderation message or appeal",
                                        )
                                        .clicked()
                                    {
                                        action = Some(MainPanelAction::OpenBrowserAs(
                                            account.user_id,
                                        ));
                                    }
                                },
                            );
                        });
                        if let Some(reason) = &info.reason {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(reason).color(fg));
                        }
                        match &info.expires_at {
                            Some(exp) => {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Expires: {}",
                                        exp.format("%Y-%m-%d %H:%M UTC")
                                    ))
                                    .small()
                                    .color(fg),
                                );
                            }
                            None if banned => {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("Permanent termination.")
                                        .small()
                                        .color(fg),
                                );
                            }
                            _ => {}
                        }
                        if let Some(checked) = &info.last_checked {
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Checked: {}",
                                    checked.format("%Y-%m-%d %H:%M UTC")
                                ))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
                ui.add_space(8.0);
            }

            // -------------------------------------------------------------
            // Hero — Launch controls. The big primary action area.
            // -------------------------------------------------------------
            section_frame.show(ui, |ui: &mut egui::Ui| {
                ui.set_min_width(ui.available_width());

                // Preset quick-select chips
                if !presets.is_empty() {
                    let label = egui::RichText::new("Presets")
                        .color(ui.visuals().weak_text_color());
                    super::preset_chips(
                        ui,
                        label,
                        presets,
                        &mut state.place_id_input,
                        &mut state.job_id_input,
                    );
                    ui.add_space(8.0);
                }

                // Floating-label inputs (label above the field, full width).
                labelled_input(ui, "Place ID", &mut state.place_id_input, "");
                ui.add_space(6.0);
                labelled_input(
                    ui,
                    "Job ID (optional)",
                    &mut state.job_id_input,
                    "Specific server GUID",
                );
                ui.add_space(10.0);

                let place_valid = state.place_id_input.parse::<u64>().is_ok();

                // Primary action row — Launch + Open browser as + save-preset
                // icon button. Launch dominates visually so the user always
                // knows the primary path.
                ui.horizontal(|ui| {
                    let avail = ui.available_width();
                    // Reserve space for two side-by-side primary buttons +
                    // a small icon button + a small kill button (if shown).
                    let primary_h = 38.0;
                    let icon_w = 38.0;
                    let kill_extra = if roblox_running { icon_w + 6.0 } else { 0.0 };
                    let primary_w = ((avail - icon_w - kill_extra - 12.0) / 2.0).max(120.0);

                    let launch_btn = ui.add_enabled(
                        place_valid,
                        egui::Button::new(
                            egui::RichText::new("\u{1f680}  Launch")
                                .size(15.0)
                                .strong()
                                .color(theme.on_accent),
                        )
                        .min_size(egui::vec2(primary_w, primary_h))
                        .fill(if place_valid {
                            theme.accent
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        }),
                    )
                    .on_hover_text(if place_valid {
                        "Launch this account into the chosen place"
                    } else {
                        "Enter a Place ID to launch"
                    });
                    launch_btn_rect = launch_btn.rect;
                    if launch_btn.clicked() {
                        if let Ok(place_id) = state.place_id_input.parse::<u64>() {
                            let job_id = parse_optional(&state.job_id_input);
                            action =
                                Some(MainPanelAction::LaunchGame { place_id, job_id });
                        }
                    }
                    // Hover/active tint to make the primary obvious.
                    if launch_btn.hovered() && place_valid {
                        ui.painter().rect_filled(
                            launch_btn.rect,
                            egui::Rounding::same(3.0),
                            theme.accent_hover.linear_multiply(0.15),
                        );
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("\u{1f310}  Open browser as")
                                    .size(15.0)
                                    .color(ui.visuals().strong_text_color()),
                            )
                            .min_size(egui::vec2(primary_w, primary_h))
                            .fill(theme.surface_raised),
                        )
                        .on_hover_text("Open a webview signed in as this account")
                        .clicked()
                    {
                        action = Some(MainPanelAction::OpenBrowserAs(account.user_id));
                    }

                    // Save-as-preset icon button
                    let save_resp = ui
                        .add_enabled(
                            place_valid,
                            egui::Button::new(
                                egui::RichText::new("\u{2b50}").size(15.0),
                            )
                            .min_size(egui::vec2(icon_w, primary_h)),
                        )
                        .on_hover_text("Save these inputs as a launch preset");
                    if save_resp.clicked() {
                        state.show_save_form = !state.show_save_form;
                        if state.show_save_form {
                            state.preset_name_input.clear();
                            state.save_form_needs_focus = true;
                        }
                    }

                    if roblox_running
                        && ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("\u{2620}").size(15.0),
                                )
                                .min_size(egui::vec2(icon_w, primary_h)),
                            )
                            .on_hover_text("Kill all running Roblox instances")
                            .clicked()
                    {
                        action = Some(MainPanelAction::KillAll);
                    }
                });

                // Inline save-as-preset popover (appears below the button row
                // when ⭐ is toggled). Stays small so it doesn't push the rest
                // of the page around dramatically.
                if state.show_save_form {
                    ui.add_space(6.0);
                    egui::Frame::default()
                        .inner_margin(egui::Margin::same(8.0))
                        .rounding(egui::Rounding::same(4.0))
                        .fill(ui.visuals().faint_bg_color)
                        .stroke(egui::Stroke::new(
                            1.0,
                            ui.visuals().widgets.noninteractive.bg_stroke.color,
                        ))
                        .show(ui, |ui: &mut egui::Ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(
                                egui::RichText::new("Save as preset")
                                    .strong(),
                            );
                            ui.add_space(4.0);
                            let txt_resp = ui.add(
                                egui::TextEdit::singleline(&mut state.preset_name_input)
                                    .hint_text("Preset name")
                                    .desired_width(f32::INFINITY),
                            );
                            if state.save_form_needs_focus {
                                txt_resp.request_focus();
                                state.save_form_needs_focus = false;
                            }
                            let enter =
                                txt_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let can_save = place_valid
                                    && !state.preset_name_input.trim().is_empty();
                                let save_clicked = ui
                                    .add_enabled(can_save, egui::Button::new("Save"))
                                    .clicked();
                                if (save_clicked || (enter && can_save))
                                    && place_valid
                                {
                                    if let Ok(pid) = state.place_id_input.parse::<u64>() {
                                        action = Some(MainPanelAction::SavePreset {
                                            name: state
                                                .preset_name_input
                                                .trim()
                                                .to_string(),
                                            place_id: pid,
                                            job_id: parse_optional(&state.job_id_input),
                                        });
                                        state.preset_name_input.clear();
                                        state.show_save_form = false;
                                    }
                                }
                                if ui.button("Cancel").clicked() {
                                    state.show_save_form = false;
                                }
                            });
                        });
                }
            });
            ui.add_space(8.0);

            // -------------------------------------------------------------
            // Account metadata — secondary info, no destructive actions.
            // -------------------------------------------------------------
            section_frame.show(ui, |ui: &mut egui::Ui| {
                ui.set_min_width(ui.available_width());

                if state.alias_for_user != Some(account.user_id) {
                    state.alias_input = account.alias.clone();
                    state.alias_for_user = Some(account.user_id);
                }

                egui::Grid::new("meta_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Alias")
                                .color(ui.visuals().weak_text_color()),
                        );
                        let alias_response =
                            ui.text_edit_singleline(&mut state.alias_input);
                        if alias_response.lost_focus()
                            && state.alias_input != account.alias
                        {
                            action = Some(MainPanelAction::UpdateAlias {
                                user_id: account.user_id,
                                alias: state.alias_input.clone(),
                            });
                        }
                        ui.end_row();

                        if !account.group.is_empty() {
                            ui.label(
                                egui::RichText::new("Group")
                                    .color(ui.visuals().weak_text_color()),
                            );
                            ui.label(&account.group);
                            ui.end_row();
                        }

                        if let Some(ts) = &account.last_validated {
                            ui.label(
                                egui::RichText::new("Validated")
                                    .color(ui.visuals().weak_text_color()),
                            );
                            let age = chrono::Utc::now() - *ts;
                            let color = if age.num_hours() > 24 {
                                theme.warning
                            } else {
                                ui.visuals().text_color()
                            };
                            ui.colored_label(
                                color,
                                ts.format("%Y-%m-%d %H:%M UTC").to_string(),
                            );
                            ui.end_row();
                        }

                        if !account.last_presence.last_location.is_empty() {
                            ui.label(
                                egui::RichText::new("Location")
                                    .color(ui.visuals().weak_text_color()),
                            );
                            ui.label(&account.last_presence.last_location);
                            ui.end_row();
                        }
                    });

                // Skip the cookie-expired banner when there's an active
                // moderation — the moderation banner already covers it, and
                // "re-add with a fresh cookie" is misleading advice when
                // Roblox has revoked the cookie as part of an enforcement.
                let mod_active = account
                    .moderation
                    .as_ref()
                    .is_some_and(|m| m.is_active());
                if account.cookie_expired && !mod_active {
                    ui.add_space(6.0);
                    egui::Frame::default()
                        .fill(theme.danger_surface)
                        .rounding(egui::Rounding::same(4.0))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.colored_label(
                                theme.danger_text,
                                "\u{26a0} Cookie expired. Remove and re-add this account with a fresh cookie.",
                            );
                        });
                }
            });
        });
    });

    MainPanelResult {
        action,
        launch_btn_rect,
    }
}

/// Show a placeholder when no account is selected.
pub fn show_empty(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(
                egui::RichText::new("\u{1f4cb}")
                    .size(48.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("No account selected")
                    .heading()
                    .color(ui.visuals().strong_text_color()),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Pick an account in the sidebar to view it.")
                    .color(ui.visuals().weak_text_color()),
            );
        });
    });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render an account avatar from cached bytes or a placeholder of the same size.
/// `anonymize` only flips the URI discriminator so egui's image cache doesn't
/// serve the wrong variant when the toggle changes; the caller is responsible
/// for handing us the already-blurred bytes when anonymize is on.
fn draw_avatar(
    ui: &mut egui::Ui,
    user_id: u64,
    bytes: Option<&Vec<u8>>,
    size: f32,
    anonymize: bool,
) {
    let sz = egui::vec2(size, size);
    if let Some(bytes) = bytes {
        let variant = if anonymize { "anon" } else { "raw" };
        let uri = format!("bytes://avatar/{variant}_{user_id}.png");
        ui.add(
            egui::Image::from_bytes(uri, bytes.clone())
                .fit_to_exact_size(sz)
                .rounding(egui::Rounding::same(size / 8.0)),
        );
    } else {
        let (rect, _) = ui.allocate_exact_size(sz, egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            size / 8.0,
            ui.theme().surface,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "…",
            egui::FontId::proportional(size * 0.45),
            ui.theme().on_accent,
        );
    }
}

/// Pill-shaped presence chip ("Online" / "In game …" / "Offline" + colored dot).
fn draw_presence_chip(ui: &mut egui::Ui, presence: &ram_core::models::Presence) {
    let color = ui.theme().presence(presence.user_presence_type);
    let label = match presence.user_presence_type {
        1 => "Online",
        2 => "In game",
        3 => "In Studio",
        _ => "Offline",
    };
    let detail = presence.status_text();
    let text: String = if presence.user_presence_type == 0 || detail == label {
        label.to_string()
    } else {
        detail.to_string()
    };
    egui::Frame::default()
        .fill(color.linear_multiply(0.18))
        .stroke(egui::Stroke::new(1.0, color.linear_multiply(0.55)))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (dot_rect, _) =
                    ui.allocate_exact_size(egui::vec2(8.0, 16.0), egui::Sense::hover());
                ui.painter().circle_filled(dot_rect.center(), 4.0, color);
                ui.label(egui::RichText::new(text).color(color).small());
            });
        });
}

/// Input with the label rendered above the field rather than to its left.
fn labelled_input(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(ui.visuals().weak_text_color())
                .small(),
        );
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(f32::INFINITY)
                .hint_text(hint),
        );
    });
}

/// Trim and turn `""` into `None`, otherwise `Some(trimmed)`.
fn parse_optional(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
