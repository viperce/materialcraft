//! Save the 3D view as an image file: a small dialog (size and quality), a native save dialog, and a
//! worker thread that renders at the chosen size and writes the PNG, so the UI never waits for it.
//!
//! The picture shows what the view shows (same scene, displaced geometry, lighting, camera and
//! channels). Only the size and the sampling quality differ.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{RichText, vec2};

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Longest side of a saved image, in pixels (the same bound as the live view's texture).
pub const MAX_SIDE: u32 = 4096;
/// The widths offered next to "as in the view" (0 in the state means the view's own size).
pub const WIDTHS: [u32; 3] = [1024, 2048, 4096];
/// Size used before the view has drawn a first frame.
const FALLBACK_SIZE: (u32, u32) = (1024, 768);
/// Sampling of a normal save: 2 x 2 per pixel, like the settled view.
#[cfg(not(target_arch = "wasm32"))]
const SUPERSAMPLE: u32 = 2;
/// Sampling of a high quality save, matching the headless `hq` render.
#[cfg(not(target_arch = "wasm32"))]
const HQ_SUPERSAMPLE: u32 = 3;
#[cfg(not(target_arch = "wasm32"))]
const HQ_AO_RAYS: u32 = 8;
#[cfg(not(target_arch = "wasm32"))]
const HQ_KEY_LIGHT: f32 = 2.5;

/// Threads a save may use: half of the machine's, at least one. The rest stay free so the live view,
/// which renders on the global pool, keeps its previews and settled frames while a save runs.
#[cfg(not(target_arch = "wasm32"))]
fn save_threads(available: usize) -> usize {
    (available / 2).max(1)
}

/// The pool the save renders on, built once; `None` if the system refuses to start its threads.
#[cfg(not(target_arch = "wasm32"))]
fn save_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let available = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
        rayon::ThreadPoolBuilder::new().num_threads(save_threads(available)).thread_name(|i| format!("materialcraft-save-{i}")).build().ok()
    })
    .as_ref()
}

/// A save in progress: the worker reports the written path or an error text.
pub struct SaveJob {
    rx: Receiver<Result<PathBuf, String>>,
}

/// Pixel size of the saved image: `width` wide (0 = the view's own size) at the view's proportions,
/// both sides kept within `1..=MAX_SIDE`.
pub fn target_size(view: (u32, u32), width: u32) -> (u32, u32) {
    let (vw, vh) = (view.0.max(1), view.1.max(1));
    let w = if width == 0 { vw } else { width };
    let h = (u64::from(w) * u64::from(vh) + u64::from(vw) / 2) / u64::from(vw);
    let (w, h) = (f64::from(w), h.max(1) as f64);
    let shrink = (w.max(h) / f64::from(MAX_SIDE)).max(1.0);
    (((w / shrink).round() as u32).clamp(1, MAX_SIDE), ((h / shrink).round() as u32).clamp(1, MAX_SIDE))
}

/// Size of the last frame the view drew, in pixels.
fn view_size(app: &MaterialCraftApp) -> (u32, u32) {
    app.view3d.shown.as_ref().map_or(FALLBACK_SIZE, |(w, h, _)| (*w, *h))
}

/// The "Save 3D view image" dialog.
pub fn dialog(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    let mut open = true;
    let mut save = false;
    let mut close = false;
    let view = view_size(app);
    crate::windows::modal(ctx, "view-image", tr("Save 3D view image"), &mut open, vec2(400.0, 200.0), |ui| {
        let tk = Tokens::get(ui.ctx());
        let current = format!("{} ({} x {})", tr("As in the view"), view.0, view.1);
        let label = match app.ui.view_image_width {
            0 => current.clone(),
            w => format!("{w} px"),
        };
        crate::widgets::combo(ui, tr("Width"), label, |ui| {
            ui.selectable_value(&mut app.ui.view_image_width, 0, current);
            for w in WIDTHS {
                ui.selectable_value(&mut app.ui.view_image_width, w, format!("{w} px"));
            }
        });
        let (w, h) = target_size(view, app.ui.view_image_width);
        ui.label(RichText::new(format!("{w} x {h} px")).size(10.5).color(tk.text_dim));
        ui.checkbox(&mut app.ui.view_image_hq, tr("High quality (slower)"));
        ui.add_space(8.0);
        let busy = app.view3d.save.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, crate::widgets::primary_button(ui, tr("Save..."))).clicked() {
                save = true;
            }
            if ui.button(tr("Close")).clicked() {
                close = true;
            }
            if busy {
                ui.spinner();
                ui.label(RichText::new(tr("Rendering...")).size(10.5).color(tk.text_dim));
            }
        });
    });
    if save {
        start(app, ctx);
    }
    if !open || close {
        app.ui.windows.view_image = false;
    }
}

/// Ask for a file, then render and write on a worker thread.
#[cfg(not(target_arch = "wasm32"))]
fn start(app: &mut MaterialCraftApp, ctx: &egui::Context) {
    if app.view3d.save.is_some() {
        return;
    }
    let name = format!("{}.png", super::scene_label(app).replace(['/', '\\', ':'], "_"));
    let Some(path) = rfd::FileDialog::new().set_file_name(name).add_filter("PNG", &["png"]).save_file() else { return };
    let hq = app.ui.view_image_hq;
    let Some((scene, light, maps, mut settings)) = super::inputs(app, if hq { HQ_SUPERSAMPLE } else { SUPERSAMPLE }) else {
        app.toast(tr("The 3D view has nothing to save yet"), true);
        return;
    };
    if hq {
        settings.ao_rays = HQ_AO_RAYS;
        settings.key_light = HQ_KEY_LIGHT;
    }
    let (w, h) = target_size(view_size(app), app.ui.view_image_width);
    let (tx, rx) = std::sync::mpsc::channel();
    let (repaint, viewport) = (ctx.clone(), ctx.viewport_id());
    let spawned = std::thread::Builder::new().name("materialcraft-save-view".into()).spawn(move || {
        let draw = || materialcraft_render::render(&scene, &maps, &light, &settings, w, h);
        let px = match save_pool() {
            Some(pool) => pool.install(draw),
            None => draw(),
        };
        let _ = tx.send(write_png(&path, w, h, px).map(|()| path));
        repaint.request_repaint_of(viewport);
    });
    match spawned {
        Ok(_) => app.view3d.save = Some(SaveJob { rx }),
        Err(e) => app.toast(format!("{}: {e}", tr("Could not start the render")), true),
    }
}

/// The browser cannot write files from here.
#[cfg(target_arch = "wasm32")]
fn start(_app: &mut MaterialCraftApp, _ctx: &egui::Context) {}

/// Encode RGBA8 pixels as a PNG and write it atomically.
#[cfg(not(target_arch = "wasm32"))]
fn write_png(path: &Path, w: u32, h: u32, rgba: Vec<u8>) -> Result<(), String> {
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| "the render has the wrong size".to_string())?;
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    materialcraft_engine::formats::write_atomic(path, &png).map_err(|e| e.to_string())
}

/// Report a finished save (a toast); call every frame.
pub fn poll(app: &mut MaterialCraftApp) {
    let Some(job) = &app.view3d.save else { return };
    let done = match job.rx.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err(tr("The render stopped unexpectedly").to_string()),
    };
    app.view3d.save = None;
    match done {
        Ok(path) => app.toast(format!("{} {}", tr("Saved"), path.display()), false),
        Err(e) => app.toast(e, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_save_takes_half_the_threads_and_never_none() {
        assert_eq!(save_threads(0), 1);
        assert_eq!(save_threads(1), 1);
        assert_eq!(save_threads(2), 1);
        assert_eq!(save_threads(8), 4);
        assert_eq!(save_threads(24), 12);
    }

    #[test]
    fn zero_width_keeps_the_view_size() {
        assert_eq!(target_size((800, 600), 0), (800, 600));
    }

    #[test]
    fn a_chosen_width_keeps_the_view_proportions() {
        assert_eq!(target_size((800, 600), 2048), (2048, 1536));
        assert_eq!(target_size((600, 800), 1024), (1024, 1365));
    }

    #[test]
    fn sides_stay_within_the_limit_and_never_reach_zero() {
        assert_eq!(target_size((100, 1000), 4096), (410, 4096));
        assert_eq!(target_size((0, 0), 0), (1, 1));
        assert_eq!(target_size((10_000, 1), 4096), (4096, 1));
        let (w, h) = target_size((u32::MAX, u32::MAX), u32::MAX);
        assert!((1..=MAX_SIDE).contains(&w) && (1..=MAX_SIDE).contains(&h));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn write_png_round_trips_and_rejects_a_wrong_buffer() {
        let dir = std::env::temp_dir().join(format!("mc-save-view-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v.png");
        write_png(&path, 2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        let back = image::open(&path).unwrap().to_rgba8();
        assert_eq!((back.width(), back.height()), (2, 1));
        assert_eq!(back.get_pixel(1, 0).0, [0, 255, 0, 255]);
        assert!(write_png(&path, 2, 2, vec![0; 4]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
