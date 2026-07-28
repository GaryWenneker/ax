use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::SavingsSummary;
use crate::pages::{err_label, fmt_compact, fmt_usd, heading, ok_label, PageCtx};
use crate::theme;

pub struct SavingsPage {
    period: String,
    data: Option<SavingsSummary>,
    err: Option<String>,
    ok: Option<String>,
    pending: Option<Receiver<Result<SavingsSummary, String>>>,
    import_pending: Option<Receiver<Result<String, String>>>,
    loaded: bool,
}

impl Default for SavingsPage {
    fn default() -> Self {
        Self {
            period: "month_to_date".into(),
            data: None,
            err: None,
            ok: None,
            pending: None,
            import_pending: None,
            loaded: false,
        }
    }
}

impl SavingsPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Context savings",
            "Measured token savings from ax graph MCP tools.",
        );

        ui.horizontal(|ui| {
            for (id, label) in [
                ("week", "Week"),
                ("month_to_date", "Month to date"),
                ("month", "Month"),
                ("year", "Year"),
            ] {
                if ui
                    .selectable_label(self.period == id, label)
                    .clicked()
                {
                    self.period = id.into();
                    self.loaded = false;
                }
            }
            if ui.button("Import sessions").clicked() {
                let c = ctx.client.clone();
                self.import_pending = Some(spawn_fetch(move || {
                    let r = c.import_savings()?;
                    Ok(format!(
                        "Imported {} Claude + {} Cursor session(s)",
                        r.claude_sessions, r.cursor_sessions
                    ))
                }));
            }
            if ui.button("Refresh").clicked() {
                self.loaded = false;
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            let period = self.period.clone();
            self.pending = Some(spawn_fetch(move || c.savings(&period, None, None)));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(s)) => {
                    self.data = Some(s);
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
        if let Some(rx) = &self.import_pending {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.ok = Some(msg);
                    self.import_pending = None;
                    self.loaded = false;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.import_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.import_pending = None,
            }
        }

        err_label(ui, &self.err);
        ok_label(ui, &self.ok);

        let Some(s) = &self.data else {
            return;
        };

        ui.label(format!("{} → {} · priced at {}", s.from, s.to, s.pricing.reference_model));

        egui::Grid::new("sv_hero")
            .num_columns(4)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                hero(ui, "Tokens saved", &fmt_compact(s.tokens_saved_est), theme::OK);
                hero(ui, "Cost saved", &fmt_usd(s.cost_saved_usd_est), theme::OK);
                hero(ui, "Graph calls", &s.graph_calls.to_string(), theme::ACCENT);
                hero(ui, "Files avoided", &s.counterfactual_files.to_string(), theme::ACCENT);
                hero(ui, "Success", &format!("{:.0}%", s.success_rate_pct), theme::OK);
                hero(ui, "Projects", &s.projects_active.to_string(), theme::ACCENT);
                hero(
                    ui,
                    "Without ax",
                    &fmt_compact(s.counterfactual_tokens_est),
                    theme::WARN,
                );
                hero(
                    ui,
                    "With ax",
                    &fmt_compact(s.graph_response_tokens_est),
                    theme::OK,
                );
            });

        // Simple daily bars
        if !s.daily.is_empty() {
            ui.add_space(10.0);
            ui.strong("Daily tokens saved");
            let max = s
                .daily
                .iter()
                .map(|d| d.tokens_saved_est)
                .max()
                .unwrap_or(1)
                .max(1) as f32;
            let height = 80.0;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), height),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(rect);
            let n = s.daily.len() as f32;
            let bar_w = (rect.width() / n).max(2.0) * 0.7;
            for (i, d) in s.daily.iter().enumerate() {
                let h = (d.tokens_saved_est as f32 / max) * (height - 4.0);
                let x = rect.min.x + (i as f32 + 0.15) * (rect.width() / n);
                let y = rect.max.y - h;
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(bar_w, h)),
                    2.0,
                    theme::OK,
                );
            }
        }

        ui.add_space(8.0);
        ui.strong("By tool");
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                egui::Grid::new("sv_tools")
                    .striped(true)
                    .num_columns(5)
                    .show(ui, |ui| {
                        ui.strong("Tool");
                        ui.strong("Calls");
                        ui.strong("Graph");
                        ui.strong("Saved");
                        ui.strong("Files");
                        ui.end_row();
                        let mut tools = s.by_tool.clone();
                        tools.sort_by_key(|t| -t.tokens_saved_est);
                        for t in tools {
                            ui.label(&t.tool);
                            ui.label(t.calls.to_string());
                            ui.label(t.graph_calls.to_string());
                            ui.label(fmt_compact(t.tokens_saved_est));
                            ui.label(t.counterfactual_files.to_string());
                            ui.end_row();
                        }
                    });
            });

        if !s.recent_calls.is_empty() {
            ui.add_space(8.0);
            ui.strong("Recent calls");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for c in s.recent_calls.iter().take(40) {
                        ui.label(format!(
                            "{}  {}  saved={}  {}",
                            c.tool,
                            c.project.as_deref().unwrap_or("—"),
                            fmt_compact(c.tokens_saved_est),
                            if c.ok { "ok" } else { "fail" }
                        ));
                    }
                });
        }
    }
}

fn hero(ui: &mut Ui, label: &str, value: &str, color: egui::Color32) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).weak().small());
        ui.label(egui::RichText::new(value).color(color).size(20.0).strong());
    });
}
