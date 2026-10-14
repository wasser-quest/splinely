//! Native file dialogs, recent files, window title and the unsaved-changes guard.
//!
//! Dialogs run on a helper thread so the editor keeps painting while the system
//! dialog is open. The typed paths under "Pfade manuell" remain as a fallback for
//! systems without a working portal/zenity.
use super::palette::Cmd;
use super::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FileAction {
    Open,
    SaveAs {
        then_close: bool,
    },
    ImportSvg,
    Export {
        interactive: bool,
    },
    /// Current frame as PNG.
    ExportPng,
    /// The active clip (or autoplay clips) as a looping GIF.
    ExportGif,
}

pub(super) struct FileDialog {
    action: FileAction,
    result: Receiver<Option<PathBuf>>,
}

const RECENT_LIMIT: usize = 8;

/// Size presets offered for new projects (label, width, height).
const NEW_PRESETS: &[(&str, f64, f64)] = &[
    ("Standard 800 × 600", 800., 600.),
    ("Full HD 1920 × 1080", 1920., 1080.),
    ("Quadrat 1080 × 1080", 1080., 1080.),
    ("Story 1080 × 1920", 1080., 1920.),
    ("A4 (595 × 842 pt)", 595., 842.),
    ("Icon 512 × 512", 512., 512.),
    ("Banner 1500 × 500", 1500., 500.),
];

/// Values typed into the new-project dialog.
#[derive(Clone, Debug)]
pub(super) struct NewProjectDraft {
    pub name: String,
    pub width: f64,
    pub height: f64,
}

pub(super) fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
        .map(|d| d.join("forma"))
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))
        .map(|d| d.join("forma"))
}

pub(super) fn load_recent() -> Vec<String> {
    if cfg!(test) {
        return vec![];
    }
    config_dir()
        .and_then(|d| fs::read_to_string(d.join("recent.txt")).ok())
        .map(|text| {
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .take(RECENT_LIMIT)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Autosave target; unsaved projects go to the user data directory, never the cwd.
pub(super) fn autosave_path(project_path: &str) -> Option<String> {
    if project_path.is_empty() {
        let dir = data_dir()?.join("autosave");
        fs::create_dir_all(&dir).ok()?;
        Some(
            dir.join("unbenannt.frm.autosave")
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        Some(format!("{project_path}.autosave"))
    }
}

fn with_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

pub(super) fn file_label(path: &str) -> String {
    if path.is_empty() {
        "Unbenannt".into()
    } else {
        Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.into())
    }
}

impl Editor {
    fn remember_recent(&mut self, path: &str) {
        let absolute = fs::canonicalize(path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.into());
        self.recent.retain(|p| p != &absolute);
        self.recent.insert(0, absolute);
        self.recent.truncate(RECENT_LIMIT);
        if cfg!(test) {
            return;
        }
        if let Some(dir) = config_dir() {
            if fs::create_dir_all(&dir).is_ok() {
                let _ = atomic_save(
                    &dir.join("recent.txt").to_string_lossy(),
                    &self.recent.join("\n"),
                );
            }
        }
    }

    pub(super) fn opened(&mut self, path: &str) {
        self.remember_recent(path);
    }

    pub(super) fn start_dialog(&mut self, action: FileAction) {
        if self.file_dialog.is_some() {
            return;
        }
        // Portals need absolute directories; relative paths would open $HOME.
        let start_dir = Path::new(&self.path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .and_then(|p| fs::canonicalize(p).ok())
            .or_else(|| std::env::current_dir().ok());
        let project_stem = Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                let name: String = self
                    .project
                    .name
                    .chars()
                    .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
                    .collect();
                if name.trim().is_empty() {
                    "projekt".into()
                } else {
                    name.trim().into()
                }
            });
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = start_dir {
                dialog = dialog.set_directory(dir);
            }
            let picked = match action {
                FileAction::Open => dialog
                    .set_title("Projekt öffnen")
                    .add_filter("Forma-Projekt", &["frm", "json"])
                    .add_filter("Alle Dateien", &["*"])
                    .pick_file(),
                FileAction::ImportSvg => dialog
                    .set_title("SVG importieren")
                    .add_filter("SVG", &["svg"])
                    .pick_file(),
                FileAction::SaveAs { .. } => dialog
                    .set_title("Projekt speichern unter")
                    .add_filter("Forma-Projekt", &["frm"])
                    .set_file_name(format!("{project_stem}.frm"))
                    .save_file()
                    .map(|p| with_extension(p, "frm")),
                FileAction::Export { interactive } => dialog
                    .set_title(if interactive {
                        "Interaktives SVG exportieren"
                    } else {
                        "Statisches SVG exportieren"
                    })
                    .add_filter("SVG", &["svg"])
                    .set_file_name(format!("{project_stem}.svg"))
                    .save_file()
                    .map(|p| with_extension(p, "svg")),
                FileAction::ExportPng => dialog
                    .set_title("Bild als PNG exportieren")
                    .add_filter("PNG", &["png"])
                    .set_file_name(format!("{project_stem}.png"))
                    .save_file()
                    .map(|p| with_extension(p, "png")),
                FileAction::ExportGif => dialog
                    .set_title("Animation als GIF exportieren")
                    .add_filter("GIF", &["gif"])
                    .set_file_name(format!("{project_stem}.gif"))
                    .save_file()
                    .map(|p| with_extension(p, "gif")),
            };
            let _ = tx.send(picked);
        });
        self.file_dialog = Some(FileDialog { action, result: rx });
    }

    /// Poll a running dialog and perform its action once a file was chosen.
    pub(super) fn poll_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &self.file_dialog else {
            return;
        };
        let picked = match dialog.result.try_recv() {
            Ok(picked) => picked,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.status =
                    "Dateidialog nicht verfügbar · Pfad unter Datei › Pfade manuell eingeben."
                        .into();
                None
            }
        };
        let action = dialog.action;
        self.file_dialog = None;
        let Some(path) = picked else {
            if let FileAction::SaveAs { then_close: true } = action {
                self.close_prompt = true;
            }
            return;
        };
        let path = path.to_string_lossy().into_owned();
        match action {
            FileAction::Open => self.request_open(path),
            FileAction::ImportSvg => self.import_svg(&path),
            FileAction::SaveAs { then_close } => {
                if self.path.is_empty() {
                    // The untitled autosave now belongs to a real file.
                    if let Some(untitled) = autosave_path("") {
                        let _ = fs::remove_file(untitled);
                    }
                }
                self.path = path;
                self.save();
                if then_close && !self.dirty {
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            FileAction::Export { interactive } => {
                self.export_path = path;
                self.export(interactive);
            }
            FileAction::ExportPng => self.export_raster(&path, false),
            FileAction::ExportGif => self.export_raster(&path, true),
        }
    }

    /// PNG of the current frame, or a GIF of the active clip, at document size
    /// (sizes, frame rate and GIF background from the settings).
    pub(super) fn export_raster(&mut self, path: &str, gif: bool) {
        // Render what the preview shows: the active clip as the only autoplay clip.
        let mut project = self.project.clone();
        let active = project.clips.get(self.active_clip).map(|c| c.id.clone());
        if let Some(active) = &active {
            for clip in &mut project.clips {
                clip.autoplay = &clip.id == active;
            }
        }
        let s = &self.settings;
        let width = if gif {
            (project.width.round() as u32).clamp(1, s.gif_max_width)
        } else {
            ((project.width * s.png_scale as f64).round() as u32).clamp(1, 8192)
        };
        let (fps, [r, g, b]) = (s.gif_fps, s.gif_background);
        let result = if gif {
            let duration = project.animation_duration();
            if duration <= 0. {
                Err(forma_document::DocumentError(
                    "kein Clip zum Animieren · erst eine Animation anlegen".into(),
                ))
            } else {
                project.render_gif(duration, fps, width, forma_document::Color::rgb(r, g, b))
            }
        } else {
            let time = if active.is_some() { self.time } else { 0. };
            project.render_png(time, width)
        };
        self.status = match result
            .map_err(|e| e.to_string())
            .and_then(|bytes| atomic_save_bytes(path, &bytes).map(|_| bytes.len()))
        {
            Ok(bytes) => format!("Exportiert: {} ({} KB)", file_label(path), bytes / 1024),
            Err(error) => format!("Export fehlgeschlagen: {error}"),
        };
    }

    /// Save to the current file, or ask for one if the project was never saved.
    pub(super) fn save_or_ask(&mut self) {
        if self.path.is_empty() {
            self.start_dialog(FileAction::SaveAs { then_close: false });
        } else {
            self.save();
        }
    }

    pub(super) fn import_svg(&mut self, path: &str) {
        self.finish_edit();
        self.finish_drag();
        let result = fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| Project::from_svg(&s).map_err(|e| e.to_string()));
        match result {
            Ok(project) => {
                self.commit(project);
                self.sync_buffers();
                self.dirty = true;
                self.path = String::new();
                self.status = format!(
                    "„{}“ als bearbeitbare Pfade importiert · rückgängig möglich.",
                    file_label(path)
                );
            }
            Err(error) => self.status = format!("SVG-Import fehlgeschlagen: {error}"),
        }
    }

    pub(super) fn new_project(&mut self) {
        let (width, height, name) =
            self.new_size
                .take()
                .unwrap_or((800., 600., "Unbenannt".into()));
        self.replace_document(Project {
            name,
            width,
            height,
            ..Project::default()
        });
        self.path = String::new();
        self.status = format!("Neues Projekt {width} × {height} · R, E oder T fügt eine Form ein.");
    }

    /// Ask for name and size before creating a new project.
    pub(super) fn open_new_dialog(&mut self) {
        if !self.modal_open() {
            self.new_dialog = Some(NewProjectDraft {
                name: "Unbenannt".into(),
                width: 800.,
                height: 600.,
            });
        }
    }

    fn new_project_ui(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.new_dialog.take() else {
            return;
        };
        let mut keep = true;
        egui::Modal::new(egui::Id::new("forma_new_project")).show(ctx, |ui| {
            ui.set_width(420.);
            ui.heading("Neues Projekt");
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(f32::INFINITY));
            });
            ui.add_space(6.);
            ui.weak("Format");
            ui.horizontal_wrapped(|ui| {
                for (label, w, h) in NEW_PRESETS {
                    let selected = draft.width == *w && draft.height == *h;
                    if ui
                        .selectable_label(selected, *label)
                        .on_hover_text(format!("{w} × {h}"))
                        .clicked()
                    {
                        draft.width = *w;
                        draft.height = *h;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Breite");
                ui.add(
                    egui::DragValue::new(&mut draft.width)
                        .range(1.0..=100_000.0)
                        .speed(1.),
                );
                if ui
                    .small_button("Drehen")
                    .on_hover_text("Hoch-/Querformat tauschen")
                    .clicked()
                {
                    std::mem::swap(&mut draft.width, &mut draft.height);
                }
                ui.label("Höhe");
                ui.add(
                    egui::DragValue::new(&mut draft.height)
                        .range(1.0..=100_000.0)
                        .speed(1.),
                );
            });
            ui.add_space(8.);
            ui.horizontal(|ui| {
                if ui.button("Erstellen").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    let name = if draft.name.trim().is_empty() {
                        "Unbenannt".to_owned()
                    } else {
                        draft.name.trim().to_owned()
                    };
                    self.new_size = Some((draft.width, draft.height, name));
                    keep = false;
                }
                if ui.button("Abbrechen").clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    keep = false;
                }
            });
        });
        if keep {
            self.new_dialog = Some(draft);
        } else if self.new_size.is_some() {
            // Unsaved work still gets the usual confirmation first.
            self.request_open(String::new());
        }
    }

    /// After opening `path` (empty = untitled), offer a newer autosave.
    pub fn check_recovery(&mut self) {
        if cfg!(test) {
            return;
        }
        let Some(autosave) = autosave_path(&self.path) else {
            return;
        };
        let modified = |p: &str| fs::metadata(p).and_then(|m| m.modified()).ok();
        let Some(saved_at) = modified(&autosave) else {
            return;
        };
        if !self.path.is_empty() && modified(&self.path).is_some_and(|t| t >= saved_at) {
            // The real file is newer: the autosave is stale.
            let _ = fs::remove_file(&autosave);
            return;
        }
        match Project::load_file(&autosave) {
            Ok(project) if project != self.project => {
                self.recovery = Some((autosave, project));
            }
            _ => {}
        }
    }

    fn recovery_ui(&mut self, ctx: &egui::Context) {
        let Some((autosave, _)) = &self.recovery else {
            return;
        };
        let age = fs::metadata(autosave)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| match d.as_secs() {
                s if s < 120 => "gerade eben".to_owned(),
                s if s < 7200 => format!("vor {} Minuten", s / 60),
                s if s < 172_800 => format!("vor {} Stunden", s / 3600),
                s => format!("vor {} Tagen", s / 86_400),
            })
            .unwrap_or_default();
        egui::Modal::new(egui::Id::new("forma_recovery")).show(ctx, |ui| {
            ui.set_width(380.);
            ui.heading("Ungespeicherte Arbeit gefunden");
            ui.label(format!(
                "Für „{}“ gibt es eine automatische Sicherung ({age}), die neuer ist als die gespeicherte Datei.",
                file_label(&self.path)
            ));
            ui.add_space(8.);
            ui.horizontal(|ui| {
                if ui.button("Wiederherstellen").clicked() {
                    if let Some((_, project)) = self.recovery.take() {
                        let path = std::mem::take(&mut self.path);
                        self.replace_document(project);
                        self.path = path;
                        self.dirty = true;
                        self.status = "Sicherung wiederhergestellt · zum Behalten speichern (Strg+S).".into();
                    }
                }
                if ui.button("Verwerfen").clicked() {
                    if let Some((autosave, _)) = self.recovery.take() {
                        let _ = fs::remove_file(autosave);
                    }
                }
            });
        });
    }

    /// Window title with file name and unsaved marker; close guard dialog.
    pub(super) fn window_chrome(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} — Forma",
            if self.dirty { "• " } else { "" },
            file_label(&self.path)
        );
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        self.recovery_ui(ctx);
        self.new_project_ui(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt = true;
        }
        if !self.close_prompt {
            return;
        }
        egui::Modal::new(egui::Id::new("forma_close_prompt")).show(ctx, |ui| {
            ui.set_width(340.);
            ui.heading("Änderungen speichern?");
            ui.label(format!(
                "„{}“ hat ungespeicherte Änderungen.",
                file_label(&self.path)
            ));
            ui.add_space(8.);
            ui.horizontal(|ui| {
                if ui.button("Speichern").clicked() {
                    self.close_prompt = false;
                    if self.path.is_empty() {
                        self.start_dialog(FileAction::SaveAs { then_close: true });
                    } else {
                        self.save();
                        if !self.dirty {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                if ui.button("Nicht speichern").clicked() {
                    self.close_prompt = false;
                    // Discarding on purpose: no recovery offer next time.
                    if let Some(autosave) = autosave_path(&self.path) {
                        let _ = fs::remove_file(autosave);
                    }
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Abbrechen").clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    self.close_prompt = false;
                }
            });
        });
    }

    pub(super) fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(240.);
        let busy = self.file_dialog.is_some();
        if ui
            .add_enabled(
                !busy,
                egui::Button::new("Neues Projekt").shortcut_text(self.binding_label(&Cmd::New)),
            )
            .clicked()
        {
            ui.close_menu();
            self.open_new_dialog();
        }
        if ui
            .add_enabled(
                !busy,
                egui::Button::new("Öffnen …")
                    .shortcut_text(self.binding_label(&Cmd::File(FileAction::Open))),
            )
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::Open);
        }
        let recent = self.recent.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button("Zuletzt geöffnet", |ui| {
                for path in recent {
                    if ui.button(file_label(&path)).on_hover_text(&path).clicked() {
                        ui.close_menu();
                        self.request_open(path);
                    }
                }
            });
        });
        ui.separator();
        if ui
            .button(if self.dirty {
                "Speichern *"
            } else {
                "Speichern"
            })
            .on_hover_text(self.binding_label(&Cmd::Save))
            .clicked()
        {
            ui.close_menu();
            self.save_or_ask();
        }
        if ui
            .add_enabled(
                !busy,
                egui::Button::new("Speichern unter …").shortcut_text(
                    self.binding_label(&Cmd::File(FileAction::SaveAs { then_close: false })),
                ),
            )
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::SaveAs { then_close: false });
        }
        ui.separator();
        if ui
            .add_enabled(!busy, egui::Button::new("SVG importieren …"))
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::ImportSvg);
        }
        if ui
            .add_enabled(
                !busy,
                egui::Button::new("Interaktives SVG exportieren …").shortcut_text(
                    self.binding_label(&Cmd::File(FileAction::Export { interactive: true })),
                ),
            )
            .on_hover_text("Mit Runtime, Zuständen und Audio · im Browser öffnen")
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::Export { interactive: true });
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Bild als PNG exportieren …"))
            .on_hover_text("Aktuelles Bild in Dokumentgröße")
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::ExportPng);
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Animation als GIF exportieren …"))
            .on_hover_text(
                "Gewählter Clip als endlos laufendes GIF · Bildrate, Breite und Hintergrund in den Einstellungen",
            )
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::ExportGif);
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Statisches SVG exportieren …"))
            .on_hover_text("Bild des aktuellen Zeitpunkts ohne Skripte")
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::Export { interactive: false });
        }
        ui.separator();
        if ui
            .add(
                egui::Button::new("Befehlspalette …")
                    .shortcut_text(self.binding_label(&Cmd::Palette)),
            )
            .clicked()
        {
            ui.close_menu();
            self.palette = Some((String::new(), 0));
        }
        if ui
            .add(
                egui::Button::new("Tastenkürzel")
                    .shortcut_text(self.binding_label(&Cmd::Shortcuts)),
            )
            .clicked()
        {
            ui.close_menu();
            self.shortcuts_open = true;
        }
        if ui
            .add(
                egui::Button::new("Einstellungen …")
                    .shortcut_text(self.binding_label(&Cmd::Settings)),
            )
            .clicked()
        {
            ui.close_menu();
            self.settings_open = true;
        }
        ui.separator();
        ui.collapsing("Pfade manuell eingeben", |ui| {
            ui.label("Projekt / SVG-Importpfad");
            ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(250.))
                .on_hover_text("Eine .frm-Datei enthält dein ganzes Projekt einschließlich Animation, Zuständen und eingebettetem Audio. Alte JSON-Projekte bleiben lesbar.");
            ui.horizontal(|ui| {
                if ui.button("Projekt laden").clicked() {
                    self.load();
                }
                if ui.button("SVG importieren").clicked() {
                    let path = self.path.clone();
                    self.import_svg(&path);
                }
            });
            ui.label("SVG-Exportpfad");
            ui.add(egui::TextEdit::singleline(&mut self.export_path).desired_width(250.));
            ui.horizontal(|ui| {
                if ui.button("Interaktiv exportieren").clicked() {
                    self.export(true);
                }
                if ui.button("Statisch exportieren").clicked() {
                    self.export(false);
                }
            });
        });
    }

    /// True while a modal dialog (recovery, close, switch) owns the window.
    pub(super) fn modal_open(&self) -> bool {
        self.recovery.is_some()
            || self.close_prompt
            || self.pending_open.is_some()
            || self.palette.is_some()
            || self.new_dialog.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_extensions() {
        assert_eq!(file_label(""), "Unbenannt");
        assert_eq!(file_label("/a/b/scene.frm"), "scene.frm");
        assert_eq!(
            with_extension(PathBuf::from("/x/scene"), "frm"),
            PathBuf::from("/x/scene.frm")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/scene.json"), "frm"),
            PathBuf::from("/x/scene.json")
        );
        assert_eq!(autosave_path("a.frm").as_deref(), Some("a.frm.autosave"));
    }
}
