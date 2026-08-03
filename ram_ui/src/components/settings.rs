//! Settings panel — global config, encryption toggles, multi-instance control.

use eframe::egui;
use ram_core::models::AppConfig;

/// Actions the settings panel can emit.
#[allow(dead_code)]
pub enum SettingsAction {
    SaveConfig,
    ChangePassword { new_password: String },
    ClearPassword,
    EnableMultiInstance,
    DisableMultiInstance,
}

/// Persistent state for the settings panel password change UI.
#[derive(Default)]
pub struct SettingsState {
    pub new_password_input: String,
    pub confirm_password_input: String,
}

/// Draw the settings UI. Returns `Some(SettingsAction)` when an action is triggered.
pub fn show(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    has_password: bool,
    settings_state: &mut SettingsState,
    roblox_running: bool,
) -> Option<SettingsAction> {
    let mut action: Option<SettingsAction> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {

    ui.heading("Settings");
    ui.separator();
    ui.add_space(8.0);

    let section_frame = egui::Frame::default()
        .inner_margin(egui::Margin::same(10.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(ui.visuals().extreme_bg_color);

    // ---- Storage backend ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Storage");
        ui.add_space(4.0);
        ui.checkbox(
            &mut config.use_credential_manager,
            "Use Windows Credential Manager (instead of encrypted file)",
        );
    });
    ui.add_space(6.0);

    // ---- Launch Behavior ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Launch Behavior");
        ui.add_space(4.0);

        let mut wants_multi = config.multi_instance_enabled;
        let toggled = ui.checkbox(
            &mut wants_multi,
            "Enable multi-instance",
        ).changed();
        if toggled {
            if wants_multi {
                action = Some(SettingsAction::EnableMultiInstance);
            } else {
                action = Some(SettingsAction::DisableMultiInstance);
            }
        }
        if config.multi_instance_enabled {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 40),
                "\u{26a0} This interacts with Hyperion anti-cheat and may carry ban risk.",
            );
        }
        if !config.multi_instance_enabled && roblox_running {
            ui.colored_label(
                egui::Color32::from_rgb(180, 180, 180),
                "Close all Roblox processes (including tray) before enabling.",
            );
        }

        ui.add_space(4.0);
        ui.checkbox(
            &mut config.kill_background_roblox,
            "Kill Roblox tray/background processes automatically",
        ).on_hover_text("Kills idle \"always running\" Roblox processes (--launch-to-tray).");
        if config.multi_instance_enabled && !config.kill_background_roblox {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 40),
                "⚠ Recommended when multi-instance is enabled. Tray processes stack up.",
            );
        }

        ui.add_space(4.0);
        ui.checkbox(
            &mut config.auto_arrange_windows,
            "Auto-arrange Roblox windows after launch",
        ).on_hover_text("Tiles Roblox windows in a grid (2 = side-by-side, 4 = 2×2, etc.).");

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Launch delay:");
            let mut secs = config.launch_delay_secs as i32;
            ui.add(
                egui::DragValue::new(&mut secs)
                    .range(0..=300)
                    .speed(0.2)
                    .suffix(" s"),
            )
            .on_hover_text(
                "Minimum gap between account launches. Applies to single and bulk launches. 0 disables throttling.",
            );
            config.launch_delay_secs = secs.max(0) as u32;
            ui.label(
                egui::RichText::new("(Roblox rate-limits some IPs)")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        });
    });
    ui.add_space(6.0);

    // ---- Privacy ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Privacy");
        ui.add_space(4.0);
        ui.checkbox(
            &mut config.privacy_mode,
            "Clear RobloxCookies.dat before each launch",
        ).on_hover_text("Prevents Roblox from associating your accounts via stored cookies.");
        ui.checkbox(
            &mut config.anonymize_names,
            "Anonymize account names",
        ).on_hover_text("Replaces usernames and display names with generic \"Account 1\", \"Account 2\", etc.");
    });
    ui.add_space(6.0);

    // ---- Developer options ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Developer Options");
        ui.add_space(4.0);
        ui.checkbox(
            &mut config.developer_options,
            "Show the Asset Manager tab",
        ).on_hover_text(
            "Upload assets to Roblox from any saved account, track moderation, and grant experiences permission to use them.",
        );
        if config.developer_options {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 40),
                "\u{26a0} Uploads are permanent and public. Every asset is moderated under the account that uploaded it.",
            );
        }
    });
    ui.add_space(6.0);

    // ---- Roblox path override ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Roblox Player Path");
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

    ui.add_space(12.0);

    if ui.button("💾  Save Settings").clicked() {
        action = Some(SettingsAction::SaveConfig);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // ---- Master password management ----
    section_frame.show(ui, |ui: &mut egui::Ui| {
        ui.set_min_width(ui.available_width());
        ui.strong("Master Password");
        ui.add_space(4.0);
        if has_password {
            ui.label("A master password is currently set.");
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 40),
                "⚠ No master password set. Add an account to set one.",
            );
        }
        ui.add_space(4.0);

        ui.label("New password:");
        ui.add(
            egui::TextEdit::singleline(&mut settings_state.new_password_input)
                .password(true)
                .hint_text("Enter new password"),
        );
        ui.label("Confirm password:");
        ui.add(
            egui::TextEdit::singleline(&mut settings_state.confirm_password_input)
                .password(true)
                .hint_text("Confirm new password"),
        );
        ui.add_space(4.0);

        let passwords_match = !settings_state.new_password_input.is_empty()
            && settings_state.new_password_input == settings_state.confirm_password_input;

        if !settings_state.new_password_input.is_empty()
            && !settings_state.confirm_password_input.is_empty()
            && !passwords_match
        {
            ui.colored_label(
                egui::Color32::from_rgb(200, 60, 60),
                "Passwords do not match.",
            );
        }

        if ui
            .add_enabled(passwords_match, egui::Button::new("🔑  Change Password"))
            .clicked()
        {
            let new_pw = settings_state.new_password_input.clone();
            settings_state.new_password_input.clear();
            settings_state.confirm_password_input.clear();
            action = Some(SettingsAction::ChangePassword {
                new_password: new_pw,
            });
        }
    });

    }); // ScrollArea

    action
}
