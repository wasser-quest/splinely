# Splinely ohne Kompilieren installieren

Jedes [Release](https://github.com/wasser-quest/splinely/releases/latest) enthält
fertige Pakete (jeweils mit `.sha256`-Datei):

| Plattform | Paket | Status |
|---|---|---|
| Linux x86_64 | `splinely-vX.Y.Z-x86_64-linux.tar.gz` | gebaut und getestet (Ubuntu 22.04, glibc 2.35+; Ubuntu 22.04+, Debian 12+, Fedora 36+ usw.) |
| Windows x86_64 | `splinely-vX.Y.Z-x86_64-windows.zip` | **konfiguriert, nicht auf echter Hardware getestet** |
| macOS Apple Silicon | `splinely-vX.Y.Z-aarch64-macos.tar.gz` | **konfiguriert, nicht auf echter Hardware getestet** |
| macOS Intel | `splinely-vX.Y.Z-x86_64-macos.tar.gz` | **konfiguriert, nicht auf echter Hardware getestet** |

> **Ehrlicher Stand (Windows/macOS):** Der Code wurde von Linux aus für
> `x86_64-pc-windows-gnu`, `x86_64-pc-windows-msvc`, `aarch64-apple-darwin` und
> `x86_64-apple-darwin` per `cargo check`/`cargo clippy -D warnings` typgeprüft
> (alle fünf Crates inkl. Editor und Viewer, auch die Tests). Es wurde dort **nichts gelinkt, gestartet oder
> getestet**. Die Release-Jobs für diese Pakete (`.github/workflows/release.yml`) sind
> geschrieben, aber noch nie auf GitHub gelaufen. Fehler beim ersten echten Lauf sind
> möglich – bitte als [Issue](https://github.com/wasser-quest/splinely/issues) melden.

## Installieren (Linux)

```sh
VERSION=$(curl -fsSL https://api.github.com/repos/wasser-quest/splinely/releases/latest | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p')
NAME="splinely-${VERSION}-x86_64-linux"
curl -fLO "https://github.com/wasser-quest/splinely/releases/download/${VERSION}/${NAME}.tar.gz"
curl -fLO "https://github.com/wasser-quest/splinely/releases/download/${VERSION}/${NAME}.tar.gz.sha256"
sha256sum -c "${NAME}.tar.gz.sha256"
mkdir -p ~/.local/opt && tar -xzf "${NAME}.tar.gz" -C ~/.local/opt
export PATH="$HOME/.local/opt/${NAME}/bin:$PATH"   # dauerhaft: in ~/.bashrc eintragen
splinely-cli --version
```

## Installieren (Windows, ungetestet)

PowerShell:

```powershell
$v = (Invoke-RestMethod https://api.github.com/repos/wasser-quest/splinely/releases/latest).tag_name
$name = "splinely-$v-x86_64-windows"
Invoke-WebRequest "https://github.com/wasser-quest/splinely/releases/download/$v/$name.zip" -OutFile "$name.zip"
Invoke-WebRequest "https://github.com/wasser-quest/splinely/releases/download/$v/$name.zip.sha256" -OutFile "$name.zip.sha256"
# Prüfsumme vergleichen (Ausgabe: True)
(Get-FileHash "$name.zip" -Algorithm SHA256).Hash.ToLower() -eq (Get-Content "$name.zip.sha256").Split(' ')[0]
Expand-Archive "$name.zip" -DestinationPath $env:LOCALAPPDATA\Programs
& "$env:LOCALAPPDATA\Programs\$name\bin\splinely-cli.exe" --version
```

- Die `.exe`-Dateien sind **nicht signiert**; Windows SmartScreen kann beim ersten Start
  warnen („Weitere Informationen“ → „Trotzdem ausführen“); ein Zip aus dem Browser trägt
  zusätzlich die Internet-Markierung (ggf. Rechtsklick → Eigenschaften → „Zulassen“ vor dem Entpacken).
- `bin\splinely.exe` ist der Editor, `bin\splinely-viewer.exe` der Viewer. Der Viewer nutzt
  **WebView2** (Edge-Laufzeit); sie ist auf den meisten aktuellen Windows-Systemen
  vorhanden, sonst bei Microsoft nachinstallieren (ungeprüft: der Viewer wurde unter Windows nie gestartet).
- Einstellungen, Plugins und Autosave liegen unter `%APPDATA%\splinely` bzw.
  `%LOCALAPPDATA%\splinely`; `SPLINELY_PLUGIN_DIR` und `XDG_*`-Variablen überschreiben das wie unter Linux.
- Die C-Bibliothek ist `lib\splinely_runtime.dll` (+ Importbibliothek `splinely_runtime.dll.lib`) mit `include\splinely_runtime.h`.
- Die native Audiovorschau im Editor ruft `ffplay` auf; es muss im `PATH` liegen.

## Installieren (macOS, ungetestet)

```sh
VERSION=$(curl -fsSL https://api.github.com/repos/wasser-quest/splinely/releases/latest | sed -n 's/.*"tag_name": *"\(v[^"]*\)".*/\1/p')
ARCH=$(uname -m); [ "$ARCH" = arm64 ] && ARCH=aarch64     # aarch64 = Apple Silicon, x86_64 = Intel
NAME="splinely-${VERSION}-${ARCH}-macos"
curl -fLO "https://github.com/wasser-quest/splinely/releases/download/${VERSION}/${NAME}.tar.gz"
curl -fLO "https://github.com/wasser-quest/splinely/releases/download/${VERSION}/${NAME}.tar.gz.sha256"
shasum -a 256 -c "${NAME}.tar.gz.sha256"
mkdir -p ~/.local/opt && tar -xzf "${NAME}.tar.gz" -C ~/.local/opt
export PATH="$HOME/.local/opt/${NAME}/bin:$PATH"
splinely-cli --version
```

- **Gatekeeper:** Die Programme sind nur *ad-hoc* signiert (`codesign -s -`), **weder mit einer
  Developer ID signiert noch notarisiert**. Mit `curl` geladene Dateien tragen keine
  Quarantäne und starten normal. Wurde das Archiv im Browser geladen, blockiert macOS den
  ersten Start; entferne die Markierung mit
  `xattr -dr com.apple.quarantine ~/.local/opt/splinely-*` (oder Rechtsklick → Öffnen /
  Systemeinstellungen → Datenschutz & Sicherheit → „Trotzdem öffnen“).
- Es gibt kein `.app`-Bundle: `bin/splinely` (Editor) startet aus dem Terminal. Der Viewer
  nutzt WKWebView (Systemkomponente, nichts nachzuinstallieren).
- Einstellungen, Plugins und Autosave liegen unter `~/Library/Application Support/splinely`.
- Die C-Bibliothek ist `lib/libsplinely_runtime.dylib`.
- Ein universelles Paket erzeugt man selbst mit `SPLINELY_UNIVERSAL=1 scripts/package-release.sh`
  (lipo; wird nicht als Release-Asset veröffentlicht).

## Inhalt (Linux; Windows/macOS siehe oben: `.exe` bzw. `.dylib`, Viewer ohne GTK)

| Datei | Zweck | Laufzeit-Abhängigkeiten |
|---|---|---|
| `bin/splinely` | Desktop-Editor | nur glibc, Grafik-/Fensterumgebung |
| `bin/splinely-cli` | Kommandozeile zum Erstellen, Prüfen und Exportieren (auch für LLMs) | nur glibc |
| `bin/splinely-viewer` | Natives Abspielfenster für `.sply`/SVG | GTK 3, WebKitGTK 4.1 |
| `bin/splinely-runtime` | Native Runtime als JSON-Zeilenprozess | nur glibc |
| `lib/libsplinely_runtime.so` + `include/splinely_runtime.h` | Native Runtime als C-Bibliothek | nur glibc |
| `LLM.md`, `CLI.md`, `prompts/` | Anleitungen für Menschen und LLMs | – |
| `examples/` | Beispielprojekte (`.sply`) und exportierte SVGs | – |

Unter Linux braucht nur der Viewer Systembibliotheken:

```sh
# Ubuntu/Debian
sudo apt install libgtk-3-0 libwebkit2gtk-4.1-0
# Fedora
sudo dnf install gtk3 webkit2gtk4.1
```

Exportierte SVGs laufen ohne Installation in jedem Browser.

## Anleitung für ein LLM

Einem LLM-Agenten mit Shell-Zugriff genügt der Satz:

> Installiere mir Splinely: https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

Die [`llms.txt`](../llms.txt) enthält die Installationsschritte und erklärt dem Agenten die Bedienung.
