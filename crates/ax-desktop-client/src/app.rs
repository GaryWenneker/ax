//! Desktop app shell: sidebar navigation + page host.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use crate::api::client::SharedClient;
use crate::api::ApiClient;
use crate::pages::{self, Page, PageCtx};
use crate::server::EmbeddedServer;
use crate::theme;

pub struct DesktopApp {
    client: SharedClient,
    root: PathBuf,
    _server: Arc<EmbeddedServer>,
    page: Page,
    version: String,
    project_name: String,
    status_msg: String,
    show_savings: bool,
    show_agent: bool,

    stats: pages::stats::StatsPage,
    nodes: pages::nodes::NodesPage,
    files: pages::files::FilesPage,
    search: pages::search::SearchPage,
    unresolved: pages::unresolved::UnresolvedPage,
    graph: pages::graph::GraphPage,
    savings: pages::savings::SavingsPage,
    prices: pages::prices::PricesPage,
    logging: pages::logging::LoggingPage,
    ship: pages::ship::ShipPage,
    settings: pages::settings::SettingsPage,
    memory: pages::memory::MemoryPage,
    policy: pages::policy::PolicyPage,
    agent: pages::agent::AgentPage,
}

impl DesktopApp {
    pub fn new(base_url: String, root: PathBuf, server: Arc<EmbeddedServer>) -> Self {
        let client = Arc::new(ApiClient::new(base_url));
        let mut app = Self {
            client: client.clone(),
            root,
            _server: server,
            page: Page::Stats,
            version: String::new(),
            project_name: String::new(),
            status_msg: "Connecting…".into(),
            show_savings: true,
            show_agent: true,
            stats: pages::stats::StatsPage::default(),
            nodes: pages::nodes::NodesPage::default(),
            files: pages::files::FilesPage::default(),
            search: pages::search::SearchPage::default(),
            unresolved: pages::unresolved::UnresolvedPage::default(),
            graph: pages::graph::GraphPage::default(),
            savings: pages::savings::SavingsPage::default(),
            prices: pages::prices::PricesPage::default(),
            logging: pages::logging::LoggingPage::new(client.clone()),
            ship: pages::ship::ShipPage::new(client.clone()),
            settings: pages::settings::SettingsPage::default(),
            memory: pages::memory::MemoryPage::default(),
            policy: pages::policy::PolicyPage::default(),
            agent: pages::agent::AgentPage::default(),
        };
        app.bootstrap();
        app
    }

    fn bootstrap(&mut self) {
        let c = self.client.clone();
        if let Ok(v) = c.version() {
            self.version = v.version;
        }
        if let Ok(s) = c.stats() {
            self.project_name = s.project_name.clone();
            self.status_msg = format!(
                "{} · {} nodes · {} edges",
                s.project_name, s.node_count, s.edge_count
            );
            self.stats.seed(s);
        } else {
            self.status_msg = "Waiting for embedded server…".into();
        }
        if let Ok(cfg) = c.ship_config() {
            self.show_savings = cfg.config.ui.show_savings;
            self.show_agent = cfg.config.ui.show_agent_terminal;
            self.settings.seed(cfg.config);
        }
    }

    fn nav_items(&self) -> Vec<(Page, &'static str)> {
        let mut items = vec![
            (Page::Stats, "Stats"),
            (Page::Nodes, "Nodes"),
            (Page::Graph, "Graph"),
            (Page::Files, "Files"),
            (Page::Search, "Search"),
            (Page::Memory, "Memory"),
            (Page::Unresolved, "Unresolved"),
        ];
        if self.show_savings {
            items.push((Page::Savings, "Savings"));
        }
        items.push((Page::Prices, "Prices"));
        items.push((Page::Ship, "Command Center"));
        if self.show_agent {
            items.push((Page::Agent, "Agent"));
        }
        items.push((Page::Settings, "Settings"));
        items.push((Page::Logging, "Logging"));
        items.push((Page::PolicyRules, "Policy Rules"));
        items.push((Page::PolicySkills, "Policy Skills"));
        items
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keep UI alive while SSE streams deliver.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::TopBottomPanel::top("titlebar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("ax")
                        .color(theme::ACCENT)
                        .strong(),
                );
                ui.label(egui::RichText::new("/ graph + policy").weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.version.is_empty() {
                        ui.label(format!("v{}", self.version));
                    }
                    ui.separator();
                    ui.label(egui::RichText::new(self.client.base_url()).weak().small());
                });
            });
        });

        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_msg);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(self.root.display().to_string())
                            .small()
                            .weak(),
                    );
                });
            });
        });

        egui::SidePanel::left("sidebar")
            .default_width(200.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Browse").small().weak());
                ui.separator();
                for (page, label) in self.nav_items() {
                    let selected = self.page == page
                        || (page == Page::PolicyRules
                            && matches!(self.page, Page::PolicyRules | Page::PolicySkills));
                    if ui
                        .selectable_label(selected && self.page == page, label)
                        .clicked()
                    {
                        self.page = page;
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut pctx = PageCtx {
                client: self.client.clone(),
                status_msg: &mut self.status_msg,
                show_savings: &mut self.show_savings,
                show_agent: &mut self.show_agent,
            };
            match self.page {
                Page::Stats => self.stats.ui(ui, &mut pctx),
                Page::Nodes => self.nodes.ui(ui, &mut pctx),
                Page::Files => self.files.ui(ui, &mut pctx),
                Page::Search => self.search.ui(ui, &mut pctx),
                Page::Unresolved => self.unresolved.ui(ui, &mut pctx),
                Page::Graph => self.graph.ui(ui, &mut pctx),
                Page::Savings => self.savings.ui(ui, &mut pctx),
                Page::Prices => self.prices.ui(ui, &mut pctx),
                Page::Logging => self.logging.ui(ui, &mut pctx),
                Page::Ship => self.ship.ui(ui, &mut pctx),
                Page::Settings => self.settings.ui(ui, &mut pctx),
                Page::Memory => self.memory.ui(ui, &mut pctx),
                Page::PolicyRules | Page::PolicySkills => {
                    self.policy.ui(ui, &mut pctx, self.page);
                }
                Page::Agent => self.agent.ui(ui, &mut pctx),
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self._server.stop();
    }
}

// Note: EmbeddedServer::Drop also stops the server if the app exits uncleanly.
