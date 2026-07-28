use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::pages::{err_label, heading, PageCtx};

#[derive(Default)]
pub struct AgentPage {
    sessions_json: String,
    err: Option<String>,
    pending: Option<Receiver<Result<String, String>>>,
    loaded: bool,
    note: String,
}

impl AgentPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(
            ui,
            "Agent",
            "Native agent terminal parity is incremental — session list from /api/agent.",
        );

        ui.label(
            egui::RichText::new(
                "Full PTY chat lives in the browser Agent page. This desktop view lists \
                 agent session metadata from the embedded ax-web API. Use ax web Agent \
                 for interactive terminals, or extend this page with WebSocket PTY next.",
            )
            .weak(),
        );

        ui.horizontal(|ui| {
            if ui.button("Refresh sessions").clicked() {
                self.loaded = false;
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || {
                let v = c.agent_sessions()?;
                Ok(serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()))
            }));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(s)) => {
                    self.sessions_json = s;
                    self.pending = None;
                    self.loaded = true;
                    self.err = None;
                    self.note = "Loaded /api/agent/sessions".into();
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.pending = None;
                    self.loaded = true;
                    self.sessions_json =
                        "{\n  \"note\": \"Agent sessions endpoint unavailable or empty\"\n}"
                            .into();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.spinner();
                }
                Err(_) => self.pending = None,
            }
        }

        err_label(ui, &self.err);
        if !self.note.is_empty() {
            ui.label(&self.note);
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.monospace(&self.sessions_json);
        });
    }
}
