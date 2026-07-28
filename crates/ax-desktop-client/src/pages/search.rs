use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::{NodeDetail, SearchResult};
use crate::pages::{err_label, heading, PageCtx};

#[derive(Default)]
pub struct SearchPage {
    q: String,
    results: Vec<SearchResult>,
    selected: Option<String>,
    detail: Option<NodeDetail>,
    err: Option<String>,
    pending: Option<Receiver<Result<Vec<SearchResult>, String>>>,
    detail_pending: Option<Receiver<Result<NodeDetail, String>>>,
    last_q: String,
}

impl SearchPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(ui, "Search", "Full-text search across indexed symbols.");

        if ui
            .add(egui::TextEdit::singleline(&mut self.q).hint_text("Search symbols…"))
            .changed()
            || (ui.button("Search").clicked())
        {
            if self.q.trim() != self.last_q && !self.q.trim().is_empty() {
                self.last_q = self.q.trim().to_string();
                let c = ctx.client.clone();
                let q = self.last_q.clone();
                self.pending = Some(spawn_fetch(move || Ok(c.search(&q, 40)?.results)));
            }
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(r)) => {
                    self.results = r;
                    self.pending = None;
                    self.err = None;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.spinner();
                }
                Err(_) => self.pending = None,
            }
        }
        if let Some(rx) = &self.detail_pending {
            match rx.try_recv() {
                Ok(Ok(d)) => {
                    self.detail = Some(d);
                    self.detail_pending = None;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.detail_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.detail_pending = None,
            }
        }

        err_label(ui, &self.err);
        ui.label(format!("{} results", self.results.len()));

        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                for r in &self.results {
                    let sel = self.selected.as_deref() == Some(r.id.as_str());
                    let label = format!("{}  {}  {}:{}", r.kind, r.name, r.file_path, r.start_line);
                    if ui.selectable_label(sel, label).clicked() {
                        self.selected = Some(r.id.clone());
                        let c = ctx.client.clone();
                        let id = r.id.clone();
                        self.detail_pending = Some(spawn_fetch(move || c.node_detail(&id)));
                    }
                }
            });
            egui::ScrollArea::vertical().show(&mut cols[1], |ui| {
                if let Some(d) = &self.detail {
                    ui.heading(&d.node.name);
                    ui.label(format!("{} · {}", d.node.kind, d.node.file_path));
                    if let Some(sig) = &d.node.signature {
                        ui.code(sig);
                    }
                } else {
                    ui.label("Select a result.");
                }
            });
        });
    }
}
