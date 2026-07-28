//! Force-directed graph canvas ported from web-ui Graph.tsx.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

use crate::api::{GraphEdge, GraphNode, GraphStreamEvent, GraphStreamMeta};
use crate::pages::{err_label, heading, PageCtx};
use crate::theme;

const COMMUNITY_COLORS: [Color32; 16] = [
    Color32::from_rgb(0x3e, 0xe4, 0xb2),
    Color32::from_rgb(0x4e, 0xc9, 0xb0),
    Color32::from_rgb(0xc5, 0x86, 0xc0),
    Color32::from_rgb(0xdc, 0xdc, 0xaa),
    Color32::from_rgb(0xce, 0x91, 0x78),
    Color32::from_rgb(0x9c, 0xdc, 0xfe),
    Color32::from_rgb(0xd7, 0xba, 0x7d),
    Color32::from_rgb(0x4f, 0xc1, 0xff),
    Color32::from_rgb(0xb5, 0xce, 0xa8),
    Color32::from_rgb(0xf4, 0x87, 0x71),
    Color32::from_rgb(0xc8, 0xc8, 0xc8),
    Color32::from_rgb(0xe2, 0xc0, 0x8d),
    Color32::from_rgb(0x6a, 0x99, 0x55),
    Color32::from_rgb(0xd1, 0x69, 0x69),
    Color32::from_rgb(0x80, 0x80, 0x80),
    Color32::from_rgb(0x7c, 0xa4, 0xcb),
];

const NODE_STEPS: [i64; 7] = [50, 100, 150, 200, 300, 400, 600];

#[derive(Clone)]
struct SimNode {
    data: GraphNode,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

struct SimEdge {
    source: usize,
    target: usize,
    confidence: Option<String>,
}

pub struct GraphPage {
    step_index: usize,
    nodes: Vec<SimNode>,
    edges: Vec<SimEdge>,
    id_index: HashMap<String, usize>,
    meta: Option<GraphStreamMeta>,
    stream: Option<Receiver<Result<GraphStreamEvent, String>>>,
    loading: bool,
    err: Option<String>,
    search: String,
    kind_filter: String,
    community_filter: String,
    selected: Option<String>,
    scale: f32,
    offset: Vec2,
    iterations: i32,
    max_iterations: i32,
    dragging: Option<usize>,
    panning: bool,
    last_pointer: Option<Pos2>,
    seeded: bool,
}

impl Default for GraphPage {
    fn default() -> Self {
        Self {
            step_index: 1,
            nodes: Vec::new(),
            edges: Vec::new(),
            id_index: HashMap::new(),
            meta: None,
            stream: None,
            loading: false,
            err: None,
            search: String::new(),
            kind_filter: String::new(),
            community_filter: String::new(),
            selected: None,
            scale: 1.0,
            offset: Vec2::ZERO,
            iterations: 0,
            max_iterations: 220,
            dragging: None,
            panning: false,
            last_pointer: None,
            seeded: false,
        }
    }
}

impl GraphPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(ui, "Graph", "Force-directed knowledge graph (wgpu / egui).");

        let limit = NODE_STEPS[self.step_index.min(NODE_STEPS.len() - 1)];

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search nodes…"));
            ui.label("Density");
            if ui
                .add(egui::Slider::new(&mut self.step_index, 0..=NODE_STEPS.len() - 1))
                .changed()
            {
                self.seeded = false;
            }
            ui.label(format!("{limit} nodes"));
            if ui.button("Reload").clicked() {
                self.start_stream(ctx, limit, false);
            }
            if ui.button("Recompute communities").clicked() {
                self.start_stream(ctx, limit, true);
            }
        });

        if !self.seeded && self.stream.is_none() {
            self.start_stream(ctx, limit, false);
            self.seeded = true;
        }

        self.poll_stream();
        if self.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Streaming graph…");
            });
        }
        err_label(ui, &self.err);

        if let Some(m) = &self.meta {
            ui.label(format!(
                "{} of {} nodes · {} edges{}",
                self.nodes.len(),
                m.total_nodes,
                self.edges.len(),
                if m.truncated { " · truncated" } else { "" }
            ));
        }

        // Kind / community filters
        ui.horizontal(|ui| {
            let kinds: Vec<String> = {
                let mut k: Vec<_> = self
                    .nodes
                    .iter()
                    .map(|n| n.data.kind.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                k.insert(0, String::new());
                k
            };
            egui::ComboBox::from_id_salt("graph_kind")
                .selected_text(if self.kind_filter.is_empty() {
                    "All kinds"
                } else {
                    &self.kind_filter
                })
                .show_ui(ui, |ui| {
                    for k in &kinds {
                        let label = if k.is_empty() { "All kinds" } else { k.as_str() };
                        if ui.selectable_label(self.kind_filter == *k, label).clicked() {
                            self.kind_filter = k.clone();
                        }
                    }
                });
        });

        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(available.x.max(200.0), (available.y - 8.0).max(200.0)),
            Sense::click_and_drag(),
        );

        // Physics step
        if !self.nodes.is_empty() && self.iterations < self.max_iterations {
            self.step_physics(rect.width(), rect.height());
            ui.ctx().request_repaint();
        }

        self.handle_input(ui, &response, rect);
        self.draw(ui, rect);
    }

    fn start_stream(&mut self, ctx: &PageCtx<'_>, limit: i64, recompute: bool) {
        self.nodes.clear();
        self.edges.clear();
        self.id_index.clear();
        self.meta = None;
        self.err = None;
        self.loading = true;
        self.iterations = 0;
        self.max_iterations = detail_iters(limit);
        self.scale = 1.0;
        self.offset = Vec2::ZERO;
        self.stream = Some(ctx.client.stream_graph(limit, recompute));
    }

    fn poll_stream(&mut self) {
        let events: Vec<_> = {
            let Some(rx) = &self.stream else {
                return;
            };
            let mut batch = Vec::new();
            loop {
                match rx.try_recv() {
                    Ok(ev) => batch.push(ev),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(_) => {
                        batch.push(Err("stream closed".into()));
                        break;
                    }
                }
            }
            batch
        };

        let mut done = false;
        for ev in events {
            match ev {
                Ok(GraphStreamEvent::Meta { meta }) => self.meta = Some(meta),
                Ok(GraphStreamEvent::Nodes { nodes }) => {
                    for n in nodes {
                        self.add_node(n);
                    }
                }
                Ok(GraphStreamEvent::Edges { edges }) => {
                    for e in edges {
                        self.add_edge(e);
                    }
                }
                Ok(GraphStreamEvent::Done) => {
                    self.loading = false;
                    done = true;
                }
                Err(e) => {
                    self.err = Some(e);
                    self.loading = false;
                    done = true;
                }
            }
        }
        if done {
            self.stream = None;
            self.loading = false;
        }
    }

    fn add_node(&mut self, n: GraphNode) {
        if self.id_index.contains_key(&n.id) {
            return;
        }
        let angle = fastrand_angle(self.nodes.len());
        let r = 120.0 + (self.nodes.len() as f32 % 40.0);
        let node = SimNode {
            data: n.clone(),
            x: 400.0 + angle.cos() * r,
            y: 300.0 + angle.sin() * r,
            vx: 0.0,
            vy: 0.0,
        };
        self.id_index.insert(n.id, self.nodes.len());
        self.nodes.push(node);
        self.iterations = 0;
    }

    fn add_edge(&mut self, e: GraphEdge) {
        let Some(&s) = self.id_index.get(&e.source) else {
            return;
        };
        let Some(&t) = self.id_index.get(&e.target) else {
            return;
        };
        if s != t {
            self.edges.push(SimEdge {
                source: s,
                target: t,
                confidence: e.confidence,
            });
        }
    }

    fn step_physics(&mut self, w: f32, h: f32) {
        let n = self.nodes.len();
        if n == 0 {
            return;
        }
        let k = ((w * h) / n as f32).sqrt() * 1.8;
        let cx = w / 2.0;
        let cy = h / 2.0;

        // Repulsion (neighbor grid simplified — O(n^2) capped)
        let cap = n.min(400);
        for i in 0..cap {
            for j in (i + 1)..cap {
                let (dx, dy, dist) = {
                    let a = &self.nodes[i];
                    let b = &self.nodes[j];
                    let mut dx = a.x - b.x;
                    let mut dy = a.y - b.y;
                    let mut dist2 = dx * dx + dy * dy;
                    if dist2 < 0.01 {
                        dx = 0.5;
                        dy = 0.5;
                        dist2 = 0.01;
                    }
                    (dx, dy, dist2.sqrt())
                };
                let force = (k * k) / dist;
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;
                self.nodes[i].vx += fx;
                self.nodes[i].vy += fy;
                self.nodes[j].vx -= fx;
                self.nodes[j].vy -= fy;
            }
        }

        for e in &self.edges {
            if e.source >= n || e.target >= n {
                continue;
            }
            let (dx, dy, dist) = {
                let a = &self.nodes[e.source];
                let b = &self.nodes[e.target];
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                (dx, dy, dist)
            };
            let force = (dist * dist) / k * 0.6;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;
            self.nodes[e.source].vx -= fx;
            self.nodes[e.source].vy -= fy;
            self.nodes[e.target].vx += fx;
            self.nodes[e.target].vy += fy;
        }

        let cooling = 1.0 - (self.iterations as f32 / self.max_iterations as f32);
        let max_disp = 30.0 * cooling + 1.0;
        let drag = self.dragging;
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.vx += (cx - node.x) * 0.001;
            node.vy += (cy - node.y) * 0.001;
            if drag == Some(i) {
                node.vx = 0.0;
                node.vy = 0.0;
                continue;
            }
            let disp = (node.vx * node.vx + node.vy * node.vy).sqrt().max(0.01);
            let limited = disp.min(max_disp);
            node.x += (node.vx / disp) * limited;
            node.y += (node.vy / disp) * limited;
            node.vx *= 0.85;
            node.vy *= 0.85;
        }
        self.iterations += 1;
    }

    fn handle_input(&mut self, ui: &Ui, response: &egui::Response, rect: Rect) {
        if let Some(pos) = response.interact_pointer_pos() {
            if response.drag_started() {
                if let Some(idx) = self.hit_test(pos, rect) {
                    self.dragging = Some(idx);
                    self.selected = Some(self.nodes[idx].data.id.clone());
                } else {
                    self.panning = true;
                    self.last_pointer = Some(pos);
                }
            }
            if response.dragged() {
                if let Some(idx) = self.dragging {
                    let world = self.screen_to_world(pos, rect);
                    self.nodes[idx].x = world.x;
                    self.nodes[idx].y = world.y;
                    self.iterations = 0;
                } else if self.panning {
                    if let Some(last) = self.last_pointer {
                        self.offset += pos - last;
                    }
                    self.last_pointer = Some(pos);
                }
            }
            if response.drag_stopped() {
                self.dragging = None;
                self.panning = false;
                self.last_pointer = None;
            }
            if response.clicked() && self.dragging.is_none() {
                if let Some(idx) = self.hit_test(pos, rect) {
                    self.selected = Some(self.nodes[idx].data.id.clone());
                }
            }
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                let new_scale = (self.scale * factor).clamp(0.15, 10.0);
                if let Some(pos) = response.hover_pos() {
                    let local = pos - rect.min;
                    self.offset = local - ((local - self.offset) * new_scale) / self.scale;
                }
                self.scale = new_scale;
            }
        }
    }

    fn hit_test(&self, pos: Pos2, rect: Rect) -> Option<usize> {
        let world = self.screen_to_world(pos, rect);
        for (i, n) in self.nodes.iter().enumerate().rev() {
            if !self.node_visible(n) {
                continue;
            }
            let r = node_radius(n.data.degree) / self.scale;
            let dx = n.x - world.x;
            let dy = n.y - world.y;
            if dx * dx + dy * dy <= (r + 4.0) * (r + 4.0) {
                return Some(i);
            }
        }
        None
    }

    fn screen_to_world(&self, pos: Pos2, rect: Rect) -> Pos2 {
        let local = pos - rect.min;
        Pos2::new(
            (local.x - self.offset.x) / self.scale,
            (local.y - self.offset.y) / self.scale,
        )
    }

    fn world_to_screen(&self, x: f32, y: f32, rect: Rect) -> Pos2 {
        Pos2::new(
            rect.min.x + x * self.scale + self.offset.x,
            rect.min.y + y * self.scale + self.offset.y,
        )
    }

    fn node_visible(&self, n: &SimNode) -> bool {
        let q = self.search.trim().to_lowercase();
        if !q.is_empty()
            && !n.data.name.to_lowercase().contains(&q)
            && !n.data.id.to_lowercase().contains(&q)
        {
            return false;
        }
        if !self.kind_filter.is_empty() && n.data.kind != self.kind_filter {
            return false;
        }
        if !self.community_filter.is_empty() {
            if let Ok(cid) = self.community_filter.parse::<i64>() {
                if n.data.community_id != cid {
                    return false;
                }
            }
        }
        true
    }

    fn draw(&self, ui: &Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(0x18, 0x18, 0x18));

        let max_edges = 1500.min(self.edges.len());
        for e in self.edges.iter().take(max_edges) {
            if e.source >= self.nodes.len() || e.target >= self.nodes.len() {
                continue;
            }
            let a = &self.nodes[e.source];
            let b = &self.nodes[e.target];
            if !self.node_visible(a) && !self.node_visible(b) {
                continue;
            }
            let pa = self.world_to_screen(a.x, a.y, rect);
            let pb = self.world_to_screen(b.x, b.y, rect);
            let color = match e.confidence.as_deref() {
                Some("inferred") => Color32::from_rgba_unmultiplied(140, 140, 160, 60),
                Some("ambiguous") => Color32::from_rgba_unmultiplied(140, 140, 160, 40),
                _ => Color32::from_rgba_unmultiplied(140, 140, 160, 70),
            };
            painter.line_segment([pa, pb], (1.0, color));
        }

        for n in &self.nodes {
            if !self.node_visible(n) {
                continue;
            }
            let p = self.world_to_screen(n.x, n.y, rect);
            if !rect.expand(20.0).contains(p) {
                continue;
            }
            let r = node_radius(n.data.degree);
            let color = if n.data.kind == "doc" {
                theme::WARN
            } else {
                color_for(n.data.community_id)
            };
            painter.circle_filled(p, r, color);
            let selected = self.selected.as_deref() == Some(n.data.id.as_str());
            if selected {
                painter.circle_stroke(p, r + 1.5, (1.5, Color32::WHITE));
            }
            if selected || self.scale > 0.8 || self.nodes.len() < 120 {
                painter.text(
                    p + Vec2::new(r + 2.0, -4.0),
                    egui::Align2::LEFT_TOP,
                    &n.data.name,
                    egui::FontId::monospace(10.0),
                    Color32::from_rgb(0xe0, 0xe0, 0xe0),
                );
            }
        }
    }
}

fn color_for(community_id: i64) -> Color32 {
    if community_id < 0 {
        Color32::from_rgb(0x66, 0x66, 0x66)
    } else {
        COMMUNITY_COLORS[(community_id as usize) % COMMUNITY_COLORS.len()]
    }
}

fn node_radius(degree: i64) -> f32 {
    (1.0 + (degree as f32).sqrt() * 0.5).min(7.0)
}

fn detail_iters(limit: i64) -> i32 {
    if limit <= 100 {
        220
    } else if limit <= 200 {
        340
    } else if limit <= 400 {
        500
    } else {
        600
    }
}

fn fastrand_angle(i: usize) -> f32 {
    // Deterministic-ish spread without extra deps.
    let x = (i as f32 * 2.399963) % std::f32::consts::TAU;
    x
}
