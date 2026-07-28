//! Live MCP verbose trace viewer (SSE).

use std::collections::HashSet;
use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::SharedClient;
use crate::api::TraceEntry;
use crate::pages::{err_label, heading, PageCtx};
use crate::theme;

pub struct LoggingPage {
    entries: Vec<TraceEntry>,
    live: bool,
    err: Option<String>,
    path: String,
    project_label: String,
    kind_filter: HashSet<String>,
    tool_filter: String,
    query: String,
    selected: Option<usize>,
    rx: Option<Receiver<Result<(String, String), String>>>,
    next_id: u64,
    verbose_mcp: bool,
    verbose_loaded: bool,
}

impl LoggingPage {
    pub fn new(client: SharedClient) -> Self {
        let rx = Some(client.stream_mcp_trace());
        let mut path = String::new();
        let mut project_label = String::new();
        if let Ok(p) = client.mcp_trace_path() {
            path = p.path;
            project_label = p.project_label;
        }
        Self {
            entries: Vec::new(),
            live: true,
            err: None,
            path,
            project_label,
            kind_filter: HashSet::new(),
            tool_filter: String::new(),
            query: String::new(),
            selected: None,
            rx,
            next_id: 0,
            verbose_mcp: false,
            verbose_loaded: false,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(ui, "Logging", "Live MCP verbose trace (newest at top).");

        if !self.verbose_loaded {
            if let Ok(resp) = ctx.client.ship_config() {
                self.verbose_mcp = resp.config.ui.verbose_mcp;
            }
            self.verbose_loaded = true;
        }

        self.poll();

        if ui
            .checkbox(
                &mut self.verbose_mcp,
                "Verbose MCP logging (reconnect MCP after enabling)",
            )
            .changed()
        {
            match ctx.client.ship_config() {
                Ok(mut resp) => {
                    resp.config.ui.verbose_mcp = self.verbose_mcp;
                    if let Err(e) = ctx.client.save_ship_config(&resp.config) {
                        self.err = Some(e.to_string());
                        self.verbose_mcp = !self.verbose_mcp;
                    }
                }
                Err(e) => {
                    self.err = Some(e.to_string());
                    self.verbose_mcp = !self.verbose_mcp;
                }
            }
        }

        ui.horizontal(|ui| {
            if self.live {
                ui.colored_label(theme::OK, "live");
            } else {
                ui.colored_label(theme::DANGER, "offline");
            }
            ui.label(format!("{} events", self.entries.len()));
            if !self.project_label.is_empty() {
                ui.label(format!("· {}", self.project_label));
            }
            ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search…"));
            ui.add(egui::TextEdit::singleline(&mut self.tool_filter).hint_text("Tool filter…"));
            if ui.button("Clear filters").clicked() {
                self.kind_filter.clear();
                self.tool_filter.clear();
                self.query.clear();
            }
        });
        ui.label(egui::RichText::new(&self.path).small().weak());
        err_label(ui, &self.err);

        // Kind chips
        ui.horizontal_wrapped(|ui| {
            let kinds = ["inbound", "outbound", "preview", "error", "enrich", "internal", "other"];
            for k in kinds {
                let count = self.entries.iter().filter(|e| e.kind == k).count();
                if count == 0 && !self.kind_filter.contains(k) {
                    continue;
                }
                let on = self.kind_filter.contains(k);
                if ui.selectable_label(on, format!("{k} ({count})")).clicked() {
                    if on {
                        self.kind_filter.remove(k);
                    } else {
                        self.kind_filter.insert(k.into());
                    }
                }
            }
        });

        let visible: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, e)| self.matches(e))
            .map(|(i, _)| i)
            .collect();

        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("trace_table")
                .show(&mut cols[0], |ui| {
                    egui::Grid::new("mcp_trace")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.strong("Time");
                            ui.strong("Kind");
                            ui.strong("Tool");
                            ui.strong("Summary");
                            ui.end_row();
                            for &i in visible.iter().take(500) {
                                let e = &self.entries[i];
                                let sel = self.selected == Some(i);
                                if ui.selectable_label(sel, &e.time).clicked() {
                                    self.selected = Some(i);
                                }
                                ui.colored_label(kind_color(&e.kind), &e.kind);
                                ui.label(e.tool.as_deref().unwrap_or("—"));
                                ui.label(truncate(&e.message, 80));
                                ui.end_row();
                            }
                        });
                });

            egui::ScrollArea::vertical()
                .id_salt("trace_inspect")
                .show(&mut cols[1], |ui| {
                    if let Some(i) = self.selected {
                        if let Some(e) = self.entries.get(i) {
                            ui.heading(e.tool.as_deref().unwrap_or("Log event"));
                            ui.label(format!("{} · {}", e.time, e.kind));
                            ui.separator();
                            ui.strong("Message");
                            ui.label(&e.message);
                            ui.separator();
                            ui.strong("Raw");
                            ui.monospace(&e.raw);
                        }
                    } else {
                        ui.label("Select a row to inspect.");
                    }
                });
        });
    }

    fn matches(&self, e: &TraceEntry) -> bool {
        if !self.kind_filter.is_empty() && !self.kind_filter.contains(&e.kind) {
            return false;
        }
        if !self.tool_filter.is_empty() {
            let Some(t) = &e.tool else {
                return false;
            };
            if !t.to_lowercase().contains(&self.tool_filter.to_lowercase()) {
                return false;
            }
        }
        if !self.query.is_empty() {
            let q = self.query.to_lowercase();
            if !e.raw.to_lowercase().contains(&q) && !e.message.to_lowercase().contains(&q) {
                return false;
            }
        }
        true
    }

    fn poll(&mut self) {
        let events: Vec<_> = {
            let Some(rx) = &self.rx else {
                return;
            };
            let mut batch = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(ev) => batch.push(ev),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(_) => {
                        self.live = false;
                        break;
                    }
                }
            }
            batch
        };

        for ev in events {
            match ev {
                Ok((event, data)) => {
                    self.live = true;
                    self.err = None;
                    match event.as_str() {
                        "reset" => {
                            self.entries.clear();
                            self.selected = None;
                        }
                        "path" => self.path = data,
                        "project" => {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                                if let Some(l) = v.get("projectLabel").and_then(|x| x.as_str()) {
                                    self.project_label = l.into();
                                }
                                if let Some(p) = v.get("path").and_then(|x| x.as_str()) {
                                    self.path = p.into();
                                }
                            }
                        }
                        "batch" => {
                            if let Ok(lines) = serde_json::from_str::<Vec<String>>(&data) {
                                for line in lines {
                                    self.ingest_line(&line);
                                }
                            }
                        }
                        "line" => self.ingest_line(&data),
                        "ready" => self.live = true,
                        _ => {}
                    }
                }
                Err(e) => {
                    self.live = false;
                    self.err = Some(e);
                }
            }
        }
    }

    fn ingest_line(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        let entry = parse_trace_line(line, self.next_id);
        self.next_id += 1;
        self.entries.push(entry);
        // Cap buffer
        if self.entries.len() > 20_000 {
            let drop = self.entries.len() - 15_000;
            self.entries.drain(0..drop);
            if let Some(sel) = self.selected {
                self.selected = sel.checked_sub(drop);
            }
        }
    }
}

fn parse_trace_line(raw: &str, id: u64) -> TraceEntry {
    let time = raw.split_whitespace().next().unwrap_or("").to_string();
    let day = time.split('T').next().unwrap_or("").to_string();
    let lower = raw.to_lowercase();
    let kind = if lower.contains(" error") || lower.contains("\terror") {
        "error"
    } else if lower.contains("inbound") || lower.contains("←") {
        "inbound"
    } else if lower.contains("outbound") || lower.contains("→") {
        "outbound"
    } else if lower.contains("preview") {
        "preview"
    } else if lower.contains("enrich") {
        "enrich"
    } else if lower.contains("internal") {
        "internal"
    } else {
        "other"
    }
    .to_string();

    let tool = raw
        .split_whitespace()
        .find(|t| t.starts_with("ax_") || t.starts_with("tool="))
        .map(|t| t.trim_start_matches("tool=").to_string());

    let message = raw
        .split_once(' ')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| raw.to_string());

    TraceEntry {
        id: format!("t{id}"),
        raw: raw.to_string(),
        time: if time.len() > 19 {
            time[11..19].to_string()
        } else {
            time
        },
        kind,
        tool,
        message,
        day,
    }
}

fn kind_color(kind: &str) -> egui::Color32 {
    match kind {
        "error" => theme::DANGER,
        "inbound" => theme::ACCENT,
        "outbound" => theme::OK,
        "enrich" => theme::WARN,
        _ => egui::Color32::LIGHT_GRAY,
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}
