use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::Stats;
use crate::pages::{err_label, heading, PageCtx};
use crate::theme;

#[derive(Default)]
pub struct StatsPage {
    data: Option<Stats>,
    err: Option<String>,
    pending: Option<Receiver<Result<Stats, String>>>,
    loaded: bool,
}

impl StatsPage {
    pub fn seed(&mut self, s: Stats) {
        self.data = Some(s);
        self.loaded = true;
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Stats",
            "Index overview for the current workspace.",
        );

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || c.stats()));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(s)) => {
                    *ctx.status_msg = format!(
                        "{} · {} nodes · {} edges",
                        s.project_name, s.node_count, s.edge_count
                    );
                    self.data = Some(s);
                    self.err = None;
                    self.pending = None;
                    self.loaded = true;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.pending = None;
                    self.loaded = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.spinner();
                    ui.label("Loading stats…");
                }
                Err(_) => {
                    self.pending = None;
                }
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                let c = ctx.client.clone();
                self.pending = Some(spawn_fetch(move || c.stats()));
                self.loaded = false;
            }
        });
        err_label(ui, &self.err);

        if let Some(s) = &self.data {
            ui.add_space(8.0);
            ui.heading(&s.project_name);
            egui::Grid::new("stats_grid")
                .num_columns(2)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    metric(ui, "Nodes", &s.node_count.to_string());
                    metric(ui, "Edges", &s.edge_count.to_string());
                    metric(ui, "Files", &s.file_count.to_string());
                    metric(ui, "Unresolved", &s.unresolved_ref_count.unwrap_or(0).to_string());
                    metric(ui, "Policy rules", &s.policy_rules_count.to_string());
                    metric(ui, "Policy skills", &s.policy_skills_count.to_string());
                    metric(ui, "DB size", &format_bytes(s.db_size_bytes));
                    metric(ui, "Readonly", if s.readonly { "yes" } else { "no" });
                });

            if !s.languages.is_empty() {
                ui.add_space(12.0);
                ui.strong("Languages");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for lang in &s.languages {
                        ui.horizontal(|ui| {
                            ui.label(&lang.language);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.colored_label(theme::ACCENT, lang.count.to_string());
                            });
                        });
                    }
                });
            }
        }
    }
}

fn metric(ui: &mut Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).weak());
    ui.strong(value);
    ui.end_row();
}

fn format_bytes(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1} MB", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1} KB", n as f64 / 1_000.0)
    } else {
        format!("{n} B")
    }
}
