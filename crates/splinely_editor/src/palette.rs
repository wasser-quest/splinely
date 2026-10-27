//! Command palette (Strg+K): find and run any editor command by typing a few
//! letters of its name or an English/German synonym. ↑/↓ choose, Enter runs,
//! Escape closes. Also hosts the shortcut overview (F1).
use super::files::FileAction;
use super::path_points::PointOp;
use super::selection::ArrangeOp;
use super::shortcuts::NewShape;
use super::*;
use splinely_document::{AlignEdge, Axis, BooleanOp, Stack};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Cmd {
    Shape(NewShape),
    Tool(CanvasTool),
    File(FileAction),
    New,
    Save,
    Undo,
    Redo,
    History,
    SelectAll,
    Duplicate,
    Delete,
    Arrange(ArrangeOp),
    Points(PointOp),
    Fit,
    ZoomSelection,
    Rulers,
    SmartGuides,
    Grid,
    Onion,
    AutoKey,
    Timeline,
    Play,
    Rewind,
    Workspace(Workspace),
    Viewer,
    Shortcuts,
    ExportPng,
    ExportGif,
    GridVisible,
    ZoomIn,
    ZoomOut,
    Zoom100,
    Eyedropper,
    Palette,
    Settings,
}

/// (label, shortcut, search keywords, command)
fn commands() -> Vec<(&'static str, &'static str, Cmd)> {
    use Cmd::*;
    vec![
        (
            tr("Neues Rechteck"),
            "rect rectangle box form",
            Shape(NewShape::Rect),
        ),
        (tr("Neuer Kreis"), "circle ellipse", Shape(NewShape::Circle)),
        (
            tr("Neuer Text"),
            "text label schrift",
            Shape(NewShape::Text),
        ),
        (
            tr("Neues Sechseck"),
            "polygon hexagon",
            Shape(NewShape::Polygon),
        ),
        (
            tr("Neues Dreieck"),
            "triangle polygon",
            Shape(NewShape::Triangle),
        ),
        (tr("Neuer Stern"), "star", Shape(NewShape::Star)),
        (tr("Neue Linie"), "line strich", Shape(NewShape::Line)),
        (
            tr("Werkzeug Auswahl"),
            "select move tool",
            Tool(CanvasTool::Selection),
        ),
        (
            tr("Werkzeug Pen"),
            "pen bezier zeichnen tool",
            Tool(CanvasTool::Pen),
        ),
        (
            tr("Werkzeug Pfadpunkte"),
            "nodes direct anchor tool",
            Tool(CanvasTool::PathPoints),
        ),
        (
            tr("Werkzeug Freihand"),
            "brush pencil draw tool",
            Tool(CanvasTool::Freehand),
        ),
        (tr("Neues Projekt"), "new file document", New),
        (tr("Öffnen …"), "open load file", File(FileAction::Open)),
        (tr("Speichern"), "save", Save),
        (
            tr("Speichern unter …"),
            "save as",
            File(FileAction::SaveAs { then_close: false }),
        ),
        (
            tr("SVG importieren …"),
            "import svg",
            File(FileAction::ImportSvg),
        ),
        (
            tr("Interaktives SVG exportieren …"),
            "export svg interactive",
            File(FileAction::Export { interactive: true }),
        ),
        (
            tr("Statisches SVG exportieren …"),
            "export svg static",
            File(FileAction::Export { interactive: false }),
        ),
        (
            tr("Bild als PNG exportieren …"),
            "export png image bitmap raster",
            ExportPng,
        ),
        (
            tr("Animation als GIF exportieren …"),
            "export gif animation",
            ExportGif,
        ),
        (tr("Rückgängig"), "undo", Undo),
        (
            tr("Verlauf anzeigen"),
            "history undo steps verlauf",
            History,
        ),
        (tr("Wiederholen"), "redo", Redo),
        (tr("Alles auswählen"), "select all", SelectAll),
        (tr("Duplizieren"), "duplicate copy", Duplicate),
        (tr("Löschen"), "delete remove", Delete),
        (tr("Gruppieren"), "group", Arrange(ArrangeOp::Group)),
        (tr("Gruppe lösen"), "ungroup", Arrange(ArrangeOp::Ungroup)),
        (
            tr("Links ausrichten"),
            "align left",
            Arrange(ArrangeOp::Align(AlignEdge::Left)),
        ),
        (
            tr("Horizontal zentrieren"),
            "align center horizontal",
            Arrange(ArrangeOp::Align(AlignEdge::HorizontalCenter)),
        ),
        (
            tr("Rechts ausrichten"),
            "align right",
            Arrange(ArrangeOp::Align(AlignEdge::Right)),
        ),
        (
            tr("Oben ausrichten"),
            "align top",
            Arrange(ArrangeOp::Align(AlignEdge::Top)),
        ),
        (
            tr("Vertikal zentrieren"),
            "align middle vertical",
            Arrange(ArrangeOp::Align(AlignEdge::VerticalCenter)),
        ),
        (
            tr("Unten ausrichten"),
            "align bottom",
            Arrange(ArrangeOp::Align(AlignEdge::Bottom)),
        ),
        (
            tr("Horizontal verteilen"),
            "distribute spacing",
            Arrange(ArrangeOp::Distribute(Axis::Horizontal)),
        ),
        (
            tr("Vertikal verteilen"),
            "distribute spacing",
            Arrange(ArrangeOp::Distribute(Axis::Vertical)),
        ),
        (
            tr("Horizontal spiegeln"),
            "flip mirror",
            Arrange(ArrangeOp::Flip(Axis::Horizontal)),
        ),
        (
            tr("Vertikal spiegeln"),
            "flip mirror",
            Arrange(ArrangeOp::Flip(Axis::Vertical)),
        ),
        (
            tr("Ganz nach vorne"),
            "bring front order",
            Arrange(ArrangeOp::Stack(Stack::Top)),
        ),
        (
            tr("Nach vorne"),
            "raise forward order",
            Arrange(ArrangeOp::Stack(Stack::Raise)),
        ),
        (
            tr("Nach hinten"),
            "lower backward order",
            Arrange(ArrangeOp::Stack(Stack::Lower)),
        ),
        (
            tr("Ganz nach hinten"),
            "send back order",
            Arrange(ArrangeOp::Stack(Stack::Bottom)),
        ),
        (
            tr("Vereinigen"),
            "union boolean combine pathfinder",
            Arrange(ArrangeOp::Boolean(BooleanOp::Union)),
        ),
        (
            tr("Abziehen"),
            "difference subtract minus boolean",
            Arrange(ArrangeOp::Boolean(BooleanOp::Difference)),
        ),
        (
            tr("Schnittmenge"),
            "intersection intersect boolean",
            Arrange(ArrangeOp::Boolean(BooleanOp::Intersection)),
        ),
        (
            tr("Ausschluss"),
            "exclusion xor exclude boolean",
            Arrange(ArrangeOp::Boolean(BooleanOp::Exclusion)),
        ),
        (
            tr("Morph: erste › zweite Form"),
            "morph tween shape animation",
            Arrange(ArrangeOp::Morph),
        ),
        (tr("Einpassen"), "fit zoom view", Fit),
        (
            tr("Auf Auswahl zoomen"),
            "zoom selection view",
            ZoomSelection,
        ),
        (tr("Lineale ein/aus"), "rulers view", Rulers),
        (
            tr("Intelligente Hilfslinien ein/aus"),
            "smart guides snapping",
            SmartGuides,
        ),
        (tr("Am Raster einrasten ein/aus"), "grid snap raster", Grid),
        (
            tr("Zwiebelhaut ein/aus"),
            "onion skin ghost animation",
            Onion,
        ),
        (tr("Zeitleiste ein/aus"), "timeline", Timeline),
        (tr("Abspielen / Pause"), "play pause animation", Play),
        (tr("Zum Anfang"), "rewind start", Rewind),
        (
            tr("Arbeitsbereich Gestalten"),
            "design workspace",
            Workspace(super::Workspace::Design),
        ),
        (
            tr("Arbeitsbereich Animieren"),
            "animate workspace",
            Workspace(super::Workspace::Animate),
        ),
        (
            tr("Arbeitsbereich Erweitert"),
            "advanced code javascript",
            Workspace(super::Workspace::Advanced),
        ),
        (
            tr("Splinely Viewer öffnen"),
            "viewer preview browser runtime",
            Viewer,
        ),
        (
            tr("Tastenkürzel anzeigen"),
            "help shortcuts keys hilfe",
            Shortcuts,
        ),
        (
            tr("Raster anzeigen ein/aus"),
            "grid show visible raster",
            GridVisible,
        ),
        (tr("Hineinzoomen"), "zoom in bigger", ZoomIn),
        (tr("Herauszoomen"), "zoom out smaller", ZoomOut),
        (tr("Zoom 100 %"), "zoom actual size reset", Zoom100),
        (
            tr("Pipette: Stil übernehmen"),
            "eyedropper pick style color",
            Eyedropper,
        ),
        (
            tr("Aufnahme (Auto-Key) ein/aus"),
            "auto key record animation",
            AutoKey,
        ),
        (
            tr("Punkte: Tangenten trennen (Ecke)"),
            "path point node cusp corner split break tangent ecke",
            Points(PointOp::Cusp),
        ),
        (
            tr("Punkte: glatt"),
            "path point node smooth glatt",
            Points(PointOp::Smooth),
        ),
        (
            tr("Punkte: symmetrisch"),
            "path point node symmetric symmetrisch",
            Points(PointOp::Symmetric),
        ),
        (
            tr("Punkte: automatisch"),
            "path point node auto automatic",
            Points(PointOp::Auto),
        ),
        (
            tr("Punkte: Griffe einziehen"),
            "path point handles retract einziehen",
            Points(PointOp::Retract),
        ),
        (
            tr("Punkte: Griffe herausziehen"),
            "path point handles extend herausziehen",
            Points(PointOp::Extend),
        ),
        (
            tr("Punkte: Pfad auftrennen"),
            "path point break split open auftrennen",
            Points(PointOp::Break),
        ),
        (
            tr("Punkte: Endpunkte verbinden"),
            "path point join connect close verbinden",
            Points(PointOp::Join),
        ),
        (
            tr("Punkte: Segment zu Linie"),
            "path segment line straight linie",
            Points(PointOp::ToLine),
        ),
        (
            tr("Punkte: Segment zu Kurve"),
            "path segment curve bezier kurve",
            Points(PointOp::ToCurve),
        ),
        (
            tr("Punkte: Pfadrichtung umkehren"),
            "path reverse direction umkehren",
            Points(PointOp::Reverse),
        ),
        (tr("Befehlspalette"), "command palette search", Palette),
        (
            tr("Einstellungen …"),
            "settings preferences options keys shortcuts optionen",
            Settings,
        ),
    ]
}

/// Subsequence match score (higher is better); `None` if not all letters occur.
fn score(query: &str, text: &str) -> Option<i32> {
    let text = text.to_lowercase();
    let mut chars = text.chars();
    let mut score = 0;
    let mut previous_matched = false;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let mut found = false;
        for c in chars.by_ref() {
            if c == q {
                score += if previous_matched { 3 } else { 1 };
                previous_matched = true;
                found = true;
                break;
            }
            previous_matched = false;
        }
        if !found {
            return None;
        }
    }
    if text.starts_with(&query.to_lowercase()) {
        score += 10;
    }
    Some(score)
}

/// Every command in palette order.
pub(super) fn all_commands() -> Vec<Cmd> {
    commands().into_iter().map(|c| c.2).collect()
}

/// German name of a command.
pub(super) fn command_label(cmd: &Cmd) -> &'static str {
    commands()
        .into_iter()
        .find(|c| c.2 == *cmd)
        .map_or("", |c| c.0)
}

/// Commands matching `query`, best first.
pub(super) fn matches(query: &str) -> Vec<(&'static str, Cmd)> {
    let mut found: Vec<(i32, usize, (&'static str, Cmd))> = commands()
        .into_iter()
        .enumerate()
        .filter_map(|(i, (label, keywords, cmd))| {
            if query.trim().is_empty() {
                return Some((0, i, (label, cmd)));
            }
            let best = score(query, label)
                .map(|s| s + 5)
                .into_iter()
                .chain(keywords.split(' ').filter_map(|k| score(query, k)))
                // Multi-word queries ("align left") match the keywords as a whole.
                .chain(score(query, keywords))
                .max()?;
            Some((best, i, (label, cmd)))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, _, c)| c).collect()
}

impl Editor {
    pub(super) fn run_command(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::Shape(shape) => {
                self.tab = Tab::Canvas;
                self.add_shape(shape);
            }
            Cmd::Tool(tool) => self.choose_tool(tool),
            Cmd::File(action) => self.start_dialog(action),
            Cmd::New => self.open_new_dialog(),
            Cmd::Save => self.save_or_ask(),
            Cmd::Undo => self.undo_redo(false),
            Cmd::Redo => self.undo_redo(true),
            Cmd::History => self.history_open = true,
            Cmd::SelectAll if self.select_all_points() => {}
            Cmd::SelectAll => {
                if matches!(self.tab, Tab::Canvas | Tab::Bones) {
                    self.choose_tool(CanvasTool::Selection);
                }
                self.select_all();
            }
            Cmd::Duplicate => self.duplicate_selected(),
            Cmd::Delete => self.delete_selected(),
            Cmd::Arrange(op) => self.apply_arrange(op),
            Cmd::Points(op) => {
                self.apply_point_op(op);
            }
            Cmd::Fit => self.fit_requested = true,
            Cmd::ZoomSelection => self.zoom_selection_requested = true,
            Cmd::Rulers => self.show_rulers = !self.show_rulers,
            Cmd::SmartGuides => self.smart_guides = !self.smart_guides,
            Cmd::Grid => self.snap_grid = !self.snap_grid,
            Cmd::Onion => self.onion_skin = !self.onion_skin,
            Cmd::AutoKey => {
                self.auto_key = !self.auto_key;
                if self.auto_key {
                    self.preview_animation = true;
                    self.timeline_visible = true;
                }
            }
            Cmd::Timeline => self.timeline_visible = !self.timeline_visible,
            Cmd::Play => {
                self.playing = !self.playing;
                self.preview_animation = true;
            }
            Cmd::Rewind => {
                self.time = 0.;
                self.playing = false;
            }
            Cmd::Workspace(workspace) => {
                self.workspace = workspace;
                self.tab = workspace.sections()[0].0;
                if workspace == Workspace::Animate {
                    self.timeline_visible = true;
                }
            }
            Cmd::Viewer => self.open_viewer(),
            Cmd::Shortcuts => self.shortcuts_open = true,
            Cmd::ExportPng => self.start_dialog(FileAction::ExportPng),
            Cmd::ExportGif => self.start_dialog(FileAction::ExportGif),
            Cmd::GridVisible => self.settings.show_grid = !self.settings.show_grid,
            Cmd::ZoomIn => self.zoom = (self.zoom * 1.2).clamp(MIN_ZOOM, MAX_ZOOM),
            Cmd::ZoomOut => self.zoom = (self.zoom / 1.2).clamp(MIN_ZOOM, MAX_ZOOM),
            Cmd::Zoom100 => self.zoom = 1.,
            Cmd::Eyedropper => {
                self.eyedropper = true;
                self.status =
                    tr("Pipette: Form anklicken, deren Stil übernommen wird · Escape bricht ab.")
                        .into();
            }
            Cmd::Palette => {
                if self.palette.is_none() {
                    self.palette = Some((String::new(), 0));
                }
            }
            Cmd::Settings => self.settings_open = true,
        }
        ctx.request_repaint();
    }

    /// The palette window and the shortcut overview (opened via the keymap).
    pub(super) fn palette_ui(&mut self, ctx: &egui::Context) {
        // Opening never toggles: egui may run a discarded pass with the same
        // input when a window first appears, which would toggle it back.
        if self.shortcuts_open
            && self.palette.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.shortcuts_open = false;
        }
        self.shortcuts_window(ctx);
        let Some((mut query, mut index)) = self.palette.take() else {
            return;
        };
        let found = matches(&query);
        let bindings = self.bindings();
        let shortcut = |cmd: &Cmd| {
            bindings
                .iter()
                .find(|(c, _)| c == cmd)
                .and_then(|(_, k)| k.first())
                .map(|k| k.label())
                .unwrap_or_else(|| keymap::fixed_label(cmd).to_owned())
        };
        let (up, down, enter, escape) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if down {
            index = (index + 1).min(found.len().saturating_sub(1));
        }
        if up {
            index = index.saturating_sub(1);
        }
        let mut run = None;
        let mut keep = !escape;
        egui::Modal::new(egui::Id::new("splinely_palette")).show(ctx, |ui| {
            ui.set_width(440.);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text(tr("Befehl suchen … (z. B. „verein“, „align“, „png“)"))
                    .desired_width(f32::INFINITY),
            );
            edit.request_focus();
            if edit.changed() {
                index = 0;
            }
            ui.add_space(4.);
            egui::ScrollArea::vertical()
                .max_height(320.)
                .show(ui, |ui| {
                    for (i, (label, cmd)) in found.iter().enumerate() {
                        let response = ui.add(
                            egui::Button::new(*label)
                                .shortcut_text(shortcut(cmd))
                                .selected(i == index)
                                .frame(i == index)
                                .min_size(egui::vec2(ui.available_width(), 0.)),
                        );
                        if i == index && (up || down) {
                            response.scroll_to_me(None);
                        }
                        if response.clicked() {
                            run = Some(cmd.clone());
                        }
                    }
                    if found.is_empty() {
                        ui.weak(tr("Kein Befehl gefunden."));
                    }
                });
            if ui.input(|i| i.pointer.any_click()) && !ui.ui_contains_pointer() {
                keep = false;
            }
        });
        if enter {
            run = found.get(index).map(|c| c.1.clone());
        }
        if let Some(cmd) = run {
            keep = false;
            self.run_command(ctx, cmd);
        }
        if keep {
            self.palette = Some((query, index));
        }
    }

    /// Everything the overview shows, as (keys, description) rows; an empty
    /// key text marks a heading. Built from the live key bindings and the
    /// fixed-key table, so the list follows both.
    pub(super) fn shortcut_rows(&self) -> Vec<(String, String)> {
        let mut rows = vec![(String::new(), tr("Befehle").to_owned())];
        for (cmd, keys) in self.bindings() {
            if !keys.is_empty() {
                let keys: Vec<String> = keys.iter().map(|k| k.label()).collect();
                rows.push((keys.join(" / "), command_label(&cmd).to_owned()));
            }
        }
        for (keys, what) in FIXED_SHORTCUTS {
            rows.push((
                if keys.is_empty() {
                    String::new()
                } else {
                    tr(keys).to_owned()
                },
                tr(what).to_owned(),
            ));
        }
        rows
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.shortcuts_open;
        let rows = self.shortcut_rows();
        let mut customize = false;
        egui::Window::new(tr("Tastenkürzel"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.)
            .show(ctx, |ui| {
                if ui.button(tr("Tastenkürzel ändern …")).clicked() {
                    customize = true;
                }
                egui::ScrollArea::vertical()
                    .max_height(480.)
                    .show(ui, |ui| {
                        egui::Grid::new("shortcut_grid")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (keys, what) in &rows {
                                    if keys.is_empty() {
                                        ui.strong(what);
                                        ui.end_row();
                                        continue;
                                    }
                                    ui.monospace(keys);
                                    ui.label(what);
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.shortcuts_open = open;
        if customize {
            self.settings_open = true;
            self.settings_tab = super::settings::SettingsTab::Keys;
        }
    }
}

/// Mouse gestures and keys that are not configurable.
const FIXED_SHORTCUTS: &[(&str, &str)] = &[
    ("", "Maus und feste Tasten"),
    ("Klick / Umschalt+Klick", "Wählen / hinzufügen"),
    ("Strg+Klick", "Einzelne Form in einer Gruppe"),
    ("Ziehen auf freier Fläche", "Auswahlrechteck"),
    ("Strg+C / X / V", "Kopieren / Ausschneiden / Einfügen"),
    ("Alt+Ziehen", "Duplizieren"),
    ("Entf", "Löschen (oder gewähltes Schlüsselbild)"),
    (
        "Pfeile (Umschalt)",
        "Verschieben (großer Schritt, siehe Einstellungen)",
    ),
    ("Esc", "Auswahl aufheben / abbrechen"),
    ("", "Tastaturbedienung der Zeichenfläche"),
    ("F6", "Zeichenfläche fokussieren"),
    (
        "Tab / Umschalt+Tab",
        "Nächste / vorherige sichtbare, entsperrte Form (Gruppe als eine Station)",
    ),
    ("Enter", "Gruppe betreten / Text bearbeiten"),
    ("Umschalt+Enter", "Gruppe verlassen"),
    (
        "Pfeile",
        "Auswahl um einen kleinen Schritt verschieben (Standard 1)",
    ),
    (
        "Esc, Esc",
        "Auswahl aufheben, dann die Zeichenfläche verlassen",
    ),
    ("? oder F1", "Diese Übersicht"),
    ("", "Pfadpunkte-Werkzeug"),
    ("Klick / Umschalt+Klick", "Punkt wählen / hinzufügen"),
    ("Ziehen auf freier Fläche", "Punkte per Rahmen wählen"),
    ("Punkt ziehen", "Gewählte Punkte samt Griffen verschieben"),
    ("Doppelklick auf Kante", "Punkt einfügen"),
    ("Entf / Rücktaste", "Gewählte Punkte löschen"),
    ("Pfeile (Umschalt)", "Gewählte Punkte verschieben"),
    (
        "Strg+C / X / V",
        "Punkte als Teilpfad kopieren / ausschneiden / einfügen",
    ),
    ("", "Zeichenstift"),
    ("Rücktaste", "Letzten Punkt entfernen"),
    ("Klick auf ersten Punkt", "Pfad schließen"),
    ("Enter / Esc", "Fertigstellen / abbrechen"),
    ("", "Maus und Ansicht"),
    ("Griffe + Umschalt / Alt", "Proportional / von der Mitte"),
    ("Strg+Mausrad", "Zoomen"),
    ("Mausrad, Leertaste+Ziehen", "Ansicht verschieben"),
    ("Leertaste", "Abspielen / Pause"),
    (
        "Zeitleiste: Raute ziehen",
        "Schlüsselbild verschieben (Alt: frei)",
    ),
    ("Zeitleiste: Doppelklick", "Schlüsselbild einfügen"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_search_finds_commands_by_german_and_english_words() {
        assert_eq!(
            matches("verein")[0].1,
            Cmd::Arrange(ArrangeOp::Boolean(BooleanOp::Union))
        );
        assert_eq!(
            matches("union")[0].1,
            Cmd::Arrange(ArrangeOp::Boolean(BooleanOp::Union))
        );
        assert_eq!(matches("png")[0].1, Cmd::ExportPng);
        assert!(matches("align left")
            .iter()
            .any(|c| c.1 == Cmd::Arrange(ArrangeOp::Align(AlignEdge::Left))));
        assert_eq!(matches("stern")[0].1, Cmd::Shape(NewShape::Star));
        assert!(matches("qqqqzz").is_empty());
        assert_eq!(matches("").len(), commands().len());
    }

    #[test]
    fn running_commands_changes_the_editor() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let count = editor.project.nodes.len();
        editor.run_command(&ctx, Cmd::Shape(NewShape::Star));
        assert_eq!(editor.project.nodes.len(), count + 1);
        editor.run_command(&ctx, Cmd::Undo);
        assert_eq!(editor.project.nodes.len(), count);
        editor.run_command(&ctx, Cmd::Workspace(Workspace::Animate));
        assert!(editor.timeline_visible);
        editor.run_command(&ctx, Cmd::Shortcuts);
        assert!(editor.shortcuts_open);
        // New project: preset size via the dialog's stored choice.
        editor.run_command(&ctx, Cmd::New);
        assert!(editor.new_dialog.is_some());
        editor.new_dialog = None;
        editor.new_size = Some((1080., 1920., "Story".into()));
        editor.dirty = false;
        editor.request_open(String::new());
        assert_eq!(
            (editor.project.width, editor.project.height),
            (1080., 1920.)
        );
        assert_eq!(editor.project.name, "Story");
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;

    #[test]
    fn raster_exports_write_png_and_gif_of_the_active_clip() {
        let dir = std::env::temp_dir().join(format!("splinely-raster-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut editor = Editor::default();
        editor.project.width = 64.;
        editor.project.height = 48.;
        let png = dir.join("a.png");
        editor.export_raster(png.to_str().unwrap(), false);
        assert!(
            std::fs::read(&png).unwrap().starts_with(b"\x89PNG"),
            "{}",
            editor.status
        );
        let gif = dir.join("a.gif");
        editor.export_raster(gif.to_str().unwrap(), true);
        assert!(
            std::fs::read(&gif).unwrap().starts_with(b"GIF89a"),
            "{}",
            editor.status
        );
        // Without any clip there is nothing to animate.
        editor.project.clips.clear();
        editor.project.state_machines.clear();
        editor.export_raster(dir.join("b.gif").to_str().unwrap(), true);
        assert!(editor.status.contains("kein Clip"), "{}", editor.status);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;

    fn frame(
        editor: &mut Editor,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                editor.shortcuts(ctx);
                editor.palette_ui(ctx)
            },
        );
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        [true, false]
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            })
            .to_vec()
    }

    #[test]
    fn f1_opens_the_shortcut_window_and_escape_closes_it() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        frame(
            &mut editor,
            &ctx,
            key(egui::Key::F1, egui::Modifiers::NONE),
            egui::Modifiers::NONE,
        );
        assert!(editor.shortcuts_open);
        // A repeated pass with the same input must not close it again.
        frame(
            &mut editor,
            &ctx,
            key(egui::Key::F1, egui::Modifiers::NONE),
            egui::Modifiers::NONE,
        );
        assert!(editor.shortcuts_open);
        frame(
            &mut editor,
            &ctx,
            key(egui::Key::Escape, egui::Modifiers::NONE),
            egui::Modifiers::NONE,
        );
        assert!(!editor.shortcuts_open);
    }

    #[test]
    fn ctrl_n_opens_the_new_project_dialog() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let _ = ctx.run(
            egui::RawInput {
                events: key(egui::Key::N, egui::Modifiers::COMMAND),
                modifiers: egui::Modifiers::COMMAND,
                ..Default::default()
            },
            |ctx| editor.shortcuts(ctx),
        );
        assert!(editor.new_dialog.is_some());
        assert!(editor.modal_open(), "dialog blocks other shortcuts");
    }

    #[test]
    fn ctrl_k_opens_typing_filters_and_enter_runs() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let count = editor.project.nodes.len();
        frame(
            &mut editor,
            &ctx,
            key(egui::Key::K, egui::Modifiers::COMMAND),
            egui::Modifiers::COMMAND,
        );
        assert!(editor.palette.is_some(), "palette opens");
        frame(&mut editor, &ctx, vec![], egui::Modifiers::NONE);
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Text("stern".into())],
            egui::Modifiers::NONE,
        );
        assert_eq!(editor.palette.as_ref().unwrap().0, "stern");
        frame(
            &mut editor,
            &ctx,
            key(egui::Key::Enter, egui::Modifiers::NONE),
            egui::Modifiers::NONE,
        );
        assert!(editor.palette.is_none());
        assert_eq!(editor.project.nodes.len(), count + 1);
        assert_eq!(editor.project.nodes.last().unwrap().name, "Stern");
    }
}
