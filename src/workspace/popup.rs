//! Ephemeral floating popup pane.
//!
//! A popup is a centered, bordered terminal rendered on top of the tiled layout
//! but tracked OUTSIDE the layout tree (it lives in [`Tab::popup`], never in
//! `Tab::panes`/`Tab::layout`). Because it is not part of the tree:
//!
//! - tiling geometry, split, resize-of-tiles and the zoom toggle never see it,
//! - session restore and live-handoff (which serialize from the tree) skip it
//!   for free — popups are deliberately ephemeral.
//!
//! Phase 2 (a self-resize API) only needs to address the popup by its pane id,
//! which it already has via [`Popup::pane_id`]; no richer focus model is needed.

use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::api::schema::{PopupBorderStyle, PopupBreakpointSpec, PopupSpec};
use crate::layout::PaneId;
use crate::pane::PaneState;

/// Minimum interior (content) size a popup is clamped to, in cells.
pub const POPUP_MIN_INNER_W: u16 = 4;
pub const POPUP_MIN_INNER_H: u16 = 2;

const POPUP_DEFAULT_WIDTH_PCT: u16 = 60;
const POPUP_DEFAULT_HEIGHT_PCT: u16 = 60;

/// A live popup pane attached to a tab, outside the layout tree.
pub struct Popup {
    /// Unique pane id (globally allocated, never registered in the layout tree
    /// or `public_pane_numbers`).
    pub pane_id: PaneId,
    /// Viewport state; its `attached_terminal_id` is how the runtime is found in
    /// the shared registry, exactly like a tiled pane.
    pub state: PaneState,
    /// Resolved styling/sizing, computed once at open time.
    pub spec: ResolvedPopupSpec,
}

impl Popup {
    pub fn new(pane_id: PaneId, state: PaneState, spec: ResolvedPopupSpec) -> Self {
        Self {
            pane_id,
            state,
            spec,
        }
    }
}

/// Concrete popup styling resolved from a [`PopupSpec`] at open time. Kept
/// separate from the wire `PopupSpec` so the render path never re-parses colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPopupSpec {
    /// Width as a percent of the terminal width (1..=100) or absolute cells.
    pub width: PopupSize,
    pub height: PopupSize,
    pub border: bool,
    pub border_style: PopupBorderStyle,
    pub border_color: Color,
    pub padding: u16,
    pub bg: Color,
    pub title: Option<String>,
    pub breakpoints: Vec<ResolvedPopupBreakpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPopupBreakpoint {
    pub max_cols: Option<u16>,
    pub max_rows: Option<u16>,
    pub width: Option<PopupSize>,
    pub height: Option<PopupSize>,
    pub border: Option<bool>,
    pub border_style: Option<PopupBorderStyle>,
    pub border_color: Option<Color>,
    pub padding: Option<u16>,
    pub bg: Option<Color>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePopupSpec {
    pub width: PopupSize,
    pub height: PopupSize,
    pub border: bool,
    pub border_style: PopupBorderStyle,
    pub border_color: Color,
    pub padding: u16,
    pub bg: Color,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupSize {
    Percent(u16),
    Cells(u16),
}

impl PopupSize {
    fn resolve(self, axis: u16) -> u16 {
        match self {
            PopupSize::Percent(pct) => ((u32::from(axis) * u32::from(pct.min(100))) / 100) as u16,
            PopupSize::Cells(cells) => cells,
        }
    }
}

impl ResolvedPopupSpec {
    /// Resolve a wire [`PopupSpec`] into concrete render values.
    ///
    /// `default_border_color` / `default_bg` come from the palette; `accent`-style
    /// colors parse via [`crate::config::theme::parse_color`].
    pub fn from_spec(spec: &PopupSpec, default_border_color: Color, default_bg: Color) -> Self {
        let parse = crate::config::parse_color;
        let breakpoints = spec
            .breakpoints
            .iter()
            .map(|breakpoint| resolve_breakpoint(breakpoint, parse))
            .collect();
        ResolvedPopupSpec {
            width: dimension_to_size(spec.width, POPUP_DEFAULT_WIDTH_PCT),
            height: dimension_to_size(spec.height, POPUP_DEFAULT_HEIGHT_PCT),
            border: spec.border.unwrap_or(true),
            // No `rounded_pane_borders` config exists on this branch; default to single.
            border_style: spec.border_style.unwrap_or(PopupBorderStyle::Single),
            border_color: spec
                .border_color
                .as_deref()
                .map(parse)
                .unwrap_or(default_border_color),
            padding: spec.padding.unwrap_or(0),
            bg: spec.bg.as_deref().map(parse).unwrap_or(default_bg),
            title: spec.title.clone(),
            breakpoints,
        }
    }

    pub fn effective(&self, area: Rect) -> EffectivePopupSpec {
        let mut effective = EffectivePopupSpec {
            width: self.width,
            height: self.height,
            border: self.border,
            border_style: self.border_style,
            border_color: self.border_color,
            padding: self.padding,
            bg: self.bg,
            title: self.title.clone(),
        };
        for breakpoint in &self.breakpoints {
            if !breakpoint.matches(area) {
                continue;
            }
            if let Some(width) = breakpoint.width {
                effective.width = width;
            }
            if let Some(height) = breakpoint.height {
                effective.height = height;
            }
            if let Some(border) = breakpoint.border {
                effective.border = border;
            }
            if let Some(border_style) = breakpoint.border_style {
                effective.border_style = border_style;
            }
            if let Some(border_color) = breakpoint.border_color {
                effective.border_color = border_color;
            }
            if let Some(padding) = breakpoint.padding {
                effective.padding = padding;
            }
            if let Some(bg) = breakpoint.bg {
                effective.bg = bg;
            }
            if let Some(title) = &breakpoint.title {
                effective.title = Some(title.clone());
            }
        }
        effective
    }

    pub fn rects(&self, area: Rect) -> Option<(Rect, Rect, EffectivePopupSpec)> {
        let effective = self.effective(area);
        let outer = effective.outer_rect(area)?;
        let inner = effective.inner_rect(outer);
        Some((outer, inner, effective))
    }
}

impl EffectivePopupSpec {
    pub fn outer_rect(&self, area: Rect) -> Option<Rect> {
        let frame = if self.border { 2 } else { 0 };
        let pad = self.padding.saturating_mul(2);
        let chrome_w = frame + pad;
        let chrome_h = frame + pad;

        let want_w = self.width.resolve(area.width);
        let want_h = self.height.resolve(area.height);

        // Clamp outer size to the available area.
        let max_w = area.width;
        let max_h = area.height;
        let min_w = POPUP_MIN_INNER_W.saturating_add(chrome_w);
        let min_h = POPUP_MIN_INNER_H.saturating_add(chrome_h);

        let outer_w = want_w.clamp(min_w, max_w.max(min_w)).min(max_w);
        let outer_h = want_h.clamp(min_h, max_h.max(min_h)).min(max_h);

        if outer_w < min_w || outer_h < min_h {
            return None;
        }

        let x = area.x + (area.width.saturating_sub(outer_w)) / 2;
        let y = area.y + (area.height.saturating_sub(outer_h)) / 2;
        Some(Rect::new(x, y, outer_w, outer_h))
    }

    pub fn inner_rect(&self, outer: Rect) -> Rect {
        let frame = if self.border { 1 } else { 0 };
        let inset = frame + self.padding;
        Rect {
            x: outer.x.saturating_add(inset),
            y: outer.y.saturating_add(inset),
            width: outer.width.saturating_sub(inset.saturating_mul(2)),
            height: outer.height.saturating_sub(inset.saturating_mul(2)),
        }
    }
}

impl ResolvedPopupBreakpoint {
    fn matches(&self, area: Rect) -> bool {
        self.max_cols.is_none_or(|max| area.width <= max)
            && self.max_rows.is_none_or(|max| area.height <= max)
    }
}

fn resolve_breakpoint(
    breakpoint: &PopupBreakpointSpec,
    parse: fn(&str) -> Color,
) -> ResolvedPopupBreakpoint {
    ResolvedPopupBreakpoint {
        max_cols: breakpoint.max_cols,
        max_rows: breakpoint.max_rows,
        width: breakpoint
            .width
            .map(|dim| dimension_to_size(Some(dim), 100)),
        height: breakpoint
            .height
            .map(|dim| dimension_to_size(Some(dim), 100)),
        border: breakpoint.border,
        border_style: breakpoint.border_style,
        border_color: breakpoint.border_color.as_deref().map(parse),
        padding: breakpoint.padding,
        bg: breakpoint.bg.as_deref().map(parse),
        title: breakpoint.title.clone(),
    }
}

fn dimension_to_size(
    dim: Option<crate::api::schema::PopupDimension>,
    default_pct: u16,
) -> PopupSize {
    match dim {
        Some(crate::api::schema::PopupDimension::Percent(pct)) => PopupSize::Percent(pct),
        Some(crate::api::schema::PopupDimension::Cells(cells)) => PopupSize::Cells(cells),
        None => PopupSize::Percent(default_pct),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ResolvedPopupSpec {
        ResolvedPopupSpec {
            width: PopupSize::Percent(50),
            height: PopupSize::Percent(50),
            border: true,
            border_style: PopupBorderStyle::Single,
            border_color: Color::Reset,
            padding: 0,
            bg: Color::Reset,
            title: None,
            breakpoints: Vec::new(),
        }
    }

    #[test]
    fn centered_rect_is_centered_and_sized() {
        let s = spec();
        let area = Rect::new(0, 0, 100, 40);
        let (outer, inner, _) = s.rects(area).unwrap();
        assert_eq!(outer.width, 50);
        assert_eq!(outer.height, 20);
        // centered
        assert_eq!(outer.x, 25);
        assert_eq!(outer.y, 10);
        // inner accounts for the border
        assert_eq!(inner.width, 48);
        assert_eq!(inner.height, 18);
    }

    #[test]
    fn padding_shrinks_inner() {
        let mut s = spec();
        s.padding = 2;
        s.width = PopupSize::Cells(40);
        s.height = PopupSize::Cells(20);
        let area = Rect::new(0, 0, 100, 40);
        let (outer, inner, _) = s.rects(area).unwrap();
        assert_eq!(outer.width, 40);
        // border (1) + padding (2) on each side = 3 inset
        assert_eq!(inner.width, 40 - 6);
        assert_eq!(inner.height, 20 - 6);
    }

    #[test]
    fn clamps_to_min_when_requested_too_small() {
        let mut s = spec();
        s.width = PopupSize::Cells(1);
        s.height = PopupSize::Cells(1);
        let area = Rect::new(0, 0, 100, 40);
        let (outer, _, _) = s.rects(area).unwrap();
        // min inner + border chrome
        assert_eq!(outer.width, POPUP_MIN_INNER_W + 2);
        assert_eq!(outer.height, POPUP_MIN_INNER_H + 2);
    }

    #[test]
    fn returns_none_when_area_smaller_than_min() {
        let s = spec();
        let area = Rect::new(0, 0, 3, 3);
        assert!(s.rects(area).is_none());
    }

    #[test]
    fn borderless_has_no_frame_inset() {
        let mut s = spec();
        s.border = false;
        s.width = PopupSize::Cells(40);
        s.height = PopupSize::Cells(20);
        let area = Rect::new(0, 0, 100, 40);
        let (_, inner, _) = s.rects(area).unwrap();
        assert_eq!(inner.width, 40);
        assert_eq!(inner.height, 20);
    }

    #[test]
    fn breakpoint_overrides_apply_when_area_matches() {
        let s = ResolvedPopupSpec::from_spec(
            &PopupSpec {
                width: Some(crate::api::schema::PopupDimension::Percent(60)),
                height: Some(crate::api::schema::PopupDimension::Percent(50)),
                border: Some(true),
                padding: Some(2),
                breakpoints: vec![crate::api::schema::PopupBreakpointSpec {
                    max_cols: Some(80),
                    width: Some(crate::api::schema::PopupDimension::Percent(100)),
                    height: Some(crate::api::schema::PopupDimension::Percent(90)),
                    border: Some(false),
                    padding: Some(0),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Color::Yellow,
            Color::Black,
        );

        let (_, inner, effective) = s.rects(Rect::new(0, 0, 80, 40)).unwrap();
        assert_eq!(inner.width, 80);
        assert_eq!(inner.height, 36);
        assert!(!effective.border);
        assert_eq!(effective.padding, 0);

        let (_, inner, effective) = s.rects(Rect::new(0, 0, 100, 40)).unwrap();
        assert_eq!(inner.width, 54); // 60 columns minus border + padding chrome
        assert_eq!(inner.height, 14); // 20 rows minus border + padding chrome
        assert!(effective.border);
        assert_eq!(effective.padding, 2);
    }
}
