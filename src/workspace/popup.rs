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

use crate::api::schema::{PopupBorderStyle, PopupSpec};
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
        }
    }

    /// Outer (bordered) rect for this popup centered within `area`, or `None`
    /// if `area` is too small to host the minimum content size.
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

    /// Inner (content / PTY) rect inside `outer`, after border + padding.
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
        }
    }

    #[test]
    fn centered_rect_is_centered_and_sized() {
        let s = spec();
        let area = Rect::new(0, 0, 100, 40);
        let outer = s.outer_rect(area).unwrap();
        assert_eq!(outer.width, 50);
        assert_eq!(outer.height, 20);
        // centered
        assert_eq!(outer.x, 25);
        assert_eq!(outer.y, 10);
        // inner accounts for the border
        let inner = s.inner_rect(outer);
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
        let outer = s.outer_rect(area).unwrap();
        assert_eq!(outer.width, 40);
        let inner = s.inner_rect(outer);
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
        let outer = s.outer_rect(area).unwrap();
        // min inner + border chrome
        assert_eq!(outer.width, POPUP_MIN_INNER_W + 2);
        assert_eq!(outer.height, POPUP_MIN_INNER_H + 2);
    }

    #[test]
    fn returns_none_when_area_smaller_than_min() {
        let s = spec();
        let area = Rect::new(0, 0, 3, 3);
        assert!(s.outer_rect(area).is_none());
    }

    #[test]
    fn borderless_has_no_frame_inset() {
        let mut s = spec();
        s.border = false;
        s.width = PopupSize::Cells(40);
        s.height = PopupSize::Cells(20);
        let area = Rect::new(0, 0, 100, 40);
        let outer = s.outer_rect(area).unwrap();
        let inner = s.inner_rect(outer);
        assert_eq!(inner.width, 40);
        assert_eq!(inner.height, 20);
    }
}
