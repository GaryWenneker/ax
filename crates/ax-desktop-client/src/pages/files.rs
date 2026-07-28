use std::sync::mpsc::Receiver;

use egui::Ui;

use crate::api::client::spawn_fetch;
use crate::api::{FileRoot, FileRow, SourceSlice};
use crate::pages::{err_label, heading, PageCtx};

#[derive(Default)]
pub struct FilesPage {
    q: String,
    lang: String,
    roots: Vec<FileRoot>,
    files: Vec<FileRow>,
    selected: Option<String>,
    preview: Option<SourceSlice>,
    err: Option<String>,
    pending: Option<Receiver<Result<FilesLoad, String>>>,
    preview_pending: Option<Receiver<Result<SourceSlice, String>>>,
    loaded: bool,
}

struct FilesLoad {
    roots: Vec<FileRoot>,
    files: Vec<FileRow>,
}

impl FilesPage {
    pub fn ui(&mut self, ui: &mut Ui, ctx: &mut PageCtx<'_>) {
        heading(ui, "Files", "Indexed source files.");

        ui.horizontal(|ui| {
            if ui
                .add(egui::TextEdit::singleline(&mut self.q).hint_text("Filter by path…"))
                .changed()
            {
                self.loaded = false;
            }
            if ui.button("Reload").clicked() {
                self.loaded = false;
            }
        });

        if !self.loaded && self.pending.is_none() {
            let c = ctx.client.clone();
            let q = self.q.clone();
            let lang = self.lang.clone();
            self.pending = Some(spawn_fetch(move || {
                let page = c.file_roots()?;
                let files = if q.is_empty() && lang.is_empty() {
                    page.files
                } else {
                    c.files(&q, &lang, None, 500, 0)?.files
                };
                Ok(FilesLoad {
                    roots: page.roots,
                    files,
                })
            }));
        }

        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(load)) => {
                    self.roots = load.roots;
                    self.files = load.files;
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
        if let Some(rx) = &self.preview_pending {
            match rx.try_recv() {
                Ok(Ok(s)) => {
                    self.preview = Some(s);
                    self.preview_pending = None;
                }
                Ok(Err(e)) => {
                    self.err = Some(e);
                    self.preview_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.preview_pending = None,
            }
        }

        err_label(ui, &self.err);

        ui.columns(2, |cols| {
            let mut open_path: Option<String> = None;
            egui::ScrollArea::vertical()
                .id_salt("files_tree")
                .show(&mut cols[0], |ui| {
                    if !self.q.is_empty() {
                        for f in &self.files {
                            let sel = self.selected.as_deref() == Some(f.path.as_str());
                            if ui
                                .selectable_label(sel, format!("{} ({})", f.path, f.language))
                                .clicked()
                            {
                                open_path = Some(f.path.clone());
                            }
                        }
                    } else {
                        for f in self.files.iter().filter(|f| !f.path.contains('/')) {
                            let sel = self.selected.as_deref() == Some(f.path.as_str());
                            if ui.selectable_label(sel, &f.path).clicked() {
                                open_path = Some(f.path.clone());
                            }
                        }
                        for r in &self.roots {
                            let root_name = r.name.clone();
                            let root_path = r.path.clone();
                            let root_count = r.count;
                            ui.collapsing(format!("{root_name} ({root_count})"), |ui| {
                                if ui.button("Load files…").clicked() {
                                    let c = ctx.client.clone();
                                    let prefix = root_path.clone();
                                    self.pending = Some(spawn_fetch(move || {
                                        let page = c.file_roots()?;
                                        let mut files = page.files;
                                        files.extend(
                                            c.files("", "", Some(&prefix), 2000, 0)?.files,
                                        );
                                        Ok(FilesLoad {
                                            roots: page.roots,
                                            files,
                                        })
                                    }));
                                    self.loaded = false;
                                }
                                for f in self.files.iter().filter(|f| {
                                    f.path == root_path
                                        || f.path.starts_with(&format!("{root_path}/"))
                                }) {
                                    let sel = self.selected.as_deref() == Some(f.path.as_str());
                                    if ui.selectable_label(sel, &f.path).clicked() {
                                        open_path = Some(f.path.clone());
                                    }
                                }
                            });
                        }
                        if self.roots.is_empty() {
                            for f in &self.files {
                                let sel = self.selected.as_deref() == Some(f.path.as_str());
                                if ui.selectable_label(sel, &f.path).clicked() {
                                    open_path = Some(f.path.clone());
                                }
                            }
                        }
                    }
                });

            if let Some(path) = open_path {
                self.open_preview(ctx, &path);
            }

            egui::ScrollArea::vertical()
                .id_salt("files_preview")
                .show(&mut cols[1], |ui| {
                    if let Some(p) = &self.preview {
                        ui.strong(&p.path);
                        ui.label(format!("lines {}–{} / {}", p.from, p.to, p.total_lines));
                        ui.separator();
                        egui::Frame::NONE
                            .fill(egui::Color32::from_rgb(0x18, 0x18, 0x18))
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                for line in &p.lines {
                                    ui.horizontal(|ui| {
                                        ui.weak(format!("{:>4}", line.no));
                                        ui.monospace(&line.text);
                                    });
                                }
                            });
                    } else {
                        ui.label("Select a file to preview.");
                    }
                });
        });
    }

    fn open_preview(&mut self, ctx: &PageCtx<'_>, path: &str) {
        self.selected = Some(path.to_string());
        let c = ctx.client.clone();
        let path = path.to_string();
        self.preview_pending = Some(spawn_fetch(move || c.source(&path, Some(1), Some(200))));
    }
}
