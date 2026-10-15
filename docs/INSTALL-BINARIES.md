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

Einem LLM-Agenten mit Shell-Zugriff genügt der Satz:

> Installiere mir Splinely: https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

Die [`llms.txt`](../llms.txt) enthält die Installationsschritte und erklärt dem Agenten die Bedienung.
