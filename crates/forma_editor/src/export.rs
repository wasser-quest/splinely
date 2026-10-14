//! Browser capabilities are explicit host choices, never executed by the document loader.
use forma_document::{Action, BrowserExportOptions, DocumentError, Project};

pub fn interactive(project: &Project, trusted_js: bool) -> Result<String, DocumentError> {
    let speech = !project.speech_cues.is_empty()
        || project.state_machines.iter().any(|m| {
            m.states
                .iter()
                .flat_map(|s| s.on_enter.iter())
                .chain(m.transitions.iter().flat_map(|t| &t.actions))
                .any(|a| matches!(a, Action::Speak { .. }))
        });
    project.to_svg_with_options(
        crate::RUNTIME,
        BrowserExportOptions {
            trusted_js,
            speech,
            audio_controls: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_code_execution_requires_explicit_export_opt_in() {
        let project = Project {
            custom_js: "forma.emit('hello'); // ]]>".into(),
            ..Project::default()
        };
        let safe = interactive(&project, false).unwrap();
        let trusted = interactive(&project, true).unwrap();
        assert!(!safe.contains("new Function"));
        assert!(trusted.contains("new Function"));
        roxmltree::Document::parse(&trusted).unwrap();
    }
}
