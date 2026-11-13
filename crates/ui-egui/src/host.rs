//! The entry points a host that shows PdfCraft inside its own window needs (Bace, `apps/bace`).
//!
//! PdfCraft's own shells own a window and a file system: they open by path, write wherever a
//! picker says, and keep preferences in `eframe` storage. A host with none of that reaches the
//! app through this module only: bytes in, bytes out, and the revision that tells an edit from a
//! repaint. Keeping it here leaves the shells' paths untouched.

use std::sync::Arc;

use pdfcraft_engine::DocId;

use crate::PdfCraftApp;

/// The paragraph [`starter_pdf`] draws.
const STARTER_TEXT: &str = "\
Bace shows Craft apps on an infinite canvas.\n\
\n\
This window hosts PdfCraft. Page through it with the wheel or the toolbar, add a comment with the \
comment tool, and the window hands the edited file back to the field, which stores it with the \
other documents.\n\
\n\
Saving is the field's: the window offers its bytes, the field keeps them, and the next reader \
opens the same annotations.";

impl PdfCraftApp {
    /// Opens `bytes`, a PDF called `name`, as a new unsaved tab in front.
    ///
    /// The document has no path of its own: Save in the app would ask where to write it, which a
    /// host that owns the bytes does not want. It calls [`Self::save_front_bytes`] instead.
    pub fn open_pdf_bytes(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), String> {
        self.open_created_bytes(name, Ok(Arc::new(bytes)))
    }

    /// The document in front as `(id, revision)`, or `None` when the window shows no document.
    ///
    /// The revision moves on every change to the working file — an edit, an undo, a save — and
    /// never on a scroll or a repaint. That is what lets a host tell "the document changed" from
    /// "the window was redrawn", and encode only when it did.
    pub fn document_revision(&self) -> Option<(u64, u64)> {
        let (_, id) = self.active_ids()?;
        Some((id.0, self.session.get(id)?.edit_generation()))
    }

    /// The bytes Save would write for the document in front, as `(id, bytes)`, without recording
    /// a save: what is typed in a field is committed first, as the app's own Save does.
    ///
    /// The host hands the bytes back with [`Self::mark_front_saved`] once they are stored. A host
    /// that asks twice without an edit in between gets the same bytes.
    pub fn save_front_bytes(&mut self) -> Result<(u64, Vec<u8>), String> {
        let (_, id) = self.active_ids().ok_or("no document in front")?;
        // A field being typed into is part of what is saved (#166). A Fill & Sign mark stays a
        // comment when it cannot be flattened: the app's own Save settles on the same.
        if !self.commit_typing_in(id) {
            return Err("a field could not be committed".to_string());
        }
        let _ = self.flatten_fill_sign_for_save(id);
        self.run_document_actions(id, pdfcraft_engine::js::DocumentScriptEvent::WillSave);
        let bytes = self.session.save_bytes(id).map_err(|e| e.to_string())?;
        Ok((id.0, bytes.as_ref().clone()))
    }

    /// Records that the bytes [`Self::save_front_bytes`] gave for document `id` are stored: the
    /// next save appends only the edits made since, and the document is no longer unsaved.
    ///
    /// Recording a save moves the revision ([`Self::document_revision`]): a host that remembers
    /// the revision it stored must re-read it afterwards, or it would store the same bytes again.
    pub fn mark_front_saved(&mut self, id: u64, bytes: Vec<u8>) -> Result<(), String> {
        let id = DocId(id);
        self.session.mark_saved(id, Arc::new(bytes), None).map_err(|e| e.to_string())?;
        // The mark was locked into the page by this save, so it is not asked about again (#2149).
        self.placed_signatures.remove(&id);
        if let Some(doc) = self.session.get(id)
            && let Some(view) = self.views.iter_mut().find(|v| v.id == id)
        {
            view.document_changed(&doc.info);
        }
        Ok(())
    }
}

/// A one-page PDF for a window that opens without a document: a short paragraph, drawn by the
/// same Create path File ▸ Create ▸ Blank text uses.
pub fn starter_pdf() -> Result<Vec<u8>, String> {
    pdfcraft_engine::Session::new().create_from_text("Sample", STARTER_TEXT).map(|bytes| bytes.as_ref().clone()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfcraft_engine::{Edit, NewAnnotation, NoteIcon, Shape, Style};

    /// A comment on the first page, placed through the same edit the comment tool runs.
    fn add_note(app: &mut PdfCraftApp) -> &'static str {
        let (_, id) = app.active_ids().expect("a document in front");
        let at = app.session.get(id).expect("the document").info.pages[0].view_to_user(100.0, 100.0);
        let shape = Shape::Note { at: [f64::from(at[0]), f64::from(at[1])], icon: NoteIcon::Comment };
        let style = Style::default_for(&shape);
        let edit = Edit::AddAnnotation(NewAnnotation { page: 0, style, shape, contents: "A note".into(), author: "Bace".into() });
        assert!(app.apply_edit(edit), "the comment is applied");
        "A note"
    }

    #[test]
    fn a_comment_survives_the_bytes_a_host_stores() {
        let opened = starter_pdf().expect("the starter document is drawn");
        let mut app = PdfCraftApp::new();
        app.open_pdf_bytes("sample.pdf", opened.clone()).expect("the starter document opens");
        let (id, revision) = app.document_revision().expect("a document in front");
        // A document opens unsaved: a host seeds its stored revision here, so the first tick
        // after an open stores nothing.
        assert!(revision > 0, "opening marks the document unsaved");

        let contents = add_note(&mut app);
        let (saved_id, saved) = app.save_front_bytes().expect("the comment is saved");
        assert_eq!(saved_id, id, "the document that was edited is the one saved");
        assert_ne!(saved, opened, "the comment is in the bytes");

        app.mark_front_saved(saved_id, saved.clone()).expect("the save is recorded");
        let after = app.document_revision().expect("a document in front");
        assert_eq!(after.0, id, "recording a save keeps the document");
        assert_ne!(after.1, revision, "recording a save moves the revision");
        assert_eq!(app.save_front_bytes().expect("nothing new is written").1, saved, "no edit, no new bytes");

        // What the field stores is what the next window opens.
        let mut reopened = PdfCraftApp::new();
        reopened.open_pdf_bytes("sample.pdf", saved).expect("the stored bytes open");
        let (_, id) = reopened.active_ids().expect("a document in front");
        let doc = reopened.session.get(id).expect("the document");
        assert_eq!(doc.info.annotations.len(), 1, "the comment is there");
        assert_eq!(doc.info.annotations[0].contents.as_deref(), Some(contents));
    }

    #[test]
    fn a_window_without_a_document_has_no_revision_and_nothing_to_save() {
        let mut app = PdfCraftApp::new();
        assert_eq!(app.document_revision(), None);
        assert_eq!(app.save_front_bytes().err().as_deref(), Some("no document in front"));
    }
}
