//! Configurable keyboard shortcuts. Every palette command can be bound to one
//! or more key combinations; defaults below, user overrides in the settings.
//! A single dispatcher replaces the former hard-wired shortcut checks.
use super::files::FileAction;
use super::palette::Cmd;
use super::selection::ArrangeOp;
use super::shortcuts::NewShape;
use super::*;
use splinely_document::{Axis, Stack};

/// A key with exact modifiers (Ctrl means Cmd on macOS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KeyCombo {
    pub key: egui::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyCombo {
    const fn new(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }
    const fn ctrl(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: false,
            alt: false,
        }
    }
    const fn shift(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: true,
            alt: false,
        }
    }
    const fn ctrl_shift(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: true,
            alt: false,
        }
    }

    pub fn matches(&self, key: egui::Key, m: egui::Modifiers) -> bool {
        self.key == key && self.ctrl == m.command && self.shift == m.shift && self.alt == m.alt
    }

    /// Storage form, e.g. "Ctrl+Shift+S".
    pub fn to_storage(self) -> String {
        let mut parts = vec![];
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut combo = KeyCombo::new(egui::Key::A);
        let mut key = None;
        for part in text.split('+') {
            match part {
                "Ctrl" => combo.ctrl = true,
                "Shift" => combo.shift = true,
                "Alt" => combo.alt = true,
                "" => key = Some(egui::Key::Plus), // "Ctrl++"
                name => key = Some(egui::Key::from_name(name)?),
            }
        }
        combo.key = key?;
        Some(combo)
    }

    /// German display form, e.g. "Strg+Umschalt+S".
    pub fn label(self) -> String {
        let mut parts: Vec<String> = vec![];
        if self.ctrl {
            parts.push("Strg".into());
        }
        if self.shift {
            parts.push("Umschalt".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        parts.push(
            match self.key {
                egui::Key::PageUp => "Bild⬆",
                egui::Key::PageDown => "Bild⬇",
                egui::Key::Space => "Leertaste",
                egui::Key::Delete => "Entf",
                egui::Key::Escape => "Esc",
                egui::Key::Home => "Pos1",
                egui::Key::End => "Ende",
                egui::Key::Enter => "Enter",
                other => other.name(),
            }
            .to_owned(),
        );
        parts.join("+")
    }
}

/// Stable id of a command for the settings file.
pub(super) fn command_id(cmd: &Cmd) -> String {
    format!("{cmd:?}")
}

/// Shortcut text for commands on fixed (non-configurable) keys.
pub(super) fn fixed_label(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Delete => "Entf",
        Cmd::Play => "Leertaste",
        _ => "",
    }
}

/// Built-in bindings.
pub(super) fn default_bindings() -> Vec<(Cmd, Vec<KeyCombo>)> {
    use egui::Key as K;
    vec![
        (Cmd::Shape(NewShape::Rect), vec![KeyCombo::new(K::R)]),
        (Cmd::Shape(NewShape::Circle), vec![KeyCombo::new(K::E)]),
        (Cmd::Shape(NewShape::Text), vec![KeyCombo::new(K::T)]),
        (Cmd::Tool(CanvasTool::Selection), vec![KeyCombo::new(K::V)]),
        (Cmd::Tool(CanvasTool::Pen), vec![KeyCombo::new(K::P)]),
        (Cmd::Tool(CanvasTool::PathPoints), vec![KeyCombo::new(K::A)]),
        (Cmd::Tool(CanvasTool::Freehand), vec![KeyCombo::new(K::B)]),
        (Cmd::New, vec![KeyCombo::ctrl(K::N)]),
        (Cmd::File(FileAction::Open), vec![KeyCombo::ctrl(K::O)]),
        (Cmd::Save, vec![KeyCombo::ctrl(K::S)]),
        (
            Cmd::File(FileAction::SaveAs { then_close: false }),
            vec![KeyCombo::ctrl_shift(K::S)],
        ),
        (
            Cmd::File(FileAction::Export { interactive: true }),
            vec![KeyCombo::ctrl(K::E)],
        ),
        (Cmd::Undo, vec![KeyCombo::ctrl(K::Z)]),
        (
            Cmd::Redo,
            vec![KeyCombo::ctrl(K::Y), KeyCombo::ctrl_shift(K::Z)],
        ),
        (Cmd::SelectAll, vec![KeyCombo::ctrl(K::A)]),
        (Cmd::Duplicate, vec![KeyCombo::ctrl(K::D)]),
        (Cmd::Arrange(ArrangeOp::Group), vec![KeyCombo::ctrl(K::G)]),
        (
            Cmd::Arrange(ArrangeOp::Ungroup),
            vec![KeyCombo::ctrl_shift(K::G)],
        ),
        (
            Cmd::Arrange(ArrangeOp::Stack(Stack::Top)),
            vec![KeyCombo::shift(K::PageUp)],
        ),
        (
            Cmd::Arrange(ArrangeOp::Stack(Stack::Raise)),
            vec![KeyCombo::new(K::PageUp)],
        ),
        (
            Cmd::Arrange(ArrangeOp::Stack(Stack::Lower)),
            vec![KeyCombo::new(K::PageDown)],
        ),
        (
            Cmd::Arrange(ArrangeOp::Stack(Stack::Bottom)),
            vec![KeyCombo::shift(K::PageDown)],
        ),
        (
            Cmd::Arrange(ArrangeOp::Flip(Axis::Horizontal)),
            vec![KeyCombo::shift(K::H)],
        ),
        (
            Cmd::Arrange(ArrangeOp::Flip(Axis::Vertical)),
            vec![KeyCombo::shift(K::V)],
        ),
        (Cmd::Fit, vec![KeyCombo::new(K::F), KeyCombo::new(K::Home)]),
        (Cmd::ZoomSelection, vec![KeyCombo::shift(K::Num2)]),
        (
            Cmd::ZoomIn,
            vec![
                KeyCombo::new(K::Plus),
                KeyCombo::new(K::Equals),
                KeyCombo::ctrl(K::Plus),
                KeyCombo::ctrl(K::Equals),
            ],
        ),
        (
            Cmd::ZoomOut,
            vec![KeyCombo::new(K::Minus), KeyCombo::ctrl(K::Minus)],
        ),
        (Cmd::Zoom100, vec![KeyCombo::new(K::Num0)]),
        (Cmd::Eyedropper, vec![KeyCombo::new(K::I)]),
        (Cmd::Palette, vec![KeyCombo::ctrl(K::K)]),
        (Cmd::Shortcuts, vec![KeyCombo::new(K::F1)]),
        (Cmd::Settings, vec![KeyCombo::ctrl(K::Comma)]),
    ]
}

impl Editor {
    /// Effective bindings: user overrides replace the defaults per command.
    pub(super) fn bindings(&self) -> Vec<(Cmd, Vec<KeyCombo>)> {
        let mut result: Vec<(Cmd, Vec<KeyCombo>)> = palette::all_commands()
            .into_iter()
            .map(|cmd| {
                let defaults = default_bindings()
                    .into_iter()
                    .find(|(c, _)| *c == cmd)
                    .map(|(_, k)| k)
                    .unwrap_or_default();
                (cmd, defaults)
            })
            .collect();
        for (cmd, combos) in &mut result {
            if let Some(custom) = self.settings.keybindings.get(&command_id(cmd)) {
                *combos = custom.iter().filter_map(|c| KeyCombo::parse(c)).collect();
            }
        }
        result
    }

    /// Display text of a command's shortcut(s), empty if unbound.
    pub(super) fn binding_label(&self, cmd: &Cmd) -> String {
        self.bindings()
            .into_iter()
            .find(|(c, _)| c == cmd)
            .map(|(_, k)| k.iter().map(|c| c.label()).collect::<Vec<_>>().join(" / "))
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| fixed_label(cmd).to_owned())
    }

    /// Whether a command makes sense right now (canvas focus, selection, …).
    fn command_allowed(&self, cmd: &Cmd) -> bool {
        let canvas = matches!(self.tab, Tab::Canvas | Tab::Bones);
        let editing = !self.preview_mode;
        let selecting =
            self.tab == Tab::Canvas && editing && self.canvas_tool() == CanvasTool::Selection;
        match cmd {
            Cmd::Shape(_) => self.tab == Tab::Canvas && editing,
            Cmd::Tool(tool) => canvas && self.canvas_tool() != *tool,
            Cmd::SelectAll => canvas && editing,
            Cmd::Duplicate | Cmd::Redo => editing,
            Cmd::Arrange(_) => selecting && !self.selection().is_empty(),
            Cmd::Eyedropper => selecting && !self.editable_selection().is_empty(),
            _ => true,
        }
    }

    /// While the shortcut list is shown, keys go to it, not to the editor.
    pub(super) fn editing_keymap(&self) -> bool {
        self.capturing_key.is_some()
            || (self.settings_open && self.settings_tab == super::settings::SettingsTab::Keys)
    }

    /// Run every bound command whose key was pressed this frame.
    pub(super) fn dispatch_keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.editing_keymap() {
            return;
        }
        let presses: Vec<(egui::Key, egui::Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        if presses.is_empty() {
            return;
        }
        let bindings = self.bindings();
        for (key, modifiers) in presses {
            let found = bindings
                .iter()
                .find(|(_, combos)| combos.iter().any(|c| c.matches(key, modifiers)))
                .map(|(cmd, _)| cmd.clone());
            let Some(cmd) = found else {
                continue;
            };
            // Dialogs only let the palette/help keys through.
            if self.modal_open() && !matches!(cmd, Cmd::Shortcuts) {
                continue;
            }
            if self.command_allowed(&cmd) {
                self.run_command(ctx, cmd);
            }
        }
    }

    /// The "Tastenkürzel" tab of the settings window.
    pub(super) fn keymap_ui(&mut self, ui: &mut egui::Ui) {
        // Capture the next key press for the command being rebound.
        if let Some(id) = self.capturing_key.clone() {
            let pressed = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = pressed {
                if key == egui::Key::Escape && !m.any() {
                    self.capturing_key = None;
                } else {
                    let combo = KeyCombo {
                        key,
                        ctrl: m.command,
                        shift: m.shift,
                        alt: m.alt,
                    };
                    // A combo belongs to one command: take it away from others.
                    for (cmd, combos) in self.bindings() {
                        let other = command_id(&cmd);
                        if other != id && combos.contains(&combo) {
                            let kept: Vec<String> = combos
                                .iter()
                                .filter(|c| **c != combo)
                                .map(|c| c.to_storage())
                                .collect();
                            self.settings.keybindings.insert(other, kept);
                            self.status =
                                format!("{} war belegt und wurde dort entfernt.", combo.label());
                        }
                    }
                    self.settings
                        .keybindings
                        .insert(id, vec![combo.to_storage()]);
                    self.capturing_key = None;
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("Suchen");
            ui.add(
                egui::TextEdit::singleline(&mut self.keymap_filter)
                    .hint_text("z. B. speichern, zoom, gruppe"),
            );
        });
        ui.weak(
            "Klick auf ein Kürzel und dann die neue Taste(nkombination) drücken · Esc bricht ab.",
        );
        ui.add_space(4.);
        let filter = self.keymap_filter.to_lowercase();
        let bindings = self.bindings();
        egui::Grid::new("keymap_grid")
            .num_columns(3)
            .striped(true)
            .spacing([12., 4.])
            .show(ui, |ui| {
                for (cmd, combos) in bindings {
                    let label = palette::command_label(&cmd);
                    if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                        continue;
                    }
                    let id = command_id(&cmd);
                    ui.label(label);
                    let capturing = self.capturing_key.as_deref() == Some(id.as_str());
                    let text = if capturing {
                        "Taste drücken …".to_owned()
                    } else if combos.is_empty() {
                        "—".to_owned()
                    } else {
                        combos
                            .iter()
                            .map(|c| c.label())
                            .collect::<Vec<_>>()
                            .join(" / ")
                    };
                    if ui
                        .add(
                            egui::Button::new(text)
                                .selected(capturing)
                                .min_size(egui::vec2(150., 0.)),
                        )
                        .clicked()
                    {
                        self.capturing_key = Some(id.clone());
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("Leeren")
                            .on_hover_text("Kein Kürzel")
                            .clicked()
                        {
                            self.settings.keybindings.insert(id.clone(), vec![]);
                        }
                        if self.settings.keybindings.contains_key(&id)
                            && ui.small_button("Standard").clicked()
                        {
                            self.settings.keybindings.remove(&id);
                        }
                    });
                    ui.end_row();
                }
            });
        ui.add_space(6.);
        ui.weak("Fest belegt: Pfeiltasten (verschieben), Entf (löschen), Esc, Leertaste (abspielen / mit Ziehen: Ansicht verschieben), Strg+C/X/V.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(editor: &mut Editor, ctx: &egui::Context, key: egui::Key, m: egui::Modifiers) {
        let events = [true, false]
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: m,
            })
            .to_vec();
        let _ = ctx.run(
            egui::RawInput {
                events,
                modifiers: m,
                ..Default::default()
            },
            |ctx| editor.dispatch_keys(ctx),
        );
    }

    #[test]
    fn combos_parse_format_and_match_exactly() {
        let combo = KeyCombo::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(combo, KeyCombo::ctrl_shift(egui::Key::S));
        assert_eq!(combo.to_storage(), "Ctrl+Shift+S");
        assert_eq!(combo.label(), "Strg+Umschalt+S");
        assert!(combo.matches(
            egui::Key::S,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
        ));
        assert!(!combo.matches(egui::Key::S, egui::Modifiers::COMMAND));
        assert_eq!(KeyCombo::parse("PageUp").unwrap().label(), "Bild⬆");
        assert!(KeyCombo::parse("Ctrl+Nonsense").is_none());
        // Every default binding survives a storage round trip.
        for (_, combos) in default_bindings() {
            for c in combos {
                assert_eq!(KeyCombo::parse(&c.to_storage()), Some(c), "{c:?}");
            }
        }
    }

    #[test]
    fn rebinding_changes_what_a_key_does() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let count = editor.project.nodes.len();
        // Default: R adds a rectangle.
        press(&mut editor, &ctx, egui::Key::R, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count + 1);
        // Rebind "rectangle" to Q; R no longer does anything.
        editor
            .settings
            .keybindings
            .insert(command_id(&Cmd::Shape(NewShape::Rect)), vec!["Q".into()]);
        press(&mut editor, &ctx, egui::Key::R, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count + 1);
        press(&mut editor, &ctx, egui::Key::Q, egui::Modifiers::NONE);
        assert_eq!(editor.project.nodes.len(), count + 2);
        assert_eq!(editor.binding_label(&Cmd::Shape(NewShape::Rect)), "Q");
        // Unbound commands show nothing.
        editor
            .settings
            .keybindings
            .insert(command_id(&Cmd::Fit), vec![]);
        assert_eq!(editor.binding_label(&Cmd::Fit), "");
    }

    #[test]
    fn every_default_combo_is_unique() {
        let mut seen = vec![];
        for (cmd, combos) in default_bindings() {
            for c in combos {
                assert!(!seen.contains(&c), "{c:?} bound twice (again for {cmd:?})");
                seen.push(c);
            }
        }
    }

    #[test]
    fn capturing_a_key_in_settings_rebinds_and_resolves_conflicts() {
        let ctx = egui::Context::default();
        let mut editor = Editor {
            settings_open: true,
            settings_tab: super::super::settings::SettingsTab::Keys,
            ..Default::default()
        };
        let fit = command_id(&Cmd::Fit);
        editor.capturing_key = Some(fit.clone());
        // Pressing R (the rectangle key) moves R to "Einpassen".
        let events = vec![egui::Event::Key {
            key: egui::Key::R,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                editor.settings_window(ctx);
                editor.shortcuts(ctx);
            },
        );
        assert_eq!(editor.capturing_key, None);
        assert_eq!(editor.binding_label(&Cmd::Fit), "R");
        assert_eq!(editor.binding_label(&Cmd::Shape(NewShape::Rect)), "");
        assert!(editor.status.contains("war belegt"), "{}", editor.status);
        // While capturing nothing else ran: no rectangle was added.
        assert_eq!(
            editor.project.nodes.len(),
            Editor::default().project.nodes.len()
        );
    }

    #[test]
    fn arrow_keys_use_the_configured_step() {
        let ctx = egui::Context::default();
        let mut editor = Editor::default();
        let id = editor
            .project
            .nodes
            .iter()
            .find(|n| !n.locked)
            .unwrap()
            .id
            .clone();
        editor.set_selection(vec![id.clone()]);
        editor.settings.nudge_small = 5.;
        let x = |e: &Editor| {
            e.project
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .transform
                .x
        };
        let events = [true, false]
            .map(|pressed| egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .to_vec();
        let start = x(&editor);
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| editor.shortcuts(ctx),
        );
        assert_eq!(x(&editor), start + 5.);
    }
}
