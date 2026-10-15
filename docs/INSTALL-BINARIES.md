# Splinely ohne Kompilieren installieren

Jedes [Release](https://github.com/wasser-quest/splinely/releases/latest) enthält
ein fertiges Paket für Linux x86_64: `splinely-vX.Y.Z-x86_64-linux.tar.gz`.
Gebaut wird es auf Ubuntu 22.04 und läuft auf Linux-Systemen mit glibc 2.35 oder
neuer (Ubuntu 22.04+, Debian 12+, Fedora 36+ usw.).

## Installieren

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

## Inhalt

| Datei | Zweck | Laufzeit-Abhängigkeiten |
|---|---|---|
| `bin/splinely` | Desktop-Editor | nur glibc, Grafik-/Fensterumgebung |
| `bin/splinely-cli` | Kommandozeile zum Erstellen, Prüfen und Exportieren (auch für LLMs) | nur glibc |
| `bin/splinely-viewer` | Natives Abspielfenster für `.sply`/SVG | GTK 3, WebKitGTK 4.1 |
| `bin/splinely-runtime` | Native Runtime als JSON-Zeilenprozess | nur glibc |
| `lib/libsplinely_runtime.so` + `include/splinely_runtime.h` | Native Runtime als C-Bibliothek | nur glibc |
| `LLM.md`, `CLI.md`, `prompts/` | Anleitungen für Menschen und LLMs | – |
| `examples/` | Beispielprojekte (`.sply`) und exportierte SVGs | – |

Nur der Viewer braucht Systembibliotheken:

```sh
# Ubuntu/Debian
sudo apt install libgtk-3-0 libwebkit2gtk-4.1-0
# Fedora
sudo dnf install gtk3 webkit2gtk4.1
```

Exportierte SVGs laufen ohne Installation in jedem Browser.

## Anleitung für ein LLM

Diesen Text einem LLM-Agenten mit Shell-Zugriff geben (z. B. Claude Code), damit
er sich Splinely selbst holt und benutzt:

```text
Installiere das SVG-Animationswerkzeug Splinely aus den fertigen GitHub-Releases,
ohne es aus dem Quellcode zu bauen:

1. Ermittle die neueste Version:
   curl -fsSL https://api.github.com/repos/wasser-quest/splinely/releases/latest
   (Feld "tag_name", z. B. v0.3.0). Nutze das Asset
   splinely-<tag>-x86_64-linux.tar.gz und die zugehörige .sha256-Datei.
2. Lade beide herunter, prüfe mit `sha256sum -c` und entpacke nach ~/.local/opt.
   Führe nichts aus, wenn die Prüfsumme nicht stimmt.
3. Füge ~/.local/opt/splinely-<tag>-x86_64-linux/bin zum PATH hinzu und prüfe mit
   `splinely-cli --version`.
4. Lies vor der ersten Benutzung `splinely-cli guide` sowie LLM.md und
   prompts/splinely-author.md aus dem entpackten Ordner. Arbeite danach nur mit
   `splinely-cli` (z. B. `new`, `node add`, `keyframe`, `check`, `render`, `export`).
5. Prüfe jedes Projekt mit `splinely-cli check`, sieh dir eine Vorschau mit
   `splinely-cli render` an und exportiere am Ende mit `splinely-cli export`.
   Nutze `--trusted-js` nur, wenn ich eigenen JavaScript-Code ausdrücklich freigebe.

Systempakete installierst du nicht ohne Rückfrage. Nur `splinely-viewer` braucht
GTK/WebKit; CLI und Editor brauchen keine zusätzlichen Bibliotheken.
```
