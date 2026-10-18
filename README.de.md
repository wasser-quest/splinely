# Splinely

**Bewegungspfade:** optionale Pfadbindung, animierbarer Fortschritt 0–1 und Tangentenausrichtung im Inspektor, in der CLI und beiden Runtimes. [Anleitung und Grenzen](docs/motion-paths.md).

[English](README.md) · **Deutsch** · [Español](README.es.md) · [Français](README.fr.md) · [日本語](README.ja.md) · [简体中文](README.zh.md)

![Splinely-Logo](assets/splinely-logo.svg)

Splinely ist ein Vektor-Animationseditor in Rust. Projekte stehen in **`.sply`-Dateien**: Formen und Pfade, Keyframes, Knochen, Zustandsmaschinen, Mausverfolgung und Audio – alles in einer Datei. Die **Splinely-Runtime** spielt `.sply`-Projekte mit ihrem ganzen Verhalten ab: im Browser, nativ (Rust/C) oder im Splinely Viewer. **Aktueller Stand: funktionierender Prototyp, nicht der vollständige Funktionsumfang aus [PLAN.md](PLAN.md).**

Der Editor spricht **Deutsch, Englisch, Spanisch, Französisch, Japanisch und vereinfachtes Chinesisch**. Er folgt der Systemsprache; umstellen unter **Einstellungen › Allgemein › Sprache**. Japanisch und Chinesisch brauchen eine installierte CJK-Schrift (z. B. Noto Sans CJK).

## So funktioniert Splinely

- **`.sply` ist die Quelle.** Darin steht, wie etwas aussieht *und* wie es sich verhält: Clips, Zustandsmaschinen, Übergänge mit Überblendung und Timern, Mausverfolgung, Audio und Variablen.
- **Die Splinely-Runtime erweckt es zum Leben.** Eine reine SVG-Datei ist nur ein Bild: Sie kennt keine Zustandsmaschinen, Timer oder Mausverfolgung. Diese Logik steckt in der Runtime – als Browser-Runtime (JavaScript), als native Runtime (`splinely_runtime`, Rust/C-ABI) und im Desktop-**Splinely Viewer**.
- **Exporte:** *Statisches SVG* ist ein Standbild eines Zeitpunkts. Der *Web-Player* (`splinely-cli export`, „Interaktives SVG exportieren“) ist eine eigenständige Datei, die das `.sply`-Projekt zusammen mit der Browser-Runtime enthält und so in jedem Browser ohne Server läuft.

### Viele Interaktionen gleichzeitig

Mehrere Zustandsmaschinen laufen nebeneinander, und jede **überblendet** zwischen ihren Zuständen (`blend` in Sekunden) – Hover, Klick und Stimmung überlagern sich weich. **Timer-Übergänge** (`after`) schalten von selbst (Blinzeln, Rückkehr in Ruhe), **Exit-Aktionen** laufen beim Verlassen eines Zustands, und mit **Mausverfolgung** folgen Formen oder Knochen dem Zeiger (Pupillen, Kopf, Arm) – zusätzlich zu allem anderen. Die Runtime beachtet die Systemeinstellung *Bewegung reduzieren*. Beispiel: das [neugierige Gesicht](examples/curious-face.svg) – Pupillen folgen der Maus, Hover lässt es sich vorbeugen, ein Klick lächelt, ein Timer blinzelt, ganz ohne eigenen Code.

```sh
splinely-cli on face.sply --event click --target face --machine mood --to happy --state face.fill=gold --blend 0.4
splinely-cli on face.sply --machine blink --from open --to shut --after 3 --state eye.scaleY=0.1 --blend 0.08
splinely-cli follow face.sply --target pupil --max 8 --strength 0.15 --smoothing 0.1
```

## Mit einem LLM installieren

Einem KI-Agenten mit Shell-Zugriff (z. B. Claude Code) einfach sagen:

> Installiere mir Splinely: https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

Die [`llms.txt`](llms.txt) beschreibt, wie der Agent das passende fertige Paket aus den Releases lädt, die Prüfsumme kontrolliert, es nach `~/.local/opt` entpackt und danach mit `splinely-cli` arbeitet.

## Start

**Ohne Kompilieren:** Fertige Linux-Pakete (`splinely-vX.Y.Z-x86_64-linux.tar.gz` mit Editor, CLI, Viewer, Runtime und `libsplinely_runtime.so`) liegen bei jedem [Release](https://github.com/wasser-quest/splinely/releases/latest). Installation: [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md).

| Plattform | Paket | Status |
|---|---|---|
| Linux x86_64 | `splinely-vX.Y.Z-x86_64-linux.tar.gz` | gebaut und getestet |
| Windows x86_64 | `splinely-vX.Y.Z-x86_64-windows.zip` | konfiguriert, **nicht auf echter Hardware getestet** |
| macOS Apple Silicon / Intel | `splinely-vX.Y.Z-{aarch64,x86_64}-macos.tar.gz` | konfiguriert, **nicht auf echter Hardware getestet** |

Ehrlicher Stand: Die Windows- und macOS-Pakete sind konfiguriert (Release-Workflow, Paketskripte), und der Code besteht von Linux aus die Typprüfung für alle vier Windows-/macOS-Ziele; gelinkt, gestartet oder getestet wurde unter echtem Windows/macOS noch nichts. Die Programme sind unsigniert (Windows) bzw. nur ad-hoc signiert und nicht notarisiert (macOS); siehe [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md).

Aus dem Quellcode:

Voraussetzungen: Rust/Cargo; für das Desktopfenster eine funktionierende Grafik-/Fensterumgebung. Beim ersten Build lädt Cargo Abhängigkeiten herunter. Node.js und Python 3 werden nur für die Prüfskripte (`scripts/check.sh`) benötigt.

Unter Linux braucht der Build von Editor und Viewer die GTK-/WebKit-, ALSA- und D-Bus-Entwicklerpakete:

```sh
# Fedora
sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel alsa-lib-devel dbus-devel libxkbcommon-devel
# Debian/Ubuntu
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config
```

```sh
git clone https://github.com/wasser-quest/splinely.git
cd splinely
cargo run -p splinely_editor --bin splinely
# Export ohne Desktopfenster:
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/splinely-demo.svg
```

Der Editor startet mit einer leeren Zeichenfläche. Eine **`.sply`-Datei hereinziehen**, unter **Datei → Öffnen …** (Strg+O, System-Dateidialog) bzw. **Zuletzt geöffnet** wählen oder direkt starten: `cargo run -p splinely_editor --bin splinely -- examples/demo.sply`. Ungespeicherte Änderungen und Zeichen-/Quelltextentwürfe werden vor einem Wechsel und beim Schließen des Fensters abgefragt; der Fenstertitel zeigt Datei und `•` für ungespeichert. **Speichern** bewahrt das bearbeitbare Projekt; SVG ist nur ein optionaler Export.

**Gestalten / Animieren / Erweitert** trennen Zeichnen, Animation und Code. Auswahl, Pen, Pfadpunkte und Freihand sind exklusive Werkzeuge; zusätzliche Optionen bleiben zugeklappt. **Vorschau**, **Abspielen** und **Splinely Viewer öffnen** sind direkt erreichbar. [Bedienung, Gesten und Grenzen](docs/usability.md).

Eigenes JavaScript läuft nur nach ausdrücklicher, sitzungsbezogener Freigabe unter **Erweitert → JavaScript**, nie in der nativen Zeichenfläche. Unter **Animieren → Audio** lässt sich eine begrenzte `ffplay`-Vorschau globaler Audiocues aktivieren; Vorbereitung erfolgt im Hintergrund.

## Eine Projektdatei: `.sply`

Geometrie, Ebenen, Bones, Keyframes, States, Blend-Gewichte, Variablen, eingebettetes Audio und eigener Code bleiben **zusammen in einer Datei**. Kein zusätzliches JSON/SVG oder Assetverzeichnis nötig. Standardmäßig wird `.sply` kompakt **binär** gespeichert (verlustfrei, streng validiert, CRC-geprüft); `.json`/`.splinely.json` bleiben lesbares JSON, und beim Öffnen wird das Format automatisch erkannt (auch ältere JSON-`.sply`). `splinely-cli cat`/`convert` liefern bzw. schreiben JSON. Die Anwendung/Runtime und installierte Editorplugins sind nicht Teil des Projektfiles. [Format und Kompatibilität](docs/sply-format.md).

Unter **Animieren** führen **Animation einfach erstellen** (Keyframe/Preset) und **Zustandsmaschine einfach erstellen** (Klick/Hover, Sichtbarkeit, Clipstart) durch die ersten Schritte. Gewichtete Mischung und zeitliche Fades bleiben neben den Expertentools erreichbar. [Schritt-für-Schritt und Blend-Grenzen](docs/animation-workflow.md).

```sh
splinely-cli new --demo --output scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-viewer scene.sply
# Nur für geprüften eigenen Code:
splinely-viewer examples/desert-stickfight.sply --trusted-js
```

## Splinely Viewer

`cargo build -p splinely_viewer` baut die echte native SVG-WebView. Im Editor öffnet **▶ Splinely Viewer** einen Snapshot des aktuellen Projekts, inklusive Runtime, States und eingebettetem Audio; die stille resvg-Zeichenfläche bleibt getrennt. Eigenes JS nur nach Freigabe in Advanced. Direkt ohne vorherigen SVG-Export: `cargo run -p splinely_viewer --bin splinely-viewer -- examples/brown-owl.sply --trusted-js` (mitgelieferten Code vorher prüfen). Bestehende SVGs bleiben ebenfalls unterstützt. Aktive fremde SVGs sind nicht vertrauenswürdig. Linux benötigt GTK3/WebKitGTK; [Details und Tests](crates/splinely_viewer/README.md).

SVG-Exporte enthalten **Created with Splinely**, Anwendungs-/Schemaversion und maschinenlesbare Generator-Metadaten (`urn:splinely:metadata`). Kein sichtbares Wasserzeichen; die Kennzeichnung ist editierbar, kein Echtheitsnachweis.

## Runtime verständlich bedienen

Wo die Runtime liegt und wie man sie einbindet (SVG, HTML, Node, nativ): [runtime/README.md](runtime/README.md#where-the-runtime-is-and-how-to-include-it). [Runtime-Kochbuch](docs/runtime-cookbook.md): Play/Pause/Neustart, Clip- und globale Uhr, HTML-Einbettung, Audio und Fehlersuche. `splinely.snapshot()` zeigt Laufzeitstatus als kopierte JSON-Daten; `splinely.reportError(error)` nutzt den einheitlichen Error-Kanal mit begrenzter Historie.

## Browserunabhängige Runtime

`splinely_runtime` stellt den Splinely-Dokumentkern als native Rust-Runtime, C-ABI (`cdylib`) und `splinely-runtime`-JSON-Zeilenprozess bereit. Andere Sprachen können Events senden, Zeit/Clip-Gewichte steuern und ausgewertete Nodes/Bones abrufen. Zeichnen, Eingabetreffer und Audio übernimmt der Host — kein Browser, aber auch kein allgemeiner SVG-JavaScript-Interpreter. [API und Python-Beispiele](docs/native-runtime.md).

```sh
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py --library target/debug/libsplinely_runtime.so --binary target/debug/splinely-runtime
```

## CLI-Workflow

CLI und Editor verwenden denselben Dokumentkern. CLI-Ergebnisse sind JSON; Fehler stehen als JSON auf stderr (mit `hint` für den nächsten Schritt) und liefern einen Fehlerstatus. Kein KI-Anbieter, API-Key oder Server ist erforderlich.

**Einstieg:** `splinely-cli guide` zeigt den kurzen Arbeitsablauf mit getesteten Rezepten. Bearbeitungsbefehle speichern ohne `--output` direkt in die Eingabedatei, `check` findet typische Fehler (verdeckte Formen, Clips, die nie abspielen, unlesbarer Text …) samt ausführbarem Fix, `batch` führt viele Schritte ganz oder gar nicht aus, `clip` verwaltet Clips und `on` macht Formen interaktiv.

```sh
F=/tmp/szene.sply
cargo run -p splinely-cli -- new --output $F --width 800 --height 450 --background "#0b1d3a"
cargo run -p splinely-cli -- node add $F --kind circle --id sonne --x 400 --y 420 --radius 60 --fill orange --pivot center
cargo run -p splinely-cli -- keyframe $F --clip aufgang --target sonne --property y --time 3 --by -260 --easing ease-out --autoplay --loop
cargo run -p splinely-cli -- on $F --event click --target sonne --set sonne.fill=gold
cargo run -p splinely-cli -- check $F
cargo run -p splinely-cli -- render $F --time 1.5 --output /tmp/szene.png
```

Mit JSON und Merge-Patch:

```sh
cargo run -p splinely-cli -- new --demo --output /tmp/draft.splinely.json
cargo run -p splinely-cli -- validate /tmp/draft.splinely.json
cargo run -p splinely-cli -- inspect /tmp/draft.splinely.json
cargo run -p splinely-cli -- evaluate /tmp/draft.splinely.json --time 0.5
printf '%s\n' '{"name":"Meine Animation"}' > /tmp/splinely-patch.json
cargo run -p splinely-cli -- apply /tmp/draft.splinely.json --patch /tmp/splinely-patch.json --output /tmp/final.splinely.json
cargo run -p splinely-cli -- export /tmp/final.splinely.json --output /tmp/final.svg
```

`schema` liefert einen Formatleitfaden mit vollständigen Beispielen, kein formales JSON Schema. `apply` nutzt RFC-7396-Merge-Patch: Arrays werden vollständig ersetzt. `evaluate` führt keine Zustandsaktionen, JS oder Audio aus; nicht automatisch aktive Clips werden dabei nicht abgespielt. Details: [CLI.md](CLI.md), [CORE_API.md](CORE_API.md). Für andere LLMs: [Authoring-Anleitung](LLM.md) und [kopierbarer Prompt](prompts/splinely-author.md); `schema` und `capabilities` liefern die tatsächlich unterstützten Features.

## Durchgängiges Offline-Beispiel

- [examples/demo.splinely.json](examples/demo.splinely.json): editierbares Projekt mit Kreis, dreisekündiger Keyframe-Bewegung, Klick-Zustandswechsel und eingebettetem WAV-Cue bei **1,5 Sekunden globaler Transportzeit**.
- [examples/demo.svg](examples/demo.svg): tatsächlicher CLI-Export mit explizit freigegebenem, kurzem `customJs`-Handler. Ein Kreis-Klick entsperrt Audio und startet die Bewegung samt Transport neu; beim ersten Klick wird der Kreis orange.
- Der Ton ist **0,4 Sekunden synthetischer, silbenartiger Klang, keine gesprochenen Worte**. PCM-WAV, mono, 8 kHz, 16 Bit; keine externen Audiodateien und keine Sprachsynthese.

```sh
# Reproduzierbar: CLI new --demo → apply → validate → export --trusted-js
bash scripts/generate-demo.sh
# Standardexport: Code bleibt nur gespeichert; Start-Control entsperrt Audio ohne Nutzercode.
cargo run -p splinely-cli -- export examples/demo.splinely.json --output /tmp/demo-safe.svg
# Nur nach Prüfung des eigenen Codes:
cargo run -p splinely-cli -- export examples/demo.splinely.json --trusted-js --output /tmp/demo-trusted.svg
```

`demo.svg` direkt im Browser als `file://` öffnen, **Start audio / animation** und anschließend den Kreis klicken. Nicht als `<img>` einbinden: dort werden SVG-Skripte nicht ausgeführt. Ohne Skriptausführung bleibt die statische Ausgangsgrafik sichtbar. Browser/CSP können Skripte oder dynamische Codekompilierung blockieren; Audio braucht eine echte Nutzerinteraktion und einen unterstützten Decoder.

Die mitgelieferte Runtime und die Beispieldatei brauchen keine nachgeladenen Bibliotheken oder Netzwerkressourcen. Das ist **keine Sandbox**: beliebiger freigegebener Nutzercode kann Netzwerkaufrufe und andere Browseraktionen ausführen. Standardmäßig bleibt `customJs` erhalten, wird aber nicht ausgeführt; `rawSvg` bleibt nur Metadaten. Unbekannte Projekte nicht mit `--trusted-js` exportieren/öffnen.

## Vektoren, Ebenen und Bones

- **Layers:** Auswahl, Reihenfolge, Sichtbarkeit und Bearbeitungssperre. Farben und Transparenz stehen im Inspektor.
- **Pfade:** Doppelklick auf einen Pfadrand fügt einen formtreuen Anker ein; Shift koppelt Bézier-Tangenten symmetrisch, Shift+Alt erhält die Nachbarlänge. [Details](docs/path-editing.md). SVG-Pfadsyntax sowie grafische Anker/Bézier-Kontrollpunkte bearbeiten; **Pen** zeichnet per Klick und Drag (Enter fertig, Escape abbrechen). **Snap 10** rastet Objektpositionen beim Loslassen lokal ein. Rechtecke/Kreise in Pfade umwandeln und unverändert zurückwandeln. SVG-Import ist bewusst begrenzt und verweigert nicht unterstützte Darstellung statt sie stillschweigend zu verlieren.
- **Bones:** Gelenke auswählen/verschieben, Elternbeziehungen und Bindungen bearbeiten, Rotation keyframen. Starre Bindung und begrenztes Zwei-Bone-IK für direkte, versatzfreie Kindgelenke; keine Mesh-Deformation. `examples/rig.splinely.json` laden; `examples/rig.svg` zeigt dieselbe Animation im Browser.
- **Navigation:** F/Home passt die Szene ein; +/- zoomt, 0 setzt 100 %. Mitteltaste oder Leertaste + linke Maustaste verschiebt die Ansicht, ohne Formen zu verändern; Space ohne Ziehen startet/stoppt beim Loslassen. Lineale sind ein-/ausblendbar und bleiben beim Pan/Zoom im Dokumentkoordinatensystem. Die Timeline lässt sich ausblenden. **Interact** trennt Szenenereignisse vom Bearbeiten.
- **Freihand/Tablet:** Freihand-Modus erzeugt editierbare SVG-Pfade, ein Strich pro Undo-Schritt. Druckabhängige Konturen nur bei verfügbaren Force-Events; keine pauschale Hardware-/Treiberzusage. [Tablet-Hinweise](docs/tablet.md).
- **Drop-in-Plugins:** `splinely-plugin.json` in den Pluginordner legen und neu starten — deklarative Formwerkzeuge und Paletten werden automatisch geladen, ohne Kompilieren. [Format, Ordner und Beispiele](docs/drop-in-plugins.md); keine automatische Ausführung fremder Skripte/Bibliotheken.
- **Plugins:** statische Rust-ECS-Erweiterungen mit eigenen Ressourcen und einer Decorate-Phase. [Vorlage](plugin-templates/editor/README.md); `splinely-cli plugin` erzeugt Quelltext, führt ihn aber niemals aus. Native Plugins sind vertrauenswürdiger Code, keine Sandbox. → [docs/plugins.md](docs/plugins.md) (hover-wiggle, `plugin list|validate|apply`, `export --plugin … --trusted-js`).

```sh
cargo run -p splinely-cli -- import-svg drawing.svg --output drawing.splinely.json
cargo run -p splinely-cli -- rig-demo --output /tmp/rig.splinely.json
cargo run -p splinely-cli -- export /tmp/rig.splinely.json --output /tmp/rig.svg
```

Formen per CLI anlegen, anordnen und als PNG prüfen (gleiche Logik wie im Editor):

```sh
cargo run -p splinely-cli -- node add scene.sply --kind circle --x 200 --y 150 --radius 40 --fill tomato --output scene.sply
cargo run -p splinely-cli -- arrange align scene.sply --nodes node-1,node-2 --edge top --output scene.sply
cargo run -p splinely-cli -- render scene.sply --output scene.png --width 800
```

Weitere Form-, Anordnungs-, Bone-, Ebenen- und Konvertierungsbefehle: [CLI.md](CLI.md). `keyframe` fügt einzelne Node-/Bone-Keys ohne komplette Array-Patches hinzu.

## Implementiert und noch offen

| Jetzt implementiert | Nicht implementiert / zurückgestellt |
| --- | --- |
| Rechteck, Kreis, Text, Pfade, Polygon/Stern/Linie; Mehrfach- und Rechteckauswahl, Skalier-/Drehgriffe, intelligente und manuelle Hilfslinien, Kontextmenü, Befehlspalette (Strg+K), Einstellungen (Strg+,) mit frei belegbaren Tastenkürzeln, hellem/dunklem Erscheinungsbild und sichtbarem Raster, Aufnahme-Modus (Auto-Key), Verlaufsliste, Maßanzeige am Zeiger, visuelle Zeitleiste mit ziehbaren Schlüsselbildern und Easing-Menü, einstellbare Zwiebelhaut (Schlüsselbilder oder Intervall, mehrere Geister), Text direkt auf der Zeichenfläche, Dokumentfarben, Sprache der Oberfläche, Formatvorlagen für neue Projekte, PNG-/GIF-Export, Pipette, lineare/radiale Verläufe, verschachtelte Gruppen (Baumansicht, `<g>`-Export), Text in Pfad, Ausrichten/Verteilen/Spiegeln, kurvenerhaltende boolesche Operationen, Pfad-Morphing, animierte Verläufe, Ebenenreihenfolge, Kopieren/Einfügen, Farben/Transparenz, Ebenen, Konvertierung, Undo/Redo | Morphing zwischen Formen mit unterschiedlicher Topologie wird angenähert (wachsende/schrumpfende Teilpfade), kein Löcher-genaues Zuordnen |
| Projekt laden/speichern, temporäre Datei + Umbenennung; Editor-Autosave für geänderte Projekte alle 30 s mit Wiederherstellungsdialog | Historische Schemamigrationen, vollständige Ressourcenquoten |
| Timeline, Keyframes, Easing, unabhängige Clip-Uhren, parallele States, gewichtete Clips und zeitliche Crossfades (`blendClip`) | Additives Blending, visueller Blend-Tree/Blend-Space |
| Zustandsmodell, Guards/Aktionen, Exit-Aktionen, Timer-Übergänge, Überblendung zwischen Zuständen (mehrere Maschinen gleichzeitig), Mausverfolgung, editierbarer Zustandsgraph, JSON-Bearbeitung und native Vorschau mit Hover | Freie Drag-Verbindungen, automatisches Graph-Layout, Bewegung entlang von Pfaden |
| Browser-Events, Clip-/Transport-API, eingebettete Audioassets/Cues | Native JS-Ausführung oder garantierte Audio-Latenz; native Audiovorschau nur globale Cues via ffplay |
| Statischer/interaktiver SVG-Export, begrenzter SVG-Vektorimport, JS-/Raw-SVG-Quelltextfelder | **Kein vollständiger Importer für beliebige SVGs**; Text, Verläufe und SVG-Data-URI-Bilder werden mit Bericht über Angenähertes/Verworfenes importiert; Rasterbilder werden geprüft, aber nicht importiert; Clips/Masken/Filter/externe Dateien werden abgelehnt; Raw-SVG bleibt Metadaten, mit Konfliktwarnung bei Abweichung ([Details](docs/svg-import.md)) |
| Bevy-ECS-Projektion, registrierbare statische Plugins, Bones und starre Bindungen | Dynamische native Plugins, Mesh-Skinning, allgemeine IK-Ketten, automatische Übersetzung beliebiger Bevy-Systeme in Browsercode, isolierte untrusted Vorschau |

Die native Vorschau rendert SVG mit resvg: Pfade, Rundungen und Transformationen sind sichtbar. Text nutzt lokale Systemfonts und kann browserabhängig abweichen. Formen werden konturgenau ausgewählt (Füllregel bzw. Randabstand); unsichtbare Formen (Deckkraft 0) sind nicht anklickbar. Schema 1 wurde erweitert; alte Projekte laden mit Standardwerten für neue Felder. Ältere Splinely-Binaries verstehen die neuen Felder nicht. Die `.sply`-Projektdatei (standardmäßig binär, verlustfrei nach JSON konvertierbar) bleibt maßgeblich; der SVG-Export ist kein verlustfreier Import-Roundtrip.

## Prüfen

```sh
bash scripts/check.sh
```

Das Skript führt Workspace-Tests, Node-Runtime-Tests, echte CLI-Demoerzeugung/-auswertung/-export und einen Python-XML-/Metadaten-/WAV-/Ressourcencheck aus. Es prüft auch die ausgelieferten Beispieldateien. **Kein Ersatz für einen echten Browser-/Audiotest.** Zusätzlich: `python3 scripts/svg-corpus.py` prüft installierte reale SVGs, `python3 scripts/browser-smoke.py examples/demo.svg /tmp/splinely-svg-corpus/manifest.json` prüft Firefox einschließlich Corpus-Pixelvergleich. Tests umfassen striktes JSON, Pfade, Ebenen, Bones, Plugin-Lebenszyklus, Runtime-Sicherheitsregressionen und jede Oberflächensprache; XML und eingebetteter WAV werden geprüft. Firefox 151.0.1: Offline-Runtime-Smoke **PASS** und echter CLI-Export mit WebDriver-Klick, Audio-Unlock/Decode und Zustandswechsel geprüft. Hörbare Hardwareausgabe und Audio-Latenz bleiben ungeprüft. Reproduktion und manuelle Abnahme: [docs/browser-tests.md](docs/browser-tests.md). Native GUI-Screenshots sowie GDB/REA-Artefaktprüfung: [docs/verification.md](docs/verification.md).

## Animierte Showcases

[Showcase-Übersicht](examples/showcase.html) · [Interaktive SVGs, direkte Events und unsichtbare States](docs/interactive-svg.md)

- **[Sand / Accord](examples/desert-stickfight.html)** ([editierbare `.sply`](examples/desert-stickfight.sply)):  originaler 20-Sekunden-Kurzfilm mit zwei Bone-Figuren im warmen Cactus-Runner-Wüstenstil, fünf Kampfphasen und freundlichem Ende; startet von selbst, ein Klick nach dem Ende spielt neu ab. [Projekt, Erzeugung und echte Firefox-Prüfung](docs/desert-stickfight.md).
- [Cactus Runner](examples/cactus-runner.html): laufende Bone-Figur, scrollende Kakteen und Klick-/Leertasten-Sprung (ohne Buttons) mit echter Walk/Jump-Gewichtsmischung; unabhängige Lauf- und Sprung-States. [Details/Prüfung](docs/cactus-runner.md).
- [Cinder / Flux – Stickfight](examples/stickfight.html): eigener achtsekündiger animierter Kampf zweier Bone-Rigs, Ausweichen/Gegenangriff, Schatten/Funken; läuft von selbst in Schleife, ohne Buttons; keine kopierten Drunkenstick-Figuren. `python3 scripts/generate-stickfight.py`, Prüfung: `python3 scripts/stickfight-smoke.py`.
- [Braune Eule](examples/brown-owl.html): Eule auf einem Ast mit direkten Klick-/Hover-States – beim Darüberfahren folgen die Augen dem Zeiger und die Federohren stellen sich auf, ein Klick schließt zufrieden die Augen und hebt die Flügel. Prüfung: `python3 scripts/owl-smoke.py`.
- [Aoi sagt Hallo](examples/anime-hello.html): eigene Anime-inspirierte Figur mit Bone-Hand. **Lokal erzeugtes WAV ist im SVG eingebettet**; keine Internetverbindung nötig. Aoi winkt von selbst; ein Klick ins Bild gibt Audio frei (Browser-Vorgabe), ein weiterer wiederholt das Hallo. Reproduktion: `python3 scripts/generate-anime-demo.py` (lokales `espeak-ng`/`espeak` nötig); Browserprüfung: `python3 scripts/anime-smoke.py`.

- [Neugieriges Gesicht](examples/curious-face.svg) ([`.sply`](examples/curious-face.sply)): drei Zustandsmaschinen gleichzeitig – Hover beugt vor, ein Klick blendet zum Lächeln über, ein Timer blinzelt – während die Pupillen der Maus folgen. Kein eigener Code. Erzeugen: `python3 scripts/generate-curious-face.py`, Prüfung: `python3 scripts/face-smoke.py`.
- [Wassertropfen auf blauer Kugel](examples/water-drop.html): eigenständige SVG-Szene mit Squash/Stretch, Glanz und Ripple; läuft von selbst in Schleife.
- [Zufallsfiguren](examples/random-figures.html): reproduzierbar per Seed, automatische Zustandsfolge mit Bildunterschriften. startet beim Laden von selbst, ohne Button. Zustandswechsel folgen Clip-Enden.
- Beide HTML-Dateien betten die benachbarte SVG per `object` ein; keine Server-API. Testserver ist nur ein lokaler Browser-Testhelfer.
- Erzeugen: `splinely-cli water-drop --help`, `splinely-cli random-figures --help`; vollständige Beispiele in [CLI.md](CLI.md). Browserprüfung: `python3 scripts/showcase-smoke.py --duration 35`.

## Logo

Eigenständig gezeichnetes SVG-Logo: ein „S“ als Bézier-Kurve mit Ankerpunkten, Anfasser und Bewegungsspur.

## Lizenz

MIT, siehe [LICENSE](LICENSE).
