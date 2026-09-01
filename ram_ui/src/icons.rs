//! SVG icon rasterization for egui.
//!
//! Lucide assets remain SVG files in the repository. egui receives the
//! rasterized pixels so the UI does not depend on Windows having a matching
//! emoji or SVG font installed.

use eframe::egui;
use std::collections::{HashMap, HashSet};

fn icon_cache_id() -> egui::Id {
    egui::Id::new("rm_svg_icon_cache")
}

#[derive(Clone, Default)]
struct IconCache {
    textures: HashMap<String, egui::TextureHandle>,
    warned_icons: HashSet<String>,
}

fn svg_bytes(name: &str) -> Option<&'static [u8]> {
    match name {
        "accounts" => Some(include_bytes!("../../assets/icons/accounts.svg")),
        "add" => Some(include_bytes!("../../assets/icons/add.svg")),
        "browser" => Some(include_bytes!("../../assets/icons/browser.svg")),
        "close" => Some(include_bytes!("../../assets/icons/close.svg")),
        "chevron-down" => Some(include_bytes!("../../assets/icons/chevron-down.svg")),
        "copy" => Some(include_bytes!("../../assets/icons/copy.svg")),
        "delete" => Some(include_bytes!("../../assets/icons/delete.svg")),
        "edit" => Some(include_bytes!("../../assets/icons/edit.svg")),
        "folder" => Some(include_bytes!("../../assets/icons/folder.svg")),
        "focus" => Some(include_bytes!("../../assets/icons/focus.svg")),
        "game" => Some(include_bytes!("../../assets/icons/game.svg")),
        "grid" => Some(include_bytes!("../../assets/icons/grid.svg")),
        "groups" => Some(include_bytes!("../../assets/icons/groups.svg")),
        "import" => Some(include_bytes!("../../assets/icons/import.svg")),
        "inventory" => Some(include_bytes!("../../assets/icons/inventory.svg")),
        "kill" => Some(include_bytes!("../../assets/icons/kill.svg")),
        "launch" => Some(include_bytes!("../../assets/icons/launch.svg")),
        "link" => Some(include_bytes!("../../assets/icons/link.svg")),
        "lock" => Some(include_bytes!("../../assets/icons/lock.svg")),
        "package" => Some(include_bytes!("../../assets/icons/package.svg")),
        "password" => Some(include_bytes!("../../assets/icons/password.svg")),
        "pin" => Some(include_bytes!("../../assets/icons/pin.svg")),
        "pinoff" => Some(include_bytes!("../../assets/icons/pin-off.svg")),
        "save" => Some(include_bytes!("../../assets/icons/save.svg")),
        "settings" => Some(include_bytes!("../../assets/icons/settings.svg")),
        "star" => Some(include_bytes!("../../assets/icons/star.svg")),
        "update" => Some(include_bytes!("../../assets/icons/update.svg")),
        "tools" => Some(include_bytes!("../../assets/icons/tools-menu.svg")),
        "more" => Some(include_bytes!("../../assets/icons/more.svg")),
        "refresh" => Some(include_bytes!("../../assets/icons/refresh.svg")),
        "remove-group" => Some(include_bytes!("../../assets/icons/remove-group.svg")),
        "search" => Some(include_bytes!("../../assets/icons/search.svg")),
        "upload" => Some(include_bytes!("../../assets/icons/upload.svg")),
        "warning" => Some(include_bytes!("../../assets/icons/warning.svg")),
        "windows" => Some(include_bytes!("../../assets/icons/windows.svg")),
        _ => None,
    }
}

fn texture(ui: &mut egui::Ui, name: &str, size: f32) -> Option<egui::TextureHandle> {
    let Some(svg) = svg_bytes(name) else {
        warn_once(ui, name, "could not find UI SVG icon asset");
        return None;
    };
    let texture = ui.ctx().data(|data| {
        data.get_temp::<IconCache>(icon_cache_id())
            .and_then(|cache| cache.textures.get(name).cloned())
    });
    let texture = texture.or_else(|| {
        let image = match rasterize_svg(svg, size.ceil() as u32) {
            Ok(image) => image,
            Err(error) => {
                warn_once(
                    ui,
                    name,
                    &format!("could not rasterize UI SVG icon: {error}"),
                );
                return None;
            }
        };
        let texture = ui.ctx().load_texture(
            format!("rm-icon-{name}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        ui.ctx().data_mut(|data| {
            let cache = data.get_temp_mut_or_default::<IconCache>(icon_cache_id());
            cache.textures.insert(name.to_string(), texture.clone());
        });
        tracing::info!(icon = name, size, "loaded UI SVG icon");
        Some(texture)
    });
    texture
}

fn warn_once(ui: &egui::Ui, name: &str, message: &str) {
    let should_warn = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<IconCache>(icon_cache_id())
            .warned_icons
            .insert(name.to_string())
    });
    if should_warn {
        tracing::warn!(icon = name, "{message}");
    }
}

/// Add a standalone SVG icon, retaining the caller's surrounding layout.
pub fn show(ui: &mut egui::Ui, name: &str, size: f32) -> bool {
    let Some(texture) = texture(ui, name, size) else {
        return false;
    };
    ui.add(
        egui::Image::from_texture(&texture)
            .fit_to_exact_size(egui::vec2(size, size))
            .tint(ui.visuals().text_color()),
    );
    true
}

/// Draw an icon in a caller-owned rectangle and return its click response.
pub fn show_at(
    ui: &mut egui::Ui,
    name: &str,
    rect: egui::Rect,
    tint: egui::Color32,
) -> egui::Response {
    let response = ui.allocate_rect(rect, egui::Sense::click());
    if let Some(texture) = texture(ui, name, rect.width().min(rect.height())) {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            tint,
        );
    }
    response
}

/// Add a selectable navigation button with a theme-tinted Lucide SVG icon.
/// The icon is part of the button's hit target and inherits its padding.
pub fn tab_button(ui: &mut egui::Ui, name: &str, label: &str, selected: bool) -> egui::Response {
    let button = match texture(ui, name, 16.0) {
        Some(texture) => egui::Button::image_and_text(
            egui::Image::from_texture((texture.id(), egui::vec2(16.0, 16.0))),
            label,
        )
        .image_tint_follows_text_color(true),
        None => egui::Button::new(label),
    }
    .selected(selected)
    .min_size(egui::vec2(0.0, 24.0));
    ui.add(button)
}

/// Draw a menu button: label, a thin divider, then a chevron - opens a dropdown on click.
#[allow(dead_code)]
pub fn menu_button<R>(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    // measure the label so we can size the whole button rect ourselves
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::default(),
        ui.visuals().text_color(),
    );

    let chevron_zone_w = 6.0 + 1.0 + 6.0 + 16.0 + 6.0; // gap + divider + gap + chevron + right pad
    let total_w = 10.0 + galley.size().x + chevron_zone_w; // left pad + label + chevron zone
    let total_h = 24.0_f32.max(galley.size().y + 8.0);

    let desired_size = egui::vec2(total_w, total_h);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        ui.painter()
            .rect(rect, visuals.rounding, visuals.bg_fill, visuals.bg_stroke);

        let x = rect.left() + 10.0;
        ui.painter().galley(
            egui::pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            visuals.text_color(),
        );

        // Divider and chevron need to remain visible against the button fill
        // instead of inheriting the same stroke color as the backdrop.
        let divider_x = rect.right() - chevron_zone_w + 6.0;
        let divider_color = ui.visuals().widgets.noninteractive.fg_stroke.color;
        ui.painter().vline(
            divider_x,
            rect.top() + 5.0..=rect.bottom() - 5.0,
            egui::Stroke::new(1.0_f32, divider_color),
        );

        if let Some(chevron_texture) = texture(ui, "chevron-down", 16.0) {
            let chevron_rect = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 16.0 - 6.0, rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            );
            ui.painter().image(
                chevron_texture.id(),
                chevron_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                visuals.text_color(),
            );
        }
    }

    let popup_id = ui.make_persistent_id(("rm_menu_button", label));
    if response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    let mut inner = None;
    if ui.memory(|mem| mem.is_popup_open(popup_id)) {
        let area_response = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_bottom() + egui::vec2(0.0, 2.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    inner = Some(add_contents(ui));
                });
            });

        if ui.input(|i| i.pointer.any_click())
            && !response.clicked()
            && !area_response.response.contains_pointer()
        {
            ui.memory_mut(|mem| mem.close_popup());
        }
    }

    response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    egui::InnerResponse::new(inner, response)
}

/// Add a compact icon-only navigation button for constrained toolbars.
pub fn compact_tab_button(
    ui: &mut egui::Ui,
    name: &str,
    label: &str,
    selected: bool,
) -> egui::Response {
    let button = match texture(ui, name, 16.0) {
        Some(texture) => egui::Button::image(egui::Image::from_texture((
            texture.id(),
            egui::vec2(16.0, 16.0),
        )))
        .image_tint_follows_text_color(true),
        None => egui::Button::new(label.chars().next().unwrap_or('?').to_string()),
    }
    .selected(selected)
    .min_size(egui::vec2(30.0, 24.0));
    ui.add(button).on_hover_text(label)
}

/// Add a compact button with a cached Lucide icon and visible text.
pub fn button(ui: &mut egui::Ui, name: &str, label: &str) -> egui::Response {
    enabled_button(ui, name, label, true)
}

/// Add an optionally disabled compact button with a cached Lucide icon.
pub fn enabled_button(ui: &mut egui::Ui, name: &str, label: &str, enabled: bool) -> egui::Response {
    let button = match texture(ui, name, 16.0) {
        Some(texture) => egui::Button::image_and_text(
            egui::Image::from_texture((texture.id(), egui::vec2(16.0, 16.0))),
            label,
        )
        .image_tint_follows_text_color(true),
        None => egui::Button::new(label),
    };
    ui.add_enabled(enabled, button)
}

/// Add an icon button with a fixed minimum size and background.
pub fn sized_button(
    ui: &mut egui::Ui,
    name: &str,
    label: &str,
    min_size: egui::Vec2,
    fill: egui::Color32,
) -> egui::Response {
    let button = match texture(ui, name, 16.0) {
        Some(texture) => egui::Button::image_and_text(
            egui::Image::from_texture((texture.id(), egui::vec2(16.0, 16.0))),
            label,
        )
        .image_tint_follows_text_color(true),
        None => egui::Button::new(label),
    };
    ui.add(button.min_size(min_size).fill(fill))
}

/// Add a sized icon button that can be disabled without losing its icon.
pub fn sized_button_enabled(
    ui: &mut egui::Ui,
    name: &str,
    label: &str,
    min_size: egui::Vec2,
    fill: egui::Color32,
    is_enabled: bool,
) -> egui::Response {
    let button = match texture(ui, name, 16.0) {
        Some(texture) => egui::Button::image_and_text(
            egui::Image::from_texture((texture.id(), egui::vec2(16.0, 16.0))),
            label,
        )
        .image_tint_follows_text_color(true),
        None => egui::Button::new(label),
    };
    ui.add_enabled(is_enabled, button.min_size(min_size).fill(fill))
}

/// Rasterize an SVG asset into an egui color image.
#[allow(dead_code)]
pub fn rasterize_svg(svg_bytes: &[u8], size: u32) -> Result<egui::ColorImage, String> {
    if size == 0 {
        return Err("icon size must be greater than zero".to_string());
    }

    let svg = std::str::from_utf8(svg_bytes)
        .map_err(|error| format!("SVG icon is not UTF-8: {error}"))?
        .replace("currentColor", "#ffffff");
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options)
        .map_err(|error| format!("could not parse SVG icon: {error}"))?;
    let target_size = resvg::usvg::Size::from_wh(size as f32, size as f32)
        .ok_or_else(|| "could not create icon size".to_string())?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        target_size.width() / tree.size().width(),
        target_size.height() / tree.size().height(),
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| "could not allocate SVG icon surface".to_string())?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [size as usize, size as usize],
        pixmap.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::rasterize_svg;

    #[test]
    fn rasterizes_a_lucide_style_svg() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 12h14" stroke="currentColor" fill="none"/></svg>"#;
        let image = rasterize_svg(svg, 16).expect("valid SVG should rasterize");
        assert_eq!(image.size, [16, 16]);
        assert!(image.pixels.iter().any(|pixel| pixel.a() > 0));
    }
}
