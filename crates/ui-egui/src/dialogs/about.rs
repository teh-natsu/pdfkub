//! Help ▸ About: PdfKub's own one-page dialog. Its icon, name, version and the fonts and libraries
//! it is built with, and a plain-text credit to upstream (PdfKub is based on PdfCraft, NOTICE). No
//! Contributors or Models tab and no project links: the owner's choice for PdfKub.

use craft::ui::contract::{Bind, Show as _};
use craft::ui::dialog::{DialogId, DialogResult, DialogStack, StackDialog, dialog_id};
use craft::ui::display::{Picture, PictureSize, Shade, Text};
use craft::ui::list::ThumbSize;
use craft::ui::patterns::form_dialogs::{AboutTab, FormDialog};
use craft::ui::sections::{DialogRole, FooterButton};
use craft::ui::style::TypeRole;
use egui::Ui;

use crate::PdfKubApp;

/// PdfKub's app icon, drawn through the framework's SVG picture.
const ICON: &[u8] = include_bytes!("../../../../assets/app-icon/pdfkub.svg");

/// The About dialog, owned by the dialog host.
pub(crate) struct AboutDialog {
    id: DialogId,
}

impl StackDialog for AboutDialog {
    type Result = ();

    fn id(&self) -> DialogId {
        self.id
    }

    fn show(&mut self, ui: &mut Ui, _: &DialogStack) -> Option<DialogResult<()>> {
        let close = FooterButton::new(tl!("Close").to_string(), DialogRole::Cancel);
        let mut nothing = ();
        let out = FormDialog::new(tl!("About PdfKub").to_string(), Bind::bound(&mut nothing)).shaped(|d| d.buttons([close])).show(ui, |ui, _, _| {
            let _ = Picture::svg("PdfKub".to_string(), ICON).extent(PictureSize::Thumb(ThumbSize::Medium)).show(ui);
            let _ = Text::new("PdfKub".to_string()).role(TypeRole::Heading).show(ui);
            let _ = Text::new(crate::i18n::fmt(tl!("Version {v}"), &[("v", &pdfcraft_engine::build_info::version())])).show(ui);
            let _ = Text::new(
                "Rendering: hayro (bootstrap) · UI: egui · Icons: Lucide (ISC) · Fonts: Inter, Noto Sans Thai, Sriracha, JetBrains Mono, Dancing Script (OFL)"
                    .to_string(),
            )
            .role(TypeRole::Caption)
            .shade(Shade::Muted)
            .show(ui);
            let _ = Text::new(tl!("Based on PdfCraft by the ArtCraft team.").to_string()).shade(Shade::Muted).show(ui);
            true
        });
        out.closed.map(|_| DialogResult::Cancel)
    }
}

impl PdfKubApp {
    /// Opens the About window (Help ▸ About, `help.about`). `_tab` is upstream's tab choice; PdfKub's
    /// About has one page.
    pub(crate) fn open_about(&mut self, _tab: AboutTab) {
        crate::dialogs::host::open(self, AboutDialog { id: dialog_id("about") });
    }

    /// Nothing to hand on: PdfKub's About has no links.
    pub(crate) fn about_pass(&mut self, _ctx: &egui::Context) {}
}
