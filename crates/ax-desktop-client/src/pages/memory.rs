use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::MemoryRow;
use crate::pages::{err_label, heading, PageCtx};

#[derive(Default)]
pub struct MemoryPage {
    memories: Vec<MemoryRow>,
    total: i64,
    selected: Option<usize>,
    err: Option<String>,
    pending: Option<Receiver<Result<(Vec<MemoryRow>, i64), String>>>,
    loaded: bool,
}

impl MemoryPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(ui, "Memory", "Project memory vault.");

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.loaded = false;
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || {
                let page = c.memories(50, 0)?;
                Ok((page.memories, page.total))
            }));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok((m, total))) => {
                    self.memories = m;
                    self.total = total;
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

        err_label(ui, &self.err);
        ui.label(format!("{} memories", self.total));

        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                for (i, m) in self.memories.iter().enumerate() {
                    let sel = self.selected == Some(i);
                    if ui
                        .selectable_label(sel, format!("[{}] {}", m.kind, m.title))
                        .clicked()
                    {
                        self.selected = Some(i);
                    }
                }
                if self.memories.is_empty() {
                    ui.label("No memories yet.");
                }
            });
            egui::ScrollArea::vertical().show(&mut cols[1], |ui| {
                if let Some(i) = self.selected {
                    if let Some(m) = self.memories.get(i) {
                        ui.heading(&m.title);
                        ui.label(format!("{} · {}", m.kind, m.source));
                        if !m.tags.is_empty() {
                            ui.label(format!("tags: {}", m.tags.join(", ")));
                        }
                        ui.separator();
                        ui.label(&m.body);
                    }
                } else {
                    ui.label("Select a memory.");
                }
            });
        });
    }
}
