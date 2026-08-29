//! SVG icon rasterization for egui.
//!
//! Lucide assets remain SVG files in the repository. egui receives the
//! rasterized pixels so the UI does not depend on Windows having a matching
//! emoji or SVG font installed.

use eframe::egui;

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
