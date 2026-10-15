# Browser- und Offline-Abnahme

## Tatsächlich geprüft

- `bash scripts/check.sh`: Rust-Workspace, 31 Node-Runtime-Tests, echte CLI-Demoerzeugung/Validierung/Auswertung/Export, XML-/Metadaten-Roundtrip und gültiger eingebetteter PCM-WAV.
- Firefox **151.0.1**, direktes `file://`-SVG: Runtime-Smoke zeigt **PASS: direct offline SVG runtime**. Prüft Auto-Mount, Loops, stille Seeks, DOM-Klick/Zustand und isolierte Instanzen.
- `python3 scripts/browser-smoke.py`: echter CLI-Export, echte WebDriver-Nutzeraktivierung am Start-Control, AudioContext `running`, WAV erfolgreich decodiert, globaler Transport läuft, Kreis-Klick schaltet Zustand/Farbe. Keine Drittanbieter-Pythonpakete erforderlich.
- Desktopfenster lässt sich öffnen; Editor-Tests prüfen u. a. Änderungen, Undo/Redo und Export.

Frühere Headless-Versuche endeten mit Timeout/`[GFX1-]: RenderCompositorSWGL failed mapping default framebuffer, no dt`. Mit frischem Profil und `MOZ_DISABLE_GPU_ACCEL=1` bestanden die späteren Läufe. Ein Timeout ist niemals ein Pass.

**Noch nicht nachgewiesen:** hörbare *Hardware*-Ausgabe (Lautsprecher/Bluetooth), WebKit-Audio, Safari sowie weitere Codecs. Chromium/Firefox/WebKit-Smokes und eine Null-Sink-Audio-/Latenzmessung existieren seit dem Abschnitt „Browsermatrix“ unten. Node-Audiotests verwenden Testdoubles. Der strukturelle Ressourcencheck ist kein allgemeiner JS-Sicherheitsbeweis oder Netzwerkmonitor.

## Reproduzieren

```sh
bash scripts/generate-demo.sh
bash scripts/check.sh
python3 scripts/browser-smoke.py   # benötigt Firefox; testet examples/demo.svg
node runtime/tests/build-browser-smoke.cjs
work=$(mktemp -d); mkdir -p "$work/profile"
MOZ_HEADLESS=1 MOZ_DISABLE_GPU_ACCEL=1 timeout 30 firefox --headless --no-remote \
  --profile "$work/profile" --screenshot "$work/smoke.png" \
  "file://$(pwd)/runtime/tests/.generated/browser-smoke.svg"
```

Der Screenshot muss **PASS** zeigen, nicht STATIC FALLBACK/FAIL. Der Python-Test gilt für das mitgelieferte Demoformat (`ball`, Zustand `clicked`); er ist keine universelle SVG-Testsuite.

## Manuelle Audio-/Browsermatrix

1. Demo-Projekt und `customJs` prüfen. `demo.svg` enthält ausdrücklich freigegebenen Code (`--trusted-js`). Keine unbekannten Projekte damit öffnen.
2. Netzwerk abschalten oder Browser offline stellen; `examples/demo.svg` direkt öffnen, nicht als `<img>` oder in einem Bildbetrachter. Browser/OS/Startkontext protokollieren und Netzwerkanzeige prüfen.
3. **Start audio / animation** klicken: Audio wird entsperrt und die globale Timeline startet. Kreis klicken: orange, Bewegung über drei Sekunden; im Trusted-Demo startet der Handler die Timeline erneut. Der 0,4-s-Ton soll ungefähr **1,5 s** später beginnen. Er enthält **keine Worte**. Timing separat messen, nicht als exakt garantiert betrachten.
4. Kreis erneut klicken: Bewegung/Ton starten neu. Konsole und `document.documentElement.splinely.diagnostics` auf Audio/CSP/JS-Fehler prüfen.
5. Standardexport ohne `--trusted-js` öffnen: Start-Control entsperrt Audio **ohne Nutzercode**; die deklarative State Machine reagiert auf den ersten Kreis-Klick. Wiederholter Neustart aus dem individuellen JS-Handler fehlt erwartungsgemäß. `customJs` bleibt Metadaten.
6. Skripte deaktivieren/als `<img>` öffnen: statische Ausgangsgrafik bleibt sichtbar; keine Interaktion oder Audio. Firefox und Chromium vergleichen.
7. Editor: Projekt laden, Keyframe ändern, speichern/neu laden/exportieren. Native Vorschau ist stumm. Trusted-JS-Freigabe ist sitzungsbezogen und wird beim Laden zurückgesetzt.


## Browsermatrix (Chromium + Firefox + WebKit) und Audio-Latenz

PLAN.md Punkt 9. Die Matrix führt die bestehenden Smoke-Szenarien in drei Playwright-Engines aus, vergleicht Zahlen zwischen den Browsern und misst hörbare Ausgabe samt Latenz. Die Python-/Marionette-Smokes (`scripts/*-smoke.py`) bleiben unverändert und sind weiter der Firefox-Referenzpfad; die Matrix ist eine zusätzliche, in Node/Playwright neu geschriebene Fassung derselben Prüfungen.

### Ausführen

```sh
npm install --prefix ~/.cache/pw playwright@1.64.0          # einmalig
~/.cache/pw/node_modules/.bin/playwright install --with-deps chromium firefox webkit   # Ubuntu/CI; --with-deps braucht sudo
export PLAYWRIGHT_MODULE=~/.cache/pw/node_modules/playwright
cargo build -p splinely-cli                                  # die Matrix baut selbst nie
bash scripts/browser-matrix.sh                               # alle drei Engines + Audio (Chromium)
bash scripts/browser-matrix.sh --browsers chromium --audio none --scenarios motion-smoke,sply-binary-web-player
bash scripts/browser-matrix.sh --audio chromium,firefox --audio-nonblocking
node scripts/browser-audio-latency.cjs --browser firefox --runs 5 --out audio.json   # nur die Audio-Messung
```

Optionen: `--browsers`, `--scenarios`, `--out-dir`, `--audio chromium[,firefox]|none`, `--audio-nonblocking`, `--allow-skip`, `--tolerance` (Standard 1e-6), `--showcase-duration`, `--scenario-timeout`. Umgebung: `PLAYWRIGHT_MODULE`, `PLAYWRIGHT_BROWSERS_PATH`, `SPLINELY_CLI`, `SPLINELY_SCRATCH` (Ausgabe/Temp, Standard `~/.cache/splinely-scratch`), `SPLINELY_MATRIX_SKIP_HOST_VALIDATION=1` (setzt `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1`, nur für nicht unterstützte Distributionen).

Szenarien (je frischer Browser-Kontext, Timeout = Fehler): `motion-smoke` (35 SVG-Matrizen gegen `splinely-cli evaluate`), `blend-spaces-smoke`, `demo-gate-and-state` (Start-Control mit echtem Klick, AudioContext `running`, Zustand/Farbe, Property-Filter, `file://`), `anime-hello-gate`, `curious-face` (echte Maus), `showcase-water-drop`, `showcase-random-figures` (alle Zustände besucht, bis ca. 25 s), `idle-timeline-interactions`, `sply-binary-web-player` (eigenständiger JS-Decoder == `splinely-cli cat` für `examples/desert-stickfight.sply`, CLI-Export mit `sply-base64` mountet, Posen), `accessibility-web-player` (Barrierefreiheit des Web-Players: Rollen und Namen über `getByRole`/Aria-Snapshot, Tab-Reihenfolge, Enter/Space löst denselben Klick wie der Zeiger aus, sichtbarer Fokusring auch mit Forced-Colors-Emulation, `aria-live`-Text, `prefers-reduced-motion`, unverändertes Verhalten ohne a11y-Daten; siehe `docs/accessibility.md`; der Test ersetzt keinen echten Screenreader), `isolated-preview` (`scripts/isolated-preview-smoke.cjs` je Engine über `SPLINELY_BROWSER`), `runtime-plugin` (PLAN.md Punkt 10, `docs/plugins.md`: Beispiel `examples/plugins/hover-wiggle` – `plugin apply` für beide Werkzeuge, Export mit `--plugin … --trusted-js`, echter Zeiger: Hover/Klick starten die deklarativen Clips **und** der Laufzeit-Hook zählt `wiggleCount`; `--plugin` ohne `--trusted-js` wird abgelehnt; derselbe Export ohne Plugin spielt die Clips, aber `wiggleCount` bleibt 0).

### Ergebnis lesen

`<out-dir>/browser-matrix.json` (Schema `splinely-browser-matrix/1`): `browsers.<engine>.scenarios.<name>` mit `status` (`pass` | `fail` | `skip`), `checks[]`, `metrics`; `crossBrowser[]` mit `maxAbsDiff` je Szenario gegen den ersten Browser; `audio.<engine>` (Latenzstatistik `latencyMs`, `contextLatencyMs`, `runs[]`, bei Chromium `ffplay`); `summary` mit `overall`.

- **Exit 0** = alles gelaufen und bestanden. **1** = mindestens ein Fehler. **3 = `incomplete`**: etwas wurde *übersprungen* (Browser nicht startbar, kein Audio-Server …). Ein Skip steht laut im Log (`SKIPPED, NOT PASSED: …`) und in `summary.skipped`; `--allow-skip` erlaubt Exit 0, ändert aber den JSON-Status nicht.
- `summary.notMeasured` nennt ausdrücklich, was die Matrix nie misst (WebKit-Audio, echte Hardware, Safari/WebKitGTK).
- Querbrowser-Vergleich: Zahlen mit Toleranz (`--tolerance`), Strings/Boolesche exakt. Verglichen werden die Motion-/Blend-Matrizen, Pupillen-/Hover-Werte, dekodierte Stickfight-Posen und besuchte Zustände.
- `--audio-nonblocking` zählt Audio-Skip/-Fehler nicht für `overall` (CI nutzt dagegen einen eigenen, `continue-on-error`-Schritt).

### Audio und Latenz

`scripts/browser-audio-latency.cjs` legt einen PulseAudio/PipeWire-Null-Sink (`splinely_matrix_sink`, `module-null-sink`) an, startet den Browser mit `PULSE_SINK` darauf und nimmt `<sink>.monitor` mit `parec` auf. Das Autoplay-Gating bleibt **an** (`--autoplay-policy=document-user-activation-required`); gemessen wird mit `examples/anime-hello.svg` (Cue `hello` bei 0,18 s). Pro Lauf geprüft: Seite lädt ohne Geste → AudioContext `suspended`; **digitale Stille vor** dem Klick; echter (`isTrusted`) Klick auf das Gate; danach AudioContext `running`, Stream hängt am Test-Sink, Pegel (RMS/Peak) passt zum dekodierten WAV (±3 dB), Cue-Beginn plausibel. Latenz = Klick-Zeitstempel (Epoch, `event.timeStamp`) bis erste Probe über Schwelle (0,003 FS); zusätzlich Klick→`AudioBufferSourceNode.start()`, `start()`→Ton und `AudioContext.baseLatency/outputLatency`. Chromium misst auch den nativen Pfad: dieselbe `ffplay`-Kommandozeile wie `crates/splinely_editor/src/audio_preview.rs` (Spawn→erster Ton).

Gemessen (diese Maschine, Fedora 43, PipeWire-pulse-Null-Sink, je 3 Läufe; Zeiten ± ca. 10 ms Aufnahme-Unsicherheit):

| Engine | Klick→Ton (Median, min–max) | Klick→`start()` | `start()`→Ton | baseLatency / outputLatency |
|---|---|---|---|---|
| Chromium 156 (headless shell) | 241,8 ms (241,4–255,6) | 14,7 ms | 226,7 ms (davon 180 ms geplanter Cue-Offset) | ≈10,7 ms / ≈24–32 ms |
| Firefox 157 | 317,8 ms (285,3–326,2) | 98 ms (64–98) | 221,3 ms | 0 ms / ≈30 ms |
| `ffplay` (Editor-Vorschau-Pfad) | Spawn→Ton ≈ 216–266 ms, Median ≈ 224 ms | – | – | – |

Pegel: RMS −18,7 dBFS, Peak −5,7 dBFS, Tondauer ≈ 380 ms (entspricht dem dekodierten Asset). Vor der Geste: Peak 0 (exakte Stille). Die Pipeline nach dem geplanten Start kostet ≈ 40–50 ms; der Rest ist der im Projekt eingestellte Cue-Offset. Zahlen sind Größenordnungen, keine Garantien.

### Bekannte Grenzen

- **WebKit** ist Playwrights WPE-Headless-MiniBrowser (WebKit 27.2), **nicht Safari** und nicht das WebKitGTK des nativen Viewers. Auf der Fedora-Entwicklungsmaschine startete es erst, nachdem `libicu*.so.74` und `libjpeg.so.8` (Ubuntu-noble-Pakete) in `…/webkit-2370/minibrowser-wpe/sys/lib/` kopiert und `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1` gesetzt wurde; auf Ubuntu übernimmt `playwright install --with-deps` das. Ohne das meldet die Matrix für WebKit einen lauten Skip.
- **WebKit-Audio wird nicht gemessen**; die Szenarien prüfen nur, dass der AudioContext entsperrt wird und `start()` läuft (das passiert dort).
- Null-Sink ≠ Lautsprecher: Gerätepuffer, Bluetooth und Mischer-Latenz echter Hardware fehlen. Chromium läuft headless (`chromium_headless_shell`); Firefox emittiert beim Seitenladen einen kurzen (~50 ms) Impuls, der separat als `loadTransient` ausgewiesen und aus dem Stille-Fenster ausgenommen wird.
- Die Matrix baut das CLI nie selbst (sie nutzt `target/debug/splinely-cli` und protokolliert Version/Zeitstempel im JSON). `examples/demo.svg` enthält ein älteres eingebettetes Runtime-Exemplar ohne `sourceProject`; das Szenario liest deshalb nur `snapshot()`/Metadaten.
- Engine-Unterschied, nicht Fehler: `window.open` aus dem sandboxed Iframe liefert in Chromium `null`, Firefox wirft; `isolated-preview-smoke.cjs` wertet beides als „verweigert“. BFCache-Wiederherstellung wird nicht geprüft.
- CI: Job `browser-matrix` in `.github/workflows/ci.yml` (siehe PLAN.md). Er wurde lokal **nicht** auf GitHub ausgeführt.
