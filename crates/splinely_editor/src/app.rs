use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use splinely_document::*;
use splinely_editor::ecs::{Projection, SceneNode, Shape};
#[cfg(test)]
use splinely_editor::projection::project_node;
use std::{fs, time::Instant};

#[cfg(test)]
#[path = "app_ui_tests.rs"]
mod app_ui_tests;
#[path = "audio_preview.rs"]
mod audio_preview;
#[path = "autokey.rs"]
mod autokey;
#[path = "blend_preview.rs"]
mod blend_preview;
#[path = "bones.rs"]
mod bones;
#[cfg(test)]
#[path = "bones_tests.rs"]
mod bones_tests;
#[path = "canvas_tools.rs"]
mod canvas_tools;
#[path = "context_menu.rs"]
mod context_menu;
#[path = "dopesheet.rs"]
mod dopesheet;
#[path = "files.rs"]
mod files;
#[path = "history_panel.rs"]
mod history_panel;
#[path = "keymap.rs"]
mod keymap;
#[path = "layers.rs"]
mod layers;
#[path = "native_audio.rs"]
mod native_audio;
#[path = "onion.rs"]
mod onion;
#[path = "palette.rs"]
mod palette;
#[cfg(test)]
#[path = "pan_tests.rs"]
mod pan_tests;
#[path = "panels.rs"]
mod panels;
#[path = "rulers.rs"]
mod rulers;
#[path = "selection.rs"]
mod selection;
#[path = "settings.rs"]
mod settings;
#[path = "shortcuts.rs"]
mod shortcuts;
#[path = "snapping.rs"]
mod snapping;
#[path = "state_graph.rs"]
mod state_graph;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
#[path = "text_edit.rs"]
mod text_edit;
#[path = "transform_handles.rs"]
mod transform_handles;
#[path = "vector.rs"]
mod vector;
use vector::tools::{CanvasMap, EditablePath, PathGesture, PenPath};

pub fn demo() -> Project {
    let mut p = Project {
        name: "Splinely demo".into(),
        ..Project::default()
    };
    let mut circle = Node::circle("circle", 180., 180., 50.);
    circle.style.fill = Color::rgb(65, 145, 235);
    p.nodes.push(circle);
    p.nodes.push(Node::text(
        "caption",
        50.,
        70.,
        "Click the circle; edit, animate, export.",
    ));
    p.clips.push(Clip {
        id: "intro".into(),
        name: "Intro".into(),
        duration: 3.,
        looping: true,
        autoplay: true,
        blend_mode: BlendMode::Override,
        weight: 1.,
        tracks: vec![Track {
            node_id: "circle".into(),
            property: Property::X,
            keyframes: vec![
                Keyframe {
                    time: 0.,
                    value: AnimValue::Number(0.),
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 1.5,
                    value: AnimValue::Number(320.),
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 3.,
                    value: AnimValue::Number(0.),
                    easing: Easing::Linear,
                },
            ],
        }],
    });
    p.state_machines.push(StateMachine {
        id: "main".into(),
        initial: "idle".into(),
        states: vec![
            State {
                id: "idle".into(),
                on_enter: vec![],
                overrides: vec![],
            },
            State {
                id: "active".into(),
                on_enter: vec![],
                overrides: vec![],
            },
        ],
        transitions: vec![Transition {
            from: "idle".into(),
            to: "active".into(),
            event: Event {
                name: "click".into(),
                target: Some("circle".into()),
            },
            guard: None,
            actions: vec![Action::PlayClip {
                clip_id: "intro".into(),
            }],
        }],
    });
    p
}
pub fn export_file(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let project = Project::load_file(input)?;
    fs::write(output, project.to_svg(crate::RUNTIME)?)?;
    Ok(())
}
pub fn export_demo(output: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(output, demo().to_svg(crate::RUNTIME)?)?;
    Ok(())
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Canvas,
    Bones,
    States,
    Audio,
    JavaScript,
    Svg,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Workspace {
    Design,
    Animate,
    Advanced,
}
impl Workspace {
    fn sections(self) -> &'static [(Tab, &'static str)] {
        match self {
            Self::Design => &[(Tab::Canvas, "Zeichenfläche")],
            Self::Animate => &[
                (Tab::Canvas, "Zeichenfläche / Zeitleiste"),
                (Tab::Bones, "Skelett"),
                (Tab::States, "Zustände"),
                (Tab::Audio, "Audio / Sprache"),
            ],
            Self::Advanced => &[(Tab::JavaScript, "JavaScript"), (Tab::Svg, "SVG-Quelltext")],
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum CanvasTool {
    Selection,
    Pen,
    PathPoints,
    Freehand,
}

pub struct Editor {
    settings: settings::Settings,
    saved_settings: Option<settings::Settings>,
    applied_theme: Option<settings::Theme>,
    settings_open: bool,
    settings_tab: settings::SettingsTab,
    capturing_key: Option<String>,
    keymap_filter: String,
    project: Project,
    history: History,
    projection: Projection,
    selected: Option<String>,
    selected_bone: Option<String>,
    ik_target: [f64; 2],
    ik_bend_positive: bool,
    fit_requested: bool,
    tab: Tab,
    workspace: Workspace,
    path: String,
    export_path: String,
    status: String,
    time: f64,
    playing: bool,
    preview_animation: bool,
    looping: bool,
    last_tick: Instant,
    active_clip: usize,
    blend_preview: blend_preview::BlendPreview,
    show_rulers: bool,
    ruler_gesture: bool,
    drag_start: Option<Project>,
    edit_start: Option<Project>,
    editing: bool,
    history_bytes: usize,
    history_steps: usize,
    zoom: f32,
    view_zoom: f32,
    pan_offset: Vec2,
    pan_gesture: PanGesture,
    space_pan_used: bool,
    code_buffer: String,
    svg_buffer: String,
    machine_buffer: String,
    variables_buffer: String,
    timeline_visible: bool,
    trusted_js_export: bool,
    viewer: Option<crate::viewer::ViewerSession>,
    pending_open: Option<String>,
    machine_index: usize,
    state_graph: state_graph::StateGraph,
    event_name: String,
    event_target: String,
    machines: Vec<MachineInstance>,
    event_log: Vec<String>,
    runtime_overrides: Vec<PropertyOverride>,
    audio_path: String,
    audio_asset: usize,
    last_autosave: Instant,
    dirty: bool,
    next_id: u64,
    raster: crate::raster::Raster,
    clipboard: shortcuts::ShapeClipboard,
    multi: Vec<String>,
    last_primary: Option<String>,
    marquee: Option<selection::Marquee>,
    pending_arrange: Option<selection::ArrangeOp>,
    transform_gesture: Option<transform_handles::TransformGesture>,
    drag_base: Option<Project>,
    drag_total: Vec2,
    drag_snap: Option<snapping::DragSnap>,
    guides: Vec<snapping::Guide>,
    smart_guides: bool,
    style_clipboard: Option<(Style, Option<splinely_document::Gradient>)>,
    zoom_selection_requested: bool,
    recovery: Option<(String, Project)>,
    eyedropper: bool,
    selected_key: Option<dopesheet::KeyRef>,
    palette: Option<(String, usize)>,
    onion_skin: bool,
    auto_key: bool,
    history_open: bool,
    pointer_hint: Option<String>,
    /// Copies made by the last Strg+D and where their sources were (bounds centre).
    last_duplicate: Option<(Vec<String>, (f64, f64))>,
    text_draft: Option<text_edit::TextDraft>,
    #[cfg(test)]
    debug_origin: Option<Pos2>,
    new_dialog: Option<files::NewProjectDraft>,
    new_size: Option<(f64, f64, String)>,
    onion_rasters: Vec<crate::raster::Raster>,
    shortcuts_open: bool,
    key_drag: Option<dopesheet::KeyRef>,
    guide_drag: Option<(Option<usize>, snapping::Guide)>,
    scene_cache: Option<(Project, String, Vec<SceneNode>)>,
    file_dialog: Option<files::FileDialog>,
    close_prompt: bool,
    allow_close: bool,
    window_title: String,
    recent: Vec<String>,
    view_center: Option<[f64; 2]>,
    preview_mode: bool,
    highlight: bool,
    snap_grid: bool,
    edit_path_handles: bool,
    path_drag: Option<(PathGesture, CanvasMap, String)>,
    pen: Option<PenPath>,
    pen_press: Option<[f64; 2]>,
    freehand: splinely_editor::freehand::FreehandTool,
    freehand_mode: bool,
    extensions: splinely_editor::extensions::ExtensionRegistry,
    audio_preview: audio_preview::AudioPreview,
    audio_enabled: bool,
    audio_load_wanted: bool,
    audio_loading: Option<std::sync::mpsc::Receiver<Result<audio_preview::AudioPreview, String>>>,
}
impl Default for Editor {
    fn default() -> Self {
        let project = demo();
        let mut projection = Projection::default();
        projection
            .add_plugin(splinely_editor::plugins::SelectionHighlightPlugin)
            .unwrap();
        let settings = settings::Settings::load();
        Self {
            code_buffer: project.custom_js.clone(),
            svg_buffer: project.raw_svg.clone(),
            machine_buffer: serde_json::to_string_pretty(&project.state_machines).unwrap(),
            variables_buffer: serde_json::to_string_pretty(&project.variables).unwrap(),
            saved_settings: Some(settings.clone()),
            applied_theme: None,
            settings_open: false,
            settings_tab: settings::SettingsTab::General,
            capturing_key: None,
            keymap_filter: String::new(),
            timeline_visible: false,
            trusted_js_export: false,
            viewer: None,
            pending_open: None,
            project,
            history: History::default(),
            projection,
            selected: None,
            selected_bone: None,
            ik_target: [300., 300.],
            ik_bend_positive: true,
            fit_requested: true,
            tab: Tab::Canvas,
            workspace: Workspace::Design,
            path: String::new(),
            export_path: "splinely.svg".into(),
            status: "Hallo! Zeichne eine Form oder zieh eine .sply-Datei herein.".into(),
            time: 0.,
            playing: false,
            preview_animation: false,
            looping: true,
            last_tick: Instant::now(),
            active_clip: 0,
            blend_preview: Default::default(),
            show_rulers: settings.show_rulers,
            ruler_gesture: false,
            drag_start: None,
            edit_start: None,
            editing: false,
            history_bytes: 0,
            history_steps: 0,
            zoom: 1.,
            view_zoom: 1.,
            pan_offset: Vec2::ZERO,
            pan_gesture: PanGesture::default(),
            space_pan_used: false,
            machine_index: 0,
            state_graph: Default::default(),
            event_name: "click".into(),
            event_target: String::new(),
            machines: vec![],
            event_log: vec![],
            runtime_overrides: vec![],
            audio_path: String::new(),
            audio_asset: 0,
            last_autosave: Instant::now(),
            dirty: false,
            next_id: 1,
            raster: Default::default(),
            clipboard: Default::default(),
            multi: vec![],
            last_primary: None,
            marquee: None,
            pending_arrange: None,
            transform_gesture: None,
            drag_base: None,
            drag_total: Vec2::ZERO,
            drag_snap: None,
            guides: vec![],
            smart_guides: settings.smart_guides,
            style_clipboard: None,
            zoom_selection_requested: false,
            recovery: None,
            eyedropper: false,
            selected_key: None,
            palette: None,
            onion_skin: settings.onion_skin,
            auto_key: false,
            history_open: false,
            pointer_hint: None,
            last_duplicate: None,
            text_draft: None,
            #[cfg(test)]
            debug_origin: None,
            new_dialog: None,
            new_size: None,
            onion_rasters: Default::default(),
            shortcuts_open: false,
            key_drag: None,
            guide_drag: None,
            scene_cache: None,
            file_dialog: None,
            close_prompt: false,
            allow_close: false,
            window_title: String::new(),
            recent: files::load_recent(),
            view_center: None,
            preview_mode: false,
            highlight: false,
            snap_grid: settings.snap_grid,
            edit_path_handles: false,
            path_drag: None,
            pen: None,
            pen_press: None,
            freehand: splinely_editor::freehand::FreehandTool::default(),
            freehand_mode: false,
            extensions: splinely_editor::extensions::ExtensionRegistry::load_default(),
            audio_preview: Default::default(),
            audio_enabled: false,
            audio_load_wanted: false,
            audio_loading: None,
            settings,
        }
    }
}
impl Editor {
    fn commit(&mut self, mut next: Project) {
        if next == self.project {
            return;
        }
        self.autokey(&mut next);
        if let Err(error) = next.validate() {
            self.status = error.to_string();
            return;
        }
        if next.audio_assets != self.project.audio_assets
            || next.audio_cues != self.project.audio_cues
        {
            self.disable_native_audio();
        }
        if self.editing {
            if self.edit_start.is_none() {
                self.edit_start = Some(self.project.clone());
            }
            self.project = next;
        } else {
            self.record(next);
        }
        self.dirty = true;
        self.machines.clear();
        self.blend_preview = Default::default();
        self.runtime_overrides.clear();
    }
    fn record(&mut self, next: Project) {
        let size = serde_json::to_vec(&self.project)
            .map(|s| s.len())
            .unwrap_or(0)
            + serde_json::to_vec(&next).map(|s| s.len()).unwrap_or(0);
        let limit = self.settings.undo_steps;
        if self.history_bytes + size > 128 * 1024 * 1024 || self.history_steps >= limit {
            self.history.clear();
            self.history_bytes = 0;
            self.history_steps = 0;
            self.status = format!(
                "Ältere Rückgängig-Schritte verworfen (Grenze: {limit} Schritte / ~128 MiB · änderbar in den Einstellungen)."
            );
        }
        let label = history_panel::describe_change(&self.project, &next);
        match self
            .history
            .execute(&mut self.project, Command::ReplaceProject(next))
        {
            Ok(()) => {
                self.history.set_last_label(label);
                self.history_bytes += size;
                self.history_steps += 1;
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    fn finish_edit(&mut self) {
        if let Some(before) = self.edit_start.take() {
            let after = self.project.clone();
            self.project = before;
            if after != self.project {
                self.record(after);
            }
        }
    }
    fn new_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}-{}", self.next_id);
            self.next_id += 1;
            let used = self.project.nodes.iter().any(|n| n.id == id)
                || self.project.bones.iter().any(|n| n.id == id)
                || self.project.state_machines.iter().any(|n| n.id == id)
                || self.project.clips.iter().any(|n| n.id == id)
                || self.project.audio_assets.iter().any(|n| n.id == id)
                || self.project.audio_cues.iter().any(|n| n.id == id);
            if !used {
                return id;
            }
        }
    }
    fn sync_buffers(&mut self) {
        // Document replacement/history must never carry script trust to another source.
        self.disable_native_audio();
        self.projection.reset_document();
        self.text_draft = None;
        self.scene_cache = None;
        self.state_graph = Default::default();
        self.path_drag = None;
        self.pen = None;
        self.pen_press = None;
        self.preview_mode = false;
        self.playing = false;
        self.preview_animation = false;
        self.trusted_js_export = false;
        self.code_buffer = self.project.custom_js.clone();
        self.svg_buffer = self.project.raw_svg.clone();
        self.machine_buffer =
            serde_json::to_string_pretty(&self.project.state_machines).unwrap_or_default();
        self.variables_buffer =
            serde_json::to_string_pretty(&self.project.variables).unwrap_or_default();
        self.machines.clear();
        self.blend_preview = Default::default();
        self.runtime_overrides.clear();
        self.selected = None;
        self.multi.clear();
        self.marquee = None;
        self.selected_bone = None;
        self.fit_requested = true;
        self.freehand.cancel();
        self.freehand_mode = false;
        self.pan_offset = Vec2::ZERO;
        self.pan_gesture = PanGesture::default();
        self.ruler_gesture = false;
        self.space_pan_used = false;
        self.active_clip = 0;
        self.machine_index = 0;
        self.time = 0.;
    }
    fn request_open(&mut self, path: String) {
        self.pending_open = None;
        let drafts = self.code_buffer != self.project.custom_js
            || self.svg_buffer != self.project.raw_svg
            || self.machine_buffer
                != serde_json::to_string_pretty(&self.project.state_machines).unwrap_or_default()
            || self.variables_buffer
                != serde_json::to_string_pretty(&self.project.variables).unwrap_or_default();
        if self.dirty
            || drafts
            || self.edit_start.is_some()
            || self.drag_start.is_some()
            || self.path_drag.is_some()
            || self.pen.is_some()
            || self.freehand.is_drawing()
        {
            self.pending_open = Some(path);
        } else if path.is_empty() {
            self.new_project();
        } else {
            self.open_dropped_project(path);
        }
    }
    /// Load a different document: a fresh start without undo into the old file.
    pub(super) fn replace_document(&mut self, project: Project) {
        self.finish_edit();
        self.finish_drag();
        self.disable_native_audio();
        self.project = project;
        self.history.clear();
        self.history_bytes = 0;
        self.history_steps = 0;
        self.scene_cache = None;
        self.sync_buffers();
        self.dirty = false;
    }
    fn open_dropped_project(&mut self, path: String) {
        match Project::load_file(&path) {
            Ok(project) => {
                self.replace_document(project);
                self.status = format!("„{}“ geöffnet.", files::file_label(&path));
                self.opened(&path);
                self.path = path;
                self.check_recovery();
            }
            Err(error) => {
                self.status = format!("Öffnen fehlgeschlagen: {error}");
                self.recent.retain(|p| p != &path);
            }
        }
    }
    fn dropped_project_ui(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if !dropped.is_empty() {
            if dropped.len() != 1 {
                self.status = "Bitte nur eine Projektdatei hereinziehen.".into();
            } else if let Some(path) = &dropped[0].path {
                let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if extension.eq_ignore_ascii_case("sply") || extension.eq_ignore_ascii_case("json")
                {
                    self.request_open(path.to_string_lossy().into());
                } else {
                    self.status = "Hier .sply oder ältere JSON-Projekte ablegen; SVG über Datei & Export importieren.".into();
                }
            }
        }
        if let Some(path) = self.pending_open.clone() {
            // An empty pending path means "new project".
            let new = path.is_empty();
            egui::Modal::new(egui::Id::new("splinely_switch_prompt")).show(ctx, |ui| {
                ui.set_width(360.);
                ui.heading(if new { "Neues Projekt beginnen?" } else { "Projekt wechseln?" });
                ui.label("Ungespeicherte Änderungen oder Zeichen-/Quelltextentwürfe gehen sonst verloren.");
                if !new {
                    ui.weak(&path);
                }
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    let label = if new { "Verwerfen & neu" } else { "Verwerfen & öffnen" };
                    if ui.button(label).clicked() {
                        self.pending_open = None;
                        if new { self.new_project(); } else { self.open_dropped_project(path); }
                    }
                    if ui.button("Abbrechen").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.pending_open = None;
                    }
                });
            });
        }
    }
    pub fn open_project_file(path: &str) -> Result<Self, String> {
        let project = Project::load_file(path).map_err(|e| e.to_string())?;
        let mut editor = Self {
            project,
            path: path.into(),
            ..Self::default()
        };
        editor.sync_buffers();
        editor.status = format!("„{}“ geöffnet.", files::file_label(path));
        editor.opened(path);
        editor.check_recovery();
        Ok(editor)
    }
    fn save(&mut self) {
        let result = self
            .project
            .save_file(&self.path)
            .map_err(|e| e.to_string());
        match result {
            Ok(()) => {
                self.status = format!("Gespeichert: {}", files::file_label(&self.path));
                self.dirty = false;
                let path = self.path.clone();
                self.opened(&path);
                // A successful save supersedes the autosave.
                if let Some(autosave) = files::autosave_path(&path) {
                    let _ = fs::remove_file(autosave);
                }
            }
            Err(e) => self.status = e,
        }
    }
    fn load(&mut self) {
        self.request_open(self.path.clone());
    }
    fn export(&mut self, interactive: bool) {
        let result = if interactive {
            crate::export::interactive(&self.project, self.trusted_js_export)
        } else {
            let mut snapshot = self.project.clone();
            snapshot.nodes = match self
                .project
                .evaluate_with_context(self.time, &self.preview_context())
            {
                Ok(nodes) => nodes,
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            };
            for node in &mut snapshot.nodes {
                node.bone_id = None;
            }
            snapshot.clips.clear();
            snapshot.state_machines.clear();
            snapshot.to_static_svg(0.)
        };
        self.status = match result {
            Ok(svg) => match fs::write(&self.export_path, svg) {
                Ok(()) => format!("Exportiert: {}", self.export_path),
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        };
    }
    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Splinely");
                if ui
                    .add_enabled(
                        self.history.can_undo() || self.edit_start.is_some(),
                        egui::Button::new("Rückgängig"),
                    )
                    .clicked()
                {
                    self.finish_edit();
                    self.finish_drag();
                    self.history.undo(&mut self.project);
                    self.dirty = true;
                    self.sync_buffers();
                }
                if ui
                    .add_enabled(self.history.can_redo(), egui::Button::new("Wiederholen"))
                    .clicked()
                {
                    self.finish_edit();
                    self.finish_drag();
                    self.history.redo(&mut self.project);
                    self.dirty = true;
                    self.sync_buffers();
                }
                if ui
                    .button(if self.dirty {
                        "Speichern *"
                    } else {
                        "Speichern"
                    })
                    .on_hover_text("Projekt speichern · Strg+S")
                    .clicked()
                {
                    self.save_or_ask();
                }
                if ui
                    .button(if self.playing {
                        "⏸ Pause"
                    } else {
                        "▶ Abspielen"
                    })
                    .on_hover_text("Animation abspielen · Leertaste kurz drücken")
                    .clicked()
                {
                    self.playing = !self.playing;
                    self.preview_animation = true;
                }
                if ui
                    .selectable_label(self.preview_mode, "Vorschau")
                    .on_hover_text("Formen anklicken, um Zustände zu testen. Ein Zeichenwerkzeug wechselt zurück zur Bearbeitung.")
                    .clicked()
                {
                    let preview = !self.preview_mode;
                    self.choose_tool(CanvasTool::Selection);
                    self.preview_mode = preview;
                }
                self.viewer_button(ui);
                ui.menu_button("Datei", |ui| self.file_menu(ui));
            });
            ui.horizontal_wrapped(|ui| {
                for (workspace, label, hint) in [
                    (
                        Workspace::Design,
                        "Gestalten",
                        "Formen zeichnen, auswählen und bearbeiten.",
                    ),
                    (
                        Workspace::Animate,
                        "Animieren",
                        "Zeitleiste, Skelett, Zustände und Audio.",
                    ),
                    (
                        Workspace::Advanced,
                        "Erweitert",
                        "JavaScript und bewahrter SVG-Quelltext.",
                    ),
                ] {
                    if ui
                        .selectable_label(self.workspace == workspace, label)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.finish_edit();
                        self.finish_drag();
                        self.workspace = workspace;
                        self.tab = workspace.sections()[0].0;
                        if workspace == Workspace::Animate {
                            self.timeline_visible = true;
                        }
                    }
                }
                ui.checkbox(&mut self.timeline_visible, "Zeitleiste");
            });
            if self.workspace != Workspace::Design {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Bereich:");
                    for &(tab, label) in self.workspace.sections() {
                        if ui.selectable_label(self.tab == tab, label).clicked() {
                            self.finish_edit();
                            self.finish_drag();
                            self.tab = tab;
                        }
                    }
                });
            }
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });
    }
    fn viewer_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Splinely Viewer öffnen")
            .on_hover_text("Echte SVG-Runtime in einem eigenen Fenster. Momentaufnahme; zum Aktualisieren erneut öffnen.").clicked() {
            self.open_viewer();
        }
    }
    fn open_viewer(&mut self) {
        {
            self.finish_edit();
            self.finish_drag();
            match crate::export::interactive(&self.project, self.trusted_js_export)
                .map_err(|e| e.to_string())
                .and_then(|svg| crate::viewer::ViewerSession::open(&svg))
            {
                Ok(viewer) => {
                    self.viewer = Some(viewer);
                    self.status = "Viewer geöffnet · eigenen Code nur nach Freigabe unter Erweitert ausführen.".into();
                }
                Err(error) => self.status = error,
            }
        }
    }
    fn transport(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline")
            .resizable(true)
            .default_height(290.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("timeline_scroll")
                    .show(ui, |ui| {
                        self.dopesheet_ui(ui);
                        ui.add_space(4.);
                        self.timeline_ui(ui);
                    });
            });
    }
    fn preview_context(&self) -> EvaluationContext {
        let mut context = EvaluationContext {
            states: self
                .machines
                .iter()
                .map(|m| (m.machine_id.clone(), m.current_state.clone()))
                .collect(),
            clips: Some(if self.preview_animation {
                self.project
                    .clips
                    .get(self.active_clip)
                    .map(|c| {
                        vec![ClipPlayback {
                            clip_id: c.id.clone(),
                            time: self.time,
                        }]
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            }),
            overrides: self
                .runtime_overrides
                .iter()
                .cloned()
                .chain(self.live_overrides())
                .collect(),
            ..Default::default()
        };
        self.blend_preview.apply(&self.project, &mut context);
        context
    }
    fn advance_layers(&mut self, mut dt: f64) {
        // Completion actions run at their exact boundary, before the remaining delta.
        let mut boundaries = 0;
        while dt > 0. && self.blend_preview.ticking() {
            let (step, ended) = self.blend_preview.step(&self.project, dt);
            dt = (dt - step).max(0.);
            let completed = !ended.is_empty();
            for id in ended {
                self.send_event("clip.end".into(), Some(id));
            }
            boundaries += 1;
            if boundaries >= 1024 || (step == 0. && !completed) {
                self.status = "Grenze der nativen Wiedergabe erreicht".into();
                break;
            }
        }
    }
    fn advance_playback(&mut self, mut dt: f64) {
        // Segment the global clock too: completion actions affect the remainder.
        for _ in 0..1024 {
            if dt <= 0.0 {
                break;
            }
            let duration = self
                .project
                .clips
                .get(self.active_clip)
                .map(|c| c.duration)
                .unwrap_or(10.);
            let step = if self.playing && !self.looping {
                dt.min((duration - self.time).max(0.))
            } else {
                dt
            };
            self.advance_layers(step);
            if self.playing {
                self.time += step;
                if self.time >= duration {
                    if self.looping {
                        self.time %= duration;
                    } else {
                        self.time = duration;
                        self.playing = false;
                        self.send_event("clip.end".into(), Some("@timeline".into()));
                    }
                }
            }
            dt -= step;
            if step == 0.0 && self.playing {
                break;
            }
        }
    }
    #[cfg(test)]
    fn evaluated(&self) -> Vec<Node> {
        self.project
            .evaluate_with_context(self.time, &self.preview_context())
            .unwrap_or_else(|_| self.project.nodes.clone())
    }
    /// Evaluated scene for the canvas. Re-evaluates only when the document, time,
    /// preview context or highlighted selection changed; idle frames reuse it.
    fn evaluate_scene(&mut self, context: &EvaluationContext) -> Vec<SceneNode> {
        let key = format!(
            "{}|{:?}|{}|{:?}",
            self.time, self.selected, self.highlight, context
        );
        if let Some((project, cached_key, scene)) = &self.scene_cache {
            if *cached_key == key && *project == self.project {
                return scene.clone();
            }
        }
        let scene = self
            .projection
            .evaluate_with_context(&self.project, self.time, context);
        self.scene_cache = Some((self.project.clone(), key, scene.clone()));
        scene
    }
    fn canvas_tool(&self) -> CanvasTool {
        if self.pen.is_some() {
            CanvasTool::Pen
        } else if self.edit_path_handles {
            CanvasTool::PathPoints
        } else if self.freehand_mode {
            CanvasTool::Freehand
        } else {
            CanvasTool::Selection
        }
    }
    fn choose_tool(&mut self, tool: CanvasTool) {
        // Finish real edits; discard only speculative path/pen/freehand gestures.
        self.freeze_canvas_gesture();
        self.eyedropper = false;
        self.freehand.cancel();
        if tool != CanvasTool::Pen {
            self.pen = None;
        } else if self.pen.is_none() {
            self.pen = Some(PenPath::default());
        }
        self.edit_path_handles = tool == CanvasTool::PathPoints;
        self.freehand_mode = tool == CanvasTool::Freehand;
        self.preview_mode = false;
        self.preview_animation = false;
        self.playing = false;
        if self.blend_preview.ticking() {
            // Returning to authoring must also end independent clocks/fades;
            // pausing a layer alone intentionally does not pause its weight ramp.
            self.blend_preview = Default::default();
            self.status = "Ebenenvorschau beendet · bereit zum Gestalten.".into();
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (shape, label) in [
                (shortcuts::NewShape::Rect, "+ Rechteck"),
                (shortcuts::NewShape::Circle, "+ Kreis"),
                (shortcuts::NewShape::Text, "+ Text"),
            ] {
                let key = self.binding_label(&palette::Cmd::Shape(shape));
                if ui
                    .button(label)
                    .on_hover_text(format!("In der Bildmitte einfügen · Taste {key}"))
                    .clicked()
                {
                    self.add_shape(shape);
                }
            }
            ui.menu_button("+ Mehr ⏷", |ui| {
                for (shape, label) in [
                    (shortcuts::NewShape::Polygon, "Sechseck"),
                    (shortcuts::NewShape::Triangle, "Dreieck"),
                    (shortcuts::NewShape::Star, "☆ Stern"),
                    (shortcuts::NewShape::Line, "/ Linie"),
                ] {
                    if ui.button(label).clicked() {
                        ui.close_menu();
                        self.add_shape(shape);
                    }
                }
                ui.weak("Wird als bearbeitbarer Pfad angelegt.");
            });
        });
        ui.horizontal_wrapped(|ui| {
            for (tool, label, hint) in [
                (CanvasTool::Selection, "Auswahl", "Form anklicken und verschieben. Pfeiltasten verschieben (Shift: großer Schritt) · Entf: löschen · Strg+C/V: kopieren/einfügen."),
                (CanvasTool::Pen, "Pen", "Anker klicken, für Tangenten ziehen. Enter: fertig · Escape: verwerfen."),
                (CanvasTool::PathPoints, "Pfadpunkte", "Anker und Tangenten ziehen. Doppelklick auf eine Kante: Anker einfügen. Shift: Tangenten koppeln · Shift+Alt: gegenüberliegende Länge erhalten · Escape: abbrechen."),
                (CanvasTool::Freehand, "Freihand", "Mit Maus oder Tablet zeichnen. Ein Strich = ein Rückgängig-Schritt. Druck nur bei gelieferten Touch-Force-Events."),
            ] {
                let key = self.binding_label(&palette::Cmd::Tool(tool));
                let hint = if key.is_empty() { hint.to_owned() } else { format!("{hint} · Taste {key}") };
                if ui.selectable_label(self.canvas_tool() == tool, label).on_hover_text(hint).clicked() {
                    self.choose_tool(tool);
                }
            }
            if self.freehand_mode {
                ui.add(egui::DragValue::new(&mut self.freehand.options.width).range(0.1..=100.).prefix("Stiftbreite "));
                ui.checkbox(&mut self.freehand.options.pressure_outline, "Druck › Kontur");
            }
            ui.menu_button("Ansicht", |ui| {
                ui.checkbox(&mut self.show_rulers, "Lineale");
                ui.checkbox(&mut self.highlight, "Auswahl hervorheben (Plugin)");
                if ui.button("Auf Auswahl zoomen").on_hover_text(self.binding_label(&palette::Cmd::ZoomSelection)).clicked() {
                    self.zoom_selection_requested = true;
                    ui.close_menu();
                }
                if ui
                    .add_enabled(!self.project.guides.is_empty(), egui::Button::new("Hilfslinien löschen"))
                    .on_hover_text("Neue Hilfslinien: vom Lineal auf die Zeichenfläche ziehen.")
                    .clicked()
                {
                    let mut next = self.project.clone();
                    next.guides.clear();
                    self.commit(next);
                    ui.close_menu();
                }
                if ui.button("Verlauf …").on_hover_text("Alle Rückgängig-Schritte · Klick springt dorthin").clicked() {
                    self.history_open = true;
                    ui.close_menu();
                }
                ui.checkbox(&mut self.onion_skin, "Zwiebelhaut (Animation)")
                    .on_hover_text("Blasse Geister der animierten Formen am vorherigen (rot) und nächsten (blau) Schlüsselbild.");
                ui.checkbox(&mut self.smart_guides, "Intelligente Hilfslinien")
                    .on_hover_text("Beim Verschieben an Kanten und Mitten anderer Formen und der Zeichenfläche einrasten.");
                ui.checkbox(&mut self.settings.show_grid, "Raster anzeigen");
                ui.checkbox(&mut self.snap_grid, format!("Am Raster einrasten ({})", self.settings.grid_size))
                    .on_hover_text("Beim Loslassen die lokale Position der Form am Raster ausrichten · Rastergröße in den Einstellungen.");
                ui.separator();
                if ui.button("Einstellungen …").on_hover_text(self.binding_label(&palette::Cmd::Settings)).clicked() {
                    self.settings_open = true;
                    ui.close_menu();
                }
            });
            ui.add(
                egui::Slider::new(&mut self.zoom, MIN_ZOOM..=MAX_ZOOM)
                    .logarithmic(true)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:.0} %", v * 100.))
                    .custom_parser(|t| t.trim().trim_end_matches('%').trim().replace(',', ".").parse::<f64>().ok().map(|v| v / 100.)),
            )
            .on_hover_text("Strg+Mausrad oder Touchpad-Geste zoomt zum Mauszeiger · +/- · 0 = 100 %");
            if ui.button("Einpassen").on_hover_text(format!("Dokument zentrieren · {}", self.binding_label(&palette::Cmd::Fit))).clicked() {
                self.fit_requested = true;
            }
        });
        if self.pen.is_some() || self.edit_path_handles {
            // Keep the existing staged path actions, including Close/Finish/Cancel.
            // Their legacy switches are normalized into the exclusive tool choice.
            let handles_before = self.edit_path_handles;
            egui::CollapsingHeader::new("Pfadaktionen · schließen / fertig / verwerfen")
                .default_open(true)
                .show(ui, |ui| self.vector_toolbar(ui));
            if self.edit_path_handles && !handles_before {
                self.choose_tool(CanvasTool::PathPoints);
            } else if self.pen.is_some() {
                self.edit_path_handles = false;
                self.freehand_mode = false;
            }
        }
        ui.small(if self.preview_mode {
            "Vorschau: Form anklicken › Zustandsereignis. Zum Bearbeiten Vorschau ausschalten."
        } else {
            match self.canvas_tool() {
                CanvasTool::Selection => "Form anklicken und ziehen · Eigenschaften rechts.",
                CanvasTool::Pen => "Anker klicken oder Tangenten ziehen · Enter: fertig · Escape: verwerfen.",
                CanvasTool::PathPoints => "Pfad auswählen · Punkte ziehen · Doppelklick auf Kante: neuer Anker.",
                CanvasTool::Freehand => "Loszeichnen! Mit Maus oder Tablet · ein Strich, ein Rückgängig-Schritt.",
            }
        }).on_hover_text("Ansicht verschieben: Mitteltaste oder Leertaste+Ziehen. F/Home: einpassen. Leertaste kurz drücken: abspielen. Pfadentwürfe bleiben beim Verschieben der Ansicht erhalten.");
        if self.project.nodes.is_empty() {
            ui.label("Platz für deine Idee! Füge eine Form hinzu oder zeichne los.");
        }
        if self.fit_requested {
            let available = if self.show_rulers {
                rulers::content_rect(Rect::from_min_size(Pos2::ZERO, ui.available_size())).size()
            } else {
                ui.available_size()
            };
            self.zoom = fit_zoom(available, self.project.width, self.project.height);
            self.pan_offset = Vec2::ZERO;
            self.fit_requested = false;
        } else {
            self.pan_offset = zoom_pan(self.pan_offset, self.view_zoom, self.zoom);
        }
        self.view_zoom = self.zoom;
        // A fixed viewport allows unrestricted panning, including beyond document edges.
        ui.scope(|ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            // Response::dragged_by and wants_keyboard_input also read context input;
            // evaluate them outside the input closure to avoid re-entering its lock.
            let primary_dragging = response.dragged_by(egui::PointerButton::Primary);
            let keyboard_available = !ui.ctx().wants_keyboard_input();
            let pan_input = ui.input(|i| PanInput {
                in_canvas: response.contains_pointer(),
                middle_pressed: i.pointer.button_pressed(egui::PointerButton::Middle),
                primary_pressed: i.pointer.button_pressed(egui::PointerButton::Primary),
                primary_dragging,
                middle_down: i.pointer.button_down(egui::PointerButton::Middle),
                primary_down: i.pointer.primary_down(),
                space: keyboard_available
                    && (i.key_down(egui::Key::Space) || i.key_pressed(egui::Key::Space)),
                delta: i.pointer.delta(),
            });
            let was_panning = self.pan_gesture.button.is_some();
            let panning = self.pan_gesture.update(pan_input, &mut self.pan_offset);
            if panning {
                if !was_panning {
                    self.freeze_canvas_gesture();
                }
                if pan_input.space {
                    self.space_pan_used = true;
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if pan_input.space && response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            let document_size = Vec2::new(self.project.width as f32, self.project.height as f32);
            let viewport = response.rect;
            self.update_ruler_gesture(ui, viewport);
            let content = if self.show_rulers {
                rulers::content_rect(viewport)
            } else {
                viewport
            };
            if response.hovered() && !panning {
                let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
                if zoom_delta != 1. {
                    let new_zoom = (self.zoom * zoom_delta).clamp(MIN_ZOOM, MAX_ZOOM);
                    if let Some(cursor) = response.hover_pos() {
                        self.pan_offset = zoom_at(
                            content,
                            document_size,
                            self.zoom,
                            new_zoom,
                            self.pan_offset,
                            cursor,
                        );
                    }
                    self.zoom = new_zoom;
                    self.view_zoom = new_zoom;
                } else if scroll != Vec2::ZERO {
                    // Plain wheel / two-finger scroll moves the view, never the shapes.
                    self.pan_offset += scroll;
                }
            }
            let origin = canvas_origin(content, document_size, self.zoom, self.pan_offset);
            let center = (content.center() - origin) / self.zoom;
            self.view_center = Some([
                (center.x as f64).clamp(0., self.project.width),
                (center.y as f64).clamp(0., self.project.height),
            ]);
            let outer_painter = painter;
            let painter = outer_painter.with_clip_rect(content);
            let document_rect = Rect::from_min_size(origin, document_size * self.zoom);
            painter.rect_filled(document_rect, 0., Color32::WHITE);
            if let Some(mut settings) = self
                .projection
                .resource_mut::<splinely_editor::plugins::SelectionHighlight>()
            {
                settings.enabled = self.highlight;
                settings.selected = self.selected.clone();
            }
            let context = self.preview_context();
            let scene = self.evaluate_scene(&context);
            if let Err(error) = self.raster.paint(
                &painter,
                document_rect,
                self.project.width,
                self.project.height,
                &scene,
            ) {
                self.status = format!("Vorschaufehler: {error}");
            }
            self.paint_onion(&painter, document_rect);
            self.paint_grid(&painter, document_rect);
            for node in &scene {
                draw(
                    &painter,
                    origin,
                    self.zoom,
                    node,
                    self.is_selected(&node.id),
                );
            }
            self.paint_selection_bounds(&painter, origin, &scene);
            self.paint_guides(&painter, origin, content);
            self.paint_pointer_hint(ui, &painter);
            if self.zoom_selection_requested {
                self.apply_zoom_to_selection(content, &scene);
                ui.ctx().request_repaint();
            }
            if self.tab == Tab::Canvas
                && !self.preview_mode
                && self.canvas_tool() == CanvasTool::Selection
            {
                if response.secondary_clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        self.context_pick(&scene, (p - origin) / self.zoom);
                    }
                }
                response.context_menu(|ui| {
                    ui.set_min_width(220.);
                    self.canvas_context_menu(ui);
                });
            }
            if self.show_rulers {
                rulers::paint(&outer_painter, viewport, origin, self.zoom);
            }
            self.paint_user_guides(&painter, origin, content);
            if !panning && self.guide_gesture(ui, &response, viewport, content, origin) {
                return;
            }
            if self.show_rulers && self.ruler_gesture && !panning {
                return;
            }
            let drawing = self.freehand_mode
                && self.tab == Tab::Canvas
                && !self.preview_mode
                && !self.playing
                && !self.blend_preview.ticking()
                && !self.preview_animation;
            match self.freehand.handle(
                ui,
                &painter,
                &response,
                origin,
                self.zoom as f64,
                &self.project,
                drawing && !panning,
            ) {
                Ok(Some((next, id))) => {
                    self.commit(next);
                    self.selected = Some(id);
                }
                Ok(None) => {}
                Err(error) => self.status = error,
            }
            if drawing && !panning {
                return;
            }
            if panning {
                // Draw the retained draft without running Pen's press/release handlers.
                if let Some(pen) = &self.pen {
                    let map =
                        CanvasMap::from_transform(origin, self.zoom as f64, Transform::default());
                    let _ = pen.path().paint_outline(
                        &painter,
                        &map,
                        egui::Stroke::new(2_f32, Color32::from_rgb(30, 170, 210)),
                    );
                    let _ = pen.path().paint_handles(&painter, &map, None);
                }
                if self.tab == Tab::Bones {
                    self.bone_guides(&painter, origin, None);
                }
                return;
            }
            if self.vector_canvas(ui, &response, &painter, origin, &scene) {
                return;
            }
            let pointer = response
                .interact_pointer_pos()
                .map(|p| (p - origin) / self.zoom);
            #[cfg(test)]
            {
                self.debug_origin = Some(origin);
            }
            self.canvas_text_edit(ui, &response, origin, &scene);
            if self.text_draft.is_some() {
                return;
            }
            if self.transform_handles(ui, &response, &painter, origin, &scene) {
                return;
            }
            let joint_picked = if self.tab == Tab::Bones {
                let joint_pick = if response.drag_started() {
                    ui.input(|i| i.pointer.press_origin())
                        .map(|p| (p - origin) / self.zoom)
                } else if response.clicked() {
                    pointer
                } else {
                    None
                };
                self.bone_guides(&painter, origin, joint_pick)
            } else {
                false
            };
            if self.eyedropper {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                if response.clicked() {
                    if let Some(point) = pointer {
                        self.eyedropper_click(&scene, point);
                    }
                    return;
                }
            }
            if (response.clicked() || response.drag_started()) && !joint_picked {
                let pick = if response.drag_started() {
                    ui.input(|i| i.pointer.press_origin())
                        .map(|p| (p - origin) / self.zoom)
                } else {
                    pointer
                };
                let (toggle, isolate) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                if let Some(point) = pick {
                    let hit_id = scene
                        .iter()
                        .rev()
                        .find(|node| {
                            hit(node, point)
                                && self.project.nodes.iter().any(|n| {
                                    n.id == node.id && n.visible && (self.preview_mode || !n.locked)
                                })
                        })
                        .map(|n| n.id.clone());
                    match hit_id {
                        Some(id) if self.preview_mode => self.set_selection(vec![id]),
                        Some(id) => {
                            if response.clicked() || !toggle {
                                self.pick_shape(&id, toggle, isolate, response.drag_started());
                            }
                        }
                        None => {
                            if response.drag_started()
                                && !self.preview_mode
                                && self.tab == Tab::Canvas
                            {
                                // Drag on empty canvas: rubber-band selection.
                                self.marquee = Some(selection::Marquee {
                                    start: point,
                                    end: point,
                                    additive: toggle,
                                });
                            } else if !toggle {
                                self.set_selection(vec![]);
                            }
                        }
                    }
                }
                if response.drag_started()
                    && !self.preview_mode
                    && self.marquee.is_none()
                    && !self.editable_selection().is_empty()
                {
                    let duplicate = ui.input(|i| i.modifiers.alt) && self.tab == Tab::Canvas;
                    self.begin_selection_drag(&scene, duplicate);
                }
                if self.preview_mode && response.clicked() && self.selected.is_some() {
                    self.send_event("click".into(), self.selected.clone());
                }
            }
            if let Some(marquee) = self.marquee.as_mut() {
                if let Some(p) = response.interact_pointer_pos().or(response.hover_pos()) {
                    marquee.end = (p - origin) / self.zoom;
                }
                let marquee = *marquee;
                self.paint_marquee(&painter, origin, &marquee);
                if response.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                    self.marquee = None;
                    self.finish_marquee(marquee, &scene);
                }
                return;
            }
            if response.dragged() && self.drag_start.is_some() {
                self.drag_by(response.drag_delta() / self.zoom);
            }
            if response.drag_stopped() {
                self.finish_drag();
            }
        });
    }
    fn update_ruler_gesture(&mut self, ui: &egui::Ui, viewport: Rect) {
        // press_origin disappears on release; ownership must survive that frame.
        ui.input(|i| {
            if i.pointer.button_pressed(egui::PointerButton::Primary) {
                self.ruler_gesture = self.show_rulers
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|pos| rulers::ruler_hit_test(viewport, pos).is_some());
            } else if !i.pointer.primary_down()
                && !i.pointer.button_released(egui::PointerButton::Primary)
            {
                self.ruler_gesture = false;
            }
        });
    }
    fn freeze_canvas_gesture(&mut self) {
        self.pointer_hint = None;
        self.cancel_transform();
        // Pending path/pen gestures are speculative; keep the existing Pen draft.
        self.path_drag = None;
        self.pen_press = None;
        self.drag_base = None;
        self.drag_snap = None;
        self.guides.clear();
        if let Some(before) = self.drag_start.take() {
            let after = self.project.clone();
            self.project = before;
            // Freeze a node at its current position, without release-time grid snapping.
            self.commit(after);
        }
        self.finish_edit();
    }
    fn drag_by(&mut self, delta: Vec2) {
        self.drag_total += delta;
        if !self.drag_selection_snapped() {
            self.drag_selection(delta);
            self.pointer_hint = Some(format!(
                "Δx {:.1}   Δy {:.1}",
                self.drag_total.x, self.drag_total.y
            ));
        }
    }
    fn finish_drag(&mut self) {
        self.pointer_hint = None;
        self.drag_total = Vec2::ZERO;
        self.drag_base = None;
        self.drag_snap = None;
        self.guides.clear();
        if let Some(before) = self.drag_start.take() {
            if self.snap_grid {
                let grid = self.settings.grid_size;
                let ids = self.editable_selection();
                for node in self
                    .project
                    .nodes
                    .iter_mut()
                    .filter(|n| ids.contains(&n.id))
                {
                    node.transform.x = (node.transform.x / grid).round() * grid;
                    node.transform.y = (node.transform.y / grid).round() * grid;
                }
            }
            let after = self.project.clone();
            self.project = before;
            self.commit(after);
        }
    }
}
impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_selection();
        if let Some(op) = self.pending_arrange.take() {
            self.apply_arrange(op);
        }
        self.dropped_project_ui(ctx);
        self.poll_dialog(ctx);
        self.window_chrome(ctx);
        self.palette_ui(ctx);
        self.history_window(ctx);
        self.settings_window(ctx);
        if let Some(viewer) = self.viewer.as_mut() {
            match viewer.finished() {
                Ok(true) => self.viewer = None,
                Err(error) => {
                    self.status = error;
                    self.viewer = None;
                }
                Ok(false) => {}
            }
        }
        self.poll_native_audio(ctx);
        self.editing = ctx.input(|i| i.pointer.primary_down()) || ctx.wants_keyboard_input();
        if !self.editing {
            self.finish_edit();
        }
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64();
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        self.last_tick = now;
        self.dispatch_events(None);
        let repaint_playback = self.playing;
        self.advance_playback(dt);
        if self.blend_preview.ticking() {
            ctx.request_repaint();
        }
        if repaint_playback {
            ctx.request_repaint();
        }
        self.shortcuts(ctx);
        if self.dirty
            && now.duration_since(self.last_autosave).as_secs() >= self.settings.autosave_seconds
            && self.drag_start.is_none()
        {
            self.last_autosave = now;
            if let (Ok(json), Some(target)) =
                (self.project.to_json(), files::autosave_path(&self.path))
            {
                if let Err(e) = atomic_save(&target, &json) {
                    self.status = format!("Autosave: {e}");
                }
            }
        }
        self.toolbar(ctx);
        if self.timeline_visible {
            self.transport(ctx);
        }
        egui::SidePanel::right("inspector")
            .default_width(280.)
            .width_range(240.0..=520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.tab == Tab::Bones {
                        self.bones_ui(ui);
                        ui.separator();
                    }
                    self.inspector(ui);
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Canvas | Tab::Bones => self.canvas(ui),
            Tab::States => self.states_ui(ui),
            Tab::Audio => self.audio_ui(ui),
            Tab::JavaScript | Tab::Svg => self.code_ui(ui),
        });
        // Defer Space's playback shortcut until release so Space+drag cannot toggle it.
        if ctx.input(|i| i.key_released(egui::Key::Space)) {
            if !ctx.wants_keyboard_input() && !self.space_pan_used && !self.modal_open() {
                self.playing = !self.playing;
                self.preview_animation = true;
            }
            self.space_pan_used = false;
        }
        self.persist_settings(ctx);
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod workspace_tests {
    use super::*;

    #[test]
    fn workspaces_keep_every_existing_panel_accessible() {
        let workspaces = [Workspace::Design, Workspace::Animate, Workspace::Advanced];
        for tab in [
            Tab::Canvas,
            Tab::Bones,
            Tab::States,
            Tab::Audio,
            Tab::JavaScript,
            Tab::Svg,
        ] {
            assert!(workspaces.iter().any(|workspace| workspace
                .sections()
                .iter()
                .any(|&(section, _)| section == tab)));
        }
        assert!(Workspace::Design.sections()[0].0 == Tab::Canvas);
        assert!(Workspace::Animate.sections()[0].0 == Tab::Canvas);
        assert!(Workspace::Advanced.sections()[0].0 == Tab::JavaScript);
    }

    #[test]
    fn source_editors_are_only_in_advanced() {
        for workspace in [Workspace::Design, Workspace::Animate] {
            assert!(workspace
                .sections()
                .iter()
                .all(|&(tab, _)| tab != Tab::JavaScript && tab != Tab::Svg));
        }
        assert_eq!(Workspace::Advanced.sections().len(), 2);
        let editor = Editor::default();
        assert_eq!(editor.workspace, Workspace::Design);
        assert!(editor.tab == Tab::Canvas);
    }
}

#[derive(Clone, Copy, Default)]
struct PanInput {
    in_canvas: bool,
    middle_pressed: bool,
    primary_pressed: bool,
    primary_dragging: bool,
    middle_down: bool,
    primary_down: bool,
    space: bool,
    delta: Vec2,
}

#[derive(Default)]
struct PanGesture {
    button: Option<egui::PointerButton>,
}
impl PanGesture {
    // Return ownership for the entire frame, including the release frame.
    fn update(&mut self, input: PanInput, offset: &mut Vec2) -> bool {
        let captured = self.button.is_some();
        if !captured {
            self.button = if input.in_canvas && input.middle_pressed {
                Some(egui::PointerButton::Middle)
            } else if input.space
                && ((input.in_canvas && input.primary_pressed) || input.primary_dragging)
            {
                Some(egui::PointerButton::Primary)
            } else {
                None
            };
        }
        let consumed = self.button.is_some();
        if captured {
            let down = match self.button {
                Some(egui::PointerButton::Middle) => input.middle_down,
                Some(egui::PointerButton::Primary) => input.primary_down,
                _ => false,
            };
            if down {
                *offset += input.delta;
            }
        }
        // Do not hand an already-held primary button back to editing after middle release.
        if !input.middle_down && !input.primary_down {
            self.button = None;
        }
        consumed
    }
}

fn apply_theme(ctx: &egui::Context, light: bool) {
    let mut style = (*ctx.style()).clone();
    style.visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    style.spacing.item_spacing = Vec2::new(8., 6.);
    style.spacing.button_padding = Vec2::new(10., 6.);
    style.spacing.interact_size.y = 28.;
    style.visuals.selection.bg_fill = Color32::from_rgb(42, 112, 126);
    style.visuals.selection.stroke = egui::Stroke::new(1_f32, Color32::WHITE);
    style.visuals.widgets.hovered.bg_fill = if light {
        Color32::from_rgb(196, 226, 231)
    } else {
        Color32::from_rgb(49, 83, 91)
    };
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(42, 112, 126);
    ctx.set_style(style);
}

fn canvas_origin(viewport: Rect, document: Vec2, zoom: f32, pan: Vec2) -> Pos2 {
    viewport.center() - document * zoom * 0.5 + pan
}

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 32.;

/// Pan offset that keeps the document point under `cursor` fixed while zooming.
fn zoom_at(
    viewport: Rect,
    document: Vec2,
    old_zoom: f32,
    new_zoom: f32,
    pan: Vec2,
    cursor: Pos2,
) -> Vec2 {
    let origin = canvas_origin(viewport, document, old_zoom, pan);
    let point = (cursor - origin) / old_zoom;
    let new_origin = cursor - point * new_zoom;
    new_origin - (viewport.center() - document * new_zoom * 0.5)
}

fn zoom_pan(pan: Vec2, old_zoom: f32, new_zoom: f32) -> Vec2 {
    // Keep the document coordinate under the viewport center invariant.
    pan * (new_zoom / old_zoom)
}

fn fit_zoom(viewport: Vec2, width: f64, height: f64) -> f32 {
    ((viewport.x - 16.).max(1.) / width as f32)
        .min((viewport.y - 16.).max(1.) / height as f32)
        .clamp(MIN_ZOOM, 8.)
}

fn atomic_save_bytes(path: &str, bytes: &[u8]) -> Result<(), String> {
    let temp = format!("{path}.tmp");
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}
fn atomic_save(path: &str, text: &str) -> Result<(), String> {
    let temp = format!("{path}.tmp");
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, path).map_err(|e| e.to_string())
}
fn local_to_scene(n: &SceneNode, p: Vec2) -> Vec2 {
    let p = Vec2::new(p.x * n.scale_x, p.y * n.scale_y);
    let a = n.rotation.to_radians();
    Vec2::new(
        p.x * a.cos() - p.y * a.sin() + n.x,
        p.x * a.sin() + p.y * a.cos() + n.y,
    )
}
fn hit(n: &SceneNode, p: Vec2) -> bool {
    // Fully transparent shapes (e.g. hidden effect sprites) are not clickable.
    if n.scale_x.abs() < 1e-6 || n.scale_y.abs() < 1e-6 || n.opacity <= 0.01 {
        return false;
    }
    let p = p - Vec2::new(n.x, n.y);
    let a = -n.rotation.to_radians();
    let p = Vec2::new(
        (p.x * a.cos() - p.y * a.sin()) / n.scale_x,
        (p.x * a.sin() + p.y * a.cos()) / n.scale_y,
    );
    match &n.shape {
        Shape::Circle { radius } => p.length_sq() <= radius * radius,
        Shape::Path { d, bounds, evenodd } => {
            let tolerance = n.stroke_width * 0.5 + 3. / n.scale_x.abs().min(n.scale_y.abs());
            let in_bounds = p.x >= bounds[0] - tolerance
                && p.y >= bounds[1] - tolerance
                && p.x <= bounds[0] + bounds[2] + tolerance
                && p.y <= bounds[1] + bounds[3] + tolerance;
            if !in_bounds {
                return false;
            }
            let visible = |color: &str| color != "none" && !color.ends_with("00");
            let stroked = visible(&n.stroke) && n.stroke_width > 0.;
            // Fall back to the bounding box if the path cannot be flattened.
            path_hit(
                d,
                p,
                *evenodd,
                n.gradient.is_some() || visible(&n.fill),
                stroked.then_some(tolerance),
            )
            .unwrap_or(true)
        }
        Shape::Rect { width, height, .. } => {
            p.x >= 0. && p.y >= 0. && p.x <= *width && p.y <= *height
        }
        Shape::Text { text, size } => {
            p.x >= 0.
                && p.x <= text.chars().count() as f32 * size * 0.6
                && p.y >= -*size
                && p.y <= size * 0.2
        }
    }
}
/// Exact path hit test in local coordinates: fill by winding rule, stroke by
/// distance to the flattened outline. `None` if the path cannot be parsed.
fn path_hit(d: &str, p: Vec2, evenodd: bool, filled: bool, stroke: Option<f32>) -> Option<bool> {
    use vector::tools::Segment;
    let path = EditablePath::parse(d).ok()?;
    let mut edges: Vec<(Vec2, Vec2)> = vec![];
    let mut closing: Vec<(Vec2, Vec2)> = vec![];
    let (mut start, mut current) = (Vec2::ZERO, Vec2::ZERO);
    let v = |q: [f64; 2]| Vec2::new(q[0] as f32, q[1] as f32);
    for segment in path.segments() {
        match *segment {
            Segment::Move(q) => {
                if current != start {
                    closing.push((current, start));
                }
                start = v(q);
                current = start;
            }
            Segment::Line(q) => {
                edges.push((current, v(q)));
                current = v(q);
            }
            Segment::Quadratic { control, end } => {
                let (c, e) = (v(control), v(end));
                let mut previous = current;
                for i in 1..=12 {
                    let t = i as f32 / 12.;
                    let q = current * (1. - t) * (1. - t) + c * 2. * t * (1. - t) + e * t * t;
                    edges.push((previous, q));
                    previous = q;
                }
                current = e;
            }
            Segment::Cubic { first, second, end } => {
                let (a, b, e) = (v(first), v(second), v(end));
                let mut previous = current;
                for i in 1..=16 {
                    let t = i as f32 / 16.;
                    let u = 1. - t;
                    let q = current * u * u * u
                        + a * 3. * u * u * t
                        + b * 3. * u * t * t
                        + e * t * t * t;
                    edges.push((previous, q));
                    previous = q;
                }
                current = e;
            }
            Segment::Close => {
                if current != start {
                    edges.push((current, start));
                }
                current = start;
            }
        }
    }
    if current != start {
        closing.push((current, start));
    }
    if let Some(tolerance) = stroke {
        let near = edges.iter().any(|&(a, b)| {
            let ab = b - a;
            let t = if ab.length_sq() > 0. {
                ((p - a).dot(ab) / ab.length_sq()).clamp(0., 1.)
            } else {
                0.
            };
            (a + ab * t - p).length() <= tolerance
        });
        if near {
            return Some(true);
        }
    }
    if !filled {
        return Some(false);
    }
    // Fill uses implicitly closed subpaths.
    let mut winding = 0;
    let mut crossings = 0;
    for &(a, b) in edges.iter().chain(closing.iter()) {
        if (a.y <= p.y) != (b.y <= p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x > p.x {
                crossings += 1;
                winding += if b.y > a.y { 1 } else { -1 };
            }
        }
    }
    Some(if evenodd {
        crossings % 2 == 1
    } else {
        winding != 0
    })
}
fn parse_color(text: &str, opacity: f32) -> Color32 {
    let text = text.trim_start_matches('#');
    let mut value = u32::from_str_radix(text, 16).unwrap_or(0x607080);
    let alpha = if text.len() == 8 {
        let a = (value & 255) as f32;
        value >>= 8;
        a
    } else {
        255.
    };
    Color32::from_rgba_unmultiplied(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
        (opacity.clamp(0., 1.) * alpha) as u8,
    )
}
fn draw(painter: &egui::Painter, origin: Pos2, zoom: f32, n: &SceneNode, selected: bool) {
    let convert = |p| origin + local_to_scene(n, p) * zoom;
    if !selected {
        return;
    }
    let fill = parse_color(&n.fill, n.opacity);
    let stroke = if n.stroke == "none" {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(n.stroke_width * zoom, parse_color(&n.stroke, n.opacity))
    };
    let points = match &n.shape {
        Shape::Path { bounds: b, .. } => vec![
            Vec2::new(b[0], b[1]),
            Vec2::new(b[0] + b[2], b[1]),
            Vec2::new(b[0] + b[2], b[1] + b[3]),
            Vec2::new(b[0], b[1] + b[3]),
        ],
        Shape::Rect { width, height, .. } => vec![
            Vec2::ZERO,
            Vec2::new(*width, 0.),
            Vec2::new(*width, *height),
            Vec2::new(0., *height),
        ],
        Shape::Circle { radius } => (0..64)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 64.;
                Vec2::new(a.cos() * radius, a.sin() * radius)
            })
            .collect(),
        Shape::Text { text, size } => {
            // Egui text does not rotate/shear; selection and hit geometry do. Export is exact SVG.
            let _ = (fill, stroke);
            vec![
                Vec2::new(0., -*size),
                Vec2::new(text.chars().count() as f32 * size * 0.6, -*size),
                Vec2::new(text.chars().count() as f32 * size * 0.6, size * 0.2),
                Vec2::new(0., size * 0.2),
            ]
        }
    };
    let points: Vec<_> = points.into_iter().map(convert).collect();
    // Actual geometry, rounded corners, strokes and rotated text are rasterized by resvg.
    if selected {
        painter.rect_stroke(
            Rect::from_points(&points).expand(3.),
            0.,
            egui::Stroke::new(1.5_f32, Color32::from_rgb(30, 120, 255)),
            egui::StrokeKind::Outside,
        );
    }
}
