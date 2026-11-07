use crate::{MaterialCraftApp, i18n::tr};
use egui::Ui;
use materialcraft_engine::FunctionEdit;

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let Some(target) = app.function_editor.as_ref().and_then(|editor| editor.command_target()) else { return };

    let operations = target.available_operations(&app.session);
    ui.add(egui::TextEdit::singleline(&mut app.ui.library_search).hint_text(tr("Search functions")));
    let search = app.ui.library_search.trim();
    let is_searching = !search.is_empty();
    let mut list: Vec<_> = operations
        .filter_map(|definition| {
            if is_searching {
                let score = crate::fuzzy::fuzzy_score(definition.label, search).max(crate::fuzzy::fuzzy_score(definition.id, search));
                score.map(|s| (s, definition))
            } else {
                Some((0, definition))
            }
        })
        .collect();
    if is_searching {
        list.sort_by(|(score_a, a), (score_b, b)| score_b.cmp(score_a).then_with(|| a.label.cmp(b.label)));
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (_, definition) in list {
            if ui.selectable_label(false, tr(definition.label)).double_clicked()
                && let Some(mut editor) = app.function_editor.take()
            {
                let at = editor.current(app).map_or([0.0; 2], |graph| [graph.nodes.len() as f32 * 140.0, 0.0]);
                editor.edit(app, FunctionEdit::Add { operation: definition.id.into(), at, snap: app.ui.prefs.snap_to_grid }, false);
                app.function_editor = Some(editor);
            }
        }
    });
}
