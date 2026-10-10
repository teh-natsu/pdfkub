//! Split view, like an editor's "Split Right": the document area shows two sides, each with its
//! own row of tabs and its own pages. A tab belongs to one side (`DocView::pane`). The side last
//! clicked has focus, and the app's `active` tab is the one it shows, so commands, side panels
//! and shortcuts act there. The other side is drawn passive: it scrolls and zooms under the
//! pointer, and a click on it moves the focus there.
//!
//! One document may be open on both sides (Split right on a single tab). Its views share the
//! document; closing one of them leaves the document open in the other.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Align, Align2, CursorIcon, Layout, Rect, Sense, Stroke, UiBuilder, pos2, vec2};

use crate::canvas::{self, DocView};
use crate::theme::{self, Tokens};
use crate::{PdfKubApp, icons};

/// A side of the split view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Left,
    Right,
}

impl Pane {
    pub fn other(self) -> Pane {
        match self {
            Pane::Left => Pane::Right,
            Pane::Right => Pane::Left,
        }
    }

    fn slot(self) -> usize {
        match self {
            Pane::Left => 0,
            Pane::Right => 1,
        }
    }
}

/// A new view's [`DocView::uid`].
pub fn next_uid() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The split view's state.
#[derive(Clone, Debug)]
pub struct SplitState {
    /// The left side's share of the width, 0.2–0.8.
    pub ratio: f32,
    /// The side with focus: new tabs open there.
    pub focus: Pane,
    /// The tab each side shows, by [`DocView::uid`].
    shown: [Option<u64>; 2],
    /// A tab being dragged to the other side, by [`DocView::uid`].
    dragging: Option<u64>,
}

impl Default for SplitState {
    fn default() -> Self {
        SplitState { ratio: 0.5, focus: Pane::Left, shown: [None; 2], dragging: None }
    }
}

const TAB_ROW: f32 = 34.0;
const DIVIDER: f32 = 6.0;

impl PdfKubApp {
    /// The side tab `i` is on (a tab opened this frame goes to the side with focus).
    pub fn pane_of(&self, i: usize) -> Pane {
        self.views.get(i).and_then(|v| v.pane).unwrap_or(self.split.focus)
    }

    /// Whether the document area is split: both sides have tabs.
    pub fn is_split(&self) -> bool {
        let right = (0..self.views.len()).filter(|&i| self.pane_of(i) == Pane::Right).count();
        right > 0 && right < self.views.len()
    }

    /// The tab side `pane` shows: the active one if it's there, else the one it showed last,
    /// else its first.
    pub fn shown_in(&self, pane: Pane) -> Option<usize> {
        if let Some(a) = self.active.filter(|&a| a < self.views.len() && self.pane_of(a) == pane) {
            return Some(a);
        }
        let uid = self.split.shown[pane.slot()];
        (0..self.views.len())
            .find(|&i| Some(self.views[i].uid) == uid && self.pane_of(i) == pane)
            .or_else(|| (0..self.views.len()).find(|&i| self.pane_of(i) == pane))
    }

    /// Once a frame: new tabs join the side with focus; with one side left, it is the only
    /// one (no split); the focus follows the active tab.
    pub(crate) fn settle_panes(&mut self) {
        let focus = self.split.focus;
        for v in &mut self.views {
            v.pane.get_or_insert(focus);
        }
        let right = self.views.iter().filter(|v| v.pane == Some(Pane::Right)).count();
        if right == 0 || right == self.views.len() {
            for v in &mut self.views {
                v.pane = Some(Pane::Left);
            }
            self.split.focus = Pane::Left;
        }
        if let Some(v) = self.active.and_then(|a| self.views.get(a)) {
            let pane = v.pane.unwrap_or(Pane::Left);
            self.split.focus = pane;
            self.split.shown[pane.slot()] = Some(v.uid);
        }
    }

    /// View ▸ Split right: show the active document on the other side too (the right side,
    /// when the view isn't split yet), with the same page and zoom, and give that side focus.
    pub fn split_right(&mut self) {
        let Some(a) = self.active.filter(|&a| a < self.views.len()) else { return };
        let side = if self.is_split() { self.pane_of(a).other() } else { Pane::Right };
        let id = self.views[a].id;
        if let Some(i) = (0..self.views.len()).find(|&i| self.views[i].id == id && self.pane_of(i) == side) {
            self.active = Some(i);
            return;
        }
        let Some(doc) = self.session.get(id) else { return };
        let from = &self.views[a];
        let mut v = DocView::new(id, &doc.info, self.view_defaults);
        v.zoom = from.zoom;
        v.fit = from.fit;
        v.layout = from.layout;
        v.rotation = from.rotation;
        v.current = from.current;
        v.goto = Some((from.current, 0.0));
        v.pane = Some(side);
        self.views.insert(a + 1, v);
        self.active = Some(a + 1);
    }

    /// Move tab `i` to the other side and give it focus there. A document already open on that
    /// side is just brought forward there; the tab moved away from it closes.
    pub fn move_to_other_side(&mut self, i: usize) {
        if i >= self.views.len() {
            return;
        }
        if self.views.len() == 1 {
            self.active = Some(i);
            self.split_right();
            return;
        }
        let side = if self.is_split() { self.pane_of(i).other() } else { Pane::Right };
        let id = self.views[i].id;
        if let Some(j) = (0..self.views.len()).find(|&j| j != i && self.views[j].id == id && self.pane_of(j) == side) {
            let uid = self.views[j].uid;
            self.remove_view(i);
            self.active = self.views.iter().position(|v| v.uid == uid);
            return;
        }
        // Every other tab on this side: they become the left side once this one is alone.
        for j in 0..self.views.len() {
            let pane = self.pane_of(j);
            self.views[j].pane = Some(pane);
        }
        self.views[i].pane = Some(side);
        self.active = Some(i);
    }

    /// View ▸ Close split view: all tabs on one side again. A document open on both sides keeps
    /// one tab.
    pub fn close_split(&mut self) {
        if !self.is_split() {
            return;
        }
        let keep = self.active.and_then(|a| self.views.get(a)).map(|v| v.uid);
        let mut i = self.views.len();
        while i > 0 {
            i -= 1;
            let id = self.views[i].id;
            let twin = (0..self.views.len()).any(|j| j != i && self.views[j].id == id);
            if twin && self.pane_of(i) == Pane::Right && Some(self.views[i].uid) != keep {
                self.remove_view(i);
            }
        }
        // The active tab may itself have been a right-side twin: drop its left twin instead.
        let mut i = self.views.len();
        while i > 0 {
            i -= 1;
            let id = self.views[i].id;
            if Some(self.views[i].uid) != keep && (0..self.views.len()).any(|j| j != i && self.views[j].id == id) {
                self.remove_view(i);
            }
        }
        for v in &mut self.views {
            v.pane = Some(Pane::Left);
        }
        self.split.focus = Pane::Left;
        self.active = keep.and_then(|uid| self.views.iter().position(|v| v.uid == uid)).or(self.active);
    }

    /// Remove tab `index` (the view only; the caller closes its document if it was the last
    /// view of it). The active tab moves to a neighbour on the same side, else to the other side.
    pub(crate) fn remove_view(&mut self, index: usize) {
        if index >= self.views.len() {
            return;
        }
        let pane = self.pane_of(index);
        self.views.remove(index);
        self.active = match self.active {
            _ if self.views.is_empty() => None,
            Some(a) if a == index => {
                let same = |i: usize| self.pane_of(i) == pane;
                (index..self.views.len()).find(|&i| same(i)).or_else(|| (0..index).rev().find(|&i| same(i))).or(Some(index.min(self.views.len() - 1)))
            }
            Some(a) if a > index => Some(a - 1),
            other => other,
        };
    }

    /// Another tab shows the same document as tab `index`.
    pub(crate) fn has_twin(&self, index: usize) -> bool {
        let Some(id) = self.views.get(index).map(|v| v.id) else { return false };
        self.views.iter().enumerate().any(|(j, v)| j != index && v.id == id)
    }
}

/// The document area split in two: each side's tabs over its pages, a divider between them.
pub fn show(app: &mut PdfKubApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let full = ui.available_rect_before_wrap();
    let ratio = app.split.ratio.clamp(0.2, 0.8);
    let left_w = ((full.width() - DIVIDER) * ratio).round();
    let left = Rect::from_min_size(full.min, vec2(left_w, full.height()));
    let divider = Rect::from_min_max(pos2(left.right(), full.top()), pos2(left.right() + DIVIDER, full.bottom()));
    let right = Rect::from_min_max(pos2(divider.right(), full.top()), full.max);

    let focus = app.active.map(|a| app.pane_of(a)).unwrap_or(app.split.focus);
    for (pane, rect) in [(Pane::Left, left), (Pane::Right, right)] {
        let focused = pane == focus;
        let mut side = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("split-side", pane.slot())).layout(Layout::top_down(Align::Min)));
        side.set_clip_rect(rect);
        tab_row(app, &mut side, pane, focused, &t);
        let Some(i) = app.shown_in(pane) else { continue };
        let body = side.available_rect_before_wrap();
        app.views[i].passive = !focused;
        canvas::document_area(app, i, &mut side);
        if let Some(v) = app.views.get_mut(i) {
            v.passive = false;
        }
        if !focused {
            // Over the passive side's pages: a click gives this side focus instead of acting on
            // the page, and the wheel still scrolls it.
            let catcher = ui.interact(body, ui.id().with(("split-focus", pane.slot())), Sense::click_and_drag());
            if (catcher.clicked() || catcher.drag_started()) && i < app.views.len() {
                app.active = Some(i);
            }
        }
    }

    let handle = ui.interact(divider, ui.id().with("split-divider"), Sense::drag());
    if handle.hovered() || handle.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if handle.dragged()
        && let Some(p) = handle.interact_pointer_pos()
        && full.width() > 0.0
    {
        app.split.ratio = ((p.x - full.left()) / full.width()).clamp(0.2, 0.8);
    }
    ui.painter().rect_filled(divider, 0.0, t.chrome);
    let line = if handle.hovered() || handle.dragged() { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.divider) };
    ui.painter().vline(divider.center().x, divider.y_range(), line);

    // A tab dragged to the other side: its name follows the pointer until it's let go.
    if let Some(uid) = app.split.dragging {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if let Some(p) = pointer
            && let Some(name) = app.views.iter().find(|v| v.uid == uid).and_then(|v| app.session.get(v.id)).map(|d| d.display_name())
        {
            let target = if right.contains(p) {
                Some(right)
            } else if left.contains(p) {
                Some(left)
            } else {
                None
            };
            if let Some(r) = target {
                ui.painter().rect_stroke(r.shrink(2.0), 6.0, Stroke::new(2.0, t.accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            }
            ui.painter().text(p + vec2(14.0, 10.0), Align2::LEFT_TOP, name, theme::medium(12.5), t.text);
        }
        if ui.input(|i| !i.pointer.any_down()) {
            app.split.dragging = None;
            if let (Some(p), Some(i)) = (pointer, app.views.iter().position(|v| v.uid == uid)) {
                let to = if right.contains(p) {
                    Some(Pane::Right)
                } else if left.contains(p) {
                    Some(Pane::Left)
                } else {
                    None
                };
                if to.is_some_and(|to| to != app.pane_of(i)) {
                    app.move_to_other_side(i);
                }
            }
        }
    }
}

/// A side's row of tabs, with the button that closes the split view on the right side's row.
fn tab_row(app: &mut PdfKubApp, ui: &mut egui::Ui, pane: Pane, focused: bool, t: &Tokens) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TAB_ROW), Sense::hover());
    ui.painter().rect_filled(row, 0.0, t.titlebar);
    if focused {
        // The side with focus: an accent line along the top of its tabs.
        ui.painter().hline(row.x_range(), row.top() + 1.0, Stroke::new(2.0, t.accent));
    }
    ui.painter().hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, t.divider));
    // The tabs stay inside their row: anything below it belongs to the pages, which would take
    // the clicks of a tab reaching into them.
    let mut inner = ui.new_child(UiBuilder::new().max_rect(row.shrink2(vec2(6.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
    inner.set_clip_rect(row.intersect(ui.clip_rect()));
    let shown = app.shown_in(pane);
    let mut close = None;
    let mut clicked = None;
    let mut move_side = None;
    let buttons = if pane == Pane::Right { 34.0 } else { 0.0 };
    inner.scope(|ui| {
        ui.style_mut().always_scroll_the_only_direction = true;
        // No scroll bar under the tabs (it would push them out of the row); the wheel scrolls them.
        egui::ScrollArea::horizontal()
            .id_salt(("split-tabs", pane.slot()))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .max_width((ui.available_width() - buttons).max(0.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for i in 0..app.views.len() {
                        if app.pane_of(i) != pane {
                            continue;
                        }
                        let Some(doc) = app.session.get(app.views[i].id) else { continue };
                        let (name, dirty) = (doc.display_name(), doc.dirty);
                        let uid = app.views[i].uid;
                        // Clicked to show it; dragged onto the other side to move it there.
                        // A tab showing the document's title names its file on hover.
                        let file = (name != doc.name).then_some(doc.name.as_str());
                        let resp =
                            crate::chrome::tab(ui, t, "file-text", &name, file, dirty, shown == Some(i), &mut close, i, None).interact(Sense::drag());
                        if resp.drag_started() {
                            app.split.dragging = Some(uid);
                        }
                        if resp.clicked() {
                            clicked = Some(i);
                        }
                        resp.context_menu(|ui| {
                            if ui.button(tl!("Move to the other side")).clicked() {
                                move_side = Some(i);
                                ui.close();
                            }
                            if ui.button(tl!("Close")).clicked() {
                                close = Some(i);
                                ui.close();
                            }
                        });
                    }
                });
            });
    });
    if pane == Pane::Right {
        inner.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icons::button(ui, "columns-2", 26.0, true, tl!("Close split view")).clicked() {
                app.close_split();
            }
        });
    }
    if let Some(i) = clicked {
        app.active = Some(i);
    }
    if let Some(i) = move_side {
        app.move_to_other_side(i);
    } else if let Some(i) = close {
        app.request_close_tab(i);
    }
}
