//! Export only: never execute untrusted custom code in the native editor/viewer.
use super::*;

impl Editor {
    pub(super) fn isolated_preview_button(&mut self, ui: &mut egui::Ui) {
        if ui.button(tr("Isolierte Vorschau exportieren…"))
            .on_hover_text(tr("Browser-HTML mit eingeschränktem Netzwerkzugriff und isolierter Herkunft speichern. Eigenes JavaScript läuft erst nach Zustimmung im Browser. Verwendet angewendeten Projektcode, keine Entwürfe. Native Plugins werden nie ausgeführt."))
            .clicked()
        {
            self.finish_edit();
            self.finish_drag();
            let Some(path) = rfd::FileDialog::new()
                .add_filter(tr("Isolierte Browservorschau"), &["html"])
                .set_file_name("splinely-isolated-preview.html")
                .save_file() else { return; };
            self.status = match self.project.to_isolated_preview_html(crate::RUNTIME)
                .map_err(|e| e.to_string())
                .and_then(|html| fs::write(&path, html).map_err(|e| e.to_string())) {
                Ok(()) => format!("{} {}", tr("Vorschau gespeichert. Im externen Browser öffnen, Warnung lesen und dort zustimmen. Der native Viewer ist keine Sandbox."), path.display()),
                Err(error) => error,
            };
        }
    }
}
