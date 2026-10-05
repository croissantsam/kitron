//! Software painter for Kitron UI.
//!
//! Pure function over a laid-out tree: fills a `tiny-skia` pixmap (rounded
//! rects + cosmic-text glyphs). The host presents the pixmap through
//! softbuffer; tests render offscreen and assert on pixels.

use crate::ui::{PlacedNode, UiKind};

/// Parse `#rrggbb` / `#rgb` (fallback: opaque black).
pub fn parse_color(s: &str) -> tiny_skia::Color {
    let h = s.trim().trim_start_matches('#');
    let (r, g, b) = match h.len() {
        6 => (
            u8::from_str_radix(&h[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&h[2..4], 16).unwrap_or(0),
            u8::from_str_radix(&h[4..6], 16).unwrap_or(0),
        ),
        3 => {
            let n = |c: char| c.to_digit(16).unwrap_or(0) as u8;
            let mut c = h.chars();
            let (r, g, b) = (c.next().unwrap_or('0'), c.next().unwrap_or('0'), c.next().unwrap_or('0'));
            (n(r) * 17, n(g) * 17, n(b) * 17)
        }
        _ => (0, 0, 0),
    };
    tiny_skia::Color::from_rgba8(r, g, b, 255)
}

pub struct Painter<'a> {
    fonts: &'a mut cosmic_text::FontSystem,
    cache: cosmic_text::SwashCache,
}

impl<'a> Painter<'a> {
    pub fn new(fonts: &'a mut cosmic_text::FontSystem) -> Self {
        Self { fonts, cache: cosmic_text::SwashCache::new() }
    }

    pub fn paint(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        items: &[PlacedNode],
        images: &std::collections::HashMap<String, crate::ui::LoadedImage>,
    ) {
        // Window background = root background or white.
        let bg = items.first().and_then(|r| r.background.as_deref()).unwrap_or("#ffffff");
        pixmap.fill(parse_color(bg));
        let mut i = 0;
        self.walk(pixmap, items, images, &mut i, 0, 0.0, 0.0);
    }

    /// Pre-order walk: children (depth+1 runs) render after their parent.
    /// Scrollable nodes paint children into a bounds-sized layer so content
    /// clips exactly — no mask bookkeeping, nesting-safe.
    fn walk(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        items: &[PlacedNode],
        images: &std::collections::HashMap<String, crate::ui::LoadedImage>,
        i: &mut usize,
        depth: usize,
        ox: f32,
        oy: f32,
    ) {
        while *i < items.len() && items[*i].depth >= depth {
            if items[*i].depth != depth {
                return; // Belongs to an ancestor level.
            }
            let item = &items[*i];
            if item.scrollable && item.width > 1.0 && item.height > 1.0 {
                self.paint_node(pixmap, item, images, ox, oy, true);
                if let Some(mut layer) = tiny_skia::Pixmap::new(item.width as u32, item.height as u32) {
                    *i += 1;
                    self.walk(&mut layer, items, images, i, depth + 1, ox - item.x, oy - item.y);
                    pixmap.draw_pixmap(
                        0,
                        0,
                        layer.as_ref(),
                        &tiny_skia::PixmapPaint::default(),
                        tiny_skia::Transform::from_row(1.0, 0.0, 0.0, 1.0, item.x, item.y),
                        None,
                    );
                } else {
                    *i += 1;
                    self.skip_subtree(items, i, depth);
                }
            } else {
                self.paint_node(pixmap, item, images, ox, oy, false);
                *i += 1;
                self.walk(pixmap, items, images, i, depth + 1, ox, oy);
            }
        }
    }

    fn skip_subtree(&mut self, items: &[PlacedNode], i: &mut usize, depth: usize) {
        while *i < items.len() && items[*i].depth > depth {
            *i += 1;
        }
    }

    fn paint_node(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        item: &PlacedNode,
        images: &std::collections::HashMap<String, crate::ui::LoadedImage>,
        ox: f32,
        oy: f32,
        bg_only: bool,
    ) {
        if item.width <= 0.0 || item.height <= 0.0 {
            return;
        }
        let (x, y) = (item.x + ox, item.y + oy);
        // Backgrounds: views (when set), buttons, inputs.
        let draws_bg = item.background.is_some()
            && !matches!(item.kind, UiKind::Text)
            || matches!(item.kind, UiKind::Button | UiKind::TextInput);
        if draws_bg {
            let bg = item.background.clone().unwrap_or_else(|| "#e8e8e8".into());
            if let Some(path) = rounded_rect(x, y, item.width, item.height, item.border_radius) {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(parse_color(&bg));
                paint.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
                // Focused input: blue outline.
                if item.focused {
                    let mut stroke = tiny_skia::Paint::default();
                    stroke.set_color(parse_color("#0a84ff"));
                    stroke.anti_alias = true;
                    pixmap.stroke_path(
                        &path,
                        &stroke,
                        &tiny_skia::Stroke { width: 2.0, ..Default::default() },
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            }
        }
        // Images paint their pixels (scaled to the node box).
        if item.kind == UiKind::Image {
            if let Some(img) = images.get(&item.id) {
                self.paint_image(pixmap, img, x, y, item.width, item.height);
            }
        }
        if bg_only {
            return;
        }
        if !item.text.is_empty() {
            let pad = 8.0;
            self.paint_text(
                pixmap,
                &item.text,
                x + pad,
                y + pad,
                (item.width - pad * 2.0).max(1.0),
                item.font_size,
                &item.color,
            );
            if item.focused {
                // Caret after the text.
                let w = text_width(self.fonts, &item.text, item.font_size, item.width);
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(parse_color(&item.color));
                if let Some(r) = tiny_skia::Rect::from_xywh(x + pad + w + 2.0, y + pad, 2.0, item.font_size * 1.2) {
                    pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
                }
            }
        }
    }

    fn paint_image(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        img: &crate::ui::LoadedImage,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        if img.width == 0 || img.height == 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        // Straight RGBA bytes → premultiplied pixmap.
        let mut prem = match tiny_skia::Pixmap::new(img.width, img.height) {
            Some(p) => p,
            None => return,
        };
        for (d, s) in prem.pixels_mut().iter_mut().zip(img.rgba.chunks_exact(4)) {            let (r, g, b, a) = (s[0] as u32, s[1] as u32, s[2] as u32, s[3] as u32);
            *d = tiny_skia::PremultipliedColorU8::from_rgba(
                (r * a / 255) as u8,
                (g * a / 255) as u8,
                (b * a / 255) as u8,
                a as u8,
            )
            .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
        pixmap.draw_pixmap(
            0,
            0,
            prem.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::from_row(
                w / img.width as f32,
                0.0,
                0.0,
                h / img.height as f32,
                x,
                y,
            ),
            None,
        );
    }

    fn paint_text(
        &mut self,
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        x: f32,
        y: f32,
        max_w: f32,
        font_size: f32,
        color: &str,
    ) {
        use cosmic_text::{Attrs, Buffer, Metrics, Shaping, Wrap};
        let metrics = Metrics::new(font_size, font_size * 1.3);
        let mut buffer = Buffer::new(self.fonts, metrics);
        buffer.set_size(Some(max_w.max(1.0)), None);
        buffer.set_wrap(Wrap::Word);
        buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        buffer.shape_until_scroll(self.fonts, false);
        let paint_color = parse_color(color);
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((x, y), 1.0);
                let key = physical.cache_key;
                let Some(image) = self.cache.get_image(self.fonts, key).as_ref() else { continue };
                blit_glyph(pixmap, image, physical.x, physical.y, paint_color);
            }
        }
    }
}

fn text_width(fonts: &mut cosmic_text::FontSystem, text: &str, font_size: f32, max_w: f32) -> f32 {
    use cosmic_text::{Attrs, Buffer, Metrics, Shaping, Wrap};
    let metrics = Metrics::new(font_size, font_size * 1.3);
    let mut buffer = Buffer::new(fonts, metrics);
    buffer.set_size(Some(max_w.max(1.0)), None);
    buffer.set_wrap(Wrap::Word);
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    buffer.layout_runs().map(|r| r.line_w).fold(0.0f32, f32::max)
}

/// Source-over blend of straight-alpha (sr,sg,sb,sa) onto a premultiplied dst.
fn blend_over(
    dst: tiny_skia::PremultipliedColorU8,
    sr: u8,
    sg: u8,
    sb: u8,
    sa: u8,
) -> tiny_skia::PremultipliedColorU8 {
    let (sa, inv) = (sa as u32, (255 - sa) as u32);
    let mix = |s: u32, d: u8| ((s * sa / 255 + d as u32 * inv / 255).min(255)) as u8;
    tiny_skia::PremultipliedColorU8::from_rgba(
        mix(sr as u32, dst.red()),
        mix(sg as u32, dst.green()),
        mix(sb as u32, dst.blue()),
        (sa + dst.alpha() as u32 * inv / 255).min(255) as u8,
    )
    .unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT)
}

fn blit_glyph(
    pixmap: &mut tiny_skia::Pixmap,
    image: &cosmic_text::SwashImage,
    dx: i32,
    dy: i32,
    color: tiny_skia::Color,
) {
    use cosmic_text::SwashContent;
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    match image.content {
        SwashContent::Mask => {
            let ox = dx + image.placement.left;
            let oy = dy + image.placement.top;
            for gy in 0..image.placement.height as i32 {
                for gx in 0..image.placement.width as i32 {
                    let px = ox + gx;
                    let py = oy + gy;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let a = image.data[(gy as usize * image.placement.width as usize) + gx as usize];
                    if a == 0 {
                        continue;
                    }
                    if let Some(dst) = pixmap.pixel(px as u32, py as u32) {
                        let o = blend_over(
                            dst,
                            (color.red() * 255.0) as u8,
                            (color.green() * 255.0) as u8,
                            (color.blue() * 255.0) as u8,
                            a,
                        );
                        pixmap.pixels_mut()[py as usize * pw as usize + px as usize] = o;
                    }
                }
            }
        }
        SwashContent::Color => {
            let ox = dx + image.placement.left;
            let oy = dy + image.placement.top;
            for gy in 0..image.placement.height as i32 {
                for gx in 0..image.placement.width as i32 {
                    let px = ox + gx;
                    let py = oy + gy;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let i = (gy as usize * image.placement.width as usize + gx as usize) * 4;
                    let (r, g, b, a) = (image.data[i], image.data[i + 1], image.data[i + 2], image.data[i + 3]);
                    if a == 0 {
                        continue;
                    }
                    if let Some(dst) = pixmap.pixel(px as u32, py as u32) {
                        let o = blend_over(dst, r, g, b, a);
                        pixmap.pixels_mut()[py as usize * pw as usize + px as usize] = o;
                    }
                }
            }
        }
        SwashContent::SubpixelMask => {
            // LCD subpixel: treat as grayscale mask (correct-ish, no fringing).
            let ox = dx + image.placement.left;
            let oy = dy + image.placement.top;
            let w = image.placement.width as usize / 3;
            for gy in 0..image.placement.height as i32 {
                for gx in 0..w as i32 {
                    let px = ox + gx;
                    let py = oy + gy;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let i = (gy as usize * image.placement.width as usize + gx as usize) * 3;
                    let a = image.data[i].max(image.data[i + 1]).max(image.data[i + 2]);
                    if a == 0 {
                        continue;
                    }
                    if let Some(dst) = pixmap.pixel(px as u32, py as u32) {
                        let o = blend_over(
                            dst,
                            (color.red() * 255.0) as u8,
                            (color.green() * 255.0) as u8,
                            (color.blue() * 255.0) as u8,
                            a,
                        );
                        pixmap.pixels_mut()[py as usize * pw as usize + px as usize] = o;
                    }
                }
            }
        }
    }
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if !(x.is_finite() && y.is_finite() && w > 0.0 && h > 0.0) {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = tiny_skia::PathBuilder::new();
    if r <= 0.5 {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }
    // Rounded rect from lines + quadratic corners.
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UiManager;

    #[test]
    fn parses_colors() {
        let c = parse_color("#ff0000");
        assert!((c.red() - 1.0).abs() < 0.01 && c.green() == 0.0);
        assert_eq!(parse_color("#fff"), parse_color("#ffffff"));
    }

    #[test]
    fn paints_button_and_text_pixels() {
        let m = UiManager::new();
        m.create("root".into(), crate::ui::UiKind::View, Default::default(), crate::ui::UiStyle {
            flex: Some(1.0),
            ..Default::default()
        }).unwrap();
        m.create("btn".into(), crate::ui::UiKind::Button,
            crate::ui::UiProps { text: "OK".into(), background: Some("#e81010".into()), ..Default::default() },
            crate::ui::UiStyle {
                width: Some(crate::ui::UiSize::Pt(200.0)),
                height: Some(crate::ui::UiSize::Pt(60.0)),
                ..Default::default()
            }).unwrap();
        m.append("root", "btn", None).unwrap();
        m.set_root("win", "root").unwrap();
        let placed = m.layout("win", 800.0, 600.0).unwrap();

        let mut fonts = cosmic_text::FontSystem::new();
        let mut painter = Painter::new(&mut fonts);
        let mut pixmap = tiny_skia::Pixmap::new(800, 600).unwrap();
        painter.paint(&mut pixmap, &placed, &std::collections::HashMap::new());

        // Button center must carry the red fill.
        let btn = placed.iter().find(|p| p.id == "btn").unwrap();
        let (cx, cy) = ((btn.x + btn.width / 2.0) as u32, (btn.y + btn.height / 2.0) as u32);
        let px = pixmap.pixel(cx, cy).unwrap();
        assert!(px.red() > 180 && px.green() < 90, "r={} g={}", px.red(), px.green());
    }
}
