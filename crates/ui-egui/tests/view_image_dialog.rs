//! The "Save 3D view image" dialog opens from the 3D view's bar and shows the size it will write.
//! Also captures screenshots when MC_SNAPSHOT_DIR is set.

use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};

fn harness() -> Harness<'static, MaterialCraftApp> {
    let app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    let builder = Harness::builder().with_size(vec2(1600.0, 960.0)).with_step_dt(1.0 / 60.0);
    let builder = if std::env::var_os("MC_SNAPSHOT_DIR").is_some() { builder.wgpu() } else { builder };
    let mut h = builder.build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app,
    );
    h.run_steps(30);
    h
}

fn save_snapshot(harness: &mut Harness<'static, MaterialCraftApp>, name: &str) {
    if let Some(directory) = std::env::var_os("MC_SNAPSHOT_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        harness.render().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn the_bar_button_opens_the_dialog_and_close_hides_it() {
    let mut h = harness();
    save_snapshot(&mut h, "view-image-1-bar");
    assert!(!h.state().ui.windows.view_image);
    // The 2D view has a button with the same name; the 3D one is the one that opens the dialog
    assert_eq!(h.get_all_by_label("Save image...").count(), 2, "one Save image button per view");
    for i in 0..2 {
        h.get_all_by_label("Save image...").nth(i).expect("button").click();
        h.run_steps(5);
        if h.state().ui.windows.view_image {
            break;
        }
    }
    assert!(h.state().ui.windows.view_image);
    h.get_by_label("Save 3D view image");
    h.get_by_label("Save...");
    save_snapshot(&mut h, "view-image-2-dialog");
    h.get_by_label("Close").click();
    h.run_steps(3);
    assert!(!h.state().ui.windows.view_image);
}

#[test]
fn a_chosen_width_is_shown_with_the_resulting_size() {
    let mut h = harness();
    h.state_mut().ui.windows.view_image = true;
    h.state_mut().ui.view_image_width = 2048;
    h.state_mut().ui.view_image_hq = true;
    h.run_steps(5);
    save_snapshot(&mut h, "view-image-3-2048-hq");
    assert!(h.query_by_label_contains("2048 x ").is_some(), "the dialog should state the output size");
}
