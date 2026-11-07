use super::{GraphCamera, NodeSearch};
use crate::{MaterialCraftApp, i18n::tr, theme::Tokens};
use egui::{Pos2, Ui};
use egui_phosphor::regular as icon;
use materialcraft_engine::core::NodeId;
use serde_json::json;

pub(super) fn open_search(app: &mut MaterialCraftApp, at: Pos2, connect: Option<(NodeId, String)>) {
    app.graph.search = Some(NodeSearch { at, text: String::new(), sel: 0, connect, focus: true });
}

/// Catalog entries matching `text` (fuzzy score across label, id, and category)
pub(super) fn matches(text: &str) -> Vec<&'static materialcraft_engine::nodes::NodeDefinition> {
    let text = text.trim();
    let catalog = materialcraft_engine::nodes::catalog().iter().filter(|d| d.id != "graph_instance");
    if text.is_empty() {
        return catalog.collect();
    }
    let mut scored: Vec<(i32, &'static materialcraft_engine::nodes::NodeDefinition)> = catalog
        .filter_map(|d| {
            let label_score = crate::fuzzy::fuzzy_score(d.label, text);
            let id_score = crate::fuzzy::fuzzy_score(d.id, text);
            let cat_score = crate::fuzzy::fuzzy_score(d.category, text).map(|s| s.saturating_sub(20));
            let score = label_score.max(id_score).max(cat_score)?;
            Some((score, d))
        })
        .collect();
    scored.sort_by(|(score_a, a), (score_b, b)| score_b.cmp(score_a).then_with(|| a.label.cmp(b.label)));
    scored.into_iter().map(|(_, d)| d).collect()
}

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, cam: &GraphCamera) {
    let Some(s) = app.graph.search.as_mut() else { return };

    let tk = Tokens::get(ui.ctx());
    let screen = cam.to_screen(s.at);
    let mut close = false;
    let mut pick: Option<&'static str> = None;
    egui::Area::new(egui::Id::unique("node-search")).fixed_pos(screen).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(260.0);
            let mut reveal_selection = s.focus;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icon::MAGNIFYING_GLASS).color(tk.text_dim));
                let r = ui.add(egui::TextEdit::singleline(&mut s.text).hint_text(tr("Search nodes...")).desired_width(220.0));
                crate::widgets::name(&r, egui::Role::SearchInput, "Search nodes");
                if s.focus {
                    r.request_focus();
                    s.focus = false;
                }
                if r.changed() {
                    s.sel = 0;
                    reveal_selection = true;
                }
            });
            let list = matches(&s.text);
            let (up, down, enter, esc) = ui.input(|i| {
                (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape))
            });
            if down {
                s.sel = (s.sel + 1).min(list.len().saturating_sub(1));
            }
            if up {
                s.sel = s.sel.saturating_sub(1);
            }
            if esc {
                close = true;
            }
            if enter && let Some(d) = list.get(s.sel) {
                pick = Some(d.id);
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (i, d) in list.iter().enumerate() {
                    let r = ui.add(
                        egui::Button::selectable(i == s.sel, format!("{}   ", d.label))
                            .min_size(egui::vec2(ui.available_width(), 0.0))
                            .right_text(egui::RichText::new(d.group()).size(10.0).color(tk.text_dim)),
                    );
                    if i == s.sel && (up || down || reveal_selection) {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        pick = Some(d.id);
                    }
                }
                if list.is_empty() {
                    ui.label(egui::RichText::new(tr("No matching node")).color(tk.text_dim));
                }
            });
        });
    });
    let clicked_out = ui.input(|i| i.pointer.any_pressed())
        && ui.ctx().pointer_interact_pos().is_some_and(|p| !ui.ctx().layer_id_at(p).is_some_and(|l| l.id == egui::Id::unique("node-search")));
    if let Some(def) = pick {
        let s = app.graph.search.take();
        if let Some(s) = s {
            let mut p = json!({"def": def, "at": [s.at.x, s.at.y - 20.0]});
            if let Some((n, port)) = s.connect {
                p["connectFrom"] = json!({"node": n.0, "port": port});
            }
            app.run("graph.node.add", p);
        }
    } else if close || clicked_out {
        app.graph.search = None;
    }
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn empty_query_matches_catalog() {
        let all = matches("");
        assert!(!all.is_empty());
        assert_eq!(all[0].id, "bitmap");
    }

    #[test]
    fn acronym_and_fuzzy_subsequence_ranks_expected_nodes() {
        let dblur = matches("dblur");
        assert!(!dblur.is_empty());
        assert_eq!(dblur[0].id, "directional_blur");

        let t2d = matches("t2d");
        assert!(!t2d.is_empty());
        assert_eq!(t2d[0].id, "transformation");

        let dirwarp = matches("dirwarp");
        assert!(!dirwarp.is_empty());
        assert_eq!(dirwarp[0].id, "directional_warp");

        let grayconv = matches("grayconv");
        assert!(!grayconv.is_empty());
        assert_eq!(grayconv[0].id, "grayscale_conversion");
    }

    #[test]
    fn exact_match_ranks_first() {
        let list = matches("blur");
        assert!(!list.is_empty());
        assert_eq!(list[0].id, "blur");
    }

    #[test]
    fn unmatched_query_returns_empty() {
        assert!(matches("nonexistentnodename12345").is_empty());
    }
}
