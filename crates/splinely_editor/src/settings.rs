//! User settings: one window (Strg+, or Ansicht › Einstellungen) for every
//! tool option, saved to `~/.config/splinely/settings.json` (or `$XDG_CONFIG_HOME`).
//! Unknown or missing fields fall back to defaults, so older files keep working.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Theme {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum OnionMode {
    /// Ghosts at the neighbouring keyframes.
    Keyframes,
    /// Ghosts every `onion_interval` seconds around the playhead.
    Interval,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Settings {
    // General
    pub ui_scale: f32,
    pub theme: Theme,
    pub autosave_seconds: u64,
    pub undo_steps: usize,
    // Canvas
    pub show_rulers: bool,
    pub show_grid: bool,
    pub grid_size: f64,
    pub snap_grid: bool,
    pub smart_guides: bool,
    pub snap_distance: f32,
    // Editing
    pub nudge_small: f64,
    pub nudge_large: f64,
    pub duplicate_offset: f64,
    pub paste_offset: f64,
    // Animation
    pub onion_skin: bool,
    pub onion_mode: OnionMode,
    pub onion_before: usize,
    pub onion_after: usize,
    pub onion_interval: f64,
    pub onion_opacity: f32,
    pub onion_color_before: [u8; 3],
    pub onion_color_after: [u8; 3],
    pub key_snap: f64,
    // Export
    pub gif_fps: f64,
    pub gif_max_width: u32,
    pub gif_background: [u8; 3],
    pub png_scale: f32,
    /// Command id → key combos; a present entry replaces the default binding.
    pub keybindings: BTreeMap<String, Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.,
            theme: Theme::Dark,
            autosave_seconds: 30,
            undo_steps: 100,
            show_rulers: true,
            show_grid: false,
            grid_size: 10.,
            snap_grid: false,
            smart_guides: true,
            snap_distance: 6.,
            nudge_small: 1.,
            nudge_large: 10.,
            duplicate_offset: 10.,
            paste_offset: 10.,
            onion_skin: false,
            onion_mode: OnionMode::Keyframes,
            onion_before: 1,
            onion_after: 1,
            onion_interval: 0.25,
            onion_opacity: 0.3,
            onion_color_before: [255, 110, 110],
            onion_color_after: [110, 170, 255],
            key_snap: 0.05,
            gif_fps: 20.,
            gif_max_width: 1600,
            gif_background: [255, 255, 255],
            png_scale: 1.,
            keybindings: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Keep every value in a sensible range (hand-edited files included).
    pub fn sanitized(mut self) -> Self {
        self.ui_scale = self.ui_scale.clamp(0.5, 3.);
        self.autosave_seconds = self.autosave_seconds.clamp(5, 3600);
        self.undo_steps = self.undo_steps.clamp(10, 1000);
        self.grid_size = if self.grid_size.is_finite() {
            self.grid_size.clamp(1., 1000.)
        } else {
            10.
        };
        self.snap_distance = self.snap_distance.clamp(1., 30.);
        for v in [
            &mut self.nudge_small,
            &mut self.nudge_large,
            &mut self.duplicate_offset,
            &mut self.paste_offset,
        ] {
            *v = if v.is_finite() {
                v.clamp(0., 1000.)
            } else {
                10.
            };
        }
        self.onion_before = self.onion_before.min(5);
        self.onion_after = self.onion_after.min(5);
        self.onion_interval = if self.onion_interval.is_finite() {
            self.onion_interval.clamp(0.01, 10.)
        } else {
            0.25
        };
        self.onion_opacity = self.onion_opacity.clamp(0.05, 1.);
        self.key_snap = if self.key_snap.is_finite() {
            self.key_snap.clamp(0., 1.)
        } else {
            0.05
        };
        self.gif_fps = if self.gif_fps.is_finite() {
            self.gif_fps.clamp(1., 50.)
        } else {
            20.
        };
        self.gif_max_width = self.gif_max_width.clamp(16, 8192);
        self.png_scale = self.png_scale.clamp(0.1, 8.);
        self
    }

    pub fn load() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        files::config_dir()
            .and_then(|d| fs::read_to_string(d.join("settings.json")).ok())
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default()
            .sanitized()
    }

    fn save(&self) -> Result<(), String> {
        if cfg!(test) {
            return Ok(());
        }
        let dir = files::config_dir().ok_or("kein Konfigurationsordner")?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        atomic_save(&dir.join("settings.json").to_string_lossy(), &json)
    }
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn color_row(ui: &mut egui::Ui, label: &str, c: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut color = rgb(*c);
        if ui.color_edit_button_srgba(&mut color).changed() {
            *c = [color.r(), color.g(), color.b()];
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsTab {
    General,
    Canvas,
    Editing,
    Animation,
    Export,
    Keys,
}

impl Editor {
    /// Copy the settings-backed toggles into the editor (startup and reset).
    pub(super) fn apply_settings_to_fields(&mut self) {
        self.show_rulers = self.settings.show_rulers;
        self.smart_guides = self.settings.smart_guides;
        self.snap_grid = self.settings.snap_grid;
        self.onion_skin = self.settings.onion_skin;
    }

    /// Mirror toggles changed elsewhere (menus, palette) into the settings and
    /// save whenever something differs from the last saved state.
    pub(super) fn persist_settings(&mut self, ctx: &egui::Context) {
        self.settings.show_rulers = self.show_rulers;
        self.settings.smart_guides = self.smart_guides;
        self.settings.snap_grid = self.snap_grid;
        self.settings.onion_skin = self.onion_skin;
        if ctx.zoom_factor() != self.settings.ui_scale {
            ctx.set_zoom_factor(self.settings.ui_scale);
        }
        if self.applied_theme != Some(self.settings.theme) {
            apply_theme(ctx, self.settings.theme == Theme::Light);
            self.applied_theme = Some(self.settings.theme);
        }
        if self.saved_settings.as_ref() != Some(&self.settings) {
            if let Err(error) = self.settings.save() {
                self.status = format!("Einstellungen nicht gespeichert: {error}");
            }
            self.saved_settings = Some(self.settings.clone());
        }
    }

    /// Visible document grid (only the lines inside the view are drawn;
    /// too dense grids skip lines so the spacing stays readable).
    pub(super) fn paint_grid(&self, painter: &egui::Painter, document_rect: Rect) {
        if !self.settings.show_grid || self.tab != Tab::Canvas {
            return;
        }
        let area = document_rect.intersect(painter.clip_rect());
        if !area.is_positive() {
            return;
        }
        let mut step = self.settings.grid_size as f32;
        let mut every = 1;
        while step * self.zoom * every as f32 > 0. && step * self.zoom * (every as f32) < 6. {
            every *= 2;
        }
        step *= every as f32;
        let screen_step = step * self.zoom;
        let minor = egui::Stroke::new(1_f32, Color32::from_rgba_unmultiplied(40, 120, 140, 30));
        let major = egui::Stroke::new(1_f32, Color32::from_rgba_unmultiplied(40, 120, 140, 70));
        let first = |min: f32, origin: f32| ((min - origin) / screen_step).floor().max(0.) as i64;
        for (horizontal, origin, lo, hi) in [
            (false, document_rect.min.x, area.min.x, area.max.x),
            (true, document_rect.min.y, area.min.y, area.max.y),
        ] {
            let mut i = first(lo, origin);
            loop {
                let pos = origin + i as f32 * screen_step;
                if pos > hi + 0.5 {
                    break;
                }
                let stroke = if i % 5 == 0 { major } else { minor };
                let pos = pos.round() + 0.5;
                if horizontal {
                    painter.hline(area.x_range(), pos, stroke);
                } else {
                    painter.vline(pos, area.y_range(), stroke);
                }
                i += 1;
            }
        }
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        egui::Window::new("Einstellungen")
            .open(&mut open)
            // Fixed size, centred: switching tabs or themes must not move it.
            .fixed_size(egui::vec2(620., 470.))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0., 0.))
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (SettingsTab::General, "Allgemein"),
                        (SettingsTab::Canvas, "Zeichenfläche"),
                        (SettingsTab::Editing, "Bearbeiten"),
                        (SettingsTab::Animation, "Animation"),
                        (SettingsTab::Export, "Export"),
                        (SettingsTab::Keys, "Tastenkürzel"),
                    ] {
                        if ui
                            .selectable_label(self.settings_tab == tab, label)
                            .clicked()
                        {
                            self.settings_tab = tab;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(400.)
                    .show(ui, |ui| match self.settings_tab {
                        SettingsTab::General => self.general_settings(ui),
                        SettingsTab::Canvas => self.canvas_settings(ui),
                        SettingsTab::Editing => self.editing_settings(ui),
                        SettingsTab::Animation => self.animation_settings(ui),
                        SettingsTab::Export => self.export_settings(ui),
                        SettingsTab::Keys => self.keymap_ui(ui),
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Alles zurücksetzen")
                        .on_hover_text("Alle Einstellungen und Tastenkürzel auf die Standardwerte")
                        .clicked()
                    {
                        self.settings = Settings::default();
                        self.apply_settings_to_fields();
                    }
                    ui.weak("Änderungen gelten sofort und werden automatisch gespeichert.");
                });
            });
        self.settings_open = open;
        if !open {
            self.capturing_key = None;
        }
    }

    fn general_settings(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        egui::Grid::new("general_settings")
            .num_columns(2)
            .spacing([16., 8.])
            .show(ui, |ui| {
                ui.label("Oberflächengröße");
                ui.add(
                    egui::Slider::new(&mut s.ui_scale, 0.75..=2.0)
                        .step_by(0.05)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.)),
                );
                ui.end_row();
                ui.label("Erscheinungsbild");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut s.theme, Theme::Dark, "Dunkel");
                    ui.selectable_value(&mut s.theme, Theme::Light, "Hell");
                });
                ui.end_row();
                ui.label("Automatisch sichern alle");
                ui.add(
                    egui::DragValue::new(&mut s.autosave_seconds)
                        .range(5..=3600)
                        .suffix(" s"),
                );
                ui.end_row();
                ui.label("Rückgängig-Schritte");
                ui.add(egui::DragValue::new(&mut s.undo_steps).range(10..=1000));
                ui.end_row();
            });
    }

    fn canvas_settings(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_rulers, "Lineale anzeigen");
        ui.checkbox(&mut self.settings.show_grid, "Raster anzeigen");
        ui.checkbox(&mut self.snap_grid, "Beim Loslassen am Raster einrasten");
        ui.horizontal(|ui| {
            ui.label("Rastergröße");
            ui.add(
                egui::DragValue::new(&mut self.settings.grid_size)
                    .range(1.0..=1000.0)
                    .speed(1.)
                    .suffix(" Einheiten"),
            );
        });
        ui.separator();
        ui.checkbox(
            &mut self.smart_guides,
            "Intelligente Hilfslinien (an Formen und Zeichenfläche einrasten)",
        );
        ui.horizontal(|ui| {
            ui.label("Fangabstand");
            ui.add(egui::Slider::new(&mut self.settings.snap_distance, 1.0..=30.0).suffix(" px"));
        });
    }

    fn editing_settings(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        egui::Grid::new("editing_settings")
            .num_columns(2)
            .spacing([16., 8.])
            .show(ui, |ui| {
                ui.label("Pfeiltaste");
                ui.add(
                    egui::DragValue::new(&mut s.nudge_small)
                        .range(0.01..=1000.0)
                        .speed(0.1),
                );
                ui.end_row();
                ui.label("Umschalt + Pfeiltaste");
                ui.add(
                    egui::DragValue::new(&mut s.nudge_large)
                        .range(0.01..=1000.0)
                        .speed(1.),
                );
                ui.end_row();
                ui.label("Versatz beim Duplizieren");
                ui.add(
                    egui::DragValue::new(&mut s.duplicate_offset)
                        .range(0.0..=1000.0)
                        .speed(1.),
                );
                ui.end_row();
                ui.label("Versatz beim Einfügen");
                ui.add(
                    egui::DragValue::new(&mut s.paste_offset)
                        .range(0.0..=1000.0)
                        .speed(1.),
                );
                ui.end_row();
            });
        ui.weak(
            "Strg+D wiederholt außerdem den Abstand, um den du die letzte Kopie verschoben hast.",
        );
    }

    fn animation_settings(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.onion_skin, "Zwiebelhaut anzeigen");
        let s = &mut self.settings;
        ui.add_enabled_ui(self.onion_skin, |ui| {
            ui.horizontal(|ui| {
                ui.label("Geister an");
                ui.selectable_value(&mut s.onion_mode, OnionMode::Keyframes, "Schlüsselbildern");
                ui.selectable_value(&mut s.onion_mode, OnionMode::Interval, "festen Abständen");
            });
            if s.onion_mode == OnionMode::Interval {
                ui.horizontal(|ui| {
                    ui.label("Abstand");
                    ui.add(
                        egui::DragValue::new(&mut s.onion_interval)
                            .range(0.01..=10.0)
                            .speed(0.01)
                            .suffix(" s"),
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Davor");
                ui.add(egui::DragValue::new(&mut s.onion_before).range(0..=5));
                ui.label("Danach");
                ui.add(egui::DragValue::new(&mut s.onion_after).range(0..=5));
            });
            ui.horizontal(|ui| {
                ui.label("Deckkraft");
                ui.add(
                    egui::Slider::new(&mut s.onion_opacity, 0.05..=1.0)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.)),
                );
            });
            color_row(ui, "Farbe davor", &mut s.onion_color_before);
            color_row(ui, "Farbe danach", &mut s.onion_color_after);
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Zeitraster der Zeitleiste");
            ui.add(
                egui::DragValue::new(&mut self.settings.key_snap)
                    .range(0.0..=1.0)
                    .speed(0.005)
                    .suffix(" s"),
            )
            .on_hover_text("0 = frei. Mit Alt wird beim Ziehen immer frei positioniert.");
        });
    }

    fn export_settings(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        egui::Grid::new("export_settings")
            .num_columns(2)
            .spacing([16., 8.])
            .show(ui, |ui| {
                ui.label("GIF: Bilder pro Sekunde");
                ui.add(
                    egui::DragValue::new(&mut s.gif_fps)
                        .range(1.0..=50.0)
                        .speed(1.),
                );
                ui.end_row();
                ui.label("GIF: höchstens so breit");
                ui.add(
                    egui::DragValue::new(&mut s.gif_max_width)
                        .range(16..=8192)
                        .suffix(" px"),
                );
                ui.end_row();
                ui.label("GIF: Hintergrund");
                let mut color = rgb(s.gif_background);
                if ui.color_edit_button_srgba(&mut color).changed() {
                    s.gif_background = [color.r(), color.g(), color.b()];
                }
                ui.end_row();
                ui.label("PNG: Skalierung");
                ui.add(
                    egui::DragValue::new(&mut s.png_scale)
                        .range(0.1..=8.0)
                        .speed(0.05)
                        .custom_formatter(|v, _| format!("{:.0} %", v * 100.)),
                );
                ui.end_row();
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_clamp_bad_values() {
        let mut s = Settings {
            grid_size: 25.,
            ..Default::default()
        };
        s.keybindings.insert("Fit".into(), vec!["Ctrl+F".into()]);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        // Missing fields use defaults; nonsense is clamped.
        let partial: Settings =
            serde_json::from_str(r#"{"gridSize": -5, "onionBefore": 99, "uiScale": 10}"#).unwrap();
        let clean = partial.sanitized();
        assert_eq!(clean.grid_size, 1.);
        assert_eq!(clean.onion_before, 5);
        assert_eq!(clean.ui_scale, 3.);
        assert_eq!(clean.nudge_large, 10.);
    }
}
