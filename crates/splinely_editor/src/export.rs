//! Browser capabilities are explicit host choices, never executed by the document loader.
use splinely_document::{BrowserExportOptions, DocumentError, Project};

pub fn interactive(project: &Project, trusted_js: bool) -> Result<String, DocumentError> {
    project.to_svg_with_options(
        crate::RUNTIME,
        BrowserExportOptions {
            trusted_js,
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
            custom_js: "splinely.emit('hello'); // ]]>".into(),
            ..Project::default()
        };
        let safe = interactive(&project, false).unwrap();
        let trusted = interactive(&project, true).unwrap();
        assert!(!safe.contains("new Function"));
        assert!(trusted.contains("new Function"));
        roxmltree::Document::parse(&trusted).unwrap();
    }
}
