//! Command Center / Ship page with SSE pipeline updates.

use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::{spawn_fetch, SharedClient};
use crate::api::ShipStatus;
use crate::pages::{err_label, heading, ok_label, PageCtx};
use crate::theme;

pub struct ShipPage {
    status: Option<ShipStatus>,
    err: Option<String>,
    ok: Option<String>,
    busy: Option<String>,
    live_step: Option<String>,
    connected: bool,
    pending: Option<Receiver<Result<ShipStatus, String>>>,
    action_pending: Option<Receiver<Result<String, String>>>,
    events: Option<Receiver<Result<serde_json::Value, String>>>,
    loaded: bool,
}

impl ShipPage {
    pub fn new(client: SharedClient) -> Self {
        Self {
            status: None,
            err: None,
            ok: None,
            busy: None,
            live_step: None,
            connected: false,
            pending: None,
            action_pending: None,
            events: Some(client.stream_ship_events()),
            loaded: false,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Command Center",
            "Git-aware quality gate with live pipeline updates.",
        );

        self.poll_events();

        ui.horizontal(|ui| {
            if self.connected {
                ui.colored_label(theme::OK, "live feed connected");
            } else {
                ui.colored_label(theme::WARN, "live feed offline");
            }
            if ui
                .add_enabled(self.busy.is_none(), egui::Button::new("Evaluate"))
                .clicked()
            {
                self.run_command(ctx, "evaluate");
            }
            if ui
                .add_enabled(self.busy.is_none(), egui::Button::new("Draft PR"))
                .clicked()
            {
                self.run_command(ctx, "draft");
            }
            if ui.button("Refresh").clicked() {
                self.loaded = false;
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || c.ship_status()));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(s)) => {
                    if s.evaluating {
                        self.busy = Some("evaluate".into());
                    }
                    self.status = Some(s);
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
                    self.ok = Some(msg);
                    self.action_pending = None;
                    self.busy = None;
                    self.loaded = false;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.action_pending = None;
                    self.busy = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.action_pending = None,
            }
        }

        err_label(ui, &self.err);
        ok_label(ui, &self.ok);

        let Some(st) = &self.status else {
            return;
        };

        ui.horizontal(|ui| {
            ui.label(format!("Branch: {}", st.branch.as_deref().unwrap_or("—")));
            if let Some(step) = &self.live_step {
                ui.colored_label(theme::ACCENT, format!("Running {step}"));
            } else if self.busy.is_some() {
                ui.colored_label(theme::ACCENT, "Evaluating…");
            }
        });

        if let Some(qg) = st.report.as_ref().and_then(|r| r.quality_gate.as_ref()) {
            let status = if qg.passed {
                ("All checks passed", theme::OK)
            } else {
                ("Checks failed", theme::DANGER)
            };
            ui.colored_label(status.1, status.0);
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for step in &qg.steps {
                    let color = match step.status.as_str() {
                        "passed" | "ok" => theme::OK,
                        "failed" | "error" => theme::DANGER,
                        "running" | "active" => theme::ACCENT,
                        _ => egui::Color32::GRAY,
                    };
                    ui.colored_label(color, format!("{}: {}", step.step, step.status));
                }
            });
        } else {
            ui.label("Not evaluated yet — click Evaluate.");
        }

        if let Some(log) = &st.last_run {
            ui.add_space(8.0);
            ui.strong("Last run log");
            if !log.ok {
                ui.colored_label(theme::DANGER, "failed");
            }
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(0x18, 0x18, 0x18))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            for line in &log.lines {
                                ui.monospace(line);
                            }
                        });
                });
        }

        if let Some(files) = st.report.as_ref().map(|r| &r.changed_files) {
            if !files.is_empty() {
                ui.add_space(8.0);
                ui.strong(format!("Changed files ({})", files.len()));
                for f in files.iter().take(50) {
                    ui.monospace(f);
                }
            }
        }
    }

    fn run_command(&mut self, ctx: &PageCtx<'_>, cmd: &str) {
        self.busy = Some(cmd.into());
        self.err = None;
        self.ok = None;
        let c = ctx.client.clone();
        let cmd = cmd.to_string();
        self.action_pending = Some(spawn_fetch(move || {
            let v = c.ship_command(&cmd)?;
            if v.get("ok").and_then(|x| x.as_bool()) == Some(false) {
                let err = v
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("Command failed");
                return Err(anyhow::anyhow!("{err}"));
            }
            Ok(if cmd == "evaluate" {
                "Evaluation started / complete".into()
            } else {
                "Done".into()
            })
        }));
    }

    fn poll_events(&mut self) {
        let events: Vec<_> = {
            let Some(rx) = &self.events else {
                return;
            };
            let mut batch = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(ev) => batch.push(ev),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(_) => {
                        self.connected = false;
                        break;
                    }
                }
            }
            batch
        };

        for ev in events {
            match ev {
                Ok(v) => {
                    self.connected = true;
                    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match ty {
                        "step_started" => {
                            self.live_step = v
                                .get("step")
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string());
                            self.busy = Some("evaluate".into());
                        }
                        "step_finished" => {
                            if self.live_step.as_deref()
                                == v.get("step").and_then(|s| s.as_str())
                            {
                                self.live_step = None;
                            }
                        }
                        "report_updated" => {
                            self.busy = None;
                            self.live_step = None;
                            self.ok = Some("Evaluation complete".into());
                            self.loaded = false;
                        }
                        "run_log_updated" => {
                            self.loaded = false;
                        }
                        "error" => {
                            self.err = v
                                .get("message")
                                .and_then(|m| m.as_str())
                                .map(|m| m.to_string());
                            self.busy = None;
                            self.live_step = None;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    self.connected = false;
                    self.err = Some(e);
                }
            }
        }
    }
}
