# Browser- und Offline-Abnahme

## Tatsächlich geprüft

- `bash scripts/check.sh`: Rust-Workspace, 31 Node-Runtime-Tests, echte CLI-Demoerzeugung/Validierung/Auswertung/Export, XML-/Metadaten-Roundtrip und gültiger eingebetteter PCM-WAV.
- Firefox **151.0.1**, direktes `file://`-SVG: Runtime-Smoke zeigt **PASS: direct offline SVG runtime**. Prüft Auto-Mount, Loops, stille Seeks, DOM-Klick/Zustand und isolierte Instanzen.
- `python3 scripts/browser-smoke.py`: echter CLI-Export, echte WebDriver-Nutzeraktivierung am Start-Control, AudioContext `running`, WAV erfolgreich decodiert, globaler Transport läuft, Kreis-Klick schaltet Zustand/Farbe. Keine Drittanbieter-Pythonpakete erforderlich.
- Desktopfenster lässt sich öffnen; Editor-Tests prüfen u. a. Änderungen, Undo/Redo und Export.

Frühere Headless-Versuche endeten mit Timeout/`[GFX1-]: RenderCompositorSWGL failed mapping default framebuffer, no dt`. Mit frischem Profil und `MOZ_DISABLE_GPU_ACCEL=1` bestanden die späteren Läufe. Ein Timeout ist niemals ein Pass.

**Noch nicht nachgewiesen:** hörbare Hardwareausgabe, genaue Audio-Latenz oder weitere Codecs/Browser. Node-Audiotests verwenden Testdoubles. Der strukturelle Ressourcencheck ist kein allgemeiner JS-Sicherheitsbeweis oder Netzwerkmonitor.

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

