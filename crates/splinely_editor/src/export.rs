//! Browser capabilities are explicit host choices, never executed by the document loader.
use splinely_document::{BrowserExportOptions, DocumentError, EmbedFormat, Project};

/// Interactive SVG; the project is embedded as base64 binary `.sply` (the default).
pub fn interactive(project: &Project, trusted_js: bool) -> Result<String, DocumentError> {
    interactive_with(project, trusted_js, EmbedFormat::Binary)
}

/// Interactive SVG with an explicit embedding; `EmbedFormat::Json` keeps the legacy readable form.
pub fn interactive_with(
    project: &Project,
    trusted_js: bool,
    embed: EmbedFormat,
) -> Result<String, DocumentError> {
    project.to_svg_with_options_embed(
        crate::RUNTIME,
        BrowserExportOptions {
            trusted_js,
            audio_controls: true,
        },
        embed,
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

    fn metadata(svg: &str) -> (Option<String>, String) {
        let doc = roxmltree::Document::parse(svg).unwrap();
        let node = doc
            .descendants()
            .find(|n| n.attribute("id") == Some("splinely-project"))
            .unwrap();
        (
            node.attribute("data-encoding").map(str::to_owned),
            node.text().unwrap().to_owned(),
        )
    }

    #[test]
    fn project_embedding_is_binary_by_default_and_json_on_request() {
        let project = Project {
            name: "Einbettung ü".into(),
            ..Project::default()
        };
        let (encoding, payload) = metadata(&interactive(&project, false).unwrap());
        assert_eq!(encoding.as_deref(), Some("sply-base64"));
        assert!(!payload.contains("Einbettung"));
        let (encoding, payload) =
            metadata(&interactive_with(&project, false, EmbedFormat::Binary).unwrap());
        assert_eq!(encoding.as_deref(), Some("sply-base64"));
        assert!(!payload.contains('{'));
        let (encoding, payload) =
            metadata(&interactive_with(&project, false, EmbedFormat::Json).unwrap());
        assert_eq!(encoding, None);
        assert_eq!(Project::from_json(&payload).unwrap(), project);
    }
}
