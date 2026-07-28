use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::{LspStatus, UnresolvedRow, UnresolvedSummary};
use crate::pages::{err_label, heading, ok_label, PageCtx};

#[derive(Default)]
pub struct UnresolvedPage {
    q: String,
    kind: String,
    refs: Vec<UnresolvedRow>,
    total: i64,
    summary: Option<UnresolvedSummary>,
    err: Option<String>,
    ok: Option<String>,
    pending: Option<Receiver<Result<Load, String>>>,
    action_pending: Option<Receiver<Result<String, String>>>,
    show_enrich: bool,
    enrich_limit: i64,
    lsp: Option<LspStatus>,
    loaded: bool,
}

struct Load {
    summary: UnresolvedSummary,
    refs: Vec<UnresolvedRow>,
    total: i64,
}

impl UnresolvedPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Unresolved references",
            "Symbol links the indexer could not resolve.",
        );

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.q).hint_text("Filter…"));
            egui::ComboBox::from_id_salt("unres_kind")
                .selected_text(if self.kind.is_empty() {
                    "All kinds"
                } else {
                    &self.kind
                })
                .show_ui(ui, |ui| {
                    for k in ["", "calls", "imports", "references", "extends", "implements"] {
                        let label = if k.is_empty() { "All kinds" } else { k };
                        if ui.selectable_label(self.kind == k, label).clicked() {
                            self.kind = k.into();
                            self.loaded = false;
                        }
                    }
                });
            if ui.button("Reload").clicked() {
                self.loaded = false;
            }
            if ui.button("Reconcile").clicked() {
                let c = ctx.client.clone();
                self.action_pending = Some(spawn_fetch(move || {
                    let r = c.reconcile_unresolved()?;
                    Ok(format!(
                        "Reconciled: {} resolved, {} remaining",
                        r.resolved.unwrap_or(0),
                        r.remaining.unwrap_or(0)
                    ))
                }));
            }
            if ui.button("Enrich with LSP…").clicked() {
                self.show_enrich = true;
                let c = ctx.client.clone();
                self.action_pending = Some(spawn_fetch(move || {
                    let s = c.lsp_status()?;
                    Ok(serde_json::to_string(&serde_json::json!({
                        "servers": s.servers.iter().map(|x| {
                            serde_json::json!({"id": x.id, "available": x.available})
                        }).collect::<Vec<_>>()
                    }))
                    .unwrap_or_default())
                }));
                // Also fetch LSP status into local state
                if let Ok(s) = ctx.client.lsp_status() {
                    self.lsp = Some(s);
                }
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            let q = self.q.clone();
            let kind = self.kind.clone();
            self.pending = Some(spawn_fetch(move || {
                let summary = c.unresolved_summary().unwrap_or_default();
                let page = c.unresolved(&q, &kind, 100, 0)?;
                Ok(Load {
                    summary,
                    refs: page.refs,
                    total: page.total,
                })
            }));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(load)) => {
                    self.summary = Some(load.summary);
                    self.refs = load.refs;
                    self.total = load.total;
                    self.pending = None;
                    self.loaded = true;
                    self.err = None;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.pending = None;
                    self.loaded = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.spinner();
                }
                Err(_) => self.pending = None,
            }
        }
        if let Some(rx) = &self.action_pending {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    if msg.starts_with('{') {
                        // LSP status JSON ignored; enrich modal handles it
                    } else {
                        self.ok = Some(msg);
                        self.loaded = false;
                    }
                    self.action_pending = None;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.action_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.action_pending = None,
            }
        }

        err_label(ui, &self.err);
        ok_label(ui, &self.ok);

        if let Some(s) = &self.summary {
            ui.horizontal(|ui| {
                ui.label(format!("Total: {}", s.total));
                for k in &s.by_kind {
                    if ui
                        .selectable_label(self.kind == k.kind, format!("{} ({})", k.kind, k.count))
                        .clicked()
                    {
                        self.kind = k.kind.clone();
                        self.loaded = false;
                    }
                }
            });
        }

        ui.label(format!("Showing {} of {}", self.refs.len(), self.total));
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("unres_grid")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Kind");
                    ui.strong("File");
                    ui.strong("Lang");
                    ui.end_row();
                    for r in &self.refs {
                        ui.label(&r.reference_name);
                        ui.label(&r.reference_kind);
                        ui.label(format!("{}:{}", r.file_path, r.line));
                        ui.label(&r.language);
                        ui.end_row();
                    }
                });
        });

        if self.show_enrich {
            egui::Window::new("Enrich with LSP")
                .collapsible(false)
                .resizable(true)
                .show(ui.ctx(), |ui| {
                    ui.label("Resolve unresolved references via local language servers.");
                    ui.label(
                        egui::RichText::new(
                            "First enrich on a large Rust workspace can take 1–3 minutes (rust-analyzer index).",
                        )
                        .weak(),
                    );
                    ui.add(egui::Slider::new(&mut self.enrich_limit, 1..=2000).text("Limit"));
                    if self.enrich_limit == 0 {
                        self.enrich_limit = 200;
                    }
                    if let Some(lsp) = &self.lsp {
                        for s in &lsp.servers {
                            ui.horizontal(|ui| {
                                ui.label(&s.id);
                                if s.available {
                                    ui.colored_label(crate::theme::OK, "available");
                                } else if s.path.as_ref().is_some_and(|p| !p.is_empty()) {
                                    ui.colored_label(
                                        crate::theme::WARN,
                                        "shim (not runnable — rustup component add …)",
                                    );
                                } else {
                                    ui.colored_label(crate::theme::WARN, "missing");
                                }
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.show_enrich = false;
                        }
                        if ui.button("Run enrich").clicked() {
                            let c = ctx.client.clone();
                            let limit = self.enrich_limit.max(1);
                            self.action_pending = Some(spawn_fetch(move || {
                                let r = c.lsp_enrich(limit)?;
                            if !r.ok {
                                return Err(anyhow::anyhow!(
                                    "{}",
                                    r.error.unwrap_or_else(|| "LSP enrich failed".into())
                                ));
                            }
                                let rep = r.report.unwrap_or_default();
                                Ok(format!(
                                    "LSP enrich: examined {}, resolved {}",
                                    rep.examined, rep.resolved
                                ))
                            }));
                            self.show_enrich = false;
                        }
                    });
                });
        }
    }
}
