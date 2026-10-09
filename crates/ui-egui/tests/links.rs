//! PdfKub shows no project links or open-source notices in the app. About credits upstream in
//! plain text ("Based on PdfCraft by the ArtCraft team.").

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::links;
use pdfcraft_ui_egui::{Dialog, PdfKubApp};

fn harness(setup: impl FnOnce(&mut PdfKubApp) + 'static) -> Harness<'static, PdfKubApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfKubApp::new();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn home_screen_has_no_project_links() {
    let h = harness(|_| {});
    h.get_by_label("Welcome to PdfKub");
    h.get_by_label("A PDF workbench — local, private, and scriptable.");
    for gone in ["PdfKub on GitHub", "PdfKub is open source"] {
        assert_eq!(h.query_all_by_label(gone).count(), 0, "{gone}");
    }
}

#[test]
fn about_dialog_credits_pdfkub_without_links() {
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    // With a document open, so the home screen is not on screen.
    let h = harness(move |app| {
        app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
        app.dialog = Some(Dialog::About);
    });
    h.get_by_label("Based on PdfCraft by the ArtCraft team.");
    assert_eq!(h.query_all_by_label("PdfKub on GitHub").count(), 0);
    assert!(links::LINKS.is_empty());
}

#[test]
fn help_commands_open_each_link() {
    for l in links::LINKS {
        let mut h = harness(|_| {});
        assert!(h.state_mut().execute(l.command), "{}", l.command);
        assert_eq!(h.state().last_opened_url.as_deref(), Some(l.url));
        assert_eq!(pdfcraft_engine::commands::command(l.command).unwrap().menu, Some("Help"));
    }
}

#[test]
fn about_dialog_has_no_contributors_or_models_tab() {
    let h = harness(|app| app.dialog = Some(Dialog::About));
    h.get_by_label("Based on PdfCraft by the ArtCraft team.");
    for gone in ["Contributors", "Models"] {
        assert_eq!(h.query_all_by_label(gone).count(), 0, "{gone}");
    }
}
