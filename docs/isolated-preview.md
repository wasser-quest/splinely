# Isolated browser preview for untrusted projects

This is a **browser-only** way to try a `.sply` project's applied `customJs` without granting it the editor, native plugin host, or parent page's capabilities. It is not the native Viewer and is not a sandbox for native code.

## Workflow

- Editor: click **Export isolated preview…** beside the Viewer button. Save the HTML and open it in an updated external browser. Apply JavaScript edits first: unapplied code-panel drafts are not included. This is a snapshot; export again after changes.
- CLI: `splinely-cli isolated-preview project.sply --output preview.html`. Open `preview.html` in an external browser. `--output -` returns JSON containing `format: "html"` and `html` (not raw HTML).
- Nothing from the project runs when the page opens. Read the warning, check the consent box, and press **Run / restart preview**. The SVG runtime, interactions, and custom JS then run inside the frame. Embedded audio still needs its start control / browser audio gesture.
- **Stop preview** destroys the frame and clears consent. Unchecking consent also stops it. Reloading and Back/Forward restoration require consent again. Restart discards runtime state. Never transfer the ordinary export's `--trusted-js` / editor trusted-export setting into this flow; they are independent.
- Runtime plugins: `splinely-cli isolated-preview project.sply --output preview.html --plugin PLUGIN_DIR` (repeatable; folder with `runtime.entry` or a `.js` file) embeds the plugin's runtime file (see [plugins.md](plugins.md)) **inside** the sandboxed, percent-encoded data document, so it runs only after the consent checkbox, under the same opaque origin and CSP. The page lists the plugin ids and SHA-256 hashes. The consent/sandbox is the trust gate here, so `--trusted-js` is not needed. Plugin code is still arbitrary JavaScript with the limits listed below; the editor's "Export isolated preview…" button does not embed plugins.
- The editor only writes the HTML. It does not execute custom JS, launch native plugins, or open the HTML in its native Viewer. The existing native Viewer and trusted standalone SVG exports remain **not sandboxes**.

## Security boundary

`Project::to_isolated_preview_html` creates a trusted static wrapper around a percent-encoded, generated SVG data document. Every SVG byte is encoded, so project strings (including `</script>`, CDATA terminators, and quotes) cannot escape into the wrapper HTML or alter its policy. Preserved `rawSvg` is project data, not imported active SVG markup. Arbitrary standalone SVG input is not supported here.

The wrapper creates no frame until consent. It sets `sandbox="allow-scripts"` **before** setting the source. There is no `allow-same-origin`: the project receives an opaque origin, not the wrapper's file/HTTP origin. Forms, popups, downloads, parent/top navigation and privileged storage are not granted. Device/clipboard permissions are additionally denied. There is no native bridge, `postMessage` command handler, or privileged host API exposed to the frame. The `splinely` object is the browser runtime, not an editor handle.

A CSP meta element precedes all executable content. Data-document policy inheritance restricts the child too:

- `default-src 'none'`, `connect-src 'none'`, `worker-src 'none'`, `object-src 'none'`, `base-uri 'none'`, `form-action 'none'`, `manifest-src 'none'`.
- Only inline script and eval are enabled. Eval is required by the existing browser custom-JS bootstrap; it does **not** give the opaque frame parent privileges. Inline styles are enabled for SVG rendering.
- Images and media allow only `data:` / `blob:` resources; HTTP(S) and file loads are not allowed. External fonts, scripts and remote audio are unavailable.
- `frame-src data:` / `child-src data:` permit the generated document, not external frame navigation. The sandbox also applies to descendants; child content cannot grant itself additional sandbox permissions.

Do not add `allow-same-origin`, popups, navigation or download allowances. Do not embed the output in a privileged native webview. Do not accept opaque-origin messages merely because their origin is `"null"`. Hosting behind additional CSP headers may prevent execution; do not loosen this policy to fix compatibility.

## Limits (not an offline execution guarantee)

- CSP restricts common network channels; it is **not a complete network firewall**. Browser-specific WebRTC/STUN/DNS and other channels may not be governed by `connect-src`. For actively malicious code or confidential projects use a disposable browser/profile inside an OS/container/VM with network disabled. Do not promise zero network egress from this HTML alone.
- Browser vulnerabilities, extensions, browser-version differences and user/browser permission decisions are outside this boundary. Use an updated browser. Current automated real-browser coverage is Chromium, not every browser engine or native WebKit.
- Scripts can hang the renderer, allocate memory, consume CPU, play embedded audio after gestures, or present misleading content **inside the frame**. Stop may be unresponsive if the renderer is hung; close the tab/process. No execution timeout, memory quota or process isolation guarantee is provided.
- Anyone receiving the HTML can read its embedded project, including custom JS and assets. This is isolation, not confidentiality or code signing. Consent authorizes that snapshot only.
- Native C/C++/Rust plugins, plugin compilation and native extension execution are explicitly **not supported or sandboxed** by this feature.

## Focused verification

```
cargo test -p splinely_document isolated_preview --lib
cargo test -p splinely-cli --test isolated_preview
cargo check -p splinely_editor
# Install Playwright and its Chromium, or point at an existing Playwright module:
PLAYWRIGHT_MODULE=/path/to/node_modules/playwright node scripts/isolated-preview-smoke.cjs
```

The unit test exercises hostile wrapper-escape strings and consent/policy generation. The CLI test exercises file and JSON stdout export. The real Chromium test executes malicious custom JS: parent DOM access, top navigation and local storage are denied; fetch rejects; beacon, image, WebSocket, worker, form and frame self-navigation probes generate no requests to a local HTTP server; popups are denied and forged messages have no host effect. It also verifies no frame before consent, runtime execution after consent, and stop, reload-while-running, and history-restoration reset. It does not assert a universal network firewall, audibility, denial-of-service resistance, native plugin isolation or cross-engine parity.
