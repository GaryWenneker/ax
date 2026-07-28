use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::ShipConfig;
use crate::pages::{err_label, heading, ok_label, PageCtx};

#[derive(Default)]
pub struct SettingsPage {
    config: ShipConfig,
    err: Option<String>,
    ok: Option<String>,
    pending: Option<Receiver<Result<ShipConfig, String>>>,
    save_pending: Option<Receiver<Result<(), String>>>,
    loaded: bool,
}

impl SettingsPage {
    pub fn seed(&mut self, config: ShipConfig) {
        self.config = config;
        self.loaded = true;
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Settings",
            "Command Center pipeline and interface options.",
        );

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || Ok(c.ship_config()?.config)));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(cfg)) => {
                    *ctx.show_savings = cfg.ui.show_savings;
                    *ctx.show_agent = cfg.ui.show_agent_terminal;
                    self.config = cfg;
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
                }
                Err(_) => self.pending = None,
            }
        }
        if let Some(rx) = &self.save_pending {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.ok = Some("Settings saved to .ax/ship.toml".into());
                    self.save_pending = None;
                    *ctx.show_savings = self.config.ui.show_savings;
                    *ctx.show_agent = self.config.ui.show_agent_terminal;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.save_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.save_pending = None,
            }
        }

        err_label(ui, &self.err);
        ok_label(ui, &self.ok);

        ui.group(|ui| {
            ui.strong("Pipeline");
            ui.horizontal(|ui| {
                ui.label("Target branch");
                ui.text_edit_singleline(&mut self.config.ship.target_branch);
            });
            ui.horizontal(|ui| {
                ui.label("Dashboard port");
                ui.add(egui::DragValue::new(&mut self.config.ship.web_port).range(1..=65535));
            });
            if !self.config.ship.git_roots.is_empty() {
                ui.label("Git repositories (auto-discovered)");
                for root in &self.config.ship.git_roots {
                    ui.monospace(root);
                }
            }
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.strong("Interface");
            if ui
                .checkbox(&mut self.config.ui.show_savings, "Show Savings page")
                .changed()
            {
                *ctx.show_savings = self.config.ui.show_savings;
            }
            if ui
                .checkbox(
                    &mut self.config.ui.show_agent_terminal,
                    "Show Agent terminal",
                )
                .changed()
            {
                *ctx.show_agent = self.config.ui.show_agent_terminal;
            }
            ui.horizontal(|ui| {
                ui.label("Timezone");
                ui.text_edit_singleline(&mut self.config.ui.timezone);
            });
            ui.label(
                egui::RichText::new("Empty timezone = local. Affects Logging timestamps.")
                    .weak()
                    .small(),
            );
        });

        ui.add_space(8.0);
        if ui.button("Save settings").clicked() {
            let c = ctx.client.clone();
            let cfg = self.config.clone();
            self.save_pending = Some(spawn_fetch(move || {
                c.save_ship_config(&cfg)?;
                Ok(())
            }));
        }
    }
}
