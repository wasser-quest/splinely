//! SVG import report dialog and the source/visual conflict indicator.
//!
//! The report comes from `Project::from_svg_report`; its per-diagnostic `message` is the
//! document crate's English technical text, everything around it is translated.
use super::*;

/// Report of the last SVG import, shown in a window until the user closes it.
pub(super) struct ImportReportView {
    pub file: String,
    pub report: ImportReport,
    /// Failed imports show the refusal; successful ones the summary.
    pub failed: bool,
    pub losses_only: bool,
}

pub(super) fn status_label(status: ImportStatus) -> &'static str {
    match status {
        ImportStatus::Imported => tr("importiert"),
        ImportStatus::Approximated => tr("angenähert"),
        ImportStatus::Dropped => tr("verworfen"),
        ImportStatus::Rejected => tr("abgelehnt"),
    }
}

fn status_color(status: ImportStatus) -> Color32 {
    match status {
        ImportStatus::Imported => Color32::from_rgb(90, 170, 90),
        ImportStatus::Approximated => Color32::from_rgb(220, 170, 60),
        ImportStatus::Dropped => Color32::from_rgb(220, 120, 60),
        ImportStatus::Rejected => Color32::from_rgb(220, 70, 70),
    }
}

/// One-line German/translated summary used in the status bar and dialog header.
pub(super) fn summary(report: &ImportReport) -> String {
    trf(
        "{imported} importiert · {approximated} angenähert · {dropped} verworfen · {rejected} abgelehnt",
        &[
            report.imported.to_string(),
            report.approximated.to_string(),
            report.dropped.to_string(),
            report.rejected.to_string(),
        ],
    )
}

pub(super) fn conflict_text(sync: SourceSync) -> Option<&'static str> {
    match sync {
        SourceSync::NoBaseline | SourceSync::InSync => None,
        SourceSync::VisualEdited => Some(tr(
            "Konflikt: Die Form wurde visuell bearbeitet, der gespeicherte SVG-Quelltext ist veraltet.",
        )),
        SourceSync::SourceEdited => Some(tr(
            "Konflikt: Der SVG-Quelltext wurde geändert, die visuelle Zeichnung zeigt diese Änderung nicht.",
        )),
        SourceSync::Both => Some(tr(
            "Konflikt: Quelltext und visuelle Zeichnung wurden beide geändert und weichen vom Import ab.",
        )),
    }
}

impl Editor {
    pub(super) fn show_import_report(&mut self, file: &str, report: ImportReport, failed: bool) {
        self.import_report = Some(ImportReportView {
            file: file.to_owned(),
            losses_only: report.has_losses() || failed,
            report,
            failed,
        });
    }

    pub(super) fn import_report_window(&mut self, ctx: &egui::Context) {
        let Some(view) = self.import_report.as_mut() else {
            return;
        };
        let mut open = true;
        let title = if view.failed {
            tr("SVG-Import abgelehnt")
        } else {
            tr("SVG-Importbericht")
        };
        let mut copy = false;
        egui::Window::new(title)
            .id(egui::Id::new("svg-import-report"))
            .open(&mut open)
            .default_width(520.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(trf("Datei: {}", &[files::file_label(&view.file).to_string()]));
                ui.strong(summary(&view.report));
                if view.failed {
                    ui.colored_label(
                        status_color(ImportStatus::Rejected),
                        tr("Nichts wurde importiert – das Projekt blieb unverändert."),
                    );
                } else if view.report.has_losses() {
                    ui.colored_label(
                        status_color(ImportStatus::Dropped),
                        tr("Teile der SVG-Datei wurden angenähert oder verworfen; nichts davon verschwindet unbemerkt."),
                    );
                }
                if view.report.truncated {
                    ui.label(tr("Die Liste ist gekürzt; die Summen oben sind vollständig."));
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut view.losses_only, tr("Nur Abweichungen zeigen"));
                    if ui.button(tr("Bericht als JSON kopieren")).clicked() {
                        copy = true;
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(360.).show(ui, |ui| {
                    for d in view
                        .report
                        .diagnostics
                        .iter()
                        .filter(|d| !view.losses_only || d.status != ImportStatus::Imported)
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(status_color(d.status), status_label(d.status));
                            let mut what = format!("<{}>", d.element);
                            if let Some(id) = &d.id {
                                what.push_str(&format!(" #{id}"));
                            }
                            if let Some(attribute) = &d.attribute {
                                what.push_str(&format!(" {attribute}"));
                            }
                            if d.count > 1 {
                                what.push_str(&format!(" ×{}", d.count));
                            }
                            ui.monospace(what);
                            ui.label(&d.message).on_hover_text(d.code);
                        });
                    }
                });
            });
        if copy {
            if let Some(view) = &self.import_report {
                ctx.copy_text(
                    serde_json::to_string_pretty(&view.report.to_json()).unwrap_or_default(),
                );
            }
        }
        if !open {
            self.import_report = None;
        }
    }

    /// Conflict banner for the SVG source tab, with the way out.
    pub(super) fn source_conflict_ui(&mut self, ui: &mut egui::Ui) {
        let sync = self.project.source_sync();
        match (conflict_text(sync), sync) {
            (Some(text), _) => {
                ui.colored_label(Color32::from_rgb(220, 120, 60), format!("⚠ {text}"));
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Aktuellen Stand als Abgleich übernehmen"))
                        .on_hover_text(tr("Bestätigt, dass Quelltext und Zeichnung bewusst verschieden sind; der Hinweis verschwindet bis zur nächsten Änderung."))
                        .clicked()
                    {
                        let mut p = self.project.clone();
                        p.accept_svg_source_state();
                        self.commit(p);
                        self.status = tr("Abgleich bestätigt.").into();
                    }
                });
            }
            (None, SourceSync::InSync) => {
                ui.colored_label(
                    status_color(ImportStatus::Imported),
                    tr("✔ Quelltext und Zeichnung stimmen mit dem SVG-Import überein."),
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splinely_editor::i18n::{set_thread_language, translation, Lang};

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
<defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient></defs>
<rect id="r" width="40" height="20" fill="url(#g)"/><text id="t" x="5" y="40" font-size="10" font-family="Serif">Hi</text>
<image href="https://example.com/a.png" width="1" height="1"/></svg>"##;

    #[test]
    fn report_is_shown_with_losses_and_failures() {
        let mut e = Editor::default();
        let ok = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
<text x="5" y="40" font-size="10" font-family="Serif">Hi</text></svg>"##;
        let outcome = Project::from_svg_report(ok).unwrap();
        e.show_import_report("a.svg", outcome.report, false);
        let view = e.import_report.as_ref().unwrap();
        assert!(!view.failed);
        assert!(view.losses_only, "font-family is reported as a loss");
        assert!(view
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == "text-font-family"));
        let failure = Project::from_svg_report(SVG).unwrap_err();
        e.show_import_report("b.svg", failure.report, true);
        let view = e.import_report.as_ref().unwrap();
        assert!(view.failed && view.report.rejected == 1);
        assert!(summary(&view.report).contains('1'));
    }

    #[test]
    fn importing_a_file_opens_the_report_and_refusals_keep_the_project() {
        let dir = std::env::temp_dir().join(format!("splinely-svg-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.svg");
        let bad = dir.join("bad.svg");
        std::fs::write(&good, r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><text x="1" y="9" font-size="8">A</text></svg>"##).unwrap();
        std::fs::write(&bad, SVG).unwrap();
        let mut e = Editor::default();
        e.import_svg(good.to_str().unwrap());
        assert_eq!(e.project.nodes.len(), 1);
        assert!(!e.import_report.as_ref().unwrap().failed);
        assert_eq!(e.project.source_sync(), SourceSync::InSync);
        e.import_svg(bad.to_str().unwrap());
        assert_eq!(
            e.project.nodes.len(),
            1,
            "refused import leaves the project"
        );
        let view = e.import_report.as_ref().unwrap();
        assert!(view.failed);
        assert!(view
            .report
            .diagnostics
            .iter()
            .any(|d| d.status == ImportStatus::Rejected));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn conflict_banner_follows_source_sync() {
        let mut p = Project::from_svg(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect id="r" width="5" height="5"/></svg>"##,
        )
        .unwrap();
        assert!(conflict_text(p.source_sync()).is_none());
        p.nodes[0].transform.x = 3.;
        assert_eq!(p.source_sync(), SourceSync::VisualEdited);
        assert!(conflict_text(p.source_sync()).is_some());
        p.accept_svg_source_state();
        assert_eq!(p.source_sync(), SourceSync::InSync);
        p.raw_svg.push(' ');
        assert_eq!(p.source_sync(), SourceSync::SourceEdited);
        p.nodes[0].transform.x = 4.;
        assert_eq!(p.source_sync(), SourceSync::Both);
    }

    /// Every German source string used by this module has a non-empty translation in all
    /// five language tables, and all tables carry the same set of keys.
    #[test]
    fn report_strings_are_translated_in_every_language() {
        let source = include_str!("svg_import_ui.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let mut keys = vec![];
        for marker in ["tr(", "trf("] {
            let mut rest = production;
            while let Some(i) = rest.find(marker) {
                rest = &rest[i + marker.len()..];
                let trimmed = rest.trim_start();
                if !trimmed.starts_with('"') {
                    continue;
                }
                let body = &trimmed[1..];
                let mut end = 0;
                let bytes = body.as_bytes();
                while bytes[end] != b'"' {
                    end += if bytes[end] == b'\\' { 2 } else { 1 };
                }
                keys.push(body[..end].replace("\\\"", "\""));
            }
        }
        assert!(keys.len() >= 20, "found {} keys", keys.len());
        for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::De) {
            for key in &keys {
                assert!(
                    translation(lang, key).is_some(),
                    "{lang:?} lacks a translation for {key:?}"
                );
            }
        }
        set_thread_language(Some(Lang::En));
        assert_eq!(status_label(ImportStatus::Dropped), "dropped");
        set_thread_language(None);
    }

    #[test]
    fn language_tables_have_identical_key_sets() {
        use splinely_editor::i18n::table_keys;
        let en = table_keys(Lang::En);
        for lang in [Lang::Es, Lang::Fr, Lang::Ja, Lang::Zh] {
            let other = table_keys(lang);
            let missing: Vec<_> = en.difference(&other).take(5).collect();
            let extra: Vec<_> = other.difference(&en).take(5).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "{lang:?}: missing {missing:?}, extra {extra:?}"
            );
        }
    }
}
