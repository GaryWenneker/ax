use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::{NodeDetail, NodeRow};
use crate::pages::{err_label, heading, PageCtx};

#[derive(Default)]
pub struct NodesPage {
    q: String,
    kind: String,
    lang: String,
    nodes: Vec<NodeRow>,
    total: i64,
    offset: i64,
    selected: Option<String>,
    detail: Option<NodeDetail>,
    err: Option<String>,
    pending: Option<Receiver<Result<(Vec<NodeRow>, i64), String>>>,
    detail_pending: Option<Receiver<Result<NodeDetail, String>>>,
    dirty: bool,
}

impl NodesPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(ui, "Nodes", "Browse indexed symbols.");

        ui.horizontal(|ui| {
            if ui
                .add(egui::TextEdit::singleline(&mut self.q).hint_text("Search symbols…"))
                .changed()
            {
                self.dirty = true;
            }
            egui::ComboBox::from_id_salt("nodes_kind")
                .selected_text(if self.kind.is_empty() {
                    "All kinds"
                } else {
                    &self.kind
                })
                .show_ui(ui, |ui| {
                    for k in [
                        "",
                        "function",
                        "method",
                        "class",
                        "struct",
                        "enum",
                        "trait",
                        "interface",
                        "type",
                        "file",
                        "doc",
                    ] {
                        let label = if k.is_empty() { "All kinds" } else { k };
                        if ui.selectable_label(self.kind == k, label).clicked() {
                            self.kind = k.into();
                            self.dirty = true;
                        }
                    }
                });
            if ui.button("Search").clicked() {
                self.dirty = true;
            }
        });

        if self.dirty && self.pending.is_none() {
            self.offset = 0;
            self.reload(ctx);
            self.dirty = false;
        }
        if self.nodes.is_empty() && self.pending.is_none() && self.err.is_none() {
            self.reload(ctx);
        }

        self.poll(ctx);
        err_label(ui, &self.err);

        ui.label(format!(
            "{} shown · {} total",
            self.nodes.len(),
            self.total
        ));

        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("nodes_list")
                .show(&mut cols[0], |ui| {
                    for n in &self.nodes {
                        let selected = self.selected.as_deref() == Some(n.id.as_str());
                        if ui
                            .selectable_label(
                                selected,
                                format!("{}  {}  {}:{}", n.kind, n.name, n.file_path, n.start_line),
                            )
                            .clicked()
                        {
                            self.selected = Some(n.id.clone());
                            let c = ctx.client.clone();
                            let id = n.id.clone();
                            self.detail_pending =
                                Some(spawn_fetch(move || c.node_detail(&id)));
                        }
                    }
                });

            egui::ScrollArea::vertical()
                .id_salt("nodes_detail")
                .show(&mut cols[1], |ui| {
                    if self.detail_pending.is_some() {
                        ui.spinner();
                    }
                    if let Some(d) = &self.detail {
                        ui.heading(&d.node.name);
                        ui.label(format!(
                            "{} · {} · {}:{}-{}",
                            d.node.kind,
                            d.node.language,
                            d.node.file_path,
                            d.node.start_line,
                            d.node.end_line
                        ));
                        if let Some(sig) = &d.node.signature {
                            ui.code(sig);
                        }
                        if let Some(doc) = &d.node.docstring {
                            ui.label(doc);
                        }
                        ui.separator();
                        ui.strong(format!("Callers ({})", d.callers.len()));
                        for c in &d.callers {
                            ui.label(format!("{} → {}", c.name, c.edge_kind));
                        }
                        ui.strong(format!("Callees ({})", d.callees.len()));
                        for c in &d.callees {
                            ui.label(format!("{} → {}", c.name, c.edge_kind));
                        }
                    } else {
                        ui.label("Select a node.");
                    }
                });
        });

        ui.horizontal(|ui| {
            let can_prev = self.offset > 0;
            if ui.add_enabled(can_prev, egui::Button::new("Prev")).clicked() {
                self.offset = (self.offset - 50).max(0);
                self.reload(ctx);
            }
            if ui
                .add_enabled(self.offset + 50 < self.total, egui::Button::new("Next"))
                .clicked()
            {
                self.offset += 50;
                self.reload(ctx);
            }
        });
    }

    fn reload(&mut self, ctx: &PageCtx<'_>) {
        let c = ctx.client.clone();
        let q = self.q.clone();
        let kind = self.kind.clone();
        let lang = self.lang.clone();
        let offset = self.offset;
        self.pending = Some(spawn_fetch(move || {
            let page = c.nodes(&q, &kind, &lang, 50, offset)?;
            Ok((page.nodes, page.total))
        }));
    }

    fn poll(&mut self, _ctx: &mut PageCtx<'_>) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok((nodes, total))) => {
                    self.nodes = nodes;
                    self.total = total;
                    self.err = None;
                    self.pending = None;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
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
    }
}
