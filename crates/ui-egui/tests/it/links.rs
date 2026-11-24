//! PdfKub shows no project links or open-source notices in the app. About credits upstream in
//! plain text ("Based on PdfCraft by the ArtCraft team.").

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfKubApp;

fn harness(setup: impl FnOnce(&mut PdfKubApp) + 'static) -> Harness<'static, PdfKubApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| {
        crate::support::install_runtime(&cc.egui_ctx);
        {
            let mut app = crate::control::new_app();
            app.set_option(&egui::Context::default(), "language", "en").unwrap();
            setup(&mut app);
            app
        }
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

/// Runs frames until the node labelled `label` exists and has stopped moving, then returns it. A table row is named by
/// its first cell, so a name may be the row's and its cell's: the first (outer) node is the one returned.
///
/// The About dialog is a modal centred on its own size, and switching tabs changes that size, so
/// for a few frames afterwards the modal grows and recentres and every control in it moves. A
/// fixed number of frames is a guess at how long that takes: it was too few after the 0.5.0
/// contributors refresh (78 names instead of 5) on some machines, the click on Table landed where
/// the button had been, and the table never opened (#671).
fn settled<'a>(h: &'a mut Harness<'static, PdfKubApp>, label: &'a str) -> egui_kittest::Node<'a> {
    let mut last = None;
    for _ in 0..60 {
        h.run_steps(1);
        let now = h.query_all_by_label(label).next().map(|n| n.rect());
        if now.is_some() && now == last {
            return h.query_all_by_label(label).next().expect("the node was just found");
        }
        last = now;
    }
    panic!("{label:?} never appeared or never stopped moving");
}

/// PdfKub's About is one page: no Contributors or Models tab.
#[test]
fn about_dialog_has_no_contributors_or_models_tab() {
    let mut h = harness(|app| app.set_option(&egui::Context::default(), "dialog", "about").unwrap());
    settled(&mut h, "Based on PdfCraft by the ArtCraft team.");
    for gone in ["Contributors", "Models"] {
        assert_eq!(h.query_all_by_label(gone).count(), 0, "{gone}");
    }
}
