use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::{PolicyRuleRow, PolicySkillRow};
use crate::pages::{err_label, heading, Page, PageCtx};

#[derive(Default)]
pub struct PolicyPage {
    rules: Vec<PolicyRuleRow>,
    skills: Vec<PolicySkillRow>,
    selected_rule: Option<usize>,
    selected_skill: Option<usize>,
    err: Option<String>,
    pending: Option<Receiver<Result<PolicyLoad, String>>>,
    loaded: bool,
}

struct PolicyLoad {
    rules: Vec<PolicyRuleRow>,
    skills: Vec<PolicySkillRow>,
}

impl PolicyPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>, page: Page) {
        match page {
            Page::PolicySkills => {
                heading(ui, "Policy skills", "Indexed skills from the policy store.");
            }
            _ => {
                heading(ui, "Policy rules", "Indexed rules from the policy store.");
            }
        }

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.loaded = false;
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            self.pending = Some(spawn_fetch(move || {
                let rules = c.policy_rules().map(|p| p.rules).unwrap_or_default();
                let skills = c.policy_skills().map(|p| p.skills).unwrap_or_default();
                Ok(PolicyLoad { rules, skills })
            }));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(load)) => {
                    self.rules = load.rules;
                    self.skills = load.skills;
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

        err_label(ui, &self.err);

        if page == Page::PolicySkills {
            self.skills_ui(ui);
        } else {
            self.rules_ui(ui);
        }
    }

    fn rules_ui(&mut self, ui: &mut Ui) {
        ui.label(format!("{} rules", self.rules.len()));
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                for (i, r) in self.rules.iter().enumerate() {
                    let sel = self.selected_rule == Some(i);
                    let label = format!(
                        "[{}] {}{}{}",
                        r.level,
                        if r.name.is_empty() { &r.id } else { &r.name },
                        if r.always_apply { " · always" } else { "" },
                        if !r.enabled { " · off" } else { "" }
                    );
                    if ui.selectable_label(sel, label).clicked() {
                        self.selected_rule = Some(i);
                    }
                }
                if self.rules.is_empty() {
                    ui.label("No rules indexed.");
                }
            });
            egui::ScrollArea::vertical().show(&mut cols[1], |ui| {
                if let Some(i) = self.selected_rule {
                    if let Some(r) = self.rules.get(i) {
                        ui.heading(&r.name);
                        ui.label(format!("level={} · id={}", r.level, r.id));
                        ui.separator();
                        ui.monospace(&r.body);
                    }
                } else {
                    ui.label("Select a rule.");
                }
            });
        });
    }

    fn skills_ui(&mut self, ui: &mut Ui) {
        ui.label(format!("{} skills", self.skills.len()));
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                for (i, s) in self.skills.iter().enumerate() {
                    let sel = self.selected_skill == Some(i);
                    let label = if s.enabled {
                        s.name.clone()
                    } else {
                        format!("{} · off", s.name)
                    };
                    if ui.selectable_label(sel, label).clicked() {
                        self.selected_skill = Some(i);
                    }
                }
                if self.skills.is_empty() {
                    ui.label("No skills indexed.");
                }
            });
            egui::ScrollArea::vertical().show(&mut cols[1], |ui| {
                if let Some(i) = self.selected_skill {
                    if let Some(s) = self.skills.get(i) {
                        ui.heading(&s.name);
                        if !s.description.is_empty() {
                            ui.label(&s.description);
                        }
                        ui.separator();
                        ui.monospace(&s.body);
                    }
                } else {
                    ui.label("Select a skill.");
                }
            });
        });
    }
}
