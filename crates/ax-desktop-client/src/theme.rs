//! Dark theme aligned with Command Center accents.

pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let accent = egui::Color32::from_rgb(0x3e, 0xe4, 0xb2);
    let bg = egui::Color32::from_rgb(0x1e, 0x1e, 0x1e);
    let panel = egui::Color32::from_rgb(0x25, 0x25, 0x26);
    let muted = egui::Color32::from_rgb(0x9d, 0x9d, 0x9d);

    style.visuals.dark_mode = true;
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(0xe0, 0xe0, 0xe0));
    style.visuals.widgets.noninteractive.bg_fill = panel;
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x2d, 0x2d, 0x2d);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x3a, 0x3a, 0x3a);
    style.visuals.widgets.active.bg_fill = accent.linear_multiply(0.35);
    style.visuals.selection.bg_fill = accent.linear_multiply(0.45);
    style.visuals.panel_fill = bg;
    style.visuals.window_fill = panel;
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(0x18, 0x18, 0x18);
    style.visuals.hyperlink_color = accent;
    style.visuals.widgets.noninteractive.fg_stroke.color = muted;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}

pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x3e, 0xe4, 0xb2);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(0xf4, 0x87, 0x71);
pub const OK: egui::Color32 = egui::Color32::from_rgb(0x4e, 0xc9, 0xb0);
pub const WARN: egui::Color32 = egui::Color32::from_rgb(0xe0, 0xb3, 0x41);
