//! Groups workspace for the currently selected accounts.

use eframe::egui;
use egui::Widget;
use ram_core::group_api::{GroupAnnouncement, GroupInfo, GroupMembership, GroupShout};
use ram_core::models::Account;

use crate::theme::ThemeUi;

pub enum GroupsPanelAction {
    Load { group_id: u64 },
    Join,
    Leave,
}

#[derive(Default)]
pub struct GroupsPanelState {
    pub group_id_input: String,
    pub loaded_group_id: Option<u64>,
    pub group: Option<GroupInfo>,
    pub icon_bytes: Option<Vec<u8>>,
    pub shout: Option<GroupShout>,
    pub announcements: Vec<GroupAnnouncement>,
    pub memberships: Vec<GroupMembership>,
    pub loading: bool,
    pub action_in_flight: bool,
    pub pending_actions: usize,
    pub error: Option<String>,
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut GroupsPanelState,
    selected_accounts: &[&Account],
) -> Option<GroupsPanelAction> {
    let mut action = None;
    let theme = ui.theme();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Groups");
        ui.label(
            egui::RichText::new("Inspect a Roblox group and manage membership for the selected accounts.")
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(8.0);

        let section_frame = egui::Frame::default()
            .inner_margin(egui::Margin::same(10.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(ui.visuals().extreme_bg_color);

        section_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label("Group ID");
                let input = ui.add(
                    egui::TextEdit::singleline(&mut state.group_id_input)
                        .hint_text("Enter a Roblox group ID")
                        .desired_width(220.0),
                );
                let load = ui.add_enabled(
                    !state.loading,
                    egui::Button::new(if state.loading { "Loading..." } else { "Load group" }),
                );
                if (load.clicked() || input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    && !state.loading
                {
                    if let Ok(group_id) = state.group_id_input.trim().parse::<u64>() {
                        state.error = None;
                        action = Some(GroupsPanelAction::Load { group_id });
                    } else {
                        state.error = Some("Enter a numeric Roblox group ID.".to_string());
                    }
                }
            });
            if let Some(error) = &state.error {
                ui.add_space(4.0);
                ui.colored_label(theme.danger_text, error);
            }
        });

        let Some(group) = state.group.as_ref() else {
            ui.add_space(10.0);
            section_frame.show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.add_space(24.0);
                    ui.label(
                        egui::RichText::new("Enter a group ID to get started")
                            .size(18.0)
                            .strong(),
                    );
                    ui.add_space(4.0);
                    ui.label("Group details, announcements, and selected-account membership will appear here.");
                    ui.add_space(24.0);
                });
            });
            return;
        };

        ui.add_space(10.0);
        section_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                if let Some(bytes) = &state.icon_bytes {
                    egui::Image::from_bytes(
                        format!("bytes://group_icon/{}", group.id),
                        bytes.clone(),
                    )
                    .fit_to_exact_size(egui::vec2(72.0, 72.0))
                    .rounding(egui::Rounding::same(8.0))
                    .ui(ui);
                    ui.add_space(10.0);
                }
                ui.vertical(|ui| {
                    ui.heading(&group.name);
                    ui.label(format!("Group ID: {}", group.id));
                    ui.label(format!("{} members", group.member_count));
                    if let Some(owner) = &group.owner {
                        ui.label(format!("Owner: {}", owner.display_name_or_username()));
                    }
                    if group.has_verified_badge {
                        ui.colored_label(theme.info, "Verified group");
                    }
                });
            });
            if !group.description.trim().is_empty() {
                ui.add_space(8.0);
                ui.label(&group.description);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let join_enabled = !state.action_in_flight
                    && !selected_accounts.is_empty()
                    && state.memberships.iter().any(|membership| !membership.joined);
                let leave_enabled = !state.action_in_flight
                    && !selected_accounts.is_empty()
                    && state.memberships.iter().any(|membership| membership.joined);
                if ui.add_enabled(join_enabled, egui::Button::new("Join selected")).clicked() {
                    action = Some(GroupsPanelAction::Join);
                }
                if ui.add_enabled(leave_enabled, egui::Button::new("Leave selected")).clicked() {
                    action = Some(GroupsPanelAction::Leave);
                }
                if selected_accounts.is_empty() {
                    ui.label("Select one or more accounts on the Accounts tab to manage membership.");
                }
            });
        });

        ui.add_space(10.0);
        section_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Selected accounts");
            ui.add_space(4.0);
            for account in selected_accounts {
                let membership = state.memberships.iter().find(|item| item.user_id == account.user_id);
                ui.horizontal(|ui| {
                    ui.label(account.label());
                    if let Some(membership) = membership {
                        if membership.joined {
                            ui.colored_label(theme.success_text, "Member");
                            if let Some(role) = &membership.role_name {
                                ui.label(format!("{role} (rank {})", membership.role_rank));
                            }
                        } else {
                            ui.colored_label(theme.text_muted, "Not a member");
                        }
                    } else if state.loading {
                        ui.colored_label(theme.text_muted, "Checking...");
                    } else {
                        ui.colored_label(theme.text_muted, "No membership data");
                    }
                });
            }
            if selected_accounts.is_empty() {
                ui.label("No accounts selected.");
            }
        });

        ui.add_space(10.0);
        section_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Announcement");
            ui.add_space(4.0);
            if let Some(shout) = &state.shout {
                ui.label(&shout.body);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format_poster_date(shout.poster.as_ref(), shout.created))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            } else {
                ui.label(
                    egui::RichText::new("No current group announcement.")
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });

        ui.add_space(10.0);
        section_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Recent wall posts");
            ui.add_space(4.0);
            if state.announcements.is_empty() {
                ui.label(
                    egui::RichText::new("No recent posts were returned for this group.")
                        .color(ui.visuals().weak_text_color()),
                );
            } else {
                for post in &state.announcements {
                    ui.separator();
                    ui.label(&post.body);
                    ui.label(
                        egui::RichText::new(format_poster_date(post.poster.as_ref(), post.created))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
            }
        });
    });

    action
}

fn format_poster_date(
    poster: Option<&ram_core::group_api::GroupPoster>,
    created: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    let author = poster
        .map(|value| value.display_name_or_username())
        .unwrap_or_else(|| "Unknown author".to_string());
    match created {
        Some(date) => format!("{author} · {}", date.format("%Y-%m-%d %H:%M UTC")),
        None => author,
    }
}

trait PosterLabel {
    fn display_name_or_username(&self) -> String;
}

impl PosterLabel for ram_core::group_api::GroupPoster {
    fn display_name_or_username(&self) -> String {
        if self.display_name.is_empty() {
            self.username.clone()
        } else {
            self.display_name.clone()
        }
    }
}

trait OwnerLabel {
    fn display_name_or_username(&self) -> String;
}

impl OwnerLabel for ram_core::group_api::GroupOwner {
    fn display_name_or_username(&self) -> String {
        if self.display_name.is_empty() {
            self.username.clone()
        } else {
            self.display_name.clone()
        }
    }
}
