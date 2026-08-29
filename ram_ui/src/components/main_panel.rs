//! Main content panel — selected account details, avatar, launch controls.

use eframe::egui;
use ram_core::models::{Account, LaunchPreset};

use crate::icons;
use crate::theme::ThemeUi;

/// Actions the main panel can request.
pub enum MainPanelAction {
    LaunchGame {
        place_id: u64,
        job_id: Option<String>,
        data: Option<String>,
    },
    OpenPathEditor,
    /// Reload the selected account's latest inventory from Roblox.
    LoadInventory(u64),
    OpenInventory(u64),
    RemoveAccount(u64),
    UpdateAlias {
        user_id: u64,
        alias: String,
    },
    /// Save the current Place ID / Job ID inputs as a named launch preset.
    SavePreset {
        name: String,
        place_id: u64,
        job_id: Option<String>,
        data: Option<String>,
    },
    KillAll,
    /// Open a webview pre-logged in as this account.
    OpenBrowserAs(u64),
    SendFriendRequest {
        target_user_id: u64,
    },
    BlockUser {
        target_user_id: u64,
    },
    SearchConnectionUsers {
        keyword: String,
    },
}

/// Persistent input state for the main panel.
#[derive(Default)]
pub struct MainPanelState {
    pub place_id_input: String,
    pub job_id_input: String,
    pub data_input: String,
    pub alias_input: String,
    /// Track which account the alias input belongs to.
    alias_for_user: Option<u64>,
    /// Name buffer for the "Save as preset" inline form.
    pub preset_name_input: String,
    /// True while the "save as preset" popover is open.
    pub show_save_form: bool,
    /// Set the frame the save popover opens so we request focus exactly once.
    save_form_needs_focus: bool,
    pub connection_target_input: String,
}

/// Result returned by the main panel.
pub struct MainPanelResult {
    pub action: Option<MainPanelAction>,
    /// Screen rect of the Launch button (for tutorial highlighting).
    pub launch_btn_rect: egui::Rect,
}

/// Draw the main panel for a selected account.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    account: &Account,
    state: &mut MainPanelState,
    roblox_running: bool,
    avatar_bytes: Option<&Vec<u8>>,
    presets: &[LaunchPreset],
    anonymize: bool,
    player_path_label: &str,
    inventory_items: &[ram_core::assets_api::UserInventoryItem],
    inventory_loading: bool,
    inventory_error: Option<&str>,
    connection_users: &[ram_core::api::UserSearchResult],
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
                                if icons::button(ui, "delete", "Remove account").clicked()
                                {
                                    action = Some(MainPanelAction::RemoveAccount(account.user_id));
                                    ui.close_menu();
                                }
                                if ui.button("Change path").clicked() {
                                    action = Some(MainPanelAction::OpenPathEditor);
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
                            "Warning: Account terminated"
                                } else {
                            "Warning: Account moderated"
                                })
                                .strong()
                                .size(15.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if icons::button(ui, "browser", "Open browser as")
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
                        &mut state.data_input,
                    );
                    ui.add_space(8.0);
                }

                // Floating-label inputs (label above the field, full width).
                labelled_input(ui, "Place ID", &mut state.place_id_input, "", false);
                ui.add_space(6.0);
                labelled_input(
                    ui,
                    "Job ID (optional)",
                    &mut state.job_id_input,
                    "Specific server GUID",
                    false,
                );
                ui.add_space(6.0);
                let invalid_data_message = super::invalid_launch_data_message(&state.data_input);
                labelled_input(
                    ui,
                    "Data (optional)",
                    &mut state.data_input,
                    "Extra launch query data",
                    invalid_data_message.is_some(),
                );
                ui.label(
                    egui::RichText::new(
                        "examples: ?linkCode=CODE    ?accessCode=CODE    ?userId=123456789    ?launchData=DATA",
                    )
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
                if let Some(message) = invalid_data_message {
                    ui.colored_label(ui.theme().danger_text, message);
                }
                ui.add_space(10.0);

                let place_valid = state.place_id_input.parse::<u64>().is_ok();
                let can_launch = place_valid && invalid_data_message.is_none() && account.can_launch();

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

                    let launch_btn = icons::sized_button_enabled(
                        ui,
                        "launch",
                        "Launch",
                        egui::vec2(primary_w, primary_h),
                        if can_launch {
                            theme.accent
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        },
                        can_launch,
                    )
                    .on_hover_text(if account.moderation.as_ref().is_some_and(|info| info.is_active()) {
                        "Launching is blocked while this account is restricted by Roblox."
                    } else if place_valid {
                        "Launch this account into the chosen place"
                    } else {
                        "Enter a Place ID to launch"
                    });
                    launch_btn_rect = launch_btn.rect;
                    if launch_btn.clicked() {
                        if let Ok(place_id) = state.place_id_input.parse::<u64>() {
                            let job_id = parse_optional(&state.job_id_input);
                            let data = parse_optional(&state.data_input);
                            action = Some(MainPanelAction::LaunchGame {
                                place_id,
                                job_id,
                                data,
                            });
                        }
                    }
                    // Hover/active tint to make the primary obvious.
                    if launch_btn.hovered() && can_launch {
                        ui.painter().rect_filled(
                            launch_btn.rect,
                            egui::Rounding::same(3.0),
                            theme.accent_hover.linear_multiply(0.15),
                        );
                    }

                    if icons::sized_button(
                        ui,
                        "browser",
                        "Open browser as",
                        egui::vec2(primary_w, primary_h),
                        theme.surface_raised,
                    )
                        .on_hover_text("Open a webview signed in as this account")
                        .clicked()
                    {
                        action = Some(MainPanelAction::OpenBrowserAs(account.user_id));
                    }

                    // Save-as-preset icon button
                    let save_resp = icons::sized_button_enabled(
                        ui,
                        "star",
                        "Save preset",
                        egui::vec2(icon_w, primary_h),
                        ui.visuals().widgets.inactive.bg_fill,
                        place_valid,
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
                        && icons::sized_button(
                                ui,
                                "kill",
                                "Kill",
                                egui::vec2(icon_w, primary_h),
                                ui.visuals().widgets.inactive.bg_fill,
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
                            1.0_f32,
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
                                    && invalid_data_message.is_none()
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
                                            data: parse_optional(&state.data_input),
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

            section_frame.show(ui, |ui: &mut egui::Ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.heading("Roblox inventory");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Open inventory")
                            .on_hover_text("Open the full inventory browser")
                            .clicked()
                        {
                            action = Some(MainPanelAction::OpenInventory(account.user_id));
                        }
                        if ui
                            .add_enabled(!inventory_loading, egui::Button::new("Refresh"))
                            .on_hover_text("Fetch hats, accessories, clothing, gear, and emotes owned by this account")
                            .clicked()
                        {
                            action = Some(MainPanelAction::LoadInventory(account.user_id));
                        }
                    });
                });
                ui.add_space(6.0);
                if inventory_loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading inventory...");
                    });
                } else if let Some(message) = inventory_error {
                    ui.colored_label(
                        theme.danger,
                        egui::RichText::new(format!("Warning: {message}"))
                            .strong(),
                    );
                } else if inventory_items.is_empty() {
                    ui.label(
                        egui::RichText::new("No user inventory loaded yet. Refresh to fetch hats, accessories, clothing, gear, and emotes.")
                            .color(ui.visuals().weak_text_color()),
                    );
                } else {
                    for item in inventory_items.iter().take(6) {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(item.name.clone()).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(&item.asset_type);
                            });
                        });
                    }
                    if inventory_items.len() > 6 {
                        ui.add_space(4.0);
                        ui.label(format!("+ {} more items", inventory_items.len() - 6))
                            .on_hover_text("More Roblox inventory items are available for this account.");
                    }
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

                        ui.label(
                            egui::RichText::new("Player path")
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.label(player_path_label);
                        ui.end_row();

                        if let Some(created_at) = account.created_at {
                            ui.label(
                                egui::RichText::new("Created")
                                    .color(ui.visuals().weak_text_color()),
                            );
                            ui.label(format!(
                                "{} ({})",
                                created_at.format("%Y-%m-%d %H:%M UTC"),
                                crate::components::format_account_age(created_at),
                            ));
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
                                "Warning: Cookie expired. Remove and re-add this account with a fresh cookie.",
                            );
                        });
                }
                });
            });
            ui.add_space(8.0);
            egui::Frame::default().inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("Connections");
                ui.label("Search a managed username or user ID, then choose an action.");
                ui.text_edit_singleline(&mut state.connection_target_input);
                let query = state.connection_target_input.trim().to_ascii_lowercase();
                let target = connection_users.iter().find(|candidate| {
                    candidate.user_id.to_string() == query
                        || candidate.username.to_ascii_lowercase() == query
                        || candidate.display_name.to_ascii_lowercase() == query
                });
                if ui.button("Search Roblox").clicked() && !query.is_empty() {
                    action = Some(MainPanelAction::SearchConnectionUsers { keyword: state.connection_target_input.trim().to_string() });
                }
                if !query.is_empty() {
                    for candidate in connection_users.iter().filter(|candidate| {
                        candidate.user_id.to_string().contains(&query)
                            || candidate.username.to_ascii_lowercase().contains(&query)
                            || candidate.display_name.to_ascii_lowercase().contains(&query)
                    }).take(5) {
                        let label = format!("{} (@{}) · ID {}", candidate.display_name, candidate.username, candidate.user_id);
                        if ui.selectable_label(target.is_some_and(|selected| selected.user_id == candidate.user_id), label).clicked() {
                            state.connection_target_input = candidate.user_id.to_string();
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(target.is_some(), egui::Button::new("Send friend request")).clicked() {
                        action = Some(MainPanelAction::SendFriendRequest { target_user_id: target.unwrap().user_id });
                    }
                    if ui.add_enabled(target.is_some(), egui::Button::new("Block user")).clicked() {
                        action = Some(MainPanelAction::BlockUser { target_user_id: target.unwrap().user_id });
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
                egui::RichText::new("Copy")
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
        ui.painter()
            .rect_filled(rect, size / 8.0, ui.theme().surface);
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
        .stroke(egui::Stroke::new(1.0_f32, color.linear_multiply(0.55)))
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
fn labelled_input(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, invalid: bool) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(ui.visuals().weak_text_color())
                .small(),
        );
        let response = ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
                .text_color(if invalid {
                    ui.theme().danger_text
                } else {
                    ui.visuals().text_color()
                }),
        );
        if invalid {
            ui.painter().rect_stroke(
                response.rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(1.0_f32, ui.theme().danger_text),
            );
        }
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
