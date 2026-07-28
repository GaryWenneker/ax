use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::{PricingCatalogResponse, PricingHistoryPoint};
use crate::pages::{err_label, fmt_usd, heading, ok_label, PageCtx};
use crate::theme;

#[derive(Default)]
pub struct PricesPage {
    catalog: Option<PricingCatalogResponse>,
    selected: Option<String>,
    history: Vec<PricingHistoryPoint>,
    filter: String,
    err: Option<String>,
    ok: Option<String>,
    pending: Option<Receiver<Result<PricingCatalogResponse, String>>>,
    hist_pending: Option<Receiver<Result<Vec<PricingHistoryPoint>, String>>>,
    sync_pending: Option<Receiver<Result<String, String>>>,
    loaded: bool,
}

impl PricesPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Model prices",
            "Daily OpenRouter rate snapshots for Savings estimates.",
        );

        ui.horizontal(|ui| {
            if ui.button("Sync now").clicked() {
                let c = ctx.client.clone();
                self.sync_pending = Some(spawn_fetch(move || {
                    let r = c.sync_pricing(true)?;
                    Ok(if r.skipped {
                        "Already synced today".into()
                    } else {
                        format!("Synced {}: {} models", r.status, r.openrouter_count)
                    })
                }));
            }
            if ui.button("Refresh").clicked() {
                self.loaded = false;
            }
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter models…"));
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || c.pricing_catalog(Some("openrouter"))));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(cat)) => {
                    if self.selected.is_none() {
                        self.selected = cat.models.first().map(|m| m.model_id.clone());
                        if let Some(id) = self.selected.clone() {
                            self.load_history(ctx, &id);
                        }
                    }
                    self.catalog = Some(cat);
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
        if let Some(rx) = &self.hist_pending {
            match rx.try_recv() {
                Ok(Ok(h)) => {
                    self.history = h;
                    self.hist_pending = None;
                }
                Ok(Err(_)) => {
                    self.history.clear();
                    self.hist_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.hist_pending = None,
            }
        }
        if let Some(rx) = &self.sync_pending {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.ok = Some(msg);
                    self.sync_pending = None;
                    self.loaded = false;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.sync_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.sync_pending = None,
            }
        }

        err_label(ui, &self.err);
        ok_label(ui, &self.ok);

        if let Some(cat) = &self.catalog {
            ui.label(format!(
                "Today {} · synced today: {} · models: {}",
                cat.status.today,
                if cat.status.synced_today { "yes" } else { "no" },
                cat.status.price_rows
            ));

            // History chart
            let series: Vec<_> = self
                .history
                .iter()
                .filter(|p| p.source == "openrouter")
                .cloned()
                .collect();
            if series.len() >= 2 {
                ui.strong(format!(
                    "Price over time — {}",
                    self.selected.as_deref().unwrap_or("")
                ));
                let max = series
                    .iter()
                    .map(|p| p.input_per_mtok.max(p.output_per_mtok))
                    .fold(0.01_f64, f64::max) as f32;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 140.0), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(0x18, 0x18, 0x18));
                let n = (series.len() - 1) as f32;
                let mut in_pts = Vec::new();
                let mut out_pts = Vec::new();
                for (i, p) in series.iter().enumerate() {
                    let x = rect.min.x + (i as f32 / n) * rect.width();
                    let yin = rect.max.y - ((p.input_per_mtok as f32 / max) * (rect.height() - 8.0));
                    let yout = rect.max.y - ((p.output_per_mtok as f32 / max) * (rect.height() - 8.0));
                    in_pts.push(egui::pos2(x, yin));
                    out_pts.push(egui::pos2(x, yout));
                }
                for w in in_pts.windows(2) {
                    painter.line_segment([w[0], w[1]], (2.0, theme::ACCENT));
                }
                for w in out_pts.windows(2) {
                    painter.line_segment([w[0], w[1]], (2.0, theme::OK));
                }
            }

            let filter = self.filter.to_lowercase();
            let mut clicked_model: Option<String> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("prices_table")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        ui.strong("Model");
                        ui.strong("Input $/M");
                        ui.strong("Output $/M");
                        ui.strong("Date");
                        ui.end_row();
                        for m in &cat.models {
                            let name = m.display_name.as_deref().unwrap_or(&m.model_id);
                            if !filter.is_empty()
                                && !name.to_lowercase().contains(&filter)
                                && !m.model_id.to_lowercase().contains(&filter)
                            {
                                continue;
                            }
                            let sel = self.selected.as_deref() == Some(m.model_id.as_str());
                            if ui.selectable_label(sel, name).clicked() {
                                clicked_model = Some(m.model_id.clone());
                            }
                            ui.label(fmt_usd(m.input_per_mtok));
                            ui.label(fmt_usd(m.output_per_mtok));
                            ui.label(&m.date);
                            ui.end_row();
                        }
                    });
            });
            if let Some(id) = clicked_model {
                self.selected = Some(id.clone());
                self.load_history(ctx, &id);
            }
        }
    }

    fn load_history(&mut self, ctx: &PageCtx<'_>, model: &str) {
        let c = ctx.client.clone();
        let model = model.to_string();
        self.hist_pending = Some(spawn_fetch(move || c.pricing_history(&model, 60)));
    }
}
