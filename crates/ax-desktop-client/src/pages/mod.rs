pub mod agent;
pub mod files;
pub mod graph;
pub mod logging;
pub mod memory;
pub mod nodes;
pub mod policy;
pub mod prices;
pub mod savings;
pub mod search;
pub mod settings;
pub mod ship;
pub mod stats;
pub mod unresolved;

use crate::api::client::SharedClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Stats,
    Nodes,
    Graph,
    Files,
    Search,
    Memory,
    Unresolved,
    Savings,
    Prices,
    Ship,
    Settings,
    Logging,
    PolicyRules,
    PolicySkills,
    Agent,
}

pub struct PageCtx<'a> {
    pub client: SharedClient,
    pub status_msg: &'a mut String,
    pub show_savings: &'a mut bool,
    pub show_agent: &'a mut bool,
}

pub fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.heading(title);
    ui.label(egui::RichText::new(subtitle).weak());
    ui.add_space(8.0);
}

pub fn err_label(ui: &mut egui::Ui, err: &Option<String>) {
    if let Some(e) = err {
        ui.colored_label(crate::theme::DANGER, e);
    }
}

pub fn ok_label(ui: &mut egui::Ui, msg: &Option<String>) {
    if let Some(m) = msg {
        ui.colored_label(crate::theme::OK, m);
    }
}

pub fn fmt_compact(n: i64) -> String {
    let n = n as f64;
    if n >= 1_000_000_000.0 {
        format!("{:.1}B", n / 1_000_000_000.0)
    } else if n >= 1_000_000.0 {
        format!("{:.1}M", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.1}K", n / 1_000.0)
    } else {
        format!("{n:.0}")
    }
}

pub fn fmt_usd(n: f64) -> String {
    if !n.is_finite() {
        return "—".into();
    }
    format!("${n:.2}")
}
