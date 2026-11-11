//! Application icon, embedded and rasterized for the window.

use resvg::tiny_skia;
use resvg::usvg::{self, TreeParsing};

pub const SVG: &[u8] = include_bytes!("../assets/argon-control-center.svg");

/// Renders the icon as a square RGBA image of `size` pixels.
pub fn rgba(size: u32) -> Option<Vec<u8>> {
    let tree = usvg::Tree::from_data(SVG, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size.width().max(tree.size.height());
    resvg::Tree::from_usvg(&tree).render(
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied alpha; egui expects straight alpha.
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    )
}

pub fn window_icon() -> Option<egui::IconData> {
    const SIZE: u32 = 128;
    Some(egui::IconData {
        rgba: rgba(SIZE)?,
        width: SIZE,
        height: SIZE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_renders_with_visible_pixels() {
        let px = rgba(64).expect("icon renders");
        assert_eq!(px.len(), 64 * 64 * 4);
        let opaque = px.chunks(4).filter(|p| p[3] > 200).count();
        assert!(opaque > 64 * 64 / 3, "only {opaque} opaque pixels");
    }

    /// Writes PNG previews: `ICON_PREVIEW_DIR=/tmp cargo test icon::tests::preview -- --ignored`
    #[test]
    #[ignore]
    fn preview() {
        let dir = std::env::var("ICON_PREVIEW_DIR").unwrap_or_else(|_| ".".into());
        for size in [256u32, 48, 24] {
            let tree = usvg::Tree::from_data(SVG, &usvg::Options::default()).unwrap();
            let mut pm = tiny_skia::Pixmap::new(size, size).unwrap();
            let s = size as f32 / 256.0;
            resvg::Tree::from_usvg(&tree)
                .render(tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
            pm.save_png(format!("{dir}/icon-{size}.png")).unwrap();
        }
    }
}
