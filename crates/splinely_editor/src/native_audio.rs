use super::*;
use std::time::Duration;
impl Editor {
    pub(super) fn disable_native_audio(&mut self) {
        self.audio_preview.stop();
        self.audio_enabled = false;
        // Keep the concurrency slot until preparation completes. Stop/reload must
        // never accumulate abandoned workers and full-project snapshots.
        self.audio_load_wanted = false;
    }
    pub(super) fn native_audio_controls(&mut self, ui: &mut egui::Ui) {
        ui.label(tr("Optionale native Vorschau globaler Audioeinsätze über ffplay. Bildbasiertes Timing; maßgeblich bleibt die Audiouhr im Browser-Export."));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.audio_loading.is_none(),
                    egui::Button::new(tr("Native Audiovorschau ein / neu laden")),
                )
                .clicked()
            {
                self.disable_native_audio();
                let project = self.project.clone();
                let (sender, receiver) = std::sync::mpsc::channel();
                self.audio_loading = Some(receiver);
                self.audio_load_wanted = true;
                let spawn = std::thread::Builder::new()
                    .name("splinely-audio-prepare".into())
                    .spawn(move || {
                        let mut preview = audio_preview::AudioPreview::new();
                        let result = preview.set_project(&project).map(|()| preview);
                        let _ = sender.send(result);
                    });
                if let Err(error) = spawn {
                    self.audio_loading = None;
                    self.audio_load_wanted = false;
                    self.status = trf(
                        "Native Audiovorschau nicht vorbereitet: {error}",
                        &[format!("{error}")],
                    );
                }
            }
            if ui.button(tr("Native Audiovorschau stoppen")).clicked() {
                self.disable_native_audio();
            }
            if self.audio_enabled && ui.button(tr("Audiogerät erneut versuchen")).clicked() {
                self.audio_preview.retry();
            }
            if self.audio_loading.is_some() {
                ui.spinner();
                ui.label(if self.audio_load_wanted {
                    tr("Audio wird im Hintergrund vorbereitet …")
                } else {
                    tr("Gestoppt · Vorbereitung im Hintergrund wird beendet …")
                });
            }
            ui.label(trf(
                "{} · {} Stimmen",
                &[
                    (if self.audio_enabled { "aktiv" } else { "aus" }).to_string(),
                    format!("{}", self.audio_preview.active_voices()),
                ],
            ));
        });
    }
    pub(super) fn poll_native_audio(&mut self, ctx: &egui::Context) {
        let result = self
            .audio_loading
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err(tr("Audiovorbereitung unerwartet beendet").into()))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.audio_loading = None;
            if !self.audio_load_wanted {
                return;
            }
            self.audio_load_wanted = false;
            match result {
                Ok(preview) => {
                    self.audio_preview = preview;
                    self.audio_enabled = true;
                    self.audio_preview.seek(self.time, self.playing);
                }
                Err(error) => {
                    self.audio_enabled = false;
                    self.status = trf(
                        "Native Audiovorschau: {error}",
                        std::slice::from_ref(&error),
                    );
                }
            }
        }
        if self.audio_enabled {
            self.audio_preview.update(self.time, self.playing);
        }
        for diagnostic in self.audio_preview.take_diagnostics() {
            self.status = trf(
                "Native Audiovorschau: {diagnostic}",
                std::slice::from_ref(&diagnostic),
            );
        }
        if self.audio_loading.is_some() || self.audio_enabled {
            ctx.request_repaint_after(Duration::from_millis(25));
        }
    }
}
