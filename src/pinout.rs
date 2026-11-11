//! Raspberry Pi 40-pin header diagram.
//!
//! `assets/gpio.svg` ("Raspberry Pi3 GPIO Diagram" by Dirk Koudstaal, element14, CC BY-SA;
//! the 40-pin layout is the same on Raspberry Pi 2, 3, 4 and 5) is rasterized with resvg,
//! using the system fonts for its labels. The position of every physical pin's number box is
//! taken from the SVG itself, so the GPIO monitor can draw live pin states on top of it.

use resvg::tiny_skia;
use resvg::usvg::{self, NodeExt, TreeParsing, TreeTextToPath};

pub const SVG: &[u8] = include_bytes!("../assets/gpio.svg");

/// Number boxes in the SVG share this id prefix (`rect3876-…`).
const PIN_BOX_PREFIX: &str = "rect3876";

/// Credit line shown under the diagram.
pub const CREDIT: &str = "Diagram: Dirk Koudstaal (element14), CC BY-SA";

/// Rasterized diagram plus the normalized (0..1) rectangle of each physical pin's number box.
pub struct Pinout {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA pixels.
    pub rgba: Vec<u8>,
    /// Index 0 is physical pin 1. Values are fractions of the image size: [x0, y0, x1, y1].
    pub pins: [[f32; 4]; 40],
}

fn font_database() -> usvg::fontdb::Database {
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    // The diagram asks for the generic `sans-serif` family; map it to a font that exists.
    let preferred = [
        "Noto Sans",
        "DejaVu Sans",
        "Liberation Sans",
        "Cantarell",
        "Ubuntu",
        "Arial",
    ];
    let installed = |name: &str| {
        db.faces().any(|f| {
            f.families
                .iter()
                .any(|(fam, _)| fam.eq_ignore_ascii_case(name))
        })
    };
    let chosen: Option<String> = preferred
        .iter()
        .find(|n| installed(n))
        .map(|n| n.to_string())
        .or_else(|| {
            db.faces()
                .next()
                .and_then(|f| f.families.first())
                .map(|f| f.0.clone())
        });
    if let Some(name) = chosen {
        db.set_sans_serif_family(name);
    }
    db
}

/// Parses the diagram, converts its text with the system fonts and locates the pin boxes.
fn load_tree() -> Option<(usvg::Tree, [[f32; 4]; 40])> {
    let mut tree = usvg::Tree::from_data(SVG, &usvg::Options::default()).ok()?;
    tree.convert_text(&font_database());

    let to_canvas =
        usvg::utils::view_box_to_transform(tree.view_box.rect, tree.view_box.aspect, tree.size);
    let (w, h) = (tree.size.width(), tree.size.height());

    let mut boxes: Vec<[f32; 4]> = Vec::new();
    for node in tree.root.descendants() {
        if !node.id().starts_with(PIN_BOX_PREFIX) {
            continue;
        }
        let Some(b) = node.calculate_bbox() else {
            continue;
        };
        let Some(b) = b.transform(to_canvas) else {
            continue;
        };
        // The same id prefix is used by the long coloured label bars; number boxes are
        // nearly square (42 x 32 in the drawing).
        if b.width() > b.height() * 2.0 {
            continue;
        }
        boxes.push([b.left() / w, b.top() / h, b.right() / w, b.bottom() / h]);
    }
    if boxes.len() != 40 {
        return None;
    }

    // Two columns: odd pins on the left, even pins on the right; top to bottom.
    let mid = boxes.iter().map(|b| (b[0] + b[2]) / 2.0).sum::<f32>() / 40.0;
    let (mut left, mut right): (Vec<_>, Vec<_>) =
        boxes.into_iter().partition(|b| (b[0] + b[2]) / 2.0 < mid);
    if left.len() != 20 || right.len() != 20 {
        return None;
    }
    left.sort_by(|a, b| a[1].total_cmp(&b[1]));
    right.sort_by(|a, b| a[1].total_cmp(&b[1]));
    let mut pins = [[0.0; 4]; 40];
    for row in 0..20 {
        pins[row * 2] = left[row];
        pins[row * 2 + 1] = right[row];
    }
    Some((tree, pins))
}

/// Renders the diagram `width` pixels wide (height follows the aspect ratio).
pub fn render(width: u32) -> Option<Pinout> {
    let (tree, pins) = load_tree()?;
    // The drawing has an outer frame around the whole canvas; it looks out of place inside a
    // card, so it is left out of the rendering (the SVG file itself is unchanged).
    let to_canvas =
        usvg::utils::view_box_to_transform(tree.view_box.rect, tree.view_box.aspect, tree.size);
    let (cw, ch) = (tree.size.width(), tree.size.height());
    let frames: Vec<_> = tree
        .root
        .descendants()
        .filter(|n| matches!(*n.borrow(), usvg::NodeKind::Path(_)))
        .filter(|n| {
            n.calculate_bbox()
                .and_then(|b| b.transform(to_canvas))
                .map(|b| b.width() > cw * 0.9 && b.height() > ch * 0.9)
                .unwrap_or(false)
        })
        .collect();
    for f in frames {
        f.detach();
    }
    let scale = width as f32 / tree.size.width();
    let height = (tree.size.height() * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::Tree::from_usvg(&tree).render(
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(Pinout {
        width,
        height,
        rgba: pixmap.take(),
        pins,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_forty_pins_in_header_order() {
        let (_, pins) = load_tree().expect("diagram parses and has 40 pin boxes");
        for (i, p) in pins.iter().enumerate() {
            assert!(p[0] < p[2] && p[1] < p[3], "pin {} has an empty box", i + 1);
            assert!(
                p.iter().all(|v| (0.0..=1.0).contains(v)),
                "pin {} outside image",
                i + 1
            );
        }
        // Odd pins on the left of even pins, rows going down the header.
        for row in 0..20 {
            assert!(pins[row * 2][2] <= pins[row * 2 + 1][0]);
            if row > 0 {
                assert!(pins[row * 2][1] > pins[row * 2 - 2][1]);
            }
        }
    }

    /// `PINOUT_PNG=/path/out.png cargo test pinout::tests::debug_overlay -- --ignored`
    /// writes the diagram with the detected pin boxes outlined (pin 1 green, pin 2 yellow,
    /// other odd pins red, even pins blue).
    #[test]
    #[ignore]
    fn debug_overlay() {
        let p = render(900).unwrap();
        let mut pix = tiny_skia::Pixmap::from_vec(
            p.rgba.clone(),
            tiny_skia::IntSize::from_wh(p.width, p.height).unwrap(),
        )
        .unwrap();
        for (i, b) in p.pins.iter().enumerate() {
            let color = match i {
                0 => tiny_skia::Color::from_rgba8(0, 200, 0, 255),
                1 => tiny_skia::Color::from_rgba8(240, 200, 0, 255),
                _ if i % 2 == 0 => tiny_skia::Color::from_rgba8(230, 0, 0, 255),
                _ => tiny_skia::Color::from_rgba8(0, 80, 230, 255),
            };
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(color);
            let r = tiny_skia::Rect::from_ltrb(
                b[0] * p.width as f32,
                b[1] * p.height as f32,
                b[2] * p.width as f32,
                b[3] * p.height as f32,
            )
            .unwrap();
            let path = tiny_skia::PathBuilder::from_rect(r);
            let stroke = tiny_skia::Stroke {
                width: 4.0,
                ..Default::default()
            };
            pix.stroke_path(
                &path,
                &paint,
                &stroke,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        let out = std::env::var("PINOUT_PNG").unwrap_or_else(|_| "pinout-debug.png".into());
        pix.save_png(out).unwrap();
    }

    #[test]
    fn renders_requested_width() {
        let p = render(400).expect("renders");
        assert_eq!(p.width, 400);
        assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
    }
}
