//! Browser-only isolated preview. Never pass this HTML to a privileged native webview.
use crate::plugin::RuntimeBundle;
use crate::{BrowserExportOptions, DocumentError, EmbedFormat, Project};

impl Project {
    /// Export a browser preview with per-open consent, not a trusted standalone SVG.
    /// No project script runs until the user presses Run inside the exported page.
    /// See docs/isolated-preview.md for the boundary and residual browser risks.
    pub fn to_isolated_preview_html(&self, runtime: &str) -> Result<String, DocumentError> {
        self.to_isolated_preview_html_with_plugins(runtime, &[])
    }
    /// Same page, but the hash-pinned runtime plugins are embedded in the previewed SVG so they
    /// only run inside the consent-gated, opaque-origin sandbox (never in the wrapper page).
    pub fn to_isolated_preview_html_with_plugins(
        &self,
        runtime: &str,
        plugins: &[RuntimeBundle],
    ) -> Result<String, DocumentError> {
        let svg = self.to_svg_with_plugins(
            runtime,
            BrowserExportOptions {
                trusted_js: true,
                audio_controls: true,
            },
            EmbedFormat::Binary,
            plugins,
        )?;
        // Encode *every* byte: project strings cannot terminate the outer script,
        // introduce markup, or modify the trusted wrapper / its CSP.
        let mut url = String::from("data:image/svg+xml,");
        use std::fmt::Write;
        for byte in svg.bytes() {
            write!(&mut url, "%{byte:02X}").unwrap();
        }
        let mut notice = String::new();
        if !plugins.is_empty() {
            notice.push_str("<p>Runtime plugins embedded (run only after consent, inside the sandbox; ids, file names and hashes are restricted to safe characters):</p><ul>");
            for p in plugins {
                notice.push_str(&format!(
                    "<li>{} &mdash; {} &mdash; {}</li>",
                    p.id, p.entry, p.sha256
                ));
            }
            notice.push_str("</ul>");
        }
        Ok(TEMPLATE
            .replace("__PLUGIN_NOTICE__", &notice)
            .replace("__PREVIEW_URL__", &url))
    }
}

const TEMPLATE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' 'unsafe-eval'; style-src 'unsafe-inline'; img-src data: blob:; media-src data: blob:; connect-src 'none'; frame-src data:; child-src data:; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; manifest-src 'none'">
<meta name="referrer" content="no-referrer">
<title>Splinely isolated browser preview</title>
<style>body{font:16px system-ui;margin:1rem;background:#171b24;color:#eee}button{margin:.5rem}iframe{display:block;width:100%;height:75vh;border:1px solid #888;background:white}</style>
</head><body>
<h1>Isolated browser preview</h1>
<p>Untrusted project JavaScript. Browser sandbox; no native plugins. Common network requests, parent access, popups and top navigation are restricted. This is not an offline VM: browser vulnerabilities, resource exhaustion and some network channels (for example WebRTC) remain possible. Use a disposable offline browser for hostile code.</p>
<label><input id="consent" type="checkbox"> I choose to run this project's custom JavaScript in the sandbox.</label>
<button id="run" disabled>Run / restart preview</button><button id="stop">Stop preview</button>
<p id="status" role="status">Not running. No project code has executed.</p>
__PLUGIN_NOTICE__
<div id="preview"></div>
<script>
'use strict';
const consent=document.getElementById('consent'), run=document.getElementById('run');
const host=document.getElementById('preview'), status=document.getElementById('status');
function stop(){host.replaceChildren();status.textContent='Stopped. Running again requires consent.';consent.checked=false;run.disabled=true;}
consent.addEventListener('change',()=>{run.disabled=!consent.checked;if(!consent.checked)stop();});
document.getElementById('stop').addEventListener('click',stop);
// History/BFCache and browser form restoration must not restore consent.
window.addEventListener('pagehide',stop);
window.addEventListener('pageshow',stop);
run.addEventListener('click',()=>{
  if(!consent.checked)return;
  host.replaceChildren();
  const frame=document.createElement('iframe');
  // Never add allow-same-origin, navigation, popups, forms or downloads.
  frame.setAttribute('sandbox','allow-scripts');
  frame.setAttribute('referrerpolicy','no-referrer');
  frame.setAttribute('allow',"camera 'none'; microphone 'none'; geolocation 'none'; display-capture 'none'; usb 'none'; serial 'none'; hid 'none'; clipboard-read 'none'; clipboard-write 'none'");
  frame.title='Isolated project runtime';
  frame.src='__PREVIEW_URL__';
  host.appendChild(frame);
  status.textContent='Running in an opaque-origin sandbox. Stop removes the frame.';
});
// Deliberately no message listener, native bridge or project-controlled parent DOM.
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_strings_cannot_escape_wrapper() {
        let p = Project {
            custom_js: "</script><script>parent.pwned=true</script> ]]>&\"".into(),
            name: "</title>".into(),
            raw_svg: "<script>alert(1)</script>".into(),
            ..Project::default()
        };
        let html = p.to_isolated_preview_html("").unwrap();
        assert!(!html.contains("parent.pwned"));
        assert!(!html.contains("__PREVIEW_URL__"));
        assert_eq!(html.matches("<script>").count(), 1);
        assert!(html.contains("setAttribute('sandbox','allow-scripts')"));
        assert!(html.contains("connect-src 'none'"));
        assert!(html.contains("worker-src 'none'"));
        assert!(html.contains("addEventListener('pagehide',stop)"));
        assert!(html.contains("addEventListener('pageshow',stop)"));
        assert!(html.contains("if(!consent.checked)return"));
        assert!(html.contains("%6E%65%77%20%46%75%6E%63%74%69%6F%6E"));
    }
}
