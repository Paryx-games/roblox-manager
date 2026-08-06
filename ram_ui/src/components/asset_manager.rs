//! Asset Manager tab — upload assets to Roblox as any saved account, track
//! moderation, and grant experiences permission to use them.
//!
//! Gated behind `AppConfig::developer_options` (off by default) because every
//! upload creates a permanent, publicly moderated asset on a real account.
//!
//! Drawing is read-only over a snapshot: each frame the panel copies the rows
//! it needs out of the index, draws from the copy, and returns edits for the
//! caller to apply. Holding a borrow of the index across `TableBuilder`'s
//! closures would fight the borrow checker for no benefit, and the copy is a
//! few hundred small structs at most.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ram_core::assets::{AssetIndex, AssetKind, AssetState, Creator};
use ram_core::assets_api::{CreationItem, GroupTarget, UniverseTarget};
use ram_core::models::Account;

use crate::theme::ThemeUi;

/// Which of the tab's two views is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Everything this app has uploaded.
    #[default]
    Library,
    /// Files staged for upload.
    ImportQueue,
}

/// A node in the left tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeNode {
    /// Everything this app uploaded, straight from the local index. Needs no
    /// network, so it always works.
    #[default]
    Uploads,
    /// A creator's inventory on Roblox, fetched live.
    Inventory(Creator),
}

/// One creator's inventory as fetched from Roblox.
///
/// The listing endpoint takes `assetType` as a **required** query parameter, so
/// "All types" is not a filter here the way it is in the local library. It is a
/// fan-out: one request per kind, merged. That is why cursors are tracked per
/// kind rather than as a single value.
#[derive(Default)]
pub struct RemoteInventory {
    pub node: Option<TreeNode>,
    /// The filter this data was fetched for. `None` means all types.
    pub filter: Option<AssetKind>,
    /// True once `filter`/`node` have been set, so an empty result is
    /// distinguishable from "nothing requested yet".
    pub requested: bool,
    pub items: Vec<CreationItem>,
    /// Next cursor per kind. A kind absent from here has no further pages.
    pub cursors: HashMap<AssetKind, String>,
    /// Outstanding requests. More than one while a fan-out is in progress.
    pub inflight: usize,
    /// Set when a listing failed. Shown on the node rather than as a toast,
    /// because the rest of the tab is unaffected.
    pub error: Option<String>,
}

impl RemoteInventory {
    pub fn loading(&self) -> bool {
        self.inflight > 0
    }

    pub fn has_more(&self) -> bool {
        !self.cursors.is_empty()
    }

    /// Whether this data answers the given node and filter.
    pub fn matches(&self, node: TreeNode, filter: Option<AssetKind>) -> bool {
        self.requested && self.node == Some(node) && self.filter == filter
    }
}

/// How the asset list is laid out, mirroring Windows Explorer's View menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// The sortable table. Explorer calls this Details.
    #[default]
    Details,
    /// Small icon plus name, flowing into columns.
    List,
    SmallIcons,
    MediumIcons,
    LargeIcons,
}

impl ViewMode {
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Details => "Details",
            ViewMode::List => "List",
            ViewMode::SmallIcons => "Small icons",
            ViewMode::MediumIcons => "Medium icons",
            ViewMode::LargeIcons => "Large icons",
        }
    }

    pub fn all() -> &'static [ViewMode] {
        &[
            ViewMode::Details,
            ViewMode::List,
            ViewMode::SmallIcons,
            ViewMode::MediumIcons,
            ViewMode::LargeIcons,
        ]
    }

    /// Icon edge length in points. `None` for Details, which draws no icon.
    fn icon_px(self) -> Option<f32> {
        match self {
            ViewMode::Details => None,
            ViewMode::List => Some(16.0),
            ViewMode::SmallIcons => Some(48.0),
            ViewMode::MediumIcons => Some(96.0),
            ViewMode::LargeIcons => Some(144.0),
        }
    }

    /// Whether tiles stack the name under the icon (grid) or beside it (list).
    fn is_grid(self) -> bool {
        matches!(
            self,
            ViewMode::SmallIcons | ViewMode::MediumIcons | ViewMode::LargeIcons
        )
    }
}

/// Sortable columns, shared by both tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    Name,
    Id,
    Kind,
    #[default]
    Date,
    CreatorName,
}

/// Draw a clickable sort header. Returns true when it was clicked.
///
/// The caret shows which way the data is actually ordered, so it must be
/// rendered from live state rather than assumed.
fn sort_header(ui: &mut egui::Ui, label: &str, key: SortKey, active: SortKey, ascending: bool) -> bool {
    let caret = if active == key {
        if ascending {
            " \u{2b06}"
        } else {
            " \u{2b07}"
        }
    } else {
        ""
    };
    let button = egui::Button::new(egui::RichText::new(format!("{label}{caret}")).strong())
        .frame(false);
    ui.add(button)
        .on_hover_text(if active == key && ascending {
            "Click to sort descending"
        } else {
            "Click to sort ascending"
        })
        .clicked()
}

/// Apply a header click: same column flips direction, a new column starts on
/// the direction that is most useful for it (newest first for dates, A to Z
/// for everything else).
fn apply_sort_click(state: &mut AssetManagerState, key: SortKey) {
    if state.sort == key {
        state.sort_ascending = !state.sort_ascending;
    } else {
        state.sort = key;
        state.sort_ascending = key != SortKey::Date;
    }
}

/// Persistent state for the Asset Manager panel.
#[derive(Default)]
pub struct AssetManagerState {
    pub view: View,
    /// Account whose cookie signs uploads. `None` until the store has loaded.
    pub acting_user_id: Option<u64>,
    /// Creator applied to newly added rows. `None` means "the acting account".
    pub batch_creator: Option<Creator>,
    pub search: String,
    pub type_filter: Option<AssetKind>,
    pub sort: SortKey,
    /// Defaults to false, i.e. newest first, because the thing you just
    /// uploaded is the thing you want to see.
    pub sort_ascending: bool,
    /// Queue rows ticked for upload.
    pub checked: HashSet<String>,
    /// Library rows selected for a bulk action.
    pub selected: HashSet<String>,
    /// Confirmation modal is open for this many rows.
    pub confirm_upload: Option<usize>,
    /// The grant-permission modal is open.
    pub grant_open: bool,
    /// Universe chosen in the grant modal.
    pub grant_universe: Option<u64>,
    /// Manual place-or-universe ID typed into the grant modal. The guaranteed
    /// path when the universe listing endpoint is unavailable.
    pub grant_manual: String,
    /// Applied to every row in the next upload batch. Unset by default, so
    /// nothing is granted unless the user asks for it.
    pub auto_grant_universe: Option<u64>,
    /// Which left-tree node is showing.
    pub node: TreeNode,
    /// Layout of the asset list.
    pub view_mode: ViewMode,
}

/// Something the panel wants the caller to do. The panel never touches
/// `AppState` itself.
pub enum AssetManagerAction {
    /// Open the native file picker and stage whatever comes back. Dropped
    /// files bypass this: the caller reads them straight off the egui context.
    PickFiles,
    RemoveRow(String),
    /// Drop every row that has finished, however it finished.
    ClearFinished,
    /// Put a failed row back in the queue.
    RetryRow(String),
    /// Ask for confirmation before uploading the ticked rows.
    RequestUpload(Vec<String>),
    /// Open the grant-permission modal for the selected library rows.
    OpenGrantDialog,
    /// Load (or reload) an inventory node from Roblox. `filter` of `None` means
    /// every kind, which the caller fans out into one request each.
    LoadInventory {
        node: TreeNode,
        filter: Option<AssetKind>,
    },
    /// Open the OS file browser with this file selected.
    RevealFile(std::path::PathBuf),
    /// Fetch the next page of the current inventory.
    LoadMoreInventory,
}

/// Everything the panel reads or edits, bundled so `show` stays well under
/// clippy's argument limit, which CI treats as a hard error.
pub struct AssetsCtx<'a> {
    pub state: &'a mut AssetManagerState,
    pub index: &'a mut AssetIndex,
    pub accounts: &'a [Account],
    pub anonymize: bool,
    /// Universes the acting account manages. Possibly empty: the listing
    /// endpoint is provisional, and the manual ID field covers that case.
    pub universes: &'a [UniverseTarget],
    /// Groups the acting account belongs to.
    pub groups: &'a [GroupTarget],
    /// The currently loaded remote inventory, if any.
    pub remote: &'a RemoteInventory,
    /// Cached thumbnail PNGs, keyed by asset ID.
    pub thumbnails: &'a HashMap<u64, Vec<u8>>,
    /// False when no master password is loaded, so no cookie can be decrypted.
    pub unlocked: bool,
    /// The index on disk must not be written (newer schema, or unreadable).
    pub read_only: bool,
}

#[derive(Default)]
pub struct AssetManagerResult {
    pub action: Option<AssetManagerAction>,
    /// The panel edited the index in place, so the caller should persist.
    pub index_changed: bool,
    /// Assets drawn this frame that have no cached thumbnail yet. Collected
    /// during drawing so only what is actually on screen gets fetched.
    pub want_thumbnails: Vec<u64>,
}

/// A row copied out of the index for drawing.
struct RowView {
    row_id: String,
    name: String,
    kind: AssetKind,
    creator: Creator,
    path: String,
    state: AssetState,
    created_at: DateTime<Utc>,
    /// Mirrors the record's fields so the status cell can tell a row that is
    /// ready from one that is serving out a retry backoff.
    attempts: u32,
    retry_at: Option<DateTime<Utc>>,
}

/// An edit collected while drawing, applied once the table's borrows are done.
enum Edit {
    Name(String, String),
    Kind(String, AssetKind),
    Creator(String, Creator),
}

pub fn show(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>) -> AssetManagerResult {
    reconcile_acting_account(cx);

    let mut result = AssetManagerResult::default();

    if cx.read_only {
        ui.colored_label(
            ui.theme().warning,
            "\u{26a0} This asset list was written by a newer version of RM, or could not be read. \
             Changes will not be saved.",
        );
        ui.add_space(4.0);
    }

    if cx.accounts.is_empty() {
        empty_state(
            ui,
            "No accounts yet",
            "Add an account on the Accounts tab to upload assets from it.",
        );
        return result;
    }

    toolbar(ui, cx, &mut result);
    ui.separator();
    ui.add_space(6.0);

    match cx.state.view {
        View::Library => {
            match cx.state.node {
                TreeNode::Uploads => library_view(ui, cx, &mut result),
                TreeNode::Inventory(_) => inventory_view(ui, cx, &mut result),
            }
            selection_footer(ui, cx, &mut result);
        }
        // The queue is always a table: its per-row creator and type pickers
        // have nowhere to live on an icon tile.
        View::ImportQueue => queue_view(ui, cx, &mut result),
    }

    result.want_thumbnails.sort_unstable();
    result.want_thumbnails.dedup();
    result
}

/// Click handling shared by the icon views: plain click replaces the
/// selection, ctrl toggles, matching Explorer.
fn apply_icon_click(ui: &egui::Ui, selected: &mut HashSet<String>, key: String) {
    let modifiers = ui.input(|i| i.modifiers);
    if !modifiers.ctrl && !modifiers.command {
        let only_this = selected.len() == 1 && selected.contains(&key);
        selected.clear();
        if only_this {
            return;
        }
    }
    if !selected.remove(&key) {
        selected.insert(key);
    }
}

/// The left tree. Drawn into its own `SidePanel` by the caller.
///
/// Flat two-level, matching the reference: bold section headers with indented
/// selectable leaves, no disclosure triangles.
pub fn show_tree(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>) -> Option<AssetManagerAction> {
    let mut action = None;
    let filter = cx.state.type_filter;

    ui.add_space(4.0);
    ui.strong("Recent");
    ui.indent("recent_group", |ui| {
        if ui
            .selectable_label(cx.state.node == TreeNode::Uploads, "Uploads")
            .on_hover_text("Everything uploaded from this app. Always available offline.")
            .clicked()
        {
            cx.state.node = TreeNode::Uploads;
            cx.state.selected.clear();
        }
    });

    ui.add_space(8.0);
    ui.strong("Inventories");
    ui.indent("inventory_group", |ui| {
        let Some(user_id) = cx.state.acting_user_id else {
            ui.weak("No account selected");
            return;
        };

        let mut nodes: Vec<(TreeNode, String)> = Vec::new();
        let self_label = cx
            .accounts
            .iter()
            .find(|a| a.user_id == user_id)
            .map(|a| account_label(a, cx.accounts, cx.anonymize))
            .unwrap_or_else(|| format!("User {user_id}"));
        nodes.push((TreeNode::Inventory(Creator::User(user_id)), self_label));
        for group in cx.groups {
            nodes.push((
                TreeNode::Inventory(Creator::Group(group.group_id)),
                group.name.clone(),
            ));
        }

        for (node, label) in nodes {
            if ui
                .selectable_label(cx.state.node == node, label)
                .clicked()
            {
                cx.state.node = node;
                cx.state.selected.clear();
                action = Some(AssetManagerAction::LoadInventory { node, filter });
            }
        }
        if cx.groups.is_empty() {
            ui.label(
                egui::RichText::new("No groups found for this account.")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
    });

    action
}

/// What a right-clicked asset offers. Built for whichever surface was clicked,
/// so one menu implementation serves both tables and the icon views.
struct MenuTarget<'a> {
    /// Selection key: the local row ID, or the asset ID for remote rows.
    key: &'a str,
    asset_id: Option<u64>,
    name: &'a str,
    /// Local index rows only. `None` for remote listings.
    file_path: Option<&'a std::path::Path>,
    /// Whether "Remove from list" applies. Local index rows only.
    removable: bool,
}

/// Draw the asset context menu on `response`.
///
/// Clipboard and browser actions are performed here, since both only need the
/// egui context. Anything that mutates app state is returned for the caller to
/// handle after the borrow ends.
fn asset_context_menu(
    response: &egui::Response,
    target: &MenuTarget<'_>,
    selection: &mut HashSet<String>,
    selected_count: usize,
) -> Option<AssetManagerAction> {
    // Right-clicking something outside the selection selects it first, so the
    // menu never acts on a different asset than the one under the cursor.
    // Right-clicking inside a multi-selection keeps it, matching Explorer.
    if response.secondary_clicked() && !selection.contains(target.key) {
        selection.clear();
        selection.insert(target.key.to_string());
    }

    let mut action = None;
    response.context_menu(|ui| {
        ui.set_min_width(190.0);

        if let Some(asset_id) = target.asset_id {
            if ui.button("Copy asset ID").clicked() {
                ui.ctx().copy_text(asset_id.to_string());
                ui.close_menu();
            }
            if ui
                .button("Copy rbxassetid://")
                .on_hover_text("The form that goes into Luau")
                .clicked()
            {
                ui.ctx().copy_text(format!("rbxassetid://{asset_id}"));
                ui.close_menu();
            }
        }
        if ui.button("Copy name").clicked() {
            ui.ctx().copy_text(target.name.to_string());
            ui.close_menu();
        }

        if let Some(asset_id) = target.asset_id {
            ui.separator();
            if ui.button("Open on Roblox").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
                    "https://www.roblox.com/library/{asset_id}"
                )));
                ui.close_menu();
            }
            ui.separator();
            let label = if selected_count > 1 {
                format!("Grant access to... ({selected_count} selected)")
            } else {
                "Grant access to...".to_string()
            };
            if ui
                .button(label)
                .on_hover_text("Let an experience use this asset")
                .clicked()
            {
                action = Some(AssetManagerAction::OpenGrantDialog);
                ui.close_menu();
            }
        }

        if let Some(path) = target.file_path {
            ui.separator();
            if ui.button("Copy file path").clicked() {
                ui.ctx().copy_text(path.to_string_lossy().into_owned());
                ui.close_menu();
            }
            if ui
                .add_enabled(path.exists(), egui::Button::new("Show in Explorer"))
                .on_disabled_hover_text("The file is no longer at that path")
                .clicked()
            {
                action = Some(AssetManagerAction::RevealFile(path.to_path_buf()));
                ui.close_menu();
            }
        }

        if target.removable {
            ui.separator();
            if ui
                .button("Remove from list")
                .on_hover_text(
                    "Removes it from this app's list only. The asset stays on Roblox: \
                     uploads cannot be deleted.",
                )
                .clicked()
            {
                action = Some(AssetManagerAction::RemoveRow(target.key.to_string()));
                ui.close_menu();
            }
        }
    });
    action
}

/// One entry in an icon view, from either the local index or a remote listing.
struct IconCell {
    /// Stable per-row key for egui IDs and selection.
    key: String,
    asset_id: Option<u64>,
    name: String,
    kind: AssetKind,
    /// Local index rows only, for the file actions in the context menu.
    file_path: Option<std::path::PathBuf>,
    removable: bool,
}

/// How many tiles fit across `usable` width.
///
/// N tiles need N widths plus N-1 gaps, which is why the spacing is added to
/// both sides of the division rather than only the denominator. Never returns
/// zero: the caller divides the tile count by it.
fn grid_columns(usable: f32, cell_width: f32, spacing: f32) -> usize {
    if !usable.is_finite() || cell_width <= 0.0 {
        return 1;
    }
    let fits = ((usable + spacing) / (cell_width + spacing)).floor();
    if fits.is_finite() && fits >= 1.0 {
        fits as usize
    } else {
        1
    }
}

/// Draw cells as icons, either flowing list rows or a wrapping grid.
///
/// Shared by the library and inventory views so the two never drift apart.
/// Returns the key of a clicked cell.
fn icon_view(
    ui: &mut egui::Ui,
    cells: &[IconCell],
    mode: ViewMode,
    selected: &mut HashSet<String>,
    thumbnails: &HashMap<u64, Vec<u8>>,
    result: &mut AssetManagerResult,
) -> Option<String> {
    let icon = mode.icon_px().unwrap_or(48.0);
    let grid = mode.is_grid();
    // Grid tiles are square-ish with room for a caption; list rows are wide and
    // short. Both are fixed so wrapping is predictable.
    let cell = if grid {
        egui::vec2(icon + 24.0, icon + 34.0)
    } else {
        egui::vec2(230.0, icon + 8.0)
    };

    const SPACING: f32 = 6.0;

    // Columns are computed rather than left to `horizontal_wrapped`. Each tile
    // is drawn inside a `push_id` child that claims the full available width,
    // so the wrapping layout never saw a reason to wrap and the tiles ran off
    // the right edge. Explicit columns also make the grid virtualizable.
    //
    // The scrollbar's width has to come off the top, otherwise the last column
    // is computed to fit and then clipped once the bar appears.
    let scrollbar = if ui.spacing().scroll.floating {
        0.0
    } else {
        ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin
    };
    let columns = grid_columns(ui.available_width() - scrollbar, cell.x, SPACING);
    let rows = cells.len().div_ceil(columns);

    // Set before `show_rows`, not inside its closure: it reads `item_spacing`
    // up front to compute the row pitch, so changing it later would drift the
    // scroll position away from what is actually drawn.
    ui.spacing_mut().item_spacing = egui::vec2(SPACING, SPACING);

    let mut clicked = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, cell.y, rows, |ui, row_range| {
            for row in row_range {
                let start = row * columns;
                let end = (start + columns).min(cells.len());
                ui.horizontal(|ui| {
                    for entry in &cells[start..end] {
                        // Requested only for tiles actually on screen, which is
                        // the point of virtualizing: scrolling a large
                        // inventory does not ask for every thumbnail at once.
                        if let Some(asset_id) = entry.asset_id {
                            if !thumbnails.contains_key(&asset_id) {
                                result.want_thumbnails.push(asset_id);
                            }
                        }
                        let is_selected = selected.contains(&entry.key);
                        let response = ui
                            .push_id(&entry.key, |ui| {
                                draw_cell(ui, entry, cell, icon, grid, is_selected, thumbnails)
                            })
                            .inner;
                        if response.clicked() {
                            clicked = Some(entry.key.clone());
                        }
                        let count = selected.len();
                        let menu = asset_context_menu(
                            &response,
                            &MenuTarget {
                                key: &entry.key,
                                asset_id: entry.asset_id,
                                name: &entry.name,
                                file_path: entry.file_path.as_deref(),
                                removable: entry.removable,
                            },
                            selected,
                            count,
                        );
                        if menu.is_some() {
                            result.action = menu;
                        }
                    }
                });
            }
        });
    clicked
}

fn draw_cell(
    ui: &mut egui::Ui,
    entry: &IconCell,
    cell: egui::Vec2,
    icon: f32,
    grid: bool,
    selected: bool,
    thumbnails: &HashMap<u64, Vec<u8>>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(cell, egui::Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
    }

    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(4.0))
            .layout(if grid {
                egui::Layout::top_down(egui::Align::Center)
            } else {
                egui::Layout::left_to_right(egui::Align::Center)
            }),
    );

    let image_rect = egui::vec2(icon, icon);
    match entry.asset_id.and_then(|id| thumbnails.get(&id)) {
        Some(bytes) => {
            // Keyed on the asset ID so egui's texture cache does not serve one
            // asset's image for another.
            let uri = format!("bytes://asset_thumb_{}", entry.asset_id.unwrap_or(0));
            content.add(
                egui::Image::from_bytes(uri, bytes.clone())
                    .fit_to_exact_size(image_rect)
                    .rounding(3.0),
            );
        }
        None => {
            // Placeholder rather than blank, so layout does not jump when the
            // real thumbnail lands.
            let (ph, _) = content.allocate_exact_size(image_rect, egui::Sense::hover());
            content
                .painter()
                .rect_filled(ph, 3.0, content.visuals().extreme_bg_color);
            if icon >= 48.0 {
                content.painter().text(
                    ph.center(),
                    egui::Align2::CENTER_CENTER,
                    kind_abbreviation(entry.kind),
                    egui::FontId::proportional((icon * 0.22).clamp(9.0, 16.0)),
                    content.visuals().weak_text_color(),
                );
            }
        }
    }

    if !grid {
        content.add_space(6.0);
    }
    content.add(
        egui::Label::new(egui::RichText::new(&entry.name).small())
            .truncate()
            .selectable(false),
    );

    response.on_hover_text(match entry.asset_id {
        Some(id) => format!("{}\n{} - {id}", entry.name, entry.kind.as_api_str()),
        None => format!("{}\n{}", entry.name, entry.kind.as_api_str()),
    })
}

/// Short tag drawn on a tile that has no thumbnail yet.
fn kind_abbreviation(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Decal => "IMG",
        AssetKind::Audio => "AUD",
        AssetKind::Model => "MDL",
        AssetKind::Animation => "ANM",
        AssetKind::Video => "VID",
        AssetKind::Other => "?",
    }
}

/// A creator's inventory as reported by Roblox.
fn inventory_view(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>, result: &mut AssetManagerResult) {
    let filter = cx.state.type_filter;

    ui.horizontal(|ui| {
        match filter {
            Some(kind) => ui.label(format!("Showing {} assets.", kind.as_api_str())),
            None => ui.label("Showing every asset type."),
        };
        if cx.remote.loading() {
            ui.spinner();
        }
        if ui.small_button("Reload").clicked() {
            result.action = Some(AssetManagerAction::LoadInventory {
                node: cx.state.node,
                filter,
            });
        }
    });
    ui.add_space(4.0);

    // Whatever is loaded does not answer the current question, so ask it.
    if !cx.remote.matches(cx.state.node, filter) {
        if !cx.remote.loading() {
            result.action = Some(AssetManagerAction::LoadInventory {
                node: cx.state.node,
                filter,
            });
        }
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading from Roblox...");
        });
        return;
    }
    if cx.remote.loading() && cx.remote.items.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading from Roblox...");
        });
        return;
    }
    if let Some(error) = &cx.remote.error {
        // Show Roblox's actual reason, not a generic one. The common case by
        // far is a group the account has no role in, and telling the user
        // "could not load" sends them looking for a fault that is not there.
        ui.colored_label(ui.theme().danger, error);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Recent > Uploads still works, and needs no network.")
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        return;
    }

    let needle = cx.state.search.trim().to_lowercase();
    let ascending = cx.state.sort_ascending;
    let mut clicked_sort: Option<SortKey> = None;
    let mut items: Vec<&CreationItem> = cx
        .remote
        .items
        .iter()
        .filter(|item| {
            needle.is_empty()
                || item.name.to_lowercase().contains(&needle)
                || item.asset_id.to_string().contains(&needle)
        })
        .collect();

    // A fan-out arrives grouped by kind, in whatever order the requests
    // finished, so this is not cosmetic: without it the merged list reads as
    // several concatenated lists.
    //
    // Creator is not a sortable key here (every row shares one creator), so it
    // falls back to date. `then_with` on the ID keeps equal keys in a stable
    // order, otherwise rows visibly swap places between frames.
    let key = match cx.state.sort {
        SortKey::CreatorName => SortKey::Date,
        other => other,
    };
    items.sort_by(|a, b| {
        let ordering = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Id => a.asset_id.cmp(&b.asset_id),
            SortKey::Kind => a.kind.as_api_str().cmp(b.kind.as_api_str()),
            // Undated rows (the details call failed for them) sort as oldest,
            // so they cluster at one end instead of scattering.
            SortKey::Date | SortKey::CreatorName => a.updated.cmp(&b.updated),
        };
        let ordering = ordering.then_with(|| a.asset_id.cmp(&b.asset_id));
        if cx.state.sort_ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });

    if items.is_empty() {
        let body = match filter {
            Some(kind) => format!("This creator has no {} assets.", kind.as_api_str()),
            None => "This creator has no assets.".to_string(),
        };
        empty_state(ui, "Nothing here", &body);
        return;
    }

    let shown = items.len();

    if cx.state.view_mode != ViewMode::Details {
        let cells: Vec<IconCell> = items
            .iter()
            .map(|i| IconCell {
                key: i.asset_id.to_string(),
                asset_id: Some(i.asset_id),
                name: i.name.clone(),
                kind: i.kind,
                // Remote rows have no local file and are not ours to remove.
                file_path: None,
                removable: false,
            })
            .collect();
        let clicked = icon_view(
            ui,
            &cells,
            cx.state.view_mode,
            &mut cx.state.selected,
            cx.thumbnails,
            result,
        );
        if let Some(key) = clicked {
            apply_icon_click(ui, &mut cx.state.selected, key);
        }
        inventory_footer(ui, cx, shown, result);
        return;
    }

    let mut selection = cx.state.selected.clone();
    let selection_count = selection.len();
    let mut menu_action: Option<AssetManagerAction> = None;
    let mut clicked_row: Option<String> = None;
    let row_selected = selection.clone();

    TableBuilder::new(ui)
        .id_salt("asset_inventory_table")
        .striped(true)
        .resizable(true)
        // Needed for both row selection and the context menu.
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(280.0).at_least(120.0).clip(true))
        .column(Column::initial(160.0).at_least(90.0))
        .column(Column::initial(90.0))
        .column(Column::remainder().at_least(120.0))
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .header(22.0, |mut header| {
            for (label, col_key) in [
                ("Name", SortKey::Name),
                ("ID", SortKey::Id),
                ("Type", SortKey::Kind),
                ("Date Modified", SortKey::Date),
            ] {
                header.col(|ui| {
                    if sort_header(ui, label, col_key, key, ascending) {
                        clicked_sort = Some(col_key);
                    }
                });
            }
        })
        .body(|body| {
            body.rows(24.0, items.len(), |mut row| {
                let item = items[row.index()];
                let key = item.asset_id.to_string();
                row.set_selected(row_selected.contains(&key));
                row.col(|ui| {
                    ui.label(&item.name).on_hover_text(&item.name);
                });
                row.col(|ui| {
                    let id = item.asset_id.to_string();
                    if ui
                        .link(&id)
                        .on_hover_text("Click to copy the asset ID")
                        .clicked()
                    {
                        ui.ctx().copy_text(id);
                    }
                });
                row.col(|ui| {
                    ui.label(item.kind.as_api_str());
                });
                row.col(|ui| {
                    ui.label(item.updated.map(format_date).unwrap_or_else(|| "-".into()));
                });
                let response = row.response();
                if response.clicked() {
                    clicked_row = Some(key.clone());
                }
                let menu = asset_context_menu(
                    &response,
                    &MenuTarget {
                        key: &key,
                        asset_id: Some(item.asset_id),
                        name: &item.name,
                        file_path: None,
                        removable: false,
                    },
                    &mut selection,
                    selection_count,
                );
                if menu.is_some() {
                    menu_action = menu;
                }
            });
        });

    cx.state.selected = selection;
    if menu_action.is_some() {
        result.action = menu_action;
    }
    if let Some(key) = clicked_row {
        apply_icon_click(ui, &mut cx.state.selected, key);
    }

    if let Some(key) = clicked_sort {
        apply_sort_click(cx.state, key);
    }
    inventory_footer(ui, cx, shown, result);
}

/// Row count plus paging, shared by the table and icon layouts.
fn inventory_footer(
    ui: &mut egui::Ui,
    cx: &AssetsCtx<'_>,
    shown: usize,
    result: &mut AssetManagerResult,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{shown} shown"))
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        if cx.remote.has_more() {
            if cx.remote.loading() {
                ui.spinner();
                ui.label("Loading more...");
            } else if ui.button("Load more").clicked() {
                result.action = Some(AssetManagerAction::LoadMoreInventory);
            }
        }
    });
}

/// Bulk actions over the library selection. Only drawn when something is
/// selected, so it never eats space it has no use for.
fn selection_footer(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>, result: &mut AssetManagerResult) {
    if cx.state.selected.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("{} selected", cx.state.selected.len()));
        if ui
            .button("Grant access to...")
            .on_hover_text("Let an experience use these assets")
            .clicked()
        {
            result.action = Some(AssetManagerAction::OpenGrantDialog);
        }
        if ui.button("Clear selection").clicked() {
            cx.state.selected.clear();
        }
    });
}

/// The universe picker, shared by the grant modal and the import queue.
/// Returns true when the selection changed.
pub fn universe_picker(
    ui: &mut egui::Ui,
    id_salt: &str,
    universes: &[UniverseTarget],
    selected: &mut Option<u64>,
) -> bool {
    let label = match selected {
        None => "None".to_string(),
        Some(id) => universes
            .iter()
            .find(|u| u.universe_id == *id)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| format!("Universe {id}")),
    };
    let before = *selected;
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(label)
        .width(200.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, "None");
            for universe in universes {
                ui.selectable_value(selected, Some(universe.universe_id), &universe.name);
            }
        });
    before != *selected
}

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

fn toolbar(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>, result: &mut AssetManagerResult) {
    ui.horizontal(|ui| {
        ui.label("Account:");
        let selected_label = cx
            .state
            .acting_user_id
            .and_then(|id| cx.accounts.iter().find(|a| a.user_id == id))
            .map(|a| account_label(a, cx.accounts, cx.anonymize))
            .unwrap_or_else(|| "Select an account".to_string());

        egui::ComboBox::from_id_salt("asset_manager_account")
            .selected_text(selected_label)
            .width(180.0)
            .show_ui(ui, |ui| {
                for account in cx.accounts {
                    let label = account_label(account, cx.accounts, cx.anonymize);
                    ui.selectable_value(
                        &mut cx.state.acting_user_id,
                        Some(account.user_id),
                        label,
                    );
                }
            });

        ui.separator();
        ui.selectable_value(&mut cx.state.view, View::Library, "Library");
        let queued = cx
            .index
            .records
            .iter()
            .filter(|r| !r.state.is_terminal())
            .count();
        let queue_label = if queued > 0 {
            format!("Import Queue ({queued})")
        } else {
            "Import Queue".to_string()
        };
        ui.selectable_value(&mut cx.state.view, View::ImportQueue, queue_label);

        ui.separator();
        ui.label("\u{1f50d}");
        ui.add(
            egui::TextEdit::singleline(&mut cx.state.search)
                .desired_width(160.0)
                .hint_text("Search"),
        );

        egui::ComboBox::from_id_salt("asset_view_mode")
            .selected_text(cx.state.view_mode.label())
            .width(110.0)
            .show_ui(ui, |ui| {
                for mode in ViewMode::all() {
                    ui.selectable_value(&mut cx.state.view_mode, *mode, mode.label());
                }
            });

        egui::ComboBox::from_id_salt("asset_type_filter")
            .selected_text(match cx.state.type_filter {
                None => "All types",
                Some(kind) => kind.as_api_str(),
            })
            .width(110.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut cx.state.type_filter, None, "All types");
                for kind in AssetKind::selectable() {
                    ui.selectable_value(&mut cx.state.type_filter, Some(*kind), kind.as_api_str());
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let import = egui::Button::new(
                egui::RichText::new("Import").color(ui.theme().on_accent),
            )
            .fill(ui.visuals().selection.bg_fill);
            if ui
                .add(import)
                .on_hover_text("Add files to the import queue")
                .clicked()
            {
                cx.state.view = View::ImportQueue;
                result.action = Some(AssetManagerAction::PickFiles);
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

fn library_view(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>, result: &mut AssetManagerResult) {
    let rows = visible_rows(cx, |state| matches!(state, AssetState::Approved { .. }));

    if rows.is_empty() {
        empty_state(
            ui,
            "Nothing uploaded yet",
            "Assets you upload from this app appear here once they clear moderation.",
        );
        return;
    }

    if cx.state.view_mode != ViewMode::Details {
        let cells: Vec<IconCell> = rows
            .iter()
            .map(|r| IconCell {
                key: r.row_id.clone(),
                asset_id: r.state.asset_id(),
                name: r.name.clone(),
                kind: r.kind,
                file_path: Some(std::path::PathBuf::from(&r.path)),
                removable: true,
            })
            .collect();
        if let Some(key) = icon_view(
            ui,
            &cells,
            cx.state.view_mode,
            &mut cx.state.selected,
            cx.thumbnails,
            result,
        ) {
            apply_icon_click(ui, &mut cx.state.selected, key);
        }
        return;
    }

    let headers = [
        ("Name", SortKey::Name),
        ("ID", SortKey::Id),
        ("Type", SortKey::Kind),
        ("Date Modified", SortKey::Date),
        ("Creator", SortKey::CreatorName),
    ];

    let mut clicked_sort: Option<SortKey> = None;
    let (sort, ascending) = (cx.state.sort, cx.state.sort_ascending);
    let mut clicked_row: Option<String> = None;
    // Cloned so the table closures can read and update selection without
    // holding a borrow of `cx` across them; merged back once drawing is done.
    let mut selection = cx.state.selected.clone();
    let selected = selection.clone();
    let selection_count = selection.len();
    let mut menu_action: Option<AssetManagerAction> = None;
    let accounts = cx.accounts;
    let anonymize = cx.anonymize;

    TableBuilder::new(ui)
        .id_salt("asset_library_table")
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(240.0).at_least(120.0).clip(true))
        .column(Column::initial(150.0).at_least(90.0))
        .column(Column::initial(90.0))
        .column(Column::initial(170.0))
        .column(Column::remainder().at_least(120.0).clip(true))
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .header(22.0, |mut header| {
            for (label, key) in headers {
                header.col(|ui| {
                    if sort_header(ui, label, key, sort, ascending) {
                        clicked_sort = Some(key);
                    }
                });
            }
        })
        .body(|body| {
            // Virtualized: only visible rows run their closure, so a large
            // inventory stays smooth.
            body.rows(24.0, rows.len(), |mut row| {
                let record = &rows[row.index()];
                row.set_selected(selected.contains(&record.row_id));
                row.col(|ui| {
                    ui.label(&record.name).on_hover_text(&record.name);
                });
                row.col(|ui| {
                    let id = record
                        .state
                        .asset_id()
                        .map(|id| id.to_string())
                        .unwrap_or_default();
                    if ui
                        .link(&id)
                        .on_hover_text("Click to copy the asset ID")
                        .clicked()
                    {
                        ui.ctx().copy_text(id);
                    }
                });
                row.col(|ui| {
                    ui.label(record.kind.as_api_str());
                });
                row.col(|ui| {
                    ui.label(format_date(record.created_at));
                });
                row.col(|ui| {
                    ui.label(creator_label(record.creator, accounts, anonymize));
                });
                let response = row.response();
                if response.clicked() {
                    clicked_row = Some(record.row_id.clone());
                }
                let menu = asset_context_menu(
                    &response,
                    &MenuTarget {
                        key: &record.row_id,
                        asset_id: record.state.asset_id(),
                        name: &record.name,
                        file_path: Some(std::path::Path::new(&record.path)),
                        removable: true,
                    },
                    &mut selection,
                    selection_count,
                );
                if menu.is_some() {
                    menu_action = menu;
                }
            });
        });

    // A right-click may have changed the selection inside the table closures.
    cx.state.selected = selection;
    if menu_action.is_some() {
        result.action = menu_action;
    }
    if let Some(key) = clicked_sort {
        apply_sort_click(cx.state, key);
    }
    if let Some(row_id) = clicked_row {
        // Same helper as the icon views, so selection behaves identically
        // whichever layout you happen to be in.
        apply_icon_click(ui, &mut cx.state.selected, row_id);
    }
}

// ---------------------------------------------------------------------------
// Import queue
// ---------------------------------------------------------------------------

fn queue_view(ui: &mut egui::Ui, cx: &mut AssetsCtx<'_>, result: &mut AssetManagerResult) {
    let rows = visible_rows(cx, |state| !matches!(state, AssetState::Approved { .. }));

    queue_actions(ui, cx, &rows, result);
    ui.add_space(6.0);

    if rows.is_empty() {
        empty_state(
            ui,
            "The import queue is empty",
            "Use Import, or drop files anywhere on this window.",
        );
        return;
    }

    let mut edits: Vec<Edit> = Vec::new();
    let mut toggled: Option<String> = None;
    let mut removed: Option<String> = None;
    let mut retried: Option<String> = None;
    let checked = cx.state.checked.clone();
    let creator_options = creator_options(cx);

    TableBuilder::new(ui)
        .id_salt("asset_queue_table")
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(24.0))
        .column(Column::initial(220.0).at_least(120.0).clip(true))
        .column(Column::initial(190.0))
        .column(Column::initial(100.0))
        .column(Column::remainder().at_least(140.0).clip(true))
        .column(Column::initial(200.0))
        .column(Column::exact(28.0))
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .header(22.0, |mut header| {
            for label in ["", "Asset", "Creator", "Type", "File Path", "Status", ""] {
                header.col(|ui| {
                    ui.strong(label);
                });
            }
        })
        .body(|body| {
            body.rows(26.0, rows.len(), |mut row| {
                let record = &rows[row.index()];
                let editable = matches!(
                    record.state,
                    AssetState::Queued | AssetState::Failed { .. } | AssetState::Invalid { .. }
                );

                row.col(|ui| {
                    let mut is_checked = checked.contains(&record.row_id);
                    if ui
                        .add_enabled(editable, egui::Checkbox::without_text(&mut is_checked))
                        .changed()
                    {
                        toggled = Some(record.row_id.clone());
                    }
                });
                row.col(|ui| {
                    let mut name = record.name.clone();
                    if ui
                        .add_enabled(
                            editable,
                            egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        edits.push(Edit::Name(record.row_id.clone(), name));
                    }
                });
                row.col(|ui| {
                    let mut creator = record.creator;
                    egui::ComboBox::from_id_salt(("queue_creator", &record.row_id))
                        .selected_text(creator_label_from(&creator_options, creator))
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (option, label) in &creator_options {
                                ui.selectable_value(&mut creator, *option, label);
                            }
                        });
                    if editable && creator != record.creator {
                        edits.push(Edit::Creator(record.row_id.clone(), creator));
                    }
                });
                row.col(|ui| {
                    let mut kind = record.kind;
                    egui::ComboBox::from_id_salt(("queue_kind", &record.row_id))
                        .selected_text(kind.as_api_str())
                        .width(85.0)
                        .show_ui(ui, |ui| {
                            for option in AssetKind::selectable() {
                                ui.selectable_value(&mut kind, *option, option.as_api_str());
                            }
                        });
                    if editable && kind != record.kind {
                        edits.push(Edit::Kind(record.row_id.clone(), kind));
                    }
                });
                row.col(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&record.path).monospace())
                            .truncate(),
                    )
                    .on_hover_text(&record.path);
                });
                row.col(|ui| {
                    if status_cell(ui, record) {
                        retried = Some(record.row_id.clone());
                    }
                });
                row.col(|ui| {
                    if ui
                        .add_enabled(
                            !record.state.is_active(),
                            egui::Button::new("\u{1f5d1}").frame(false),
                        )
                        .on_hover_text("Remove from the queue")
                        .clicked()
                    {
                        removed = Some(record.row_id.clone());
                    }
                });
            });
        });

    for edit in edits {
        let changed = match edit {
            Edit::Name(row_id, name) => cx
                .index
                .get_mut(&row_id)
                .map(|r| r.display_name = name)
                .is_some(),
            Edit::Kind(row_id, kind) => {
                cx.index.get_mut(&row_id).map(|r| r.kind = kind).is_some()
            }
            Edit::Creator(row_id, creator) => cx
                .index
                .get_mut(&row_id)
                .map(|r| r.creator = creator)
                .is_some(),
        };
        result.index_changed |= changed;
    }
    if let Some(row_id) = toggled {
        if !cx.state.checked.remove(&row_id) {
            cx.state.checked.insert(row_id);
        }
    }
    if let Some(row_id) = removed {
        result.action = Some(AssetManagerAction::RemoveRow(row_id));
    }
    if let Some(row_id) = retried {
        result.action = Some(AssetManagerAction::RetryRow(row_id));
    }
}

fn queue_actions(
    ui: &mut egui::Ui,
    cx: &mut AssetsCtx<'_>,
    rows: &[RowView],
    result: &mut AssetManagerResult,
) {
    // Every failure is offered, not just the ones classified retryable, for the
    // same reason the per-row Retry button is: the classification is a guess
    // about Roblox's mood, and being wrong about it must not strand a row.
    let uploadable: Vec<String> = rows
        .iter()
        .filter(|r| {
            cx.state.checked.contains(&r.row_id)
                && matches!(r.state, AssetState::Queued | AssetState::Failed { .. })
        })
        .map(|r| r.row_id.clone())
        .collect();

    ui.horizontal(|ui| {
        if ui.button("Add files...").clicked() {
            result.action = Some(AssetManagerAction::PickFiles);
        }
        if ui
            .button("Clear finished")
            .on_hover_text("Remove uploaded, rejected and failed rows from the queue")
            .clicked()
        {
            result.action = Some(AssetManagerAction::ClearFinished);
        }

        ui.separator();
        ui.label("New files upload as:");
        let options = creator_options(cx);
        let mut batch = cx.state.batch_creator.or_else(|| options.first().map(|o| o.0));
        egui::ComboBox::from_id_salt("asset_batch_creator")
            .selected_text(
                batch
                    .map(|c| creator_label_from(&options, c))
                    .unwrap_or_else(|| "Select".to_string()),
            )
            .width(180.0)
            .show_ui(ui, |ui| {
                for (option, label) in &options {
                    ui.selectable_value(&mut batch, Some(*option), label);
                }
            });
        cx.state.batch_creator = batch;

        ui.separator();
        ui.label("Grant access to:")
            .on_hover_text("Assets in this batch are granted Use on this experience once they clear moderation.");
        universe_picker(
            ui,
            "asset_auto_grant",
            cx.universes,
            &mut cx.state.auto_grant_universe,
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let enabled = !uploadable.is_empty() && cx.unlocked && !cx.read_only;
            let label = if uploadable.is_empty() {
                "Upload".to_string()
            } else {
                format!("Upload {}", uploadable.len())
            };
            let button =
                egui::Button::new(egui::RichText::new(label).color(ui.theme().on_accent))
                    .fill(ui.visuals().selection.bg_fill);
            let response = ui.add_enabled(enabled, button);
            let response = if !cx.unlocked {
                response.on_disabled_hover_text(
                    "Unlock the account store first, so cookies can be decrypted.",
                )
            } else {
                response
            };
            if response.clicked() {
                result.action = Some(AssetManagerAction::RequestUpload(uploadable.clone()));
            }
        });
    });
}

/// Draw a row's status. Returns `true` when the user asked to retry it.
fn status_cell(ui: &mut egui::Ui, row: &RowView) -> bool {
    // Not a uniform choice. Green and blue take the `_text` tokens, the
    // variants meant to be read on a dark surface. Amber and red take the
    // fill-strength `warning` and `danger`, which is what these two labels
    // have been drawn in since before the theme existed; the conversion kept
    // the shipped values rather than restyling the table.
    let theme = ui.theme();
    let amber = theme.warning;
    let red = theme.danger;
    let green = theme.success_text;
    let blue = theme.accent_text;
    let mut retry = false;

    match &row.state {
        AssetState::Queued => match row.retry_at {
            // A row that failed and is waiting out its backoff. Saying "Ready"
            // here reads as stuck, because nothing appears to happen for the
            // next minute or two.
            Some(at) => {
                ui.spinner();
                ui.colored_label(amber, format!("Retrying {}", format_countdown(at)))
                    .on_hover_text(format!("Attempt {} starts shortly", row.attempts + 1));
            }
            None => {
                ui.weak("Ready");
            }
        },
        AssetState::Invalid { reason } => {
            ui.colored_label(red, "Not supported").on_hover_text(reason);
        }
        AssetState::Duplicate { asset_id } => {
            ui.colored_label(amber, "Already uploaded")
                .on_hover_text(format!("Uploaded before as {asset_id}"));
        }
        AssetState::Uploading => {
            ui.spinner();
            ui.label("Uploading");
        }
        AssetState::Pending { since, .. } => {
            ui.spinner();
            ui.label(format!("Processing {}", format_elapsed(*since)))
                .on_hover_text("Roblox is still ingesting the file.");
        }
        AssetState::InReview {
            asset_id, since, ..
        } => {
            ui.spinner();
            ui.colored_label(blue, format!("In review {}", format_elapsed(*since)))
                .on_hover_text(format!(
                    "Uploaded as {asset_id}. Roblox has not published a moderation verdict yet, so the asset may not be usable."
                ));
        }
        AssetState::Approved { asset_id, .. } => {
            let id = asset_id.to_string();
            if ui
                .colored_label(green, &id)
                .on_hover_text("Approved. Click to copy the asset ID")
                .clicked()
            {
                ui.ctx().copy_text(id);
            }
        }
        AssetState::Rejected { reason } => {
            ui.colored_label(red, "Rejected").on_hover_text(reason);
        }
        AssetState::Failed { message, retryable } => {
            let color = if *retryable { amber } else { red };
            ui.colored_label(color, "Failed").on_hover_text(message);
            // Offered whatever the classification. `retryable` decides whether
            // the app re-sends on its own; it must not decide whether the user
            // is allowed to, because a misclassified failure otherwise leaves
            // the row with no way forward but deleting and re-adding the file.
            if ui
                .small_button("Retry")
                .on_hover_text(if *retryable {
                    "Send this file again"
                } else {
                    "Send this file again. Roblox called this permanent, so it will probably fail the same way."
                })
                .clicked()
            {
                retry = true;
            }
        }
        AssetState::Expired { .. } => {
            ui.weak("Timed out").on_hover_text(
                "Roblox stopped reporting on this upload. It may still have been published.",
            );
        }
        AssetState::Cancelled => {
            ui.weak("Cancelled");
        }
    }
    retry
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Point `acting_user_id` at a usable account when it is unset or stale. Called
/// every frame: the store can load, change, or lose accounts at any point, and
/// a dangling ID would leave the picker blank.
fn reconcile_acting_account(cx: &mut AssetsCtx<'_>) {
    let still_valid = cx
        .state
        .acting_user_id
        .is_some_and(|id| cx.accounts.iter().any(|a| a.user_id == id));
    if still_valid {
        return;
    }
    cx.state.acting_user_id = cx
        .accounts
        .iter()
        .find(|a| !a.cookie_expired)
        .or_else(|| cx.accounts.first())
        .map(|a| a.user_id);
    // The old creator belonged to the old account.
    cx.state.batch_creator = None;
}

/// Rows passing the current search, type filter and view predicate, sorted.
fn visible_rows(cx: &AssetsCtx<'_>, keep: impl Fn(&AssetState) -> bool) -> Vec<RowView> {
    let needle = cx.state.search.trim().to_lowercase();
    let mut rows: Vec<RowView> = cx
        .index
        .records
        .iter()
        .filter(|r| keep(&r.state))
        .filter(|r| cx.state.type_filter.is_none_or(|k| k == r.kind))
        .filter(|r| {
            if needle.is_empty() {
                return true;
            }
            r.display_name.to_lowercase().contains(&needle)
                || r.state
                    .asset_id()
                    .is_some_and(|id| id.to_string().contains(&needle))
        })
        .map(|r| RowView {
            row_id: r.row_id.clone(),
            name: r.display_name.clone(),
            kind: r.kind,
            creator: r.creator,
            path: r.file_path.to_string_lossy().into_owned(),
            state: r.state.clone(),
            created_at: r.updated_at.unwrap_or(r.created_at),
            attempts: r.attempts,
            retry_at: r.retry_at,
        })
        .collect();

    // `then_with` on row_id keeps equal keys in a stable order. Without it rows
    // with the same timestamp visibly swap places between frames.
    rows.sort_by(|a, b| {
        let ordering = match cx.state.sort {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Id => a.state.asset_id().cmp(&b.state.asset_id()),
            SortKey::Kind => a.kind.as_api_str().cmp(b.kind.as_api_str()),
            SortKey::Date => a.created_at.cmp(&b.created_at),
            SortKey::CreatorName => a.creator.id().cmp(&b.creator.id()),
        };
        let ordering = ordering.then_with(|| a.row_id.cmp(&b.row_id));
        if cx.state.sort_ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    rows
}

/// Creators the acting account can publish under: itself, plus every group it
/// belongs to. Group publish rights are not knowable from the group list, so
/// all are offered and a 403 at upload time is the answer.
fn creator_options(cx: &AssetsCtx<'_>) -> Vec<(Creator, String)> {
    let Some(user_id) = cx.state.acting_user_id else {
        return Vec::new();
    };
    let self_label = cx
        .accounts
        .iter()
        .find(|a| a.user_id == user_id)
        .map(|a| format!("Me ({})", account_label(a, cx.accounts, cx.anonymize)))
        .unwrap_or_else(|| "Me".to_string());

    let mut options = vec![(Creator::User(user_id), self_label)];
    options.extend(
        cx.groups
            .iter()
            .map(|g| (Creator::Group(g.group_id), g.name.clone())),
    );
    options
}

fn creator_label_from(options: &[(Creator, String)], creator: Creator) -> String {
    options
        .iter()
        .find(|(option, _)| *option == creator)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| match creator {
            Creator::User(id) => format!("User {id}"),
            Creator::Group(id) => format!("Group {id}"),
        })
}

fn creator_label(creator: Creator, accounts: &[Account], anonymize: bool) -> String {
    match creator {
        Creator::User(id) => accounts
            .iter()
            .find(|a| a.user_id == id)
            .map(|a| account_label(a, accounts, anonymize))
            .unwrap_or_else(|| format!("User {id}")),
        Creator::Group(id) => format!("Group {id}"),
    }
}

/// Display name for an account, honoring anonymize mode. The anonymized form is
/// positional ("Account 3") to match the sidebar.
fn account_label(account: &Account, accounts: &[Account], anonymize: bool) -> String {
    if !anonymize {
        return account.label().to_string();
    }
    let index = accounts
        .iter()
        .position(|a| a.user_id == account.user_id)
        .map_or(0, |i| i + 1);
    format!("Account {index}")
}

fn format_date(when: DateTime<Utc>) -> String {
    when.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn format_elapsed(since: DateTime<Utc>) -> String {
    let seconds = Utc::now().signed_duration_since(since).num_seconds().max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

/// Time until `at`, for a row waiting out a retry backoff. Clamped at zero: the
/// pump runs on a one-second tick, so "in 0s" is briefly truthful and a
/// negative countdown never is.
fn format_countdown(at: DateTime<Utc>) -> String {
    let seconds = at.signed_duration_since(Utc::now()).num_seconds().max(0);
    if seconds < 60 {
        format!("in {seconds}s")
    } else {
        format!("in {}m", seconds / 60)
    }
}

fn empty_state(ui: &mut egui::Ui, heading: &str, body: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(50.0);
        ui.heading(heading);
        ui.add_space(4.0);
        ui.label(body);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tiles that ran off the right edge instead of wrapping was a real bug.
    // These pin the arithmetic that replaced `horizontal_wrapped`.

    #[test]
    fn columns_account_for_gaps_between_tiles() {
        // 3 tiles of 100 with 2 gaps of 6 is 312. A naive width/(cell+spacing)
        // would say 2, leaving a column's worth of dead space.
        assert_eq!(grid_columns(312.0, 100.0, 6.0), 3);
        assert_eq!(grid_columns(311.0, 100.0, 6.0), 2);
    }

    #[test]
    fn an_exact_fit_is_not_off_by_one() {
        assert_eq!(grid_columns(100.0, 100.0, 6.0), 1);
        assert_eq!(grid_columns(206.0, 100.0, 6.0), 2);
    }

    #[test]
    fn never_returns_zero_columns() {
        // The caller does `len().div_ceil(columns)`, so zero would panic.
        assert_eq!(grid_columns(10.0, 100.0, 6.0), 1);
        assert_eq!(grid_columns(0.0, 100.0, 6.0), 1);
        assert_eq!(grid_columns(-50.0, 100.0, 6.0), 1);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        assert_eq!(grid_columns(f32::NAN, 100.0, 6.0), 1);
        assert_eq!(grid_columns(f32::INFINITY, 100.0, 6.0), 1);
        assert_eq!(grid_columns(500.0, 0.0, 6.0), 1);
    }

    #[test]
    fn wider_windows_fit_more_columns() {
        let narrow = grid_columns(400.0, 168.0, 6.0);
        let wide = grid_columns(1200.0, 168.0, 6.0);
        assert!(wide > narrow, "{wide} should exceed {narrow}");
    }

    #[test]
    fn larger_icons_fit_fewer_columns() {
        let small = grid_columns(1000.0, 72.0, 6.0);
        let large = grid_columns(1000.0, 168.0, 6.0);
        assert!(small > large, "{small} should exceed {large}");
    }
}
