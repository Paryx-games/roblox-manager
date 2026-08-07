//! Left sidebar — account list, search/filter, status indicators, account grouping.
//! Supports multi-select via Ctrl+click (toggle) and Shift+click (range).
//! Accounts are organized into groups via drag-and-drop:
//!   - Drag an account onto another ungrouped account → prompt to create a group.
//!   - Drag an account onto a group header → add it to that group.
//!   - Groups are collapsible, colored containers that visually encapsulate their accounts.

use eframe::egui;
use ram_core::instances::TrackedInstance;
use ram_core::models::{Account, GroupMeta};

use crate::theme::{Theme, ThemeUi};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Stable 5-character anonymized tag derived from a user ID.
pub fn anon_tag(user_id: u64) -> String {
    let mut h = std::hash::DefaultHasher::new();
    user_id.hash(&mut h);
    let hash = h.finish();
    // Base-36 encoding of the lower bits, truncated to 5 chars
    let mut buf = String::with_capacity(5);
    let mut val = hash;
    for _ in 0..5 {
        let digit = (val % 36) as u8;
        buf.push(if digit < 10 { (b'0' + digit) as char } else { (b'a' + digit - 10) as char });
        val /= 36;
    }
    buf
}

/// Preset colors offered when creating/editing a group.
const GROUP_PRESETS: &[([u8; 3], &str)] = &[
    ([220, 60, 60], "Red"),
    ([60, 180, 60], "Green"),
    ([60, 120, 220], "Blue"),
    ([220, 180, 50], "Yellow"),
    ([160, 60, 220], "Purple"),
    ([60, 200, 200], "Cyan"),
    ([220, 130, 50], "Orange"),
    ([220, 80, 160], "Pink"),
];

/// Payload carried during drag-and-drop.
#[derive(Clone)]
enum DragPayload {
    /// Dragging an account row.
    Account { user_id: u64, label: String },
    /// Dragging a group header.
    Group { name: String },
}

/// Sort order for the account list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortOrder {
    /// Manual drag-to-reorder (persisted via `sort_order` fields).
    Custom,
    Name,
    Status,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortOrder::Custom => write!(f, "Custom"),
            SortOrder::Name => write!(f, "Name"),
            SortOrder::Status => write!(f, "Status"),
        }
    }
}

/// State for the group editor popup (create or edit).
pub struct GroupEditorState {
    pub name: String,
    pub color: [u8; 3],
    /// `None` = creating new group, `Some(old_name)` = editing existing.
    pub original_name: Option<String>,
    /// Accounts to assign to the group after creation.
    pub pending_assign: Vec<u64>,
}

/// Persistent state for the sidebar widget.
pub struct SidebarState {
    pub search_query: String,
    /// Index of the last clicked account in the flat display list (for Shift-range).
    last_clicked_index: Option<usize>,
    pub sort_order: SortOrder,
    /// Which groups are currently collapsed in the sidebar.
    pub collapsed_groups: HashSet<String>,
    /// Group editor popup state.
    pub editing_group: Option<GroupEditorState>,
    /// Pending sort change that needs user confirmation (warning about losing custom order).
    pub pending_sort_change: Option<SortOrder>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            last_clicked_index: None,
            sort_order: SortOrder::Custom,
            collapsed_groups: HashSet::new(),
            editing_group: None,
            pending_sort_change: None,
        }
    }
}

/// Actions the sidebar can request from the parent.
pub enum SidebarAction {
    /// Replace the entire selection with this single account.
    Select(u64),
    /// Toggle this account in/out of the selection (Ctrl+click).
    ToggleSelect(u64),
    /// Range-select from the last click to this account (Shift+click).
    RangeSelect(Vec<u64>),
    /// Copy text to clipboard.
    CopyJobId(String),
    AddAccountDialog,
    /// Double-click: quick-launch this account.
    QuickLaunch(u64),
    /// Open a webview pre-logged in as this account.
    OpenBrowserAs(u64),
    /// Assign accounts to a group (empty string = remove from group).
    AssignGroup { user_ids: Vec<u64>, group: String },
    /// Create a new group and optionally assign accounts to it.
    CreateGroup {
        name: String,
        color: [u8; 3],
        assign_user_ids: Vec<u64>,
    },
    /// Delete a group (its accounts become ungrouped).
    DeleteGroup(String),
    /// Edit a group's name and/or color.
    EditGroup {
        old_name: String,
        new_name: String,
        color: [u8; 3],
    },
    /// Reorder an account within its group/ungrouped list.
    ReorderAccount {
        user_id: u64,
        target_user_id: u64,
        /// If true, insert after the target instead of before.
        insert_after: bool,
    },
    /// Reorder a group relative to another group.
    ReorderGroup {
        group_name: String,
        target_group: String,
        /// If true, insert after the target instead of before.
        insert_after: bool,
    },
    /// User confirmed changing from custom sort → lose custom order.
    ResetCustomOrder,
    /// Bring this account's Roblox client to the front.
    FocusInstance(u32),
    /// Close this account's Roblox client. Only ever emitted for an attribution
    /// RM confirmed by reading the client's command line; `ram_core` re-checks
    /// that immediately before terminating anyway.
    KillInstance(u32),
    /// Launch `user_id` into whatever server `target_user_id` is currently in.
    JoinAccountServer { user_id: u64, target_user_id: u64 },
}

/// Sidebar result: actions to process and the list of currently visible user IDs.
pub struct SidebarResult {
    pub actions: Vec<SidebarAction>,
    pub visible_user_ids: Vec<u64>,
    /// Screen rect of the "Add Account" button (for tutorial highlighting).
    pub add_btn_rect: egui::Rect,
    /// Screen rect of the accounts scroll area (for tutorial highlighting).
    pub accounts_rect: egui::Rect,
}

/// Draw the sidebar. Returns actions + visible account IDs.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    state: &mut SidebarState,
    accounts: &[Account],
    selected_ids: &HashSet<u64>,
    anonymize: bool,
    groups: &HashMap<String, GroupMeta>,
    avatar_bytes: &HashMap<u64, Vec<u8>>,
    // Running clients RM has matched to an account, so a row can offer to
    // focus or close its own.
    instances: &[TrackedInstance],
) -> SidebarResult {
    let mut actions: Vec<SidebarAction> = Vec::new();
    let mut add_btn_rect = egui::Rect::NOTHING;
    let mut accounts_rect = egui::Rect::NOTHING;

    // Group editor floating window.
    show_group_editor(ui, state, &mut actions);

    ui.vertical(|ui| {
        ui.heading("Accounts");
        ui.separator();

        // Search bar
        ui.horizontal(|ui| {
            ui.label("\u{1f50d}");
            ui.text_edit_singleline(&mut state.search_query);
        });

        // Sort selector
        ui.horizontal(|ui| {
            ui.label("Sort:");
            let prev_sort = state.sort_order;
            egui::ComboBox::from_id_salt("sort_order")
                .selected_text(state.sort_order.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.sort_order, SortOrder::Custom, "Custom");
                    ui.selectable_value(&mut state.sort_order, SortOrder::Name, "Name");
                    ui.selectable_value(&mut state.sort_order, SortOrder::Status, "Status");
                });
            // If switching away from Custom that has real positions, show warning
            if prev_sort == SortOrder::Custom && state.sort_order != SortOrder::Custom {
                let has_custom_positions = accounts.iter().any(|a| a.sort_order != u32::MAX)
                    || groups.values().any(|g| g.sort_order != u32::MAX);
                if has_custom_positions {
                    // Revert and show confirmation dialog
                    let desired = state.sort_order;
                    state.sort_order = SortOrder::Custom;
                    state.pending_sort_change = Some(desired);
                }
            }
            // If switching TO Custom from another sort, auto-assign positions
            if prev_sort != SortOrder::Custom && state.sort_order == SortOrder::Custom {
                // Positions will be assigned by the handler (no custom positions yet)
            }
        });

        // Warning dialog for losing custom order
        show_sort_warning(ui, state, &mut actions);

        ui.add_space(4.0);

        // Add account button
        let add_btn_resp = ui.button("\u{2795}  Add Account");
        if add_btn_resp.clicked() {
            actions.push(SidebarAction::AddAccountDialog);
        }
        add_btn_rect = add_btn_resp.rect;

        // Selection count badge
        if selected_ids.len() > 1 {
            ui.colored_label(
                ui.theme().accent_text,
                format!("{} selected", selected_ids.len()),
            );
        }

        ui.separator();

        // ---- Build filtered + sorted list ----
        let query = state.search_query.to_lowercase();
        let is_searching = !query.is_empty();
        let mut filtered: Vec<(usize, &Account)> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if query.is_empty() {
                    return true;
                }
                a.username.to_lowercase().contains(&query)
                    || a.display_name.to_lowercase().contains(&query)
                    || a.alias.to_lowercase().contains(&query)
            })
            .collect();

        // Sort helper for the fallback (name-based) tiebreaker.
        let name_cmp = |a: &Account, b: &Account| {
            a.label().to_lowercase().cmp(&b.label().to_lowercase())
        };

        match state.sort_order {
            SortOrder::Custom => {
                // Primary: sort_order ascending, tiebreak: name
                filtered.sort_by(|(_, a), (_, b)| {
                    a.sort_order.cmp(&b.sort_order).then_with(|| name_cmp(a, b))
                });
            }
            SortOrder::Name => {
                filtered.sort_by(|(_, a), (_, b)| name_cmp(a, b));
            }
            SortOrder::Status => {
                filtered.sort_by(|(_, a), (_, b)| {
                    b.last_presence
                        .user_presence_type
                        .cmp(&a.last_presence.user_presence_type)
                        .then_with(|| name_cmp(a, b))
                });
            }
        }

        // ---- Partition by group ----
        let mut ungrouped: Vec<(usize, &Account)> = Vec::new();
        let mut by_group: HashMap<&str, Vec<(usize, &Account)>> = HashMap::new();
        for &item in &filtered {
            if item.1.group.is_empty() {
                ungrouped.push(item);
            } else {
                by_group.entry(item.1.group.as_str()).or_default().push(item);
            }
        }

        // Sort group names: Custom mode uses GroupMeta.sort_order, otherwise alphabetical.
        let mut group_names: Vec<&str> = by_group.keys().copied().collect();
        match state.sort_order {
            SortOrder::Custom => {
                group_names.sort_by(|a, b| {
                    let a_ord = groups.get(*a).map(|m| m.sort_order).unwrap_or(u32::MAX);
                    let b_ord = groups.get(*b).map(|m| m.sort_order).unwrap_or(u32::MAX);
                    a_ord.cmp(&b_ord).then_with(|| a.cmp(b))
                });
            }
            _ => {
                group_names.sort_unstable();
            }
        }

        // Build flat display list for Shift+click range selection.
        // Groups first, then ungrouped.
        let mut flat_list: Vec<(usize, &Account)> = Vec::new();
        for name in &group_names {
            if is_searching || !state.collapsed_groups.contains(*name) {
                flat_list.extend_from_slice(&by_group[name]);
            }
        }
        flat_list.extend_from_slice(&ungrouped);

        let is_custom = state.sort_order == SortOrder::Custom;

        // ---- Render ----
        accounts_rect = ui.available_rect_before_wrap();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut flat_idx: usize = 0;

                // Groups first (above ungrouped accounts)
                for &group_name in &group_names {
                    let meta = groups.get(group_name);
                    let color = meta.map(|m| m.color).unwrap_or([130, 130, 130]);
                    let members = &by_group[group_name];
                    let is_collapsed = state.collapsed_groups.contains(group_name);
                    let show_members = is_searching || !is_collapsed;
                    let group_color = egui::Color32::from_rgb(color[0], color[1], color[2]);
                    let bg_color = egui::Color32::from_rgba_premultiplied(
                        color[0] / 8,
                        color[1] / 8,
                        color[2] / 8,
                        30,
                    );

                    // Outer frame for the whole group
                    egui::Frame::none()
                        .fill(bg_color)
                        .stroke(egui::Stroke::new(1.0, group_color.gamma_multiply(0.4)))
                        .rounding(egui::Rounding::same(4.0))
                        .inner_margin(egui::Margin::same(2.0))
                        .outer_margin(egui::Margin { top: 2.0, bottom: 4.0, ..Default::default() })
                        .show(ui, |ui: &mut egui::Ui| {
                            // ---- Group header ----
                            let header_height = 24.0;
                            let sense = if is_custom {
                                egui::Sense::click_and_drag()
                            } else {
                                egui::Sense::click()
                            };
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), header_height),
                                sense,
                            );

                            // Drag source: group header (Custom mode only)
                            if is_custom && response.drag_started() {
                                response.dnd_set_drag_payload(DragPayload::Group {
                                    name: group_name.to_string(),
                                });
                            }

                            // Header background on hover
                            if response.hovered() || response.contains_pointer() {
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    group_color.gamma_multiply(0.15),
                                );
                            }

                            // DnD: drop account on group header → assign to group
                            let header_bottom_half = ui.ctx().pointer_latest_pos()
                                .is_some_and(|pos| pos.y > rect.center().y);

                            if let Some(payload) = response.dnd_hover_payload::<DragPayload>() {
                                match payload.as_ref() {
                                    DragPayload::Account { user_id, .. } => {
                                        ui.painter().rect_stroke(
                                            rect.expand(2.0),
                                            4.0,
                                            egui::Stroke::new(2.0, group_color),
                                        );
                                        let already_in = accounts
                                            .iter()
                                            .find(|a| a.user_id == *user_id)
                                            .is_some_and(|a| a.group == group_name);
                                        if !already_in {
                                            let hint = format!("Add to {}", group_name);
                                            ui.painter().text(
                                                egui::pos2(rect.center().x, rect.max.y + 2.0),
                                                egui::Align2::CENTER_TOP,
                                                hint,
                                                egui::FontId::proportional(10.0),
                                                group_color,
                                            );
                                        }
                                    }
                                    DragPayload::Group { name } if is_custom && *name != group_name => {
                                        // Reorder hint: show line at top or bottom
                                        let line_y = if header_bottom_half { rect.max.y } else { rect.min.y };
                                        ui.painter().hline(
                                            rect.x_range(),
                                            line_y,
                                            egui::Stroke::new(2.0, ui.theme().drop_reorder),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            if let Some(payload) = response.dnd_release_payload::<DragPayload>() {
                                match payload.as_ref() {
                                    DragPayload::Account { user_id, .. } => {
                                        actions.push(SidebarAction::AssignGroup {
                                            user_ids: vec![*user_id],
                                            group: group_name.to_string(),
                                        });
                                    }
                                    DragPayload::Group { name } if is_custom && *name != group_name => {
                                        actions.push(SidebarAction::ReorderGroup {
                                            group_name: name.clone(),
                                            target_group: group_name.to_string(),
                                            insert_after: header_bottom_half,
                                        });
                                    }
                                    _ => {}
                                }
                            }

                            let painter = ui.painter_at(rect);

                            // Color accent bar on the left
                            painter.rect_filled(
                                egui::Rect::from_min_size(rect.min, egui::vec2(3.0, header_height)),
                                0.0,
                                group_color,
                            );

                            // Group name
                            painter.text(
                                egui::pos2(rect.min.x + 10.0, rect.min.y + 4.0),
                                egui::Align2::LEFT_TOP,
                                group_name,
                                egui::FontId::proportional(13.0),
                                group_color,
                            );

                            // Member count
                            painter.text(
                                egui::pos2(rect.max.x - 6.0, rect.min.y + 5.0),
                                egui::Align2::RIGHT_TOP,
                                format!("{}", members.len()),
                                egui::FontId::proportional(11.0),
                                ui.theme().text_muted,
                            );

                            // Click header to toggle collapse
                            if response.clicked() {
                                if is_collapsed {
                                    state.collapsed_groups.remove(group_name);
                                } else {
                                    state.collapsed_groups.insert(group_name.to_string());
                                }
                            }

                            // Group header context menu
                            response.context_menu(|ui: &mut egui::Ui| {
                                if ui.button("\u{270f}  Edit Group").clicked() {
                                    state.editing_group = Some(GroupEditorState {
                                        name: group_name.to_string(),
                                        color,
                                        original_name: Some(group_name.to_string()),
                                        pending_assign: Vec::new(),
                                    });
                                    ui.close_menu();
                                }
                                if ui.button("\u{1f5d1}  Delete Group").clicked() {
                                    actions.push(SidebarAction::DeleteGroup(
                                        group_name.to_string(),
                                    ));
                                    ui.close_menu();
                                }
                            });

                            // ---- Member rows ----
                            if show_members {
                                for &(_orig_idx, account) in members {
                                    render_account_row(
                                        ui,
                                        account,
                                        flat_idx,
                                        selected_ids,
                                        anonymize,
                                        &flat_list,
                                        state,
                                        &mut actions,
                                        groups,
                                        is_custom,
                                        avatar_bytes,
                                        instances,
                                    );
                                    flat_idx += 1;
                                }
                            }
                        });
                }

                // Spacer between groups and ungrouped
                if !group_names.is_empty() && !ungrouped.is_empty() {
                    ui.add_space(6.0);
                }

                // Ungrouped accounts last
                for &(_orig_idx, account) in &ungrouped {
                    render_account_row(
                        ui,
                        account,
                        flat_idx,
                        selected_ids,
                        anonymize,
                        &flat_list,
                        state,
                        &mut actions,
                        groups,
                        is_custom,
                        avatar_bytes,
                        instances,
                    );
                    flat_idx += 1;
                }
            });

        // ---- Floating drag label ----
        // Show a small label following the pointer while dragging.
        if let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ui.ctx()) {
            if let Some(pos) = ui.ctx().pointer_latest_pos() {
                let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dnd_label"));
                let painter = ui.ctx().layer_painter(layer);
                let text = match payload.as_ref() {
                    DragPayload::Account { label, user_id } => {
                        if anonymize { format!("Account #{}", anon_tag(*user_id)) } else { label.clone() }
                    }
                    DragPayload::Group { name } => format!("\u{1f4c1} {}", name),
                };
                let theme = ui.theme();
                let galley = painter.layout_no_wrap(
                    text,
                    egui::FontId::proportional(12.0),
                    theme.on_accent,
                );
                let text_size = galley.size();
                let label_rect = egui::Rect::from_min_size(
                    egui::pos2(pos.x + 12.0, pos.y - 8.0),
                    text_size + egui::vec2(10.0, 6.0),
                );
                painter.rect_filled(label_rect, 4.0, theme.overlay);
                painter.galley(
                    label_rect.min + egui::vec2(5.0, 3.0),
                    galley,
                    theme.on_accent,
                );
            }
        }
    });

    let visible_user_ids = {
        let query = state.search_query.to_lowercase();
        accounts
            .iter()
            .filter(|a| {
                if query.is_empty() {
                    return true;
                }
                a.username.to_lowercase().contains(&query)
                    || a.display_name.to_lowercase().contains(&query)
                    || a.alias.to_lowercase().contains(&query)
            })
            .map(|a| a.user_id)
            .collect()
    };

    SidebarResult {
        actions,
        visible_user_ids,
        add_btn_rect,
        accounts_rect,
    }
}

// ---------------------------------------------------------------------------
// Account row — supports drag-and-drop
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_account_row(
    ui: &mut egui::Ui,
    account: &Account,
    flat_idx: usize,
    selected_ids: &HashSet<u64>,
    anonymize: bool,
    flat_list: &[(usize, &Account)],
    state: &mut SidebarState,
    actions: &mut Vec<SidebarAction>,
    _groups: &HashMap<String, GroupMeta>,
    is_custom: bool,
    avatar_bytes: &HashMap<u64, Vec<u8>>,
    instances: &[TrackedInstance],
) {
    let theme = ui.theme();
    let is_selected = selected_ids.contains(&account.user_id);
    let has_subtitle =
        !anonymize && account.alias.is_empty() && account.display_name != account.username;
    let row_height = 40.0;
    let avatar_size = 32.0;

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_height),
        egui::Sense::click_and_drag(),
    );

    // ---- Drag source ----
    let display_label = if anonymize {
        format!("Account #{}", anon_tag(account.user_id))
    } else {
        account.label().to_string()
    };
    if response.drag_started() {
        response.dnd_set_drag_payload(DragPayload::Account {
            user_id: account.user_id,
            label: display_label.clone(),
        });
    }

    // ---- Drop target ----
    let is_being_dragged_over = response.dnd_hover_payload::<DragPayload>().is_some();
    let is_ungrouped = account.group.is_empty();

    // Determine if cursor is in the bottom half of this row (for insert-after).
    let pointer_in_bottom_half = ui.ctx().pointer_latest_pos()
        .is_some_and(|pos| pos.y > rect.center().y);

    if let Some(payload) = response.dnd_hover_payload::<DragPayload>() {
        if let DragPayload::Account { user_id, .. } = payload.as_ref() {
            if *user_id != account.user_id {
                let drag_group = flat_list.iter()
                    .find(|(_, a)| a.user_id == *user_id)
                    .map(|(_, a)| a.group.as_str())
                    .unwrap_or("");
                let same_group = drag_group == account.group;

                if is_custom && same_group {
                    // Same group or both ungrouped in custom mode → show reorder hint
                    ui.painter()
                        .rect_filled(rect, 2.0, Theme::wash(theme.drop_reorder, 40));
                    // Draw an insertion line at the top or bottom of the row
                    let line_y = if pointer_in_bottom_half { rect.max.y } else { rect.min.y };
                    ui.painter().hline(
                        rect.x_range(),
                        line_y,
                        egui::Stroke::new(2.0, theme.drop_reorder),
                    );
                } else {
                    // Different groups / assign-to-group drop. Green reads as
                    // "this creates a group", blue as "this moves into one".
                    let highlight_color = if is_ungrouped {
                        Theme::wash(theme.success_text, 50)
                    } else {
                        Theme::wash(theme.accent_text, 50)
                    };
                    ui.painter().rect_filled(rect, 2.0, highlight_color);
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.5, theme.success_text),
                    );
                }
            }
        }
    }
    if let Some(payload) = response.dnd_release_payload::<DragPayload>() {
        if let DragPayload::Account { user_id, .. } = payload.as_ref() {
            if *user_id != account.user_id {
                let drag_group = flat_list.iter()
                    .find(|(_, a)| a.user_id == *user_id)
                    .map(|(_, a)| a.group.as_str())
                    .unwrap_or("");

                if is_custom && drag_group == account.group {
                    // Reorder within the same group/ungrouped
                    actions.push(SidebarAction::ReorderAccount {
                        user_id: *user_id,
                        target_user_id: account.user_id,
                        insert_after: pointer_in_bottom_half,
                    });
                } else if is_ungrouped {
                    // Both ungrouped → prompt to create a group with both
                    state.editing_group = Some(GroupEditorState {
                        name: String::new(),
                        color: GROUP_PRESETS[0].0,
                        original_name: None,
                        pending_assign: vec![*user_id, account.user_id],
                    });
                } else {
                    // Drop onto a grouped account → add to that account's group
                    actions.push(SidebarAction::AssignGroup {
                        user_ids: vec![*user_id],
                        group: account.group.clone(),
                    });
                }
            }
        }
    }

    // Background (don't overdraw if drag highlight is active)
    if !is_being_dragged_over {
        if is_selected {
            ui.painter()
                .rect_filled(rect, 2.0, ui.style().visuals.selection.bg_fill);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 2.0, ui.style().visuals.widgets.hovered.bg_fill);
        }
    }

    // ---- Avatar thumbnail (or gray placeholder) ----
    let avatar_pad = (row_height - avatar_size) / 2.0;
    let avatar_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 6.0, rect.min.y + avatar_pad),
        egui::vec2(avatar_size, avatar_size),
    );
    // The caller decides which avatar map to pass (originals vs blurred), so
    // we just render whatever bytes are present. The URI gets an "anon"
    // discriminator so egui's image cache doesn't serve the wrong variant
    // when the user toggles anonymize.
    if let Some(bytes) = avatar_bytes.get(&account.user_id) {
        let variant = if anonymize { "anon" } else { "raw" };
        let uri = format!("bytes://sidebar_avatar/{}_{}.png", variant, account.user_id);
        egui::Image::from_bytes(uri, bytes.clone())
            .rounding(egui::Rounding::same(avatar_size / 2.0))
            .paint_at(ui, avatar_rect);
    } else {
        ui.painter().rect_filled(
            avatar_rect,
            avatar_size / 2.0,
            theme.surface,
        );
        ui.painter().text(
            avatar_rect.center(),
            egui::Align2::CENTER_CENTER,
            "…",
            egui::FontId::proportional(avatar_size * 0.45),
            theme.text_faint,
        );
    }

    // ---- Status dot overlaid on bottom-right of the avatar ----
    // Three mutually-exclusive states take precedence over plain presence:
    //   1. moderated → orange dot with "!" (the strongest "needs attention" cue)
    //   2. cookie_expired → red dot with "x"
    //   3. otherwise → green/blue/orange/gray presence dot
    let moderated = account.moderation.as_ref().is_some_and(|m| m.is_active());
    let (dot_color, dot_glyph) = if moderated {
        (theme.moderated, Some("!"))
    } else if account.cookie_expired {
        (theme.danger, Some("x"))
    } else {
        (theme.presence(account.last_presence.user_presence_type), None)
    };
    let dot_center =
        egui::pos2(avatar_rect.max.x - 5.0, avatar_rect.max.y - 5.0);
    // Background "halo" so the dot reads against any avatar color
    ui.painter().circle_filled(
        dot_center,
        6.0,
        ui.visuals().panel_fill,
    );
    ui.painter().circle_filled(dot_center, 4.5, dot_color);
    if let Some(glyph) = dot_glyph {
        ui.painter().text(
            dot_center,
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(8.0),
            theme.on_accent,
        );
    }

    // ---- Label + optional subtitle ----
    let text_x = avatar_rect.max.x + 8.0;
    let painter = ui.painter_at(rect);
    if has_subtitle {
        painter.text(
            egui::pos2(text_x, rect.min.y + 5.0),
            egui::Align2::LEFT_TOP,
            &display_label,
            egui::FontId::proportional(13.5),
            ui.style().visuals.text_color(),
        );
        painter.text(
            egui::pos2(text_x, rect.min.y + 22.0),
            egui::Align2::LEFT_TOP,
            &account.display_name,
            egui::FontId::proportional(11.0),
            ui.visuals().weak_text_color(),
        );
    } else {
        // Single line — vertically center it within the row.
        painter.text(
            egui::pos2(text_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &display_label,
            egui::FontId::proportional(13.5),
            ui.style().visuals.text_color(),
        );
    }

    // Click handling (only if not dragging)
    if response.clicked() {
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.ctrl || modifiers.mac_cmd {
            actions.push(SidebarAction::ToggleSelect(account.user_id));
            state.last_clicked_index = Some(flat_idx);
        } else if modifiers.shift {
            let anchor = state.last_clicked_index.unwrap_or(0);
            let lo = anchor.min(flat_idx);
            let hi = anchor.max(flat_idx);
            let range_ids: Vec<u64> =
                flat_list[lo..=hi].iter().map(|(_, a)| a.user_id).collect();
            actions.push(SidebarAction::RangeSelect(range_ids));
        } else {
            actions.push(SidebarAction::Select(account.user_id));
            state.last_clicked_index = Some(flat_idx);
        }
    }

    if response.double_clicked() {
        actions.push(SidebarAction::QuickLaunch(account.user_id));
    }

    // Right-click context menu
    response.context_menu(|ui| {
        if ui.button("\u{1f310}  Open browser as").clicked() {
            actions.push(SidebarAction::OpenBrowserAs(account.user_id));
            ui.close_menu();
        }
        ui.separator();

        // ---- This account's running clients ----
        let mine: Vec<&TrackedInstance> = instances
            .iter()
            .filter(|i| i.user_id == account.user_id)
            .collect();
        if !mine.is_empty() {
            // The PID is only worth showing when there is more than one client
            // to tell apart. With a single one it is noise.
            let show_pid = mine.len() > 1;
            for instance in &mine {
                let suffix = if show_pid {
                    format!(" (PID {})", instance.pid)
                } else {
                    String::new()
                };
                if ui.button(format!("\u{1f5b5}  Focus client{suffix}")).clicked() {
                    actions.push(SidebarAction::FocusInstance(instance.pid));
                    ui.close_menu();
                }
                // Killing is gated on RM having actually read this client's
                // command line. A mapping that fell back to appearance order
                // cannot be re-verified at kill time, so the button is shown
                // disabled with the reason rather than offered and then
                // refused, or worse, honoured on a guess.
                let can_kill = instance.attribution.is_exact();
                let kill = ui.add_enabled(
                    can_kill,
                    egui::Button::new(format!("\u{2716}  Close client{suffix}")),
                );
                if !can_kill {
                    kill.on_disabled_hover_text(
                        "RM could not read this client's command line, so it cannot \
                         confirm the client belongs to this account. Use Kill All.",
                    );
                } else if kill.clicked() {
                    actions.push(SidebarAction::KillInstance(instance.pid));
                    ui.close_menu();
                }
            }
            ui.separator();
        }

        // ---- Join another managed account's server ----
        let joinable: Vec<&Account> = flat_list
            .iter()
            .map(|(_, a)| *a)
            .filter(|a| {
                a.user_id != account.user_id
                    && a.last_presence.user_presence_type == 2
                    && a.last_presence.place_id.is_some()
                    && a.last_presence.game_id.is_some()
            })
            .collect();
        if !joinable.is_empty() {
            ui.menu_button("\u{1f517}  Join server of", |ui| {
                for other in &joinable {
                    let name = if anonymize {
                        format!("Account #{}", anon_tag(other.user_id))
                    } else {
                        other.label().to_string()
                    };
                    if ui.button(name).clicked() {
                        actions.push(SidebarAction::JoinAccountServer {
                            user_id: account.user_id,
                            target_user_id: other.user_id,
                        });
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
        }

        // Game session info
        if let Some(ref gid) = account.last_presence.game_id {
            if account.last_presence.user_presence_type == 2 {
                if ui.button("\u{1f4cb}  Copy Job ID").clicked() {
                    actions.push(SidebarAction::CopyJobId(gid.clone()));
                    ui.close_menu();
                }
                if let Some(pid) = account.last_presence.place_id {
                    if ui.button("\u{1f4cb}  Copy Place ID").clicked() {
                        actions.push(SidebarAction::CopyJobId(pid.to_string()));
                        ui.close_menu();
                    }
                }
                ui.separator();
            }
        }

        // Remove from group option
        if !account.group.is_empty() {
            if ui.button("\u{2934}  Remove from Group").clicked() {
                actions.push(SidebarAction::AssignGroup {
                    user_ids: vec![account.user_id],
                    group: String::new(),
                });
                ui.close_menu();
            }
            ui.separator();
        }

        if anonymize {
            ui.label(format!("Account #{}", anon_tag(account.user_id)));
        } else {
            ui.label(format!("@{}", account.username));
            ui.label(format!("ID: {}", account.user_id));
        }
    });

    // Tooltip (not during drag)
    if !response.dragged() {
        let tip = if anonymize {
            format!("Account #{}", anon_tag(account.user_id))
        } else {
            format!(
                "{}: {}",
                account.username,
                account.last_presence.status_text()
            )
        };
        response.on_hover_text(tip);
    }
}

// ---------------------------------------------------------------------------
// Group editor popup
// ---------------------------------------------------------------------------

fn show_group_editor(
    ui: &mut egui::Ui,
    state: &mut SidebarState,
    actions: &mut Vec<SidebarAction>,
) {
    if state.editing_group.is_none() {
        return;
    }

    let is_edit = state
        .editing_group
        .as_ref()
        .unwrap()
        .original_name
        .is_some();
    let title = if is_edit { "Edit Group" } else { "New Group" };

    let mut open = true;
    let mut should_close = false;

    {
        let editor = state.editing_group.as_mut().unwrap();

        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let te = ui.text_edit_singleline(&mut editor.name);
                    // Auto-focus the name field when creating a new group
                    if !is_edit {
                        te.request_focus();
                    }
                });

                ui.add_space(4.0);
                ui.label("Color:");
                ui.horizontal_wrapped(|ui| {
                    for &(preset_color, label) in GROUP_PRESETS {
                        let c = egui::Color32::from_rgb(
                            preset_color[0],
                            preset_color[1],
                            preset_color[2],
                        );
                        let is_sel = editor.color == preset_color;
                        let size = if is_sel { 22.0 } else { 18.0 };
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(size, size),
                            egui::Sense::click(),
                        );
                        ui.painter().rect_filled(rect, 4.0, c);
                        if is_sel {
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                egui::Stroke::new(2.0, ui.theme().on_accent),
                            );
                        }
                        if resp.clicked() {
                            editor.color = preset_color;
                        }
                        resp.on_hover_text(label);
                    }
                });

                ui.add_space(8.0);
                let name_valid = !editor.name.trim().is_empty();

                // Enter key to confirm
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));

                ui.horizontal(|ui| {
                    let save_clicked = ui
                        .add_enabled(name_valid, egui::Button::new("Save"))
                        .clicked();
                    if save_clicked || (enter && name_valid) {
                        let name = editor.name.trim().to_string();
                        let color = editor.color;
                        if let Some(ref old_name) = editor.original_name {
                            actions.push(SidebarAction::EditGroup {
                                old_name: old_name.clone(),
                                new_name: name,
                                color,
                            });
                        } else {
                            actions.push(SidebarAction::CreateGroup {
                                name,
                                color,
                                assign_user_ids: editor.pending_assign.clone(),
                            });
                        }
                        should_close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                });
            });
    }

    if should_close || !open {
        state.editing_group = None;
    }
}

// ---------------------------------------------------------------------------
// Sort-change warning dialog
// ---------------------------------------------------------------------------

fn show_sort_warning(
    ui: &mut egui::Ui,
    state: &mut SidebarState,
    actions: &mut Vec<SidebarAction>,
) {
    let Some(desired) = state.pending_sort_change else {
        return;
    };

    let mut should_close = false;

    egui::Window::new("Change Sort Order?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.label("Switching away from Custom sort will discard your manual ordering.");
            ui.label("This cannot be undone.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Switch Anyway").clicked() {
                    state.sort_order = desired;
                    actions.push(SidebarAction::ResetCustomOrder);
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });
        });

    if should_close {
        state.pending_sort_change = None;
    }
}

// ---------------------------------------------------------------------------


