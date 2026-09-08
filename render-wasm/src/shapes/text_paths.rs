use crate::shapes::text::TextContent;
use crate::shapes::VerticalAlign;
use skia_safe::{
    self as skia,
    textlayout::{paragraph::VisitorInfo, Paragraph as SkiaParagraph, TextDecoration},
    FontMetrics, Point, Rect,
};
use std::ops::Deref;

pub struct TextPaths(TextContent);

/// Copied out of the paragraph because `visit` borrows it mutably.
struct RunStyle {
    start: usize,
    paint: skia::Paint,
    decoration: TextDecoration,
    font_metrics: FontMetrics,
    font_size: f32,
}

impl TextPaths {
    pub fn new(text_content: TextContent) -> Self {
        Self(text_content)
    }

    /// Converts the text into filled paths, one per shaped run.
    pub fn get_paths(
        &self,
        antialias: bool,
        vertical_align: VerticalAlign,
    ) -> Vec<(skia::Path, skia::Paint)> {
        let mut paragraph_builders = self.0.paragraph_builder_group_from_text(None);
        let mut paragraphs = Vec::new();

        for group in paragraph_builders.iter_mut() {
            let Some(paragraph_builder) = group.first_mut() else {
                continue;
            };
            let mut paragraph = paragraph_builder.build();
            paragraph.layout(self.bounds.width());
            paragraphs.push(paragraph);
        }

        let total_height: f32 = paragraphs.iter().map(|p| p.height()).sum();
        let vertical_offset = match vertical_align {
            VerticalAlign::Center => (self.bounds.height() - total_height) / 2.0,
            VerticalAlign::Bottom => self.bounds.height() - total_height,
            VerticalAlign::Top => 0.0,
        };

        let mut paths = Vec::new();
        let mut offset_y = self.bounds.y() + vertical_offset;

        for paragraph in paragraphs.iter_mut() {
            let origin = Point::new(self.bounds.x(), offset_y);
            Self::collect_paragraph_paths(paragraph, origin, antialias, &mut paths);
            offset_y += paragraph.height();
        }

        paths
    }

    fn collect_paragraph_paths(
        paragraph: &mut SkiaParagraph,
        origin: Point,
        antialias: bool,
        paths: &mut Vec<(skia::Path, skia::Paint)>,
    ) {
        let line_styles = Self::line_styles(paragraph);

        paragraph.visit(|line_index: usize, info: Option<&VisitorInfo>| {
            let Some(info) = info else {
                return;
            };

            let font = info.font();
            let run_origin = origin + info.origin();
            let style = info
                .utf8_starts()
                .first()
                .and_then(|start| Self::style_at(&line_styles, line_index, *start as usize));

            let mut builder = skia::PathBuilder::new();
            let mut has_geometry = false;

            for (glyph, position) in info.glyphs().iter().zip(info.positions().iter()) {
                let Some(glyph_path) = font.get_path(*glyph) else {
                    continue;
                };
                builder.add_path(&glyph_path.with_offset(run_origin + *position));
                has_geometry = true;
            }

            if let Some(style) = style {
                if let Some(rect) = Self::decoration_rect(style, run_origin, info.advance_x()) {
                    builder.add_rect(rect, None, None);
                    has_geometry = true;
                }
            }

            if !has_geometry {
                return;
            }

            let mut paint = style
                .map(|style| style.paint.clone())
                .unwrap_or_else(skia::Paint::default);
            paint.set_anti_alias(antialias);

            paths.push((builder.detach(), paint));
        });
    }

    fn line_styles(paragraph: &SkiaParagraph) -> Vec<Vec<RunStyle>> {
        paragraph
            .get_line_metrics()
            .iter()
            .map(|line| {
                line.get_style_metrics(line.start_index..line.end_index)
                    .into_iter()
                    .map(|(start, style_metric)| RunStyle {
                        start,
                        paint: style_metric.text_style.foreground(),
                        decoration: style_metric.text_style.decoration().ty,
                        font_metrics: style_metric.font_metrics,
                        font_size: style_metric.text_style.font_size(),
                    })
                    .collect()
            })
            .collect()
    }

    fn style_at(
        line_styles: &[Vec<RunStyle>],
        line_index: usize,
        start: usize,
    ) -> Option<&RunStyle> {
        let styles = line_styles.get(line_index)?;
        styles
            .iter()
            .rev()
            .find(|style| style.start <= start)
            .or_else(|| styles.first())
    }

    /// `run_origin` sits on the baseline. Keep in sync with
    /// `render::text::calculate_decoration_metrics`.
    fn decoration_rect(style: &RunStyle, run_origin: Point, advance_x: f32) -> Option<Rect> {
        let font_metrics = &style.font_metrics;
        let reference_size = font_metrics
            .cap_height
            .abs()
            .max(font_metrics.x_height.abs());
        let min_thickness = (reference_size * 0.06).max(1.0);
        let thickness_factor = style.font_size.powf(0.4) * 6.0 / 18.0;
        let thickness = (font_metrics.underline_thickness().unwrap_or(1.0) * thickness_factor)
            .max(min_thickness);

        let y = match style.decoration {
            TextDecoration::UNDERLINE => run_origin.y + style.font_size / 9.0,
            TextDecoration::LINE_THROUGH => {
                run_origin.y
                    + font_metrics
                        .strikeout_position()
                        .unwrap_or(-font_metrics.cap_height / 2.0)
            }
            _ => return None,
        };

        Some(Rect::new(
            run_origin.x,
            y - thickness / 2.0,
            run_origin.x + advance_x,
            y + thickness / 2.0,
        ))
    }
}

impl Deref for TextPaths {
    type Target = TextContent;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
