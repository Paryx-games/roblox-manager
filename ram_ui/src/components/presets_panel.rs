//! Launch-presets management tab — list, create, edit, delete saved presets.
//!
//! Presets are persisted as individual JSON files by [`ram_core::presets`].
//! The "Open folder" button on this tab reveals that directory in Explorer
//! so users can hand-edit, copy, or share preset files outside the app.

use std::path::PathBuf;

use eframe::egui;
use ram_core::models::LaunchPreset;

use crate::theme::ThemeUi;

/// Actions the presets panel can request from the app.
pub enum PresetsAction {
    /// Persist `preset`. If `path` is `Some` it's an in-place edit of that
    /// file; if `None`, allocate a new file under the presets directory.
    Save {
        path: Option<PathBuf>,
        preset: LaunchPreset,
    },
    /// Remove the preset file at `path`.
    Delete(PathBuf),
    /// Reveal the presets directory in the OS file manager.
    RevealFolder,
}

/// Persistent form / editor state for the presets panel.
#[derive(Default)]
pub struct PresetsState {
    pub name_input: String,
    pub place_id_input: String,
    pub job_id_input: String,
    /// `Some(path)` when editing an existing preset, `None` when creating.
    pub editing: Option<PathBuf>,
    pub error: Option<String>,
}

impl PresetsState {
    fn clear_form(&mut self) {
        self.name_input.clear();
        self.place_id_input.clear();
        self.job_id_input.clear();
        self.editing = None;
        self.error = None;
    }

    fn load_into_form(&mut self, path: PathBuf, preset: &LaunchPreset) {
        self.name_input = preset.name.clone();
        self.place_id_input = preset.place_id.to_string();
        self.job_id_input = preset.job_id.clone().unwrap_or_default();
        self.editing = Some(path);
        self.error = None;
    }
}

/// Draw the presets management panel. Returns an optional action.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut PresetsState,
    presets: &[(PathBuf, LaunchPreset)],
) -> Option<PresetsAction> {
    let mut action: Option<PresetsAction> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        let section_frame = egui::Frame::default()
            .inner_margin(egui::Margin::same(10.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(ui.visuals().extreme_bg_color);

        // ---- Editor ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
            let is_edit = state.editing.is_some();
            ui.horizontal(|ui| {
                ui.heading(if is_edit { "Edit Preset" } else { "New Preset" });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("\u{1f4c2}  Open folder")
                        .on_hover_text("Reveal the presets folder in Explorer")
                        .clicked()
                    {
                        action = Some(PresetsAction::RevealFolder);
                    }
                });
            });
            ui.add_space(4.0);

            egui::Grid::new("preset_form")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.name_input)
                            .hint_text("e.g. Adopt Me"),
                    );
                    ui.end_row();

                    ui.label("Place ID:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.place_id_input)
                            .hint_text("e.g. 920587237"),
                    );
                    ui.end_row();

                    ui.label("Job ID (optional):");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.job_id_input)
                            .hint_text("Specific server GUID"),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);
            if let Some(err) = &state.error {
                ui.colored_label(ui.theme().danger_text, err);
                ui.add_space(2.0);
            }

            ui.horizontal(|ui| {
                let name_ok = !state.name_input.trim().is_empty();
                let pid_ok = state.place_id_input.trim().parse::<u64>().is_ok();
                let can_save = name_ok && pid_ok;
                let save_label = if is_edit { "Save changes" } else { "+ Add Preset" };
                if ui
                    .add_enabled(can_save, egui::Button::new(save_label))
                    .clicked()
                {
                    match state.place_id_input.trim().parse::<u64>() {
                        Ok(place_id) => {
                            let job_id = {
                                let t = state.job_id_input.trim();
                                if t.is_empty() {
                                    None
                                } else {
                                    Some(t.to_string())
                                }
                            };
                            let preset = LaunchPreset {
                                name: state.name_input.trim().to_string(),
                                place_id,
                                job_id,
                            };
                            action = Some(PresetsAction::Save {
                                path: state.editing.clone(),
                                preset,
                            });
                            state.clear_form();
                        }
                        Err(_) => {
                            state.error = Some("Place ID must be a number.".into());
                        }
                    }
                }
                if is_edit && ui.button("Cancel").clicked() {
                    state.clear_form();
                }
            });
        });

        ui.add_space(8.0);

        // ---- Saved presets list ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Saved Presets");
            ui.add_space(4.0);

            if presets.is_empty() {
                ui.colored_label(
                    ui.theme().text_muted,
                    "No presets yet. Create one above to launch favorite games faster.",
                );
                return;
            }

            for (path, preset) in presets {
                ui.push_id(path, |ui| {
                    egui::Frame::default()
                        .inner_margin(egui::Margin::same(6.0))
                        .rounding(egui::Rounding::same(4.0))
                        .fill(ui.visuals().faint_bg_color)
                        .stroke(egui::Stroke::new(
                            0.5,
                            ui.visuals().widgets.noninteractive.bg_stroke.color,
                        ))
                        .show(ui, |ui: &mut egui::Ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.strong(&preset.name);
                                    let detail = match &preset.job_id {
                                        Some(j) if !j.is_empty() => format!(
                                            "Place {}, Job {}",
                                            preset.place_id, j
                                        ),
                                        _ => format!("Place {}", preset.place_id),
                                    };
                                    ui.colored_label(ui.theme().text_muted, detail);
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("\u{1f5d1}")
                                            .on_hover_text("Delete")
                                            .clicked()
                                        {
                                            action = Some(PresetsAction::Delete(path.clone()));
                                        }
                                        if ui
                                            .small_button("\u{270f}")
                                            .on_hover_text("Edit")
                                            .clicked()
                                        {
                                            state.load_into_form(path.clone(), preset);
                                        }
                                    },
                                );
                            });
                        });
                });
                ui.add_space(4.0);
            }
        });
    });

    action
}
