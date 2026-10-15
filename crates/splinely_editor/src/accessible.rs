//! Accessible names for icon-only controls.
use eframe::egui;

/// Gives an icon-only control an accessible name: reported as a widget-info
/// output event when it gains focus or is used (the hover text alone is only
/// for the mouse). Without the `accesskit` feature that event stream is all
/// egui offers.
pub trait Named {
    fn named(self, name: &str) -> Self;
}

impl Named for egui::Response {
    fn named(self, name: &str) -> Self {
        let enabled = self.enabled();
        self.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_button_reports_its_label_in_the_output_events() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(200., 100.),
            )),
            ..Default::default()
        };
        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = ui.button("🗑");
                response.request_focus();
                response.named("Ebene löschen");
            });
        });
        let labels: Vec<String> = output
            .platform_output
            .events
            .iter()
            .filter_map(|e| match e {
                egui::output::OutputEvent::FocusGained(info) => info.label.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["Ebene löschen"]);
    }
}
