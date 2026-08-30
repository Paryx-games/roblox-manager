//! Settings panel — global config, encryption toggles, multi-instance control.

use eframe::egui;
use ram_core::models::{AppConfig, LogLevel};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

use super::discord_webhook::DiscordWebhookState;
use crate::theme::ThemeUi;

const INFO_WHITE_PNG: &[u8] = include_bytes!("../../../assets/info_white.png");
#[allow(dead_code)]
const INFO_BLACK_PNG: &[u8] = include_bytes!("../../../assets/info_black.png");

/// Actions the settings panel can emit.
#[allow(dead_code)]
pub enum SettingsAction {
    SaveConfig,
    ApplyLogLevel { level: LogLevel },
    SetStartupWithWindows(bool),
    RotateMacAddress,
    ChangePassword { new_password: String },
    ClearPassword,
    EnableMultiInstance,
    DisableMultiInstance,
    TileWindowsNow,
    OpenDataFolder,
    CleanOrphanedData,
    ClearCaches,
}

/// Persistent state for the settings panel password change UI.
#[derive(Default)]
pub struct SettingsState {
    pub new_password_input: String,
    pub confirm_password_input: String,
    pub log_level_pending: Option<LogLevel>,
    pub log_level_warning_open: bool,
}

#[derive(Deserialize)]
struct InfoCard {
    #[serde(rename = "type")]
    kind: InfoCardType,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum InfoCardType {
    Info,
    Warning,
    Caution,
}

fn info_cards() -> &'static HashMap<String, InfoCard> {
    static CARDS: OnceLock<HashMap<String, InfoCard>> = OnceLock::new();
    CARDS.get_or_init(|| {
        serde_json::from_str(include_str!("../../../infocards.json")).unwrap_or_default()
    })
}

fn setting_info(ui: &mut egui::Ui, reference_id: &str) {
    let Some(card) = info_cards().get(reference_id) else {
        return;
    };
    let (icon_bytes, icon_name, color) = match card.kind {
        InfoCardType::Info => (INFO_WHITE_PNG, "info_white", egui::Color32::from_gray(180)),
        InfoCardType::Warning => (
            INFO_WHITE_PNG,
            "info_white",
            egui::Color32::from_rgb(235, 190, 65),
        ),
        InfoCardType::Caution => (
            INFO_WHITE_PNG,
            "info_white",
            egui::Color32::from_rgb(220, 85, 85),
        ),
    };
    let response = egui::Frame::default()
        .inner_margin(egui::Margin::same(2.0))
        .show(ui, |ui| {
            ui.add(
                egui::Image::from_bytes(
                    format!("bytes://settings/{icon_name}.png"),
                    icon_bytes.to_vec(),
                )
                .fit_to_exact_size(egui::vec2(16.0, 16.0))
                .tint(color)
                .sense(egui::Sense::hover()),
            )
        })
        .inner;
    response.on_hover_ui(|ui| {
        let surface = color.gamma_multiply(0.22);
        egui::Frame::default()
            .fill(surface)
            .stroke(egui::Stroke::new(1.0_f32, color))
            .rounding(egui::Rounding::same(4.0))
            .outer_margin(egui::Margin::same(-4.0))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.colored_label(color, &card.text);
            });
    });
}

fn setting_row<R>(
    ui: &mut egui::Ui,
    reference_id: &str,
    add_setting: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        let result = add_setting(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            setting_info(ui, reference_id);
        });
        result
    })
    .inner
}

/// Draw the settings UI. Returns `Some(SettingsAction)` when an action is triggered.
pub fn show(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    has_password: bool,
    settings_state: &mut SettingsState,
    roblox_running: bool,
    webhook_state: &mut DiscordWebhookState,
) -> Option<SettingsAction> {
    let theme = ui.theme();
    let mut action: Option<SettingsAction> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {

    ui.heading("Settings");
    ui.separator();
    ui.add_space(8.0);

    let section_frame = egui::Frame::default()
        .inner_margin(egui::Margin::same(10.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(ui.visuals().extreme_bg_color);

    // ---- Account storage ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Account storage");
        ui.add_space(4.0);
        setting_row(ui, "credential_manager", |ui| {
            ui.checkbox(
                &mut config.use_credential_manager,
                "Use Windows Credential Manager (instead of encrypted file)",
            );
        });
    });
    ui.add_space(6.0);

    // ---- Launching ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Launching");
        ui.add_space(4.0);

        ui.strong("App startup");
        let mut wants_startup = config.startup_with_windows;
        let startup_toggled = setting_row(ui, "startup_with_windows", |ui| {
            ui.checkbox(&mut wants_startup, "Start RM with Windows").changed()
        });
        if startup_toggled {
            config.startup_with_windows = wants_startup;
            action = Some(SettingsAction::SetStartupWithWindows(wants_startup));
        }

        ui.add_space(4.0);
        setting_row(ui, "refresh_on_startup", |ui| {
            ui.checkbox(
                &mut config.refresh_on_startup,
                "Revalidate accounts on startup",
            )
        });

        ui.add_space(4.0);
        setting_row(ui, "auto_launch_on_startup", |ui| {
            let mut wants_auto = config.auto_launch_on_startup;
            if ui.checkbox(&mut wants_auto, "Auto-launch on startup").changed() {
                config.auto_launch_on_startup = wants_auto;
            }
            if wants_auto {
                ui.horizontal(|ui| {
                    ui.label("Account ID:");
                    let mut id_str = config.auto_launch_account_id
                        .map(|id| id.to_string())
                        .unwrap_or_default();
                    if ui.text_edit_singleline(&mut id_str).changed() {
                        config.auto_launch_account_id = id_str.trim().parse().ok();
                    }
                });
            }
        });

        ui.add_space(6.0);
        ui.strong("Launch safeguards");
        let mut wants_multi = config.multi_instance_enabled;
        let toggled = setting_row(ui, "multi_instance", |ui| {
            ui.checkbox(&mut wants_multi, "Enable multi-instance").changed()
        });
        if toggled {
            if wants_multi {
                action = Some(SettingsAction::EnableMultiInstance);
            } else {
                action = Some(SettingsAction::DisableMultiInstance);
            }
        }
        if config.multi_instance_enabled {
            ui.colored_label(
                theme.warning,
                "Warning: This interacts with Hyperion anti-cheat and may carry ban risk.",
            );
        }
        if !config.multi_instance_enabled && roblox_running {
            ui.colored_label(
                theme.text_muted,
                "Close all Roblox processes (including tray) before enabling.",
            );
        }

        ui.add_space(4.0);
        setting_row(ui, "kill_background_roblox", |ui| {
            ui.checkbox(
                &mut config.kill_background_roblox,
                "Kill Roblox tray/background processes automatically",
            );
        });
        setting_row(ui, "confirm_kill_all", |ui| {
            ui.checkbox(
                &mut config.confirm_kill_all,
                "Confirm before killing all Roblox instances",
            );
        });
        if config.multi_instance_enabled && !config.kill_background_roblox {
            ui.colored_label(
                theme.warning,
                "Warning: recommended when multi-instance is enabled. Tray processes stack up.",
            );
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.strong("Window layout");
        ui.add_space(4.0);
        setting_row(ui, "auto_arrange_windows", |ui| {
            ui.checkbox(
                &mut config.auto_arrange_windows,
                "Auto-arrange Roblox windows after launch",
            );
        });

        // Multi-monitor and custom layout configuration
        let monitors = ram_core::process::enumerate_monitors();

        ui.indent("tiling_options_indent", |ui| {
            ui.add_space(2.0);

            // 1. Target monitor selection
            setting_row(ui, "target_display", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Target Display:");
                    let current_label = match &config.tiling_target_monitor {
                    ram_core::models::MonitorTarget::Primary => "Primary Monitor".to_string(),
                    ram_core::models::MonitorTarget::All => "All Monitors (Distribute / Span)".to_string(),
                    ram_core::models::MonitorTarget::Index(i) => {
                        monitors
                            .get(*i)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| format!("Monitor {}", i + 1))
                    }
                    };

                    egui::ComboBox::from_id_salt("tiling_target_monitor_combo")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut config.tiling_target_monitor,
                            ram_core::models::MonitorTarget::Primary,
                            "Primary Monitor",
                        );
                        if monitors.len() > 1 {
                            ui.selectable_value(
                                &mut config.tiling_target_monitor,
                                ram_core::models::MonitorTarget::All,
                                "All Monitors (Distribute / Span)",
                            );
                        }
                        for (idx, mon) in monitors.iter().enumerate() {
                            ui.selectable_value(
                                &mut config.tiling_target_monitor,
                                ram_core::models::MonitorTarget::Index(idx),
                                &mon.name,
                            );
                        }
                        });
                    });
            });

            ui.add_space(2.0);

            // 2. Layout mode selection
            setting_row(ui, "grid_layout", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Grid Layout:");
                    let current_layout_label = match &config.tiling_layout_mode {
                    ram_core::models::TilingLayoutMode::Auto => "Auto Grid (Square-like)",
                    ram_core::models::TilingLayoutMode::FixedColumns(_) => "Fixed Columns",
                    ram_core::models::TilingLayoutMode::FixedRows(_) => "Fixed Rows",
                    ram_core::models::TilingLayoutMode::CustomGrid { .. } => {
                        "Custom Grid (Cols × Rows)"
                    }
                    ram_core::models::TilingLayoutMode::SideBySide => "Side-by-Side (1 Row)",
                    ram_core::models::TilingLayoutMode::Stacked => "Stacked (1 Column)",
                    };

                    egui::ComboBox::from_id_salt("tiling_layout_mode_combo")
                        .selected_text(current_layout_label)
                        .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                matches!(
                                    config.tiling_layout_mode,
                                    ram_core::models::TilingLayoutMode::Auto
                                ),
                                "Auto Grid (Square-like)",
                            )
                            .clicked()
                        {
                            config.tiling_layout_mode = ram_core::models::TilingLayoutMode::Auto;
                        }
                        if ui
                            .selectable_label(
                                matches!(
                                    config.tiling_layout_mode,
                                    ram_core::models::TilingLayoutMode::FixedColumns(_)
                                ),
                                "Fixed Columns",
                            )
                            .clicked()
                        {
                            config.tiling_layout_mode =
                                ram_core::models::TilingLayoutMode::FixedColumns(
                                    config.tiling_custom_cols,
                                );
                        }
                        if ui
                            .selectable_label(
                                matches!(
                                    config.tiling_layout_mode,
                                    ram_core::models::TilingLayoutMode::FixedRows(_)
                                ),
                                "Fixed Rows",
                            )
                            .clicked()
                        {
                            config.tiling_layout_mode =
                                ram_core::models::TilingLayoutMode::FixedRows(
                                    config.tiling_custom_rows,
                                );
                        }
                        if ui
                            .selectable_label(
                                matches!(
                                    config.tiling_layout_mode,
                                    ram_core::models::TilingLayoutMode::CustomGrid { .. }
                                ),
                                "Custom Grid (Cols × Rows)",
                            )
                            .clicked()
                        {
                            config.tiling_layout_mode =
                                ram_core::models::TilingLayoutMode::CustomGrid {
                                    cols: config.tiling_custom_cols,
                                    rows: config.tiling_custom_rows,
                                };
                        }
                        if ui
                            .selectable_label(
                                matches!(
                                    config.tiling_layout_mode,
                                    ram_core::models::TilingLayoutMode::SideBySide
                                ),
                                "Side-by-Side (1 Row)",
                            )
                            .clicked()
                        {
                            config.tiling_layout_mode =
                                ram_core::models::TilingLayoutMode::SideBySide;
                        }
                        if ui
                            .selectable_label(
                                matches!(
                                    config.tiling_layout_mode,
                                    ram_core::models::TilingLayoutMode::Stacked
                                ),
                                "Stacked (1 Column)",
                            )
                            .clicked()
                        {
                            config.tiling_layout_mode = ram_core::models::TilingLayoutMode::Stacked;
                        }
                        });
                    });
            });

            // Dynamic layout parameters
            match &mut config.tiling_layout_mode {
                ram_core::models::TilingLayoutMode::FixedColumns(cols) => {
                    setting_row(ui, "layout_dimensions", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Columns:");
                            let mut c = *cols as i32;
                            if ui
                                .add(egui::DragValue::new(&mut c).range(1..=12).speed(0.1))
                                .changed()
                            {
                                let val = c.clamp(1, 12) as u32;
                                *cols = val;
                                config.tiling_custom_cols = val;
                            }
                        });
                    });
                }
                ram_core::models::TilingLayoutMode::FixedRows(rows) => {
                    setting_row(ui, "layout_rows", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Rows:");
                            let mut r = *rows as i32;
                            if ui
                                .add(egui::DragValue::new(&mut r).range(1..=12).speed(0.1))
                                .changed()
                            {
                                let val = r.clamp(1, 12) as u32;
                                *rows = val;
                                config.tiling_custom_rows = val;
                            }
                        });
                    });
                }
                ram_core::models::TilingLayoutMode::CustomGrid { cols, rows } => {
                    setting_row(ui, "layout_columns", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Columns:");
                            let mut c = *cols as i32;
                            if ui
                                .add(egui::DragValue::new(&mut c).range(1..=12).speed(0.1))
                                .changed()
                            {
                                let val = c.clamp(1, 12) as u32;
                                *cols = val;
                                config.tiling_custom_cols = val;
                            }
                            ui.label("Rows:");
                            let mut r = *rows as i32;
                            if ui
                                .add(egui::DragValue::new(&mut r).range(1..=12).speed(0.1))
                                .changed()
                            {
                                let val = r.clamp(1, 12) as u32;
                                *rows = val;
                                config.tiling_custom_rows = val;
                            }
                        });
                    });
                }
                _ => {}
            }

            ui.add_space(2.0);
            setting_row(ui, "window_padding", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Window Padding:");
                    let mut pad = config.tiling_padding as i32;
                    if ui
                        .add(
                            egui::DragValue::new(&mut pad)
                                .range(0..=50)
                                .suffix(" px")
                                .speed(0.2),
                        )
                        .changed()
                    {
                        config.tiling_padding = pad.clamp(0, 50) as u32;
                    }
                });
            });

            ui.add_space(6.0);
            if ui
                .button("Tile Windows Now")
                .on_hover_text("Immediately arrange all open Roblox windows using these settings.")
                .clicked()
            {
                action = Some(SettingsAction::TileWindowsNow);
            }
        });

        ui.add_space(4.0);
        setting_row(ui, "rename_windows", |ui| {
            ui.checkbox(
                &mut config.rename_roblox_windows,
                "Name Roblox windows after their account",
            );
        });
        if config.rename_roblox_windows && !config.anonymize_names {
            ui.colored_label(
                theme.text_muted,
                "Window titles are readable by any program, and show up in screenshots and streams.",
            );
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.strong("Launch pacing");
        ui.add_space(4.0);
        setting_row(ui, "launch_delay", |ui| {
            ui.horizontal(|ui| {
                ui.label("Launch delay:");
                let mut secs = config.launch_delay_secs as i32;
                ui.add(
                    egui::DragValue::new(&mut secs)
                        .range(0..=300)
                        .speed(0.2)
                        .suffix(" s"),
                );
                config.launch_delay_secs = secs.max(0) as u32;
                ui.label(
                    egui::RichText::new("(Roblox rate-limits some IPs)")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });
    });
    ui.add_space(6.0);

    // ---- Privacy and identity ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Privacy and identity");
        ui.add_space(4.0);
        ui.strong("Privacy cleanup");
        setting_row(ui, "privacy_mode", |ui| {
            ui.checkbox(
                &mut config.privacy_mode,
                "Clean before launch",
            );
        });
        ui.indent("privacy_cleanup_options", |ui| {
            ui.add_enabled_ui(config.privacy_mode, |ui| {
                setting_row(ui, "privacy_cookies", |ui| {
                    ui.checkbox(
                        &mut config.privacy_clean_cookies,
                        "Clean cookies",
                    );
                });
                setting_row(ui, "privacy_local_storage", |ui| {
                    ui.checkbox(
                        &mut config.privacy_clean_local_storage,
                        "Clean cookies and LocalStorage",
                    );
                });
                setting_row(ui, "privacy_full_profile", |ui| {
                    ui.checkbox(
                        &mut config.privacy_clean_full_profile,
                        "Clean full Roblox cache/profile",
                    );
                });
                setting_row(ui, "privacy_on_exit", |ui| {
                    ui.checkbox(
                        &mut config.privacy_clean_on_exit,
                        "Clean selected privacy data on exit",
                    );
                });
                setting_row(ui, "privacy_clear_clipboard", |ui| {
                    ui.checkbox(
                        &mut config.privacy_clear_clipboard,
                        "Clear clipboard after launch",
                    );
                });
            });
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.strong("Network identity");
        ui.add_space(4.0);
        setting_row(ui, "mac_rotation", |ui| {
            ui.checkbox(
                &mut config.mac_rotation_enabled,
                "Enable MAC address rotation",
            );
        });
        if config.mac_rotation_enabled {
            ui.indent("mac_rotation_options", |ui| {
                setting_row(ui, "mac_preserve_oui", |ui| {
                    ui.checkbox(
                        &mut config.mac_preserve_oui,
                        "Keep this PC's adapter OUI",
                    );
                });
                if !config.mac_preserve_oui {
                    setting_row(ui, "mac_alternate_oui", |ui| {
                        egui::ComboBox::from_id_salt("mac_alternate_oui")
                            .selected_text(match config.mac_alternate_oui.as_str() {
                                "00:1B:21" => "Intel (00:1B:21)",
                                "00:E0:4C" => "Realtek (00:E0:4C)",
                                "3C:52:82" => "Microsoft (3C:52:82)",
                                _ => "Custom / saved OUI",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut config.mac_alternate_oui,
                                    "00:1B:21".to_string(),
                                    "Intel (00:1B:21)",
                                );
                                ui.selectable_value(
                                    &mut config.mac_alternate_oui,
                                    "00:E0:4C".to_string(),
                                    "Realtek (00:E0:4C)",
                                );
                                ui.selectable_value(
                                    &mut config.mac_alternate_oui,
                                    "3C:52:82".to_string(),
                                    "Microsoft (3C:52:82)",
                                );
                            });
                    });
                }
                if ui.button("Rotate MAC address now").clicked() {
                    action = Some(SettingsAction::RotateMacAddress);
                }
                ui.colored_label(
                    theme.text_muted,
                    "The adapter briefly disconnects and Windows may request administrator permission.",
                );
            });
        }
        ui.add_space(4.0);
        ui.strong("Displayed identity");
        setting_row(ui, "anonymize_names", |ui| {
            ui.checkbox(&mut config.anonymize_names, "Anonymize account names");
        });
    });
    ui.add_space(6.0);

    // ---- App and data ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("App and data");
        ui.add_space(4.0);
        ui.strong("Development");
        setting_row(ui, "utility_enabled", |ui| {
            ui.checkbox(&mut config.utility_enabled, "Show the Utility tab");
        });
        setting_row(ui, "developer_options", |ui| {
            ui.checkbox(&mut config.developer_options, "Show the Assets tab in Utility");
        });
        ui.add_space(4.0);
        ui.strong("Logging");
        let mut selected_log_level = settings_state
            .log_level_pending
            .unwrap_or(config.log_level);
        ui.horizontal(|ui| {
            ui.label("Log level");
            egui::ComboBox::from_id_salt("rm_log_level_combo")
                .selected_text(selected_log_level.label())
                .show_ui(ui, |ui| {
                    for level in [
                        LogLevel::Error,
                        LogLevel::Warn,
                        LogLevel::Info,
                        LogLevel::Debug,
                        LogLevel::Trace,
                    ] {
                        let enabled = level.allowed_in_profile();
                        let response = ui.add_enabled(
                            enabled,
                            egui::SelectableLabel::new(selected_log_level == level, level.label()),
                        );
                        if response.clicked() && enabled {
                            selected_log_level = level;
                            settings_state.log_level_pending = Some(level);
                            settings_state.log_level_warning_open = true;
                        }
                    }
                });
        });
        if let Some(pending) = settings_state.log_level_pending.filter(|level| *level != config.log_level) {
            let mut warning_open = settings_state.log_level_warning_open || settings_state.log_level_pending.is_some();
            let mut should_apply = false;
            let mut should_cancel = false;

            let current = config.log_level.label();
            let target = pending.label();

            egui::Window::new("Change log level?")
                .open(&mut warning_open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Switch log level from {current} to {target}?"));
                    ui.label("Lower log levels show less detail, which can make it harder to diagnose issues later - higher levels show more but can get noisy.");
                    ui.label(
                        egui::RichText::new(if cfg!(debug_assertions) {
                            "Debug is recommended for development builds."
                        } else {
                            "Info is recommended for release builds."
                        })
                        .strong(),
                    );
                    ui.label("The app will restart once you confirm.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Proceed").clicked() {
                            should_apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_cancel = true;
                        }
                    });
                });

            settings_state.log_level_warning_open = warning_open;
            if should_apply {
                action = Some(SettingsAction::ApplyLogLevel { level: pending });
                settings_state.log_level_pending = None;
                settings_state.log_level_warning_open = false;
            } else if should_cancel || !warning_open {
                settings_state.log_level_pending = None;
                settings_state.log_level_warning_open = false;
            }
        }
        if ui
            .button("Clean orphaned data")
            .on_hover_text("Remove browse-as profiles for accounts no longer in RM")
            .clicked()
        {
            action = Some(SettingsAction::CleanOrphanedData);
        }
        if config.developer_options {
            ui.colored_label(
                theme.warning,
                "Warning: Uploads are permanent and public. Every asset is moderated under the account that uploaded it.",
            );
            if ui
                .button("Clear application caches")
                .on_hover_text(
                    "Remove avatars, thumbnails, metadata, and inventory caches. Accounts and assets are kept.",
                )
                .clicked()
            {
                action = Some(SettingsAction::ClearCaches);
            }
        }
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);
        ui.strong("Data location");
        ui.add_space(4.0);
        setting_row(ui, "data_folder", |ui| {
            if ui.button("Open RM data folder").clicked() {
                action = Some(SettingsAction::OpenDataFolder);
            }
        });
    });
    ui.add_space(6.0);

    // ---- Roblox installation ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.strong("Roblox installation");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                setting_info(ui, "roblox_player_path");
            });
        });
        ui.add_space(4.0);
        ui.label("Leave empty for auto-detect:");
        let mut path_str = config
            .roblox_player_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if ui.text_edit_singleline(&mut path_str).changed() {
            config.roblox_player_path = if path_str.trim().is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(path_str))
            };
        }
    });
    ui.add_space(6.0);

    // ---- Advanced ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Advanced");
        ui.add_space(4.0);

        ui.strong("Launch arguments");
        ui.add_space(2.0);
        setting_row(ui, "custom_game_args", |ui| {
            ui.label("Custom Roblox args:");
            ui.text_edit_singleline(&mut config.custom_game_args)
        });
        ui.colored_label(
            theme.text_muted,
            "Examples: -a 3 -t username=... (passed directly to RobloxPlayerBeta.exe)",
        );

        ui.add_space(6.0);
        ui.strong("Fast flags");
        ui.add_space(2.0);
        setting_row(ui, "roblox_fast_flags", |ui| {
            if config.roblox_fast_flags.is_empty() {
                ui.label("(empty)");
            } else {
                ui.vertical(|ui| {
                    for (key, value) in config.roblox_fast_flags.iter() {
                        ui.label(format!("{} = {}", key, value));
                    }
                });
            }
            false
        });
        ui.colored_label(
            theme.text_muted,
            "Experimental Roblox ClientSettings toggles; written before launch",
        );
    });

    ui.add_space(12.0);

    // ---- Integrations ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Integrations");
        ui.add_space(4.0);

        ui.strong("Discord Notifications");
        ui.add_space(2.0);
        setting_row(ui, "discord_webhook", |ui| {
            if config.discord_webhook_url.is_empty() {
                if ui.button("Add Discord Webhook").clicked() {
                    webhook_state.modal_open = true;
                }
            } else {
                ui.label("✓ Webhook configured");
                if ui.button("Change").clicked() {
                    webhook_state.input_url = config.discord_webhook_url.clone();
                    webhook_state.modal_open = true;
                }
                if ui.button("Remove").clicked() {
                    config.discord_webhook_url.clear();
                }
            }
        });
        ui.add_space(2.0);
        ui.colored_label(
            theme.text_muted,
            "Get notifications for launches and account moderation events.",
        );
    });

    ui.add_space(12.0);

    if ui.button("Save Settings").clicked() {
        action = Some(SettingsAction::SaveConfig);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // ---- Account encryption ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Account encryption");
        ui.add_space(4.0);

        if has_password {
            ui.label("Accounts are encrypted with your master password.");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "RM asks for it every time it starts. If you forget it, the accounts \
                     cannot be recovered.",
                )
                .small()
                .weak(),
            );
        } else {
            ui.label("Accounts are encrypted and unlock automatically on this PC.");
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(
                    "The key is held in Windows Credential Manager, so the file is useless \
                     on its own. Anything running as you can still read it.",
                )
                .small()
                .weak(),
            );
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(if has_password {
                "Change your master password:"
            } else {
                "Require a master password at startup:"
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                setting_info(ui, "encryption_password");
            });
        });
        ui.add_space(4.0);

        ui.add(
            egui::TextEdit::singleline(&mut settings_state.new_password_input)
                .password(true)
                .hint_text("New password"),
        );
        ui.add(
            egui::TextEdit::singleline(&mut settings_state.confirm_password_input)
                .password(true)
                .hint_text("Confirm password"),
        );
        ui.add_space(4.0);

        let passwords_match = !settings_state.new_password_input.is_empty()
            && settings_state.new_password_input == settings_state.confirm_password_input;

        if !settings_state.new_password_input.is_empty()
            && !settings_state.confirm_password_input.is_empty()
            && !passwords_match
        {
            ui.colored_label(
                theme.danger,
                "Passwords do not match.",
            );
        }

        ui.horizontal(|ui| {
            let label = if has_password {
                "Change password"
            } else {
                "Set password"
            };
            if ui
                .add_enabled(passwords_match, egui::Button::new(label))
                .clicked()
            {
                let new_pw = settings_state.new_password_input.clone();
                settings_state.new_password_input.clear();
                settings_state.confirm_password_input.clear();
                action = Some(SettingsAction::ChangePassword {
                    new_password: new_pw,
                });
            }

            // Only offered to someone who has a password to remove. The store
            // stays encrypted either way, so this is a convenience toggle
            // rather than a way to turn encryption off.
            if has_password && ui.button("Stop asking for a password").clicked() {
                settings_state.new_password_input.clear();
                settings_state.confirm_password_input.clear();
                action = Some(SettingsAction::ClearPassword);
            }
        });
    });

    }); // ScrollArea

    action
}
