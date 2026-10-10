//! Split view, like an editor's Split Right: two sides with their own tabs, focus that follows
//! the side clicked, one document open on both sides, and the split ending with a side's last tab.

use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfKubApp;
/// `n` pages of 200×300; page i says "Page i+1" plus "page" and "Pages" for find tests.
fn fixture(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for i in 0..n {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
        let body = format!("BT /F1 18 Tf 20 150 Td (Page {} pages PAGE) Tj ET", i + 1);
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn harness(files: &[(&'static str, usize)]) -> Harness<'static, PdfKubApp> {
    let files = files.to_vec();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfKubApp::new();
        app.set_option("language", "en").unwrap();
        for (name, pages) in &files {
            app.open_bytes(name, None, fixture(*pages)).unwrap();
        }
        app
    });
    settle(&mut h);
    h
}

/// Run frames until no view is waiting for pages (each side must get its own rasters).
fn settle(h: &mut Harness<'static, PdfKubApp>) {
    for _ in 0..80 {
        h.run_steps(2);
        if !h.state().render_pending() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("a view never got its pages");
}

fn name_of(app: &PdfKubApp, i: usize) -> String {
    app.session.get(app.views[i].id).unwrap().display_name()
}

#[test]
fn split_right_shows_one_document_on_both_sides() {
    let mut h = harness(&[("a.pdf", 5)]);
    h.state_mut().execute("view.split_right");
    settle(&mut h);
    let app = h.state();
    assert!(app.is_split());
    assert_eq!(app.views.len(), 2);
    assert_eq!(app.views[0].id, app.views[1].id, "the same document on both sides");
    let (left, right) = (app.shown_in(pdfcraft_ui_egui::split::Pane::Left).unwrap(), app.shown_in(pdfcraft_ui_egui::split::Pane::Right).unwrap());
    assert_ne!(left, right);
    assert_eq!(app.active, Some(right), "the new side has focus");
    // Both sides draw their pages, side by side.
    let l = app.views[left].page_screen_rect(0).expect("left page on screen");
    let r = app.views[right].page_screen_rect(0).expect("right page on screen");
    assert!(l.right() < r.left(), "{l:?} {r:?}");
}

#[test]
fn ctrl_backslash_splits_and_close_split_view_keeps_one_tab_per_document() {
    let mut h = harness(&[("a.pdf", 3), ("b.pdf", 2)]);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Backslash);
    settle(&mut h);
    assert!(h.state().is_split());
    assert_eq!(h.state().views.len(), 3);
    h.state_mut().execute("view.split_close");
    h.run_steps(2);
    let app = h.state();
    assert!(!app.is_split());
    let mut names: Vec<String> = (0..app.views.len()).map(|i| name_of(app, i)).collect();
    names.sort();
    assert_eq!(names, ["a.pdf", "b.pdf"]);
    assert_eq!(app.active.map(|i| name_of(app, i)).as_deref(), Some("b.pdf"), "the tab that had focus stays active");
}

#[test]
fn tabs_move_between_sides_and_a_click_gives_the_other_side_focus() {
    let mut h = harness(&[("a.pdf", 3), ("b.pdf", 2)]);
    let b = (0..2).find(|&i| name_of(h.state(), i) == "b.pdf").unwrap();
    h.state_mut().move_to_other_side(b);
    settle(&mut h);
    use pdfcraft_ui_egui::split::Pane;
    let a = (0..2).find(|&i| name_of(h.state(), i) == "a.pdf").unwrap();
    assert!(h.state().is_split());
    assert_eq!(h.state().shown_in(Pane::Left), Some(a));
    assert_eq!(h.state().shown_in(Pane::Right), Some(b));
    assert_eq!(h.state().active, Some(b));
    // A click on the left side's page moves the focus there; it doesn't select or edit anything.
    let page = h.state().views[a].page_screen_rect(0).expect("left page on screen");
    h.drag_at(page.center());
    h.run_steps(1);
    h.drop_at(page.center());
    h.run_steps(2);
    assert_eq!(h.state().active, Some(a));
    assert!(h.state().views[a].selected_text().is_none());
    // Commands act on the side with focus.
    h.state_mut().execute("view.layout.two_up");
    h.run_steps(2);
    assert_eq!(h.state().views[a].layout, pdfcraft_ui_egui::canvas::PageLayout::TwoUp);
    assert_ne!(h.state().views[b].layout, pdfcraft_ui_egui::canvas::PageLayout::TwoUp);
}

#[test]
fn closing_a_sides_last_tab_ends_the_split() {
    let mut h = harness(&[("a.pdf", 3), ("b.pdf", 2)]);
    let b = (0..2).find(|&i| name_of(h.state(), i) == "b.pdf").unwrap();
    h.state_mut().move_to_other_side(b);
    h.run_steps(2);
    assert!(h.state().is_split());
    h.state_mut().request_close_tab(b);
    h.run_steps(2);
    let app = h.state();
    assert!(!app.is_split());
    assert_eq!(app.views.len(), 1);
    assert_eq!(name_of(app, 0), "a.pdf");
    assert_eq!(app.active, Some(0));
}

#[test]
fn closing_one_side_of_a_document_open_on_both_keeps_it_open_without_asking() {
    let mut h = harness(&[("a.pdf", 3)]);
    h.state_mut().execute("view.split_right");
    h.run_steps(2);
    let id = h.state().views[0].id;
    h.state_mut().session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
    h.state_mut().request_close_tab(1);
    h.run_steps(2);
    let app = h.state();
    assert!(app.close_request.is_none(), "no save prompt: the document is still open");
    assert_eq!(app.views.len(), 1);
    assert!(app.session.get(id).is_some_and(|d| d.dirty), "the edit is still there");
    assert!(!app.is_split());
}

/// A pointer click on a tab in a side's row shows it there, and the tabs sit above the pages
/// (a tab reaching into the page area lost its clicks to the page).
#[test]
fn clicking_a_tab_on_a_side_shows_it() {
    use egui_kittest::kittest::Queryable;
    use pdfcraft_ui_egui::split::Pane;
    let mut h = harness(&[("a.pdf", 3), ("b.pdf", 2), ("c.pdf", 2)]);
    let index = |h: &Harness<'static, PdfKubApp>, name: &str| (0..h.state().views.len()).find(|&i| name_of(h.state(), i) == name).unwrap();
    let c = index(&h, "c.pdf");
    h.state_mut().move_to_other_side(c);
    settle(&mut h);
    let (a, b) = (index(&h, "a.pdf"), index(&h, "b.pdf"));
    // The left side shows one of a and b; click the other one's tab.
    let shown = h.state().shown_in(Pane::Left).unwrap();
    let (other, name) = if shown == a { (b, "b.pdf") } else { (a, "a.pdf") };
    let tab = h.get_by_label(name).rect();
    let page = h.state().views[shown].page_screen_rect(0).expect("left page on screen");
    assert!(tab.bottom() <= page.top(), "tab {tab:?} reaches into the page {page:?}");
    h.hover_at(tab.center());
    h.run_steps(1);
    h.drag_at(tab.center());
    h.run_steps(1);
    h.drop_at(tab.center());
    h.run_steps(2);
    assert_eq!(h.state().active, Some(other));
    assert_eq!(h.state().shown_in(Pane::Left), Some(other));
    assert_eq!(h.state().shown_in(Pane::Right).map(|i| name_of(h.state(), i)).as_deref(), Some("c.pdf"));
}

/// One document on both sides at different zooms shares one render pool: each side asks for and
/// gets its own rasters, and neither keeps replacing the other's queue.
#[test]
fn one_document_on_both_sides_renders_each_side_at_its_own_zoom() {
    let mut h = harness(&[("a.pdf", 3)]);
    h.state_mut().execute("view.split_right");
    settle(&mut h);
    let right = h.state().active.unwrap();
    h.state_mut().views[right].set_zoom(2.0);
    settle(&mut h);
    let app = h.state();
    let left = app.shown_in(pdfcraft_ui_egui::split::Pane::Left).unwrap();
    assert!((app.views[left].zoom - app.views[right].zoom).abs() > 0.1, "the sides keep their own zoom");
    assert!(!app.views[left].render_pending() && !app.views[right].render_pending());
    assert!(app.views[left].page_screen_rect(0).is_some() && app.views[right].page_screen_rect(0).is_some());
}

/// Two documents, one on each side: both stay rendered, and the tab hidden behind another on a
/// side gives up its rasters (only the documents in view keep textures).
#[test]
fn two_documents_side_by_side_both_render() {
    let mut h = harness(&[("a.pdf", 2), ("b.pdf", 2), ("c.pdf", 2)]);
    h.state_mut().active = Some(1);
    h.state_mut().execute("view.split_right");
    settle(&mut h);
    let app = h.state();
    assert!(app.is_split());
    let (left, right) = (app.shown_in(pdfcraft_ui_egui::split::Pane::Left).unwrap(), app.shown_in(pdfcraft_ui_egui::split::Pane::Right).unwrap());
    assert!(app.views[left].page_screen_rect(0).is_some() && app.views[right].page_screen_rect(0).is_some());
    assert!(!app.render_pending(), "every view in sight has its pages, hidden ones ask for none");
}

/// Drag a tab by the pointer from its middle to just past the middle of `onto`.
fn drag_tab(h: &mut Harness<'static, PdfKubApp>, name: &str, onto: &str) {
    use egui_kittest::kittest::Queryable;
    let from = h.get_by_label(name).rect().center();
    let to = h.get_by_label(onto).rect();
    let past = egui::pos2(if to.center().x > from.x { to.right() - 6.0 } else { to.left() + 6.0 }, to.center().y);
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    for k in 1..=6 {
        h.hover_at(from + (past - from) * (k as f32 / 6.0));
        h.run_steps(1);
    }
    h.drop_at(past);
    h.run_steps(2);
}

fn order(app: &PdfKubApp) -> Vec<String> {
    (0..app.views.len()).map(|i| name_of(app, i)).collect()
}

#[test]
fn dragging_a_tab_along_the_title_bar_reorders_the_tabs() {
    let mut h = harness(&[("a.pdf", 2), ("b.pdf", 2), ("c.pdf", 2)]);
    assert_eq!(order(h.state()), ["a.pdf", "b.pdf", "c.pdf"]);
    drag_tab(&mut h, "a.pdf", "c.pdf");
    assert_eq!(order(h.state()), ["b.pdf", "c.pdf", "a.pdf"]);
    // The active tab stays the same document.
    assert_eq!(h.state().active.map(|i| name_of(h.state(), i)).as_deref(), Some("c.pdf"));
}

#[test]
fn dragging_a_tab_along_a_sides_row_reorders_it_there() {
    let mut h = harness(&[("a.pdf", 2), ("b.pdf", 2), ("c.pdf", 2), ("d.pdf", 2)]);
    let d = (0..4).find(|&i| name_of(h.state(), i) == "d.pdf").unwrap();
    h.state_mut().move_to_other_side(d);
    settle(&mut h);
    drag_tab(&mut h, "c.pdf", "a.pdf");
    let app = h.state();
    let left: Vec<String> =
        (0..app.views.len()).filter(|&i| app.pane_of(i) == pdfcraft_ui_egui::split::Pane::Left).map(|i| name_of(app, i)).collect();
    assert_eq!(left, ["c.pdf", "a.pdf", "b.pdf"]);
    assert!(app.is_split(), "a drag within a side doesn't move the tab across");
}

#[test]
fn ctrl_tab_cycles_tabs_on_the_side_with_focus() {
    let mut h = harness(&[("a.pdf", 2), ("b.pdf", 2), ("c.pdf", 2)]);
    let active = |h: &Harness<'static, PdfKubApp>| h.state().active.map(|i| name_of(h.state(), i)).unwrap();
    assert_eq!(active(&h), "c.pdf");
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Tab);
    h.run_steps(1);
    assert_eq!(active(&h), "a.pdf", "wraps around");
    h.key_press_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Tab);
    h.run_steps(1);
    assert_eq!(active(&h), "c.pdf");
    // Split: only the focused side's tabs.
    let c = (0..3).find(|&i| name_of(h.state(), i) == "c.pdf").unwrap();
    h.state_mut().move_to_other_side(c);
    h.run_steps(2);
    let a = (0..3).find(|&i| name_of(h.state(), i) == "a.pdf").unwrap();
    h.state_mut().active = Some(a);
    h.run_steps(1);
    for expected in ["b.pdf", "a.pdf"] {
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Tab);
        h.run_steps(1);
        assert_eq!(active(&h), expected);
    }
}
