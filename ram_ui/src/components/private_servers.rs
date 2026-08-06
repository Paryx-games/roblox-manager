//! Private server management tab — add, remove, and launch from saved private servers.

use std::collections::HashMap;

use eframe::egui;
use ram_core::models::PrivateServer;

use crate::theme::ThemeUi;

/// Actions the private servers panel can request.
pub enum PrivateServerAction {
    /// Add a new private server entry and resolve its place name.
    Add(PrivateServer),
    /// Remove a private server by index.
    Remove(usize),
    /// Launch selected account(s) into a private server.
    Launch {
        place_id: u64,
        link_code: String,
        access_code: String,
    },
    /// Resolve the place name for a server entry (by index).
    Resolve(usize),
    /// Resolve a share link code into (place_id, link_code) via the Roblox API.
    ResolveShareLink {
        share_code: String,
        server_name: String,
    },
}

/// Persistent input state for the private servers panel.
#[derive(Default)]
pub struct PrivateServerState {
    pub name_input: String,
    pub url_input: String,
    pub place_id_input: String,
    pub add_error: Option<String>,
}

/// Result of parsing a private server URL.
enum ParsedUrl {
    /// Old format: URL contained both place ID and link code.
    Full { place_id: u64, link_code: String },
    /// Share format: URL contained only a link code; place ID needed separately.
    ShareCode(String),
}

/// Parse a private server URL.
/// Supports formats:
///   - `https://www.roblox.com/games/12345?privateServerLinkCode=ABCDE`
///   - `https://www.roblox.com/share?code=ABCDE&type=Server`
fn parse_private_server_url(input: &str) -> Option<ParsedUrl> {
    let input = input.trim();

    // Try: roblox.com/games/PLACE_ID?...privateServerLinkCode=CODE
    if let Some(idx) = input.find("/games/") {
        let after_games = &input[idx + 7..];
        let place_str: String = after_games.chars().take_while(|c| c.is_ascii_digit()).collect();
        let place_id: u64 = place_str.parse().ok()?;
        if let Some(code) = extract_param(input, "privateServerLinkCode") {
            return Some(ParsedUrl::Full { place_id, link_code: code });
        }
    }

    // Try: roblox.com/share?code=CODE&type=Server
    if let Some(code) = extract_param(input, "code") {
        if input.contains("/share") || input.contains("type=Server") {
            return Some(ParsedUrl::ShareCode(code));
        }
    }

    None
}

fn extract_param(url: &str, param: &str) -> Option<String> {
    // Match param preceded by ? or & to avoid matching inside another param name
    // e.g. "code" should not match inside "privateServerLinkCode"
    let url_lower = url.to_lowercase();
    let param_lower = param.to_lowercase();
    for prefix in ['?', '&'] {
        let search = format!("{prefix}{param_lower}=");
        if let Some(idx) = url_lower.find(&search) {
            let start = idx + search.len();
            let rest = &url[start..];
            let value: String = rest.chars().take_while(|c| *c != '&' && *c != '#').collect();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Draw the private servers management panel. Returns an optional action.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut PrivateServerState,
    servers: &[PrivateServer],
    has_selection: bool,
    game_icon_bytes: &HashMap<u64, Vec<u8>>,
) -> Option<PrivateServerAction> {
    let mut action: Option<PrivateServerAction> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        let section_frame = egui::Frame::default()
            .inner_margin(egui::Margin::same(10.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(ui.visuals().extreme_bg_color);

        // ---- Add new private server ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Add Private Server");
            ui.add_space(4.0);

            egui::Grid::new("ps_add_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut state.name_input);
                    ui.end_row();

                    ui.label("URL:");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.url_input)
                            .hint_text("Paste private server link"),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);

            if let Some(ref err) = state.add_error {
                ui.colored_label(ui.theme().danger_text, err);
                ui.add_space(2.0);
            }

            let name_ok = !state.name_input.trim().is_empty();
            let url_ok = !state.url_input.trim().is_empty();
            if ui
                .add_enabled(name_ok && url_ok, egui::Button::new("+ Add Server"))
                .clicked()
            {
                match parse_private_server_url(&state.url_input) {
                    Some(ParsedUrl::Full { place_id, link_code }) => {
                        let server = PrivateServer {
                            name: state.name_input.trim().to_string(),
                            place_id,
                            universe_id: None,
                            link_code,
                            access_code: String::new(),
                            place_name: String::new(),
                        };
                        action = Some(PrivateServerAction::Add(server));
                        state.name_input.clear();
                        state.url_input.clear();
                        state.place_id_input.clear();
                        state.add_error = None;
                    }
                    Some(ParsedUrl::ShareCode(share_code)) => {
                        action = Some(PrivateServerAction::ResolveShareLink {
                            share_code,
                            server_name: state.name_input.trim().to_string(),
                        });
                        state.name_input.clear();
                        state.url_input.clear();
                        state.place_id_input.clear();
                        state.add_error = None;
                    }
                    None => {
                        state.add_error = Some(format!(
                            "Could not parse URL. Supported formats:\n\
                             • https://www.roblox.com/games/PLACE_ID?privateServerLinkCode=CODE\n\
                             • https://www.roblox.com/share?code=CODE&type=Server\n\n\
                             Tried: {}",
                            state.url_input.trim()
                        ));
                    }
                }
            }
        });

        ui.add_space(8.0);

        // ---- Server list (grouped by game) ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
            ui.heading("Saved Private Servers");
            ui.add_space(4.0);

            if servers.is_empty() {
                ui.colored_label(ui.theme().text_muted, "No private servers saved yet.");
            } else {
                let mut remove_idx: Option<usize> = None;
                let mut resolve_idx: Option<usize> = None;

                // Build groups: ordered unique place_ids, preserving first-seen order.
                let mut seen_place_ids = Vec::new();
                for server in servers.iter() {
                    if !seen_place_ids.contains(&server.place_id) {
                        seen_place_ids.push(server.place_id);
                    }
                }

                for &pid in &seen_place_ids {
                    // Collect servers for this game (with original indices).
                    let group: Vec<(usize, &PrivateServer)> = servers
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.place_id == pid)
                        .collect();

                    // Determine the game name from the first server that has one.
                    let game_name = group
                        .iter()
                        .find_map(|(_, s)| {
                            if s.place_name.is_empty() {
                                None
                            } else {
                                Some(s.place_name.as_str())
                            }
                        })
                        .unwrap_or("");

                    ui.push_id(pid, |ui| {
                        // Game group header
                        egui::Frame::default()
                            .inner_margin(egui::Margin::same(8.0))
                            .rounding(egui::Rounding::same(6.0))
                            .fill(ui.visuals().faint_bg_color)
                            .stroke(egui::Stroke::new(
                                0.5,
                                ui.visuals().widgets.noninteractive.bg_stroke.color,
                            ))
                            .show(ui, |ui: &mut egui::Ui| {
                                ui.set_min_width(ui.available_width());

                                // Header row: icon + game name
                                ui.horizontal(|ui| {
                                    // Game icon
                                    let icon_size = egui::vec2(48.0, 48.0);
                                    if let Some(bytes) = game_icon_bytes.get(&pid) {
                                        let uri = format!("bytes://game_icon/{pid}.png");
                                        ui.add(
                                            egui::Image::from_bytes(uri, bytes.clone())
                                                .fit_to_exact_size(icon_size)
                                                .rounding(egui::Rounding::same(6.0)),
                                        );
                                    } else {
                                        // Placeholder
                                        let (rect, _) = ui.allocate_exact_size(
                                            icon_size,
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            6.0,
                                            ui.visuals().widgets.noninteractive.bg_fill,
                                        );
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "\u{1f3ae}",
                                            egui::FontId::proportional(20.0),
                                            ui.visuals().text_color(),
                                        );
                                    }

                                    ui.vertical(|ui| {
                                        if !game_name.is_empty() {
                                            ui.strong(game_name);
                                        } else {
                                            ui.strong(format!("Place {pid}"));
                                        }
                                        ui.colored_label(
                                            ui.theme().text_muted,
                                            format!(
                                                "{} server{}",
                                                group.len(),
                                                if group.len() == 1 { "" } else { "s" }
                                            ),
                                        );
                                    });
                                });

                                ui.add_space(4.0);

                                // Individual servers in this group
                                for &(i, server) in &group {
                                    ui.push_id(i, |ui| {
                                        egui::Frame::default()
                                            .inner_margin(egui::Margin::same(6.0))
                                            .rounding(egui::Rounding::same(4.0))
                                            .fill(ui.visuals().extreme_bg_color)
                                            .show(ui, |ui: &mut egui::Ui| {
                                                ui.set_min_width(ui.available_width());
                                                ui.horizontal(|ui| {
                                                    ui.strong(&server.name);
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            if ui
                                                                .small_button("\u{1f5d1}")
                                                                .on_hover_text("Remove")
                                                                .clicked()
                                                            {
                                                                remove_idx = Some(i);
                                                            }
                                                            if server.place_name.is_empty()
                                                                && ui
                                                                    .small_button("\u{1f504}")
                                                                    .on_hover_text(
                                                                        "Resolve place name",
                                                                    )
                                                                    .clicked()
                                                            {
                                                                resolve_idx = Some(i);
                                                            }
                                                            if ui
                                                                .add_enabled(
                                                                    has_selection,
                                                                    egui::Button::new(
                                                                        "\u{1f680} Launch",
                                                                    ),
                                                                )
                                                                .on_hover_text(if has_selection {
                                                                    "Launch selected account(s)"
                                                                } else {
                                                                    "Select an account first"
                                                                })
                                                                .clicked()
                                                            {
                                                                action =
                                                                    Some(PrivateServerAction::Launch {
                                                                        place_id: server.place_id,
                                                                        link_code: server
                                                                            .link_code
                                                                            .clone(),
                                                                        access_code: server
                                                                            .access_code
                                                                            .clone(),
                                                                    });
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                    });
                                    ui.add_space(2.0);
                                }
                            });
                    });
                    ui.add_space(6.0);
                }

                if let Some(idx) = remove_idx {
                    action = Some(PrivateServerAction::Remove(idx));
                } else if let Some(idx) = resolve_idx {
                    action = Some(PrivateServerAction::Resolve(idx));
                }
            }
        });
    });

    action
}
