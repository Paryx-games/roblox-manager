//! SVG icon rasterization for egui.
//!
//! Lucide assets remain SVG files in the repository. egui receives the
//! rasterized pixels so the UI does not depend on Windows having a matching
//! emoji or SVG font installed.

use eframe::egui;
use std::collections::HashMap;

fn icon_cache_id() -> egui::Id {
    egui::Id::new("rm_svg_icon_cache")
}

#[derive(Clone, Default)]
struct IconCache {
    textures: HashMap<String, egui::TextureHandle>,
}

fn svg_bytes(name: &str) -> Option<&'static [u8]> {
    match name {
        "accounts" => Some(include_bytes!("../../assets/icons/accounts.svg")),
        "browser" => Some(include_bytes!("../../assets/icons/browser.svg")),
        "delete" => Some(include_bytes!("../../assets/icons/delete.svg")),
        "folder" => Some(include_bytes!("../../assets/icons/folder.svg")),
        "groups" => Some(include_bytes!("../../assets/icons/groups.svg")),
        "import" => Some(include_bytes!("../../assets/icons/import.svg")),
        "inventory" => Some(include_bytes!("../../assets/icons/inventory.svg")),
        "kill" => Some(include_bytes!("../../assets/icons/kill.svg")),
        "lock" => Some(include_bytes!("../../assets/icons/lock.svg")),
        "package" => Some(include_bytes!("../../assets/icons/package.svg")),
        "password" => Some(include_bytes!("../../assets/icons/password.svg")),
        "pin" => Some(include_bytes!("../../assets/icons/pin.svg")),
        "save" => Some(include_bytes!("../../assets/icons/save.svg")),
        "settings" => Some(include_bytes!("../../assets/icons/settings.svg")),
        "star" => Some(include_bytes!("../../assets/icons/star.svg")),
        "update" => Some(include_bytes!("../../assets/icons/update.svg")),
        "warning" => Some(include_bytes!("../../assets/icons/warning.svg")),
        "windows" => Some(include_bytes!("../../assets/icons/windows.svg")),
        _ => None,
    }
}

/// Add one cached SVG icon to a UI. Returns false when the named asset could
/// not be found or rasterized, allowing the text label to remain usable.
pub fn show(ui: &mut egui::Ui, name: &str, size: f32) -> bool {
    let Some(svg) = svg_bytes(name) else {
        tracing::debug!(icon = name, "could not find UI SVG icon asset");
        return false;
    };
    let texture = ui.ctx().data(|data| {
        data.get_temp::<IconCache>(icon_cache_id())
            .and_then(|cache| cache.textures.get(name).cloned())
    });
    let texture = texture.or_else(|| {
        let image = match rasterize_svg(svg, size.ceil() as u32) {
            Ok(image) => image,
            Err(error) => {
                tracing::debug!(icon = name, %error, "could not rasterize UI SVG icon");
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
        Some(texture)
    });
    let Some(texture) = texture else {
        return false;
    };
    ui.add(
        egui::Image::from_texture(&texture)
            .fit_to_exact_size(egui::vec2(size, size))
            .tint(ui.visuals().text_color()),
    );
    true
}

/// Rasterize an SVG asset into an egui color image.
#[allow(dead_code)]
pub fn rasterize_svg(svg_bytes: &[u8], size: u32) -> Result<egui::ColorImage, String> {
    if size == 0 {
        return Err("icon size must be greater than zero".to_string());
    }

    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(svg_bytes, &options)
        .map_err(|error| format!("could not parse SVG icon: {error}"))?;
    let target_size = resvg::usvg::Size::from_wh(size as f32, size as f32)
        .ok_or_else(|| "could not create icon size".to_string())?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        target_size.width() as f32 / tree.size().width() as f32,
        target_size.height() as f32 / tree.size().height() as f32,
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
