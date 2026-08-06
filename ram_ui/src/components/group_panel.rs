//! Group control panel — shown when multiple accounts are selected.
//! Provides bulk launch, bulk remove, and selection summary.

use eframe::egui;
use ram_core::models::{Account, LaunchPreset};

use crate::theme::ThemeUi;

/// Actions the group panel can request.
pub enum GroupPanelAction {
    /// Launch all selected accounts into the given place/server.
    BulkLaunch {
        place_id: u64,
        job_id: Option<String>,
    },
    /// Deselect all.
    ClearSelection,
    /// Kill all Roblox instances.
    KillAll,
}

/// Draw the group control panel for multiple selected accounts.
/// `place_id_input` and `job_id_input` are owned by the parent so single-launch
/// and bulk-launch views share the same fields — typing a Place ID into one
/// makes it appear in the other immediately.
pub fn show(
    ui: &mut egui::Ui,
    selected_accounts: &[&Account],
    place_id_input: &mut String,
    job_id_input: &mut String,
    presets: &[LaunchPreset],
    roblox_running: bool,
    anonymize: bool,
) -> Option<GroupPanelAction> {
    let mut action: Option<GroupPanelAction> = None;
    let count = selected_accounts.len();

    egui::ScrollArea::vertical().show(ui, |ui| {
    ui.vertical(|ui| {
        // Header
        ui.horizontal(|ui| {
            ui.heading(format!("{count} Accounts Selected"));
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
        ui.separator();
        ui.add_space(8.0);

        // Bulk launch controls
        ui.heading("Bulk Launch");
        ui.add_space(4.0);
        ui.label("All selected accounts will join the same server sequentially.");
        ui.add_space(4.0);

        // Preset quick-select chips (same set as the single-launch view).
        if !presets.is_empty() {
            super::preset_chips(ui, "Presets:", presets, place_id_input, job_id_input);
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
            });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let place_valid = place_id_input.parse::<u64>().is_ok();
            let btn = ui.add_enabled(
                place_valid,
                egui::Button::new(format!("\u{1f680}  Launch {count} Accounts")),
            );
            if btn.clicked() {
                if let Ok(place_id) = place_id_input.parse::<u64>() {
                    let job_id = if job_id_input.trim().is_empty() {
                        None
                    } else {
                        Some(job_id_input.trim().to_string())
                    };
                    action = Some(GroupPanelAction::BulkLaunch { place_id, job_id });
                }
            }

            if roblox_running
                && ui.button("\u{2620}  Kill All Instances").clicked()
            {
                action = Some(GroupPanelAction::KillAll);
            }
        });
    });
    }); // ScrollArea

    action
}
