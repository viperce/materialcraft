//! Native file dialogs. On macOS they are sheets: a dialog run modally inside the window's event
//! handler gets the window events of its own resizing sent back into that handler, and winit
//! aborts the app. A sheet returns at once and the window keeps running; a thread of its own waits
//! for the answer. Elsewhere the dialog blocks where it is asked for.
//!
//! [`Picker::ask`] shows a dialog for a `tag` naming what asked for it; the answer is
//! [`Picker::take`]n (with its tag) in the same frame, or a later one on macOS.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// The pseudo command of the folder button beside an image input: asks for an image file to bind.
/// The dialog is tagged `PICK_IMAGE:<input identifier>` ([`crate::windows::picked`]).
pub const PICK_IMAGE: &str = "ui.pickInputImage";

/// What a dialog picks.
pub enum Kind {
    Open,
    /// With the suggested file name.
    Save(String),
    Folder,
}

/// A dialog to show: what it picks and its file types (label, extensions).
pub struct Request {
    pub kind: Kind,
    pub filters: Vec<(&'static str, Vec<&'static str>)>,
}

impl Request {
    pub fn open(label: &'static str, exts: &[&'static str]) -> Self {
        Request { kind: Kind::Open, filters: vec![(label, exts.to_vec())] }
    }

    pub fn save(name: String, filters: Vec<(&'static str, Vec<&'static str>)>) -> Self {
        Request { kind: Kind::Save(name), filters }
    }

    pub fn folder() -> Self {
        Request { kind: Kind::Folder, filters: Vec::new() }
    }
}

/// The open dialog, if any, and the path it picked.
#[derive(Default)]
pub struct Picker {
    pending: Option<(String, Receiver<Option<PathBuf>>)>,
    picked: Option<(String, PathBuf)>,
}

impl Picker {
    /// Shows `request` for `tag`: one dialog at a time.
    pub fn ask(&mut self, tag: impl Into<String>, request: Request, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let tag = tag.into();
        #[cfg(target_os = "macos")]
        match sheet(request, ctx) {
            Some(answer) => self.pending = Some((tag, answer)),
            None => log::warn!("no file dialog could be shown"),
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = ctx;
            self.picked = blocking(request).map(|p| (tag, p));
        }
    }

    /// Collects the answer of the open dialog, once it has one.
    pub fn poll(&mut self) {
        let Some((tag, answer)) = &self.pending else { return };
        match answer.try_recv() {
            Err(TryRecvError::Empty) => {}
            Ok(path) => {
                self.picked = path.map(|p| (tag.clone(), p));
                self.pending = None;
            }
            Err(TryRecvError::Disconnected) => self.pending = None,
        }
    }

    /// The tag and path of a dialog that picked something, once.
    pub fn take(&mut self) -> Option<(String, PathBuf)> {
        self.picked.take()
    }

    /// Whether a dialog is open.
    pub fn open(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(not(target_os = "macos"))]
fn blocking(request: Request) -> Option<PathBuf> {
    let dialog = request.filters.iter().fold(rfd::FileDialog::new(), |d, (label, exts)| d.add_filter(*label, exts));
    match request.kind {
        Kind::Open => dialog.pick_file(),
        Kind::Save(name) => dialog.set_file_name(name).save_file(),
        Kind::Folder => dialog.pick_folder(),
    }
}

/// The sheet is begun here, on the main thread (as rfd needs); a thread waits for its answer.
#[cfg(target_os = "macos")]
fn sheet(request: Request, ctx: &egui::Context) -> Option<Receiver<Option<PathBuf>>> {
    type Answer = std::pin::Pin<Box<dyn std::future::Future<Output = Option<PathBuf>> + Send>>;
    let (sheet_tx, sheet_rx) = std::sync::mpsc::channel::<Answer>();
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    // The thread first, so the answer of a sheet once shown always has somewhere to go.
    std::thread::Builder::new()
        .name("file-dialog".into())
        .spawn(move || {
            if let Ok(answer) = sheet_rx.recv() {
                // The app gone meanwhile has no use for the answer.
                let _ = tx.send(pollster::block_on(answer));
                ctx.request_repaint();
            }
        })
        .ok()?;
    let dialog = request.filters.iter().fold(rfd::AsyncFileDialog::new(), |d, (label, exts)| d.add_filter(*label, exts));
    let path = |f: rfd::FileHandle| f.path().to_path_buf();
    let answer: Answer = match request.kind {
        Kind::Open => {
            let picked = dialog.pick_file();
            Box::pin(async move { picked.await.map(path) })
        }
        Kind::Save(name) => {
            let picked = dialog.set_file_name(name).save_file();
            Box::pin(async move { picked.await.map(path) })
        }
        Kind::Folder => {
            let picked = dialog.pick_folder();
            Box::pin(async move { picked.await.map(path) })
        }
    };
    let _ = sheet_tx.send(answer);
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dialog that has not answered is open and picks nothing yet; its answer is taken once.
    #[test]
    fn an_answer_is_taken_once_after_it_arrives() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut picker = Picker { pending: Some(("open".into(), rx)), picked: None };
        picker.poll();
        assert!(picker.open() && picker.take().is_none(), "no answer yet");
        tx.send(Some(PathBuf::from("/tmp/a.sbs"))).unwrap();
        picker.poll();
        assert!(!picker.open());
        assert_eq!(picker.take(), Some(("open".to_string(), PathBuf::from("/tmp/a.sbs"))));
        assert!(picker.take().is_none());
    }

    /// A cancelled dialog picks nothing.
    #[test]
    fn a_cancelled_dialog_picks_nothing() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut picker = Picker { pending: Some(("open".into(), rx)), picked: None };
        tx.send(None).unwrap();
        picker.poll();
        assert!(!picker.open() && picker.take().is_none());
    }
}
