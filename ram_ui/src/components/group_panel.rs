//! Group control panel — shown when multiple accounts are selected.
//! Provides bulk launch, bulk remove, and selection summary.

use eframe::egui;
use ram_core::models::{Account, LaunchPreset};

use crate::icons;
use crate::theme::ThemeUi;

fn bulk_connection_selected_user_id(
    query: &str,
    candidates: &[ram_core::api::UserSearchResult],
) -> Option<u64> {
    let normalized = query.trim();
    if normalized.is_empty() {
        return None;
    }

    let lowered = normalized.to_ascii_lowercase();
    candidates.iter().find_map(|candidate| {
        let matches_id = candidate.user_id.to_string() == lowered;
        let matches_username = candidate.username.to_ascii_lowercase() == lowered;
        let matches_display_name = candidate.display_name.to_ascii_lowercase() == lowered;
        if matches_id || matches_username || matches_display_name {
            Some(candidate.user_id)
        } else {
            None
        }
    })
}

/// Actions the group panel can request.
pub enum GroupPanelAction {
    /// Launch all selected accounts into the given place/server.
    BulkLaunch {
        place_id: u64,
        job_id: Option<String>,
        data: Option<String>,
    },
    /// Revalidate the selected cookies in the background.
    RevalidateSelected,
    /// Open a browser window for each selected account.
    OpenBrowsers,
    /// Copy the selected account IDs to the clipboard.
    CopyIds,
    /// Find inventory items shared by every selected account.
    FindCommonInventory,
    /// Open the path editor for all selected accounts.
    OpenPathEditor,
    /// Deselect all.
    ClearSelection,
    /// Tile all running Roblox instances according to configured settings.
    TileWindows,
    /// Kill all Roblox instances.
    KillAll,
    /// Search Roblox users for a social action to apply across the selected accounts.
    SearchConnectionUsers { keyword: String },
    /// Send a friend request from every selected account.
    SendFriendRequestToSelected { target_user_id: u64 },
    /// Follow a user from every selected account.
    FollowSelectedUsers { target_user_id: u64 },
    /// Unfollow a user from every selected account.
    UnfollowSelectedUsers { target_user_id: u64 },
    /// Join a selected user's game from every selected account.
    JoinSelectedUsersGame { target_user_id: u64 },
    /// Block a user from every selected account.
    BlockSelectedUsers { target_user_id: u64 },
}

/// Draw the group control panel for multiple selected accounts.
/// `place_id_input`, `job_id_input`, and `data_input` are owned by the parent so single-launch
/// and bulk-launch views share the same fields — typing a Place ID into one
/// makes it appear in the other immediately.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    selected_accounts: &[&Account],
    place_id_input: &mut String,
    job_id_input: &mut String,
    data_input: &mut String,
    presets: &[LaunchPreset],
    roblox_running: bool,
    anonymize: bool,
    common_inventory_items: &[ram_core::assets_api::UserInventoryItem],
    common_inventory_loading: bool,
    common_inventory_message: Option<&str>,
    connection_search_input: &mut String,
    connection_last_search: &mut String,
    connection_search_deadline: &mut Option<std::time::Instant>,
    connection_users: &[ram_core::api::UserSearchResult],
) -> Option<GroupPanelAction> {
    let mut action: Option<GroupPanelAction> = None;
    let count = selected_accounts.len();

    egui::ScrollArea::vertical().show(ui, |ui| {
    ui.vertical(|ui| {
        // Header
        ui.horizontal(|ui| {
            ui.heading(format!("{count} Accounts Selected"));
            egui::menu::menu_button(ui, "...", |ui| {
                if ui.button("Change path").clicked() {
                    action = Some(GroupPanelAction::OpenPathEditor);
                    ui.close_menu();
                }
            });
            if ui.small_button("Clear selection").clicked() {
                action = Some(GroupPanelAction::ClearSelection);
            }
        });
        ui.separator();
        ui.add_space(4.0);

        // Selected account list (compact)
        egui::ScrollArea::vertical()
            .max_height(150.0)
            .show(ui, |ui| {
                for (idx, account) in selected_accounts.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let dot = ui.theme().presence(account.last_presence.user_presence_type);
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            dot_rect.center(),
                            4.0,
                            dot,
                        );
                        ui.label(if anonymize {
                            format!("Account {}", idx + 1)
                        } else {
                            account.label().to_string()
                        });
                    });
                }
            });

        ui.add_space(8.0);
        ui.heading("Bulk Actions");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Revalidate").clicked() {
                action = Some(GroupPanelAction::RevalidateSelected);
            }
            if ui.button("Open browsers").clicked() {
                action = Some(GroupPanelAction::OpenBrowsers);
            }
            if icons::button(ui, "copy", "Copy IDs").clicked() {
                action = Some(GroupPanelAction::CopyIds);
            }
            if ui
                .add_enabled(!common_inventory_loading, egui::Button::new("Common inventory"))
                .on_hover_text("Find hats, accessories, clothing, gear, and emotes present in every selected account")
                .clicked()
            {
                action = Some(GroupPanelAction::FindCommonInventory);
            }
            if ui.button("Change path").clicked() {
                action = Some(GroupPanelAction::OpenPathEditor);
            }
        });

        if common_inventory_loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking common inventory...");
            });
        } else if let Some(message) = common_inventory_message {
            ui.label(message);
        } else if !common_inventory_items.is_empty() {
                ui.label(format!(
                "{} common Roblox inventory item(s) found",
                common_inventory_items.len()
            ));
            for item in common_inventory_items.iter().take(6) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(item.name.clone()).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{} ({})", item.asset_id, item.asset_type));
                    });
                });
            }
            if common_inventory_items.len() > 6 {
                ui.label(format!("+ {} more items", common_inventory_items.len() - 6));
            }
        }
        ui.add_space(8.0);

        ui.heading("Connections");
        ui.add_space(4.0);
        let search_value = connection_search_input.trim().to_string();
        let valid_search = !search_value.is_empty() && search_value.len() >= 3;
        let now = std::time::Instant::now();
        let should_debounce = valid_search
            && *connection_last_search != search_value
            && connection_search_deadline
                .is_none_or(|deadline| now >= deadline);

        ui.horizontal(|ui| {
            let search_button_width = 120.0;
            let clear_button_width = 60.0;
            let input_width = (ui.available_width() - search_button_width - clear_button_width - 12.0)
                .max(180.0);
            ui.add_sized(
                egui::vec2(input_width, 30.0),
                egui::TextEdit::singleline(connection_search_input)
                    .hint_text("Search username, display name, or user ID"),
            );
            if ui
                .add_enabled(
                    valid_search,
                    egui::Button::new("Search Roblox").min_size(egui::vec2(search_button_width, 30.0)),
                )
                .clicked()
            {
                *connection_last_search = search_value.clone();
                *connection_search_deadline = Some(now + std::time::Duration::from_millis(200));
                action = Some(GroupPanelAction::SearchConnectionUsers {
                    keyword: search_value.clone(),
                });
            }
            if ui
                .add(egui::Button::new("Clear").min_size(egui::vec2(clear_button_width, 30.0)))
                .clicked()
            {
                connection_search_input.clear();
                connection_last_search.clear();
                *connection_search_deadline = None;
            }
        });

        if !search_value.is_empty() && !valid_search {
            ui.add_space(4.0);
            ui.colored_label(
                ui.theme().warning_text,
                "Use at least 3 characters for a Roblox search.",
            );
        }

        if should_debounce {
            *connection_last_search = search_value.clone();
            *connection_search_deadline = Some(now + std::time::Duration::from_millis(250));
            action = Some(GroupPanelAction::SearchConnectionUsers {
                keyword: search_value.clone(),
            });
        }

        let query = search_value.to_ascii_lowercase();
        let target = if query.is_empty() {
            None
        } else {
            bulk_connection_selected_user_id(&search_value, connection_users)
        };

        if !query.is_empty() {
            ui.add_space(6.0);
            if connection_users.is_empty() {
                ui.label(
                    egui::RichText::new("No matches found. Try a wider username or ID.")
                        .color(ui.visuals().weak_text_color()),
                );
            } else {
                ui.label(
                    egui::RichText::new("Results")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(4.0);
                egui::Frame::default()
                    .inner_margin(egui::Margin::same(8.0))
                    .rounding(egui::Rounding::same(6.0))
                    .fill(ui.visuals().panel_fill)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        for candidate in connection_users.iter().take(6) {
                            let selected = target == Some(candidate.user_id);
                            let label = format!(
                                "{} (@{}) · ID {}",
                                candidate.display_name,
                                candidate.username,
                                candidate.user_id,
                            );
                            if ui.selectable_label(selected, label).clicked() {
                                *connection_search_input = candidate.user_id.to_string();
                            }
                        }
                    });
            }
        }

        ui.add_space(8.0);
        if let Some(selected_id) = target {
            ui.horizontal_wrapped(|ui| {
                let selected_user = connection_users
                    .iter()
                    .find(|candidate| candidate.user_id == selected_id)
                    .unwrap();
                ui.label(
                    egui::RichText::new(format!(
                        "Selected: {} (@{})",
                        selected_user.display_name, selected_user.username,
                    ))
                    .strong(),
                );
            });
        }

        ui.add_space(6.0);
        if let Some(target_user_id) = target {
            ui.horizontal_wrapped(|ui| {
                let width = (ui.available_width() / 5.0 - 12.0).clamp(90.0, 152.0);
                if ui
                    .add_enabled(true, egui::Button::new("Friend request").min_size(egui::vec2(width, 32.0)))
                    .clicked()
                {
                    action = Some(GroupPanelAction::SendFriendRequestToSelected { target_user_id });
                }
                if ui
                    .add_enabled(true, egui::Button::new("Follow").min_size(egui::vec2(width, 32.0)))
                    .clicked()
                {
                    action = Some(GroupPanelAction::FollowSelectedUsers { target_user_id });
                }
                if ui
                    .add_enabled(true, egui::Button::new("Unfollow").min_size(egui::vec2(width, 32.0)))
                    .clicked()
                {
                    action = Some(GroupPanelAction::UnfollowSelectedUsers { target_user_id });
                }
                if ui
                    .add_enabled(true, egui::Button::new("Join their game").min_size(egui::vec2(width, 32.0)))
                    .clicked()
                {
                    action = Some(GroupPanelAction::JoinSelectedUsersGame { target_user_id });
                }
                if ui
                    .add_enabled(true, egui::Button::new("Block user").min_size(egui::vec2(width, 32.0)))
                    .clicked()
                {
                    action = Some(GroupPanelAction::BlockSelectedUsers { target_user_id });
                }
            });
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Bulk launch controls
        ui.heading("Bulk Launch");
        ui.add_space(4.0);
        ui.label("All selected accounts will join the same server sequentially.");
        ui.add_space(4.0);

        // Preset quick-select chips (same set as the single-launch view).
        if !presets.is_empty() {
            super::preset_chips(
                ui,
                "Presets:",
                presets,
                place_id_input,
                job_id_input,
                data_input,
            );
            ui.add_space(4.0);
        }

        egui::Grid::new("bulk_launch_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Place ID:");
                ui.text_edit_singleline(place_id_input);
                ui.end_row();

                ui.label("Job ID (optional):");
                ui.text_edit_singleline(job_id_input);
                ui.end_row();

                ui.label("Data (optional):");
                ui.text_edit_singleline(data_input);
                ui.end_row();
            });

        ui.label(
            egui::RichText::new(
                "examples: ?linkCode=CODE    ?accessCode=CODE    ?userId=123456789    ?launchData=DATA",
            )
            .small()
            .color(ui.visuals().weak_text_color()),
        );
        let invalid_data_message = super::invalid_launch_data_message(data_input);
        if let Some(message) = invalid_data_message {
            ui.colored_label(ui.theme().danger_text, message);
        }

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let place_valid = place_id_input.parse::<u64>().is_ok();
            let btn = icons::enabled_button(
                ui,
                "launch",
                &format!("Launch {count} Accounts"),
                place_valid && invalid_data_message.is_none(),
            );
            if btn.clicked() {
                if let Ok(place_id) = place_id_input.parse::<u64>() {
                    let job_id = if job_id_input.trim().is_empty() {
                        None
                    } else {
                        Some(job_id_input.trim().to_string())
                    };
                    let data = if data_input.trim().is_empty() {
                        None
                    } else {
                        Some(data_input.trim().to_string())
                    };
                    action = Some(GroupPanelAction::BulkLaunch {
                        place_id,
                        job_id,
                        data,
                    });
                }
            }

            if roblox_running {
                if ui
                    .button("Tile Windows")
                    .on_hover_text(
                        "Tile and arrange running Roblox windows according to your configured layout.",
                    )
                    .clicked()
                {
                    action = Some(GroupPanelAction::TileWindows);
                }
                if icons::button(ui, "kill", "Kill All Instances").clicked() {
                    action = Some(GroupPanelAction::KillAll);
                }
            }
        });
    });
    }); // ScrollArea

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_connection_target_matches_user_id_and_username() {
        let candidates = [ram_core::api::UserSearchResult {
            user_id: 12345,
            username: "alice".to_string(),
            display_name: "Alice".to_string(),
        }];

        assert_eq!(
            bulk_connection_selected_user_id("12345", &candidates),
            Some(12345)
        );
        assert_eq!(
            bulk_connection_selected_user_id("alice", &candidates),
            Some(12345)
        );
        assert_eq!(
            bulk_connection_selected_user_id("missing", &candidates),
            None
        );
    }
}
