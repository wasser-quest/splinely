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
    // XDG on Linux, %APPDATA% on Windows, ~/Library/Application Support on macOS.
    splinely_document::platform_dirs::base_dir(splinely_document::platform_dirs::Kind::Config)
        .map(|d| d.join("splinely"))
}

fn data_dir() -> Option<PathBuf> {
    splinely_document::platform_dirs::base_dir(splinely_document::platform_dirs::Kind::Data)
        .map(|d| d.join("splinely"))
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
            dir.join("unbenannt.sply.autosave")
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        Some(format!("{project_path}.autosave"))
    }
}

/// True for `.sply` (any case): saved as compact binary. Other extensions
/// (`.splinely.json`, `.json`, …) keep readable canonical JSON text. Loading
/// sniffs the content (`Project::load_file`), so both always open.
pub(super) fn is_binary_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("sply"))
}

/// Validate and atomically write a project in the encoding its extension implies.
pub(super) fn save_project_file(project: &Project, path: &str) -> Result<(), String> {
    if is_binary_path(path) {
        project.save_file_binary(path)
    } else {
        project.save_file(path)
    }
    .map_err(|e| e.to_string())
}

fn with_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

pub(super) fn file_label(path: &str) -> String {
    if path.is_empty() {
        tr("Unbenannt").into()
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
                    .set_title(tr("Projekt öffnen"))
                    .add_filter(tr("Splinely-Projekt"), &["sply", "json"])
                    .add_filter(tr("Alle Dateien"), &["*"])
                    .pick_file(),
                FileAction::ImportSvg => dialog
                    .set_title(tr("SVG importieren"))
                    .add_filter("SVG", &["svg"])
                    .pick_file(),
                FileAction::SaveAs { .. } => dialog
                    .set_title(tr("Projekt speichern unter"))
                    .add_filter(tr("Splinely-Projekt"), &["sply"])
                    .set_file_name(format!("{project_stem}.sply"))
                    .save_file()
                    .map(|p| with_extension(p, "sply")),
                FileAction::Export { interactive } => dialog
                    .set_title(if interactive {
                        tr("Interaktives SVG exportieren")
                    } else {
                        tr("Statisches SVG exportieren")
                    })
                    .add_filter("SVG", &["svg"])
                    .set_file_name(format!("{project_stem}.svg"))
                    .save_file()
                    .map(|p| with_extension(p, "svg")),
                FileAction::ExportPng => dialog
                    .set_title(tr("Bild als PNG exportieren"))
                    .add_filter("PNG", &["png"])
                    .set_file_name(format!("{project_stem}.png"))
                    .save_file()
                    .map(|p| with_extension(p, "png")),
                FileAction::ExportGif => dialog
                    .set_title(tr("Animation als GIF exportieren"))
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
                    tr("Dateidialog nicht verfügbar · Pfad unter Datei › Pfade manuell eingeben.")
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
                Err(splinely_document::DocumentError(
                    tr("kein Clip zum Animieren · erst eine Animation anlegen").into(),
                ))
            } else {
                project.render_gif(duration, fps, width, splinely_document::Color::rgb(r, g, b))
            }
        } else {
            let time = if active.is_some() { self.time } else { 0. };
            project.render_png(time, width)
        };
        self.status = match result
            .map_err(|e| e.to_string())
            .and_then(|bytes| atomic_save_bytes(path, &bytes).map(|_| bytes.len()))
        {
            Ok(bytes) => trf(
                "Exportiert: {} ({} KB)",
                &[file_label(path).to_string(), format!("{}", bytes / 1024)],
            ),
            Err(error) => trf(
                "Export fehlgeschlagen: {error}",
                std::slice::from_ref(&error),
            ),
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
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                self.status = trf("SVG-Import fehlgeschlagen: {error}", &[error.to_string()]);
                return;
            }
        };
        match Project::from_svg_report(&source) {
            Ok(outcome) => {
                let summary = super::svg_import_ui::summary(&outcome.report);
                let losses = outcome.report.has_losses();
                self.commit(outcome.project);
                self.sync_buffers();
                self.dirty = true;
                self.path = String::new();
                self.status = trf(
                    "„{}“ als bearbeitbare Pfade importiert · rückgängig möglich.",
                    &[file_label(path).to_string()],
                );
                if losses {
                    self.status.push_str(" · ");
                    self.status.push_str(&summary);
                }
                self.show_import_report(path, outcome.report, false);
            }
            Err(failure) => {
                self.status = trf(
                    "SVG-Import fehlgeschlagen: {error}",
                    &[failure.error.to_string()],
                );
                self.show_import_report(path, failure.report, true);
            }
        }
    }

    pub(super) fn new_project(&mut self) {
        let (width, height, name) =
            self.new_size
                .take()
                .unwrap_or((800., 600., tr("Unbenannt").into()));
        self.replace_document(Project {
            name,
            width,
            height,
            ..Project::default()
        });
        self.path = String::new();
        self.status = trf(
            "Neues Projekt {width} × {height} · R, E oder T fügt eine Form ein.",
            &[format!("{width}"), format!("{height}")],
        );
    }

    /// Ask for name and size before creating a new project.
    pub(super) fn open_new_dialog(&mut self) {
        if !self.modal_open() {
            self.new_dialog = Some(NewProjectDraft {
                name: tr("Unbenannt").into(),
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
        egui::Modal::new(egui::Id::new("splinely_new_project")).show(ctx, |ui| {
            ui.set_width(420.);
            ui.heading(tr("Neues Projekt"));
            ui.horizontal(|ui| {
                ui.label(tr("Name"));
                ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(f32::INFINITY));
            });
            ui.add_space(6.);
            ui.weak(tr("Format"));
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
                ui.label(tr("Breite"));
                ui.add(
                    egui::DragValue::new(&mut draft.width)
                        .range(1.0..=100_000.0)
                        .speed(1.),
                );
                if ui
                    .small_button(tr("Drehen"))
                    .on_hover_text(tr("Hoch-/Querformat tauschen"))
                    .clicked()
                {
                    std::mem::swap(&mut draft.width, &mut draft.height);
                }
                ui.label(tr("Höhe"));
                ui.add(
                    egui::DragValue::new(&mut draft.height)
                        .range(1.0..=100_000.0)
                        .speed(1.),
                );
            });
            ui.add_space(8.);
            ui.horizontal(|ui| {
                if ui.button(tr("Erstellen")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    let name = if draft.name.trim().is_empty() {
                        tr("Unbenannt").to_owned()
                    } else {
                        draft.name.trim().to_owned()
                    };
                    self.new_size = Some((draft.width, draft.height, name));
                    keep = false;
                }
                if ui.button(tr("Abbrechen")).clicked()
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
                s if s < 120 => tr("gerade eben").to_owned(),
                s if s < 7200 => trf("vor {} Minuten", &[format!("{}", s / 60)]),
                s if s < 172_800 => trf("vor {} Stunden", &[format!("{}", s / 3600)]),
                s => trf("vor {} Tagen", &[format!("{}", s / 86_400)]),
            })
            .unwrap_or_default();
        egui::Modal::new(egui::Id::new("splinely_recovery")).show(ctx, |ui| {
            ui.set_width(380.);
            ui.heading(tr("Ungespeicherte Arbeit gefunden"));
            ui.label(trf("Für „{}“ gibt es eine automatische Sicherung ({age}), die neuer ist als die gespeicherte Datei.", &[file_label(&self.path).to_string(), age.to_string()]));
            ui.add_space(8.);
            ui.horizontal(|ui| {
                if ui.button(tr("Wiederherstellen")).clicked() {
                    if let Some((_, project)) = self.recovery.take() {
                        let path = std::mem::take(&mut self.path);
                        self.replace_document(project);
                        self.path = path;
                        self.dirty = true;
                        self.status = tr("Sicherung wiederhergestellt · zum Behalten speichern (Strg+S).").into();
                    }
                }
                if ui.button(tr("Verwerfen")).clicked() {
                    if let Some((autosave, _)) = self.recovery.take() {
                        let _ = fs::remove_file(autosave);
                    }
                }
            });
        });
    }

    /// Window title with file name and unsaved marker; close guard dialog.
    pub(super) fn window_chrome(&mut self, ctx: &egui::Context) {
        let title = trf(
            "{}{} — Splinely",
            &[
                (if self.dirty { "• " } else { "" }).to_string(),
                file_label(&self.path).to_string(),
            ],
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
        egui::Modal::new(egui::Id::new("splinely_close_prompt")).show(ctx, |ui| {
            ui.set_width(340.);
            ui.heading(tr("Änderungen speichern?"));
            ui.label(trf(
                "„{}“ hat ungespeicherte Änderungen.",
                &[file_label(&self.path).to_string()],
            ));
            ui.add_space(8.);
            ui.horizontal(|ui| {
                if ui.button(tr("Speichern")).clicked() {
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
                if ui.button(tr("Nicht speichern")).clicked() {
                    self.close_prompt = false;
                    // Discarding on purpose: no recovery offer next time.
                    if let Some(autosave) = autosave_path(&self.path) {
                        let _ = fs::remove_file(autosave);
                    }
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(tr("Abbrechen")).clicked()
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
                egui::Button::new(tr("Neues Projekt")).shortcut_text(self.binding_label(&Cmd::New)),
            )
            .clicked()
        {
            ui.close_menu();
            self.open_new_dialog();
        }
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(tr("Öffnen …"))
                    .shortcut_text(self.binding_label(&Cmd::File(FileAction::Open))),
            )
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::Open);
        }
        let recent = self.recent.clone();
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button(tr("Zuletzt geöffnet"), |ui| {
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
                tr("Speichern *")
            } else {
                tr("Speichern")
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
                egui::Button::new(tr("Speichern unter …")).shortcut_text(
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
            .add_enabled(!busy, egui::Button::new(tr("SVG importieren …")))
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::ImportSvg);
        }
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(tr("Interaktives SVG exportieren …")).shortcut_text(
                    self.binding_label(&Cmd::File(FileAction::Export { interactive: true })),
                ),
            )
            .on_hover_text(tr("Mit Runtime, Zuständen und Audio · im Browser öffnen"))
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::Export { interactive: true });
        }
        if ui
            .add_enabled(!busy, egui::Button::new(tr("Bild als PNG exportieren …")))
            .on_hover_text(tr("Aktuelles Bild in Dokumentgröße"))
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::ExportPng);
        }
        if ui
            .add_enabled(!busy, egui::Button::new(tr("Animation als GIF exportieren …")))
            .on_hover_text(
                tr("Gewählter Clip als endlos laufendes GIF · Bildrate, Breite und Hintergrund in den Einstellungen"),
            )
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::ExportGif);
        }
        if ui
            .add_enabled(!busy, egui::Button::new(tr("Statisches SVG exportieren …")))
            .on_hover_text(tr("Bild des aktuellen Zeitpunkts ohne Skripte"))
            .clicked()
        {
            ui.close_menu();
            self.start_dialog(FileAction::Export { interactive: false });
        }
        ui.separator();
        if ui
            .add(
                egui::Button::new(tr("Befehlspalette …"))
                    .shortcut_text(self.binding_label(&Cmd::Palette)),
            )
            .clicked()
        {
            ui.close_menu();
            self.palette = Some((String::new(), 0));
        }
        if ui
            .add(
                egui::Button::new(tr("Tastenkürzel"))
                    .shortcut_text(self.binding_label(&Cmd::Shortcuts)),
            )
            .clicked()
        {
            ui.close_menu();
            self.shortcuts_open = true;
        }
        if ui
            .add(
                egui::Button::new(tr("Einstellungen …"))
                    .shortcut_text(self.binding_label(&Cmd::Settings)),
            )
            .clicked()
        {
            ui.close_menu();
            self.settings_open = true;
        }
        if ui
            .button(tr("Problem melden …"))
            .on_hover_text(tr("Öffnet ein vorausgefülltes GitHub-Issue im Browser. Gesendet wird erst, wenn du es dort abschickst."))
            .clicked()
        {
            ui.close_menu();
            self.open_issue(None);
        }
        ui.separator();
        ui.collapsing(tr("Pfade manuell eingeben"), |ui| {
            ui.label(tr("Projekt / SVG-Importpfad"));
            ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(250.))
                .on_hover_text(tr("Eine .sply-Datei enthält dein ganzes Projekt einschließlich Animation, Zuständen und eingebettetem Audio. Alte JSON-Projekte bleiben lesbar."));
            ui.horizontal(|ui| {
                if ui.button(tr("Projekt laden")).clicked() {
                    self.load();
                }
                if ui.button(tr("SVG importieren")).clicked() {
                    let path = self.path.clone();
                    self.import_svg(&path);
                }
            });
            ui.label(tr("SVG-Exportpfad"));
            ui.add(egui::TextEdit::singleline(&mut self.export_path).desired_width(250.));
            ui.horizontal(|ui| {
                if ui.button(tr("Interaktiv exportieren")).clicked() {
                    self.export(true);
                }
                if ui.button(tr("Statisch exportieren")).clicked() {
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
        assert_eq!(file_label("/a/b/scene.sply"), "scene.sply");
        assert_eq!(
            with_extension(PathBuf::from("/x/scene"), "sply"),
            PathBuf::from("/x/scene.sply")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/scene.json"), "sply"),
            PathBuf::from("/x/scene.json")
        );
        assert_eq!(autosave_path("a.sply").as_deref(), Some("a.sply.autosave"));
    }

    fn scratch(name: &str) -> PathBuf {
        let thread = std::thread::current()
            .name()
            .unwrap_or("t")
            .replace("::", "-");
        let dir = std::env::temp_dir().join(format!(
            "splinely-editor-files-{}-{thread}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn sply_saves_binary_and_other_extensions_keep_json() {
        let project = Project {
            name: "Größe".into(),
            ..Project::default()
        };
        let binary = scratch("a.sply");
        let upper = scratch("b.SPLY");
        let json = scratch("c.splinely.json");
        let plain = scratch("d.json");
        for path in [&binary, &upper, &json, &plain] {
            save_project_file(&project, path.to_str().unwrap()).unwrap();
        }
        for (path, is_binary) in [
            (&binary, true),
            (&upper, true),
            (&json, false),
            (&plain, false),
        ] {
            let bytes = fs::read(path).unwrap();
            assert_eq!(Project::is_binary(&bytes), is_binary, "{path:?}");
            assert_eq!(is_binary_path(path.to_str().unwrap()), is_binary);
            // Both encodings reload through the sniffing loader into the same project.
            assert_eq!(Project::load_file(path).unwrap(), project);
        }
        assert!(fs::read(&json).unwrap().starts_with(b"{"));
        // Re-saving a legacy JSON-text `.sply` migrates it to binary.
        let legacy = scratch("legacy.sply");
        fs::write(&legacy, project.to_json().unwrap()).unwrap();
        assert!(!Project::is_binary(&fs::read(&legacy).unwrap()));
        let loaded = Project::load_file(&legacy).unwrap();
        save_project_file(&loaded, legacy.to_str().unwrap()).unwrap();
        assert!(Project::is_binary(&fs::read(&legacy).unwrap()));
        assert_eq!(Project::load_file(&legacy).unwrap(), project);
        // A failed save leaves the previous file untouched.
        let invalid = Project {
            width: -1.0,
            ..Project::default()
        };
        let before = fs::read(&binary).unwrap();
        assert!(save_project_file(&invalid, binary.to_str().unwrap()).is_err());
        assert_eq!(fs::read(&binary).unwrap(), before);
        let _ = fs::remove_dir_all(binary.parent().unwrap());
    }

    #[test]
    fn editor_opens_binary_and_json_projects() {
        let project = Project {
            name: "Offen".into(),
            ..Project::default()
        };
        let binary = scratch("open.sply");
        let json = scratch("open.splinely.json");
        save_project_file(&project, binary.to_str().unwrap()).unwrap();
        save_project_file(&project, json.to_str().unwrap()).unwrap();
        for path in [&binary, &json] {
            let editor = Editor::open_project_file(path.to_str().unwrap()).unwrap();
            assert_eq!(editor.project.name, "Offen");
        }
        let _ = fs::remove_dir_all(binary.parent().unwrap());
    }
}
