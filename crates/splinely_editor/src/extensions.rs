//! Editor sidebar for drop-in plugins. All manifest parsing, validation and the candidate/apply
//! path live in the GUI-free `splinely_document::plugin` module so `splinely-cli plugin ...`
//! runs exactly the same code. No executable plugin code is loaded here: the optional runtime
//! (browser) part of a plugin is only displayed; it is embedded into an export solely by
//! `splinely-cli export --plugin ... --trusted-js`. See docs/plugins.md and docs/drop-in-plugins.md.
use eframe::egui;
use splinely_document::plugin::{CandidateError, ExtensionRegistry as Core};
use splinely_document::Project;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub struct ExtensionRegistry {
    core: Core,
    /// Last failed tool application; stays visible in the panel until a tool succeeds.
    last_error: Option<String>,
}

impl ExtensionRegistry {
    pub fn load_default() -> Self {
        Self::from_core(Core::load_default())
    }
    pub fn load_directory(directory: impl Into<PathBuf>) -> Self {
        Self::from_core(Core::load_directory(directory))
    }
    fn from_core(core: Core) -> Self {
        Self {
            core,
            last_error: None,
        }
    }
    pub fn directory(&self) -> Option<&std::path::Path> {
        self.core.directory.as_deref()
    }
    /// Discovery failures are retained even when other plugins load successfully.
    pub fn diagnostics(&self) -> &[String] {
        &self.core.diagnostics
    }
    pub fn plugins(&self) -> &[splinely_document::plugin::Extension] {
        self.core.plugins()
    }
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Returns a validated, atomic replacement; the input is never mutated.
    /// The host must commit through its normal history/dirty-state mechanism.
    pub fn candidate(
        &self,
        plugin_id: &str,
        tool_id: &str,
        project: &Project,
        selected: Option<&str>,
    ) -> Result<Project> {
        use crate::i18n::{tr, trf};
        self.core
            .candidate(plugin_id, tool_id, project, selected)
            .map_err(|e| match e {
                CandidateError::UnknownPlugin => tr("Unbekanntes Plugin").to_owned(),
                CandidateError::UnknownTool => tr("Unbekanntes Plugin-Werkzeug").to_owned(),
                CandidateError::NeedSelection => tr("Zuerst eine Form auswählen").to_owned(),
                CandidateError::SelectionMissing => {
                    tr("Ausgewählte Form existiert nicht mehr").to_owned()
                }
                CandidateError::SelectionLocked => tr("Ausgewählte Form ist gesperrt").to_owned(),
                CandidateError::Rejected(e) => trf("Plugin-Ergebnis abgelehnt: {e}", &[e]),
                CandidateError::Other(e) => e,
            })
    }

    /// Header text of the sidebar section: a warning count keeps problems visible while collapsed.
    pub fn header(&self) -> String {
        let problems = self.core.diagnostics.len() + usize::from(self.last_error.is_some());
        let title = crate::i18n::tr("Erweiterungen");
        if problems == 0 {
            title.to_owned()
        } else {
            format!("{title} ⚠ {problems}")
        }
    }
    pub fn has_problems(&self) -> bool {
        !self.core.diagnostics.is_empty() || self.last_error.is_some()
    }

    /// Render in the right sidebar. Diagnostics remain visible, not just in logs.
    /// Returns at most one candidate per frame; no selection/history side effects.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: Option<&str>,
    ) -> Result<Option<Project>> {
        use crate::i18n::{tr, trf};
        ui.heading(tr("Drop-in-Plugins"));
        if let Some(directory) = &self.core.directory {
            ui.small(trf("Ordner: {}", &[format!("{}", directory.display())]));
        }
        ui.small(tr(
            "splinely-plugin.json einlegen › Neustart. Nur Daten-Werkzeuge, kein Fremdcode.",
        ));
        for diagnostic in &self.core.diagnostics {
            ui.colored_label(egui::Color32::YELLOW, diagnostic);
        }
        if let Some(error) = &self.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        if self.core.plugins().is_empty() {
            ui.label(tr(
                "Keine Plugins geladen (nach dem Installieren neu starten).",
            ));
        }
        let mut outcome = None;
        for plugin in self.core.plugins() {
            let mut clicked = None;
            ui.push_id(&plugin.id, |ui| {
                ui.collapsing(&plugin.title, |ui| {
                    for (label, tools) in [
                        (tr("Werkzeuge"), &plugin.tools),
                        (tr("Palette"), &plugin.palette),
                    ] {
                        if tools.is_empty() {
                            continue;
                        }
                        ui.label(label);
                        for tool in tools {
                            let enabled = !Core::needs_selection(tool)
                                || selected.is_some_and(|id| {
                                    project.nodes.iter().any(|n| n.id == id && !n.locked)
                                });
                            if ui
                                .add_enabled(enabled, egui::Button::new(&tool.title))
                                .on_disabled_hover_text(tr("Zuerst eine Form auswählen"))
                                .clicked()
                            {
                                clicked = Some(tool.id.as_str());
                            }
                        }
                    }
                    if let Some(runtime) = &plugin.runtime {
                        ui.separator();
                        ui.label(tr("Laufzeit-Teil (Browser)"));
                        ui.small(trf(
                            "Datei: {} · {} Bytes · API {}",
                            &[
                                runtime.entry.clone(),
                                runtime.bytes.to_string(),
                                runtime.api_version.to_string(),
                            ],
                        ));
                        ui.small(trf("Hash: {}", std::slice::from_ref(&runtime.sha256)));
                        if !runtime.description.is_empty() {
                            ui.small(&runtime.description);
                        }
                        ui.small(tr(
                            "Wird nur mit ausdrücklicher Freigabe exportiert: splinely-cli export --plugin … --trusted-js. Der Editor führt diesen Code nie aus.",
                        ));
                    }
                });
            });
            if let Some(id) = clicked {
                outcome = Some(self.candidate(&plugin.id, id, project, selected));
                break;
            }
        }
        match outcome {
            Some(Ok(next)) => {
                self.last_error = None;
                Ok(Some(next))
            }
            Some(Err(error)) => {
                self.last_error = Some(error.clone());
                Err(error)
            }
            None => Ok(None),
        }
    }
}
