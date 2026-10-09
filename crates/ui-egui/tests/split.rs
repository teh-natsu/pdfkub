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
