//! Browser-only export; consent happens in the generated HTML on every open.
use crate::*;

pub(super) fn export(input: String, output: String, plugins: Vec<String>) -> Result<Value> {
    let project = load(&input)?;
    // Consent and the opaque-origin sandbox are the trust gate here (like the project's customJs).
    let bundles = crate::plugins::bundles(&plugins, true)?;
    let html = project
        .to_isolated_preview_html_with_plugins(RUNTIME, &bundles)
        .map_err(|e| failure("export_error", e))?;
    if output == "-" {
        Ok(json!({"ok":true,"format":"html","html":html}))
    } else {
        atomic_write(&output, html.as_bytes())?;
        Ok(
            json!({"ok":true,"output":output,"format":"html","bytes":html.len(),"customJs":"browser-consent-required",
            "plugins":bundles.iter().map(|b| json!({"id":b.id,"entry":b.entry,"sha256":b.sha256})).collect::<Vec<_>>()}),
        )
    }
}
