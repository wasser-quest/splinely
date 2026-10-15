# Interaktive Figuren: Design, States und SVG-Export

## Ist das SVG wirklich eigenständig?

**Interactive SVG** bzw. `splinely-cli export` bettet Projekt und JavaScript-Runtime in dieselbe SVG-Datei ein. Die Runtime ist für Keyframes, Zustandsmaschinen und Event-Handler nötig, wird aber nicht als zweite Datei geladen. Die mitgelieferten Szenen benötigen keinen Server/API und keine nachgeladenen Bibliotheken. HTML-Dateien im Showcase sind nur optionale Präsentationshüllen.

- SVG direkt im Browser oder in einem HTML-`<object>` öffnen: Skripte und direkte Figur-Events können laufen (abhängig von Browser/CSP).
- Als `<img>`, CSS-Hintergrund oder in einem Skripte unterdrückenden Viewer: nur die Grafik; keine interaktive Runtime.
- **Static SVG** ist ein anderer Export: Momentaufnahme ohne Verhalten.
- `--trusted-js` führt ausdrücklich eigenen eingebetteten Code aus. Dieser ist **nicht sandboxed**. Nur geprüfte eigene Projekte freigeben; fremden Code nicht ungeprüft aktivieren.

## Eingebettetes Audio

Eigene Aufnahmen (WAV, MP3, OGG …) werden als AudioAsset mit Cue ins Projekt eingebettet (Editor: **Animieren › Audio**, CLI: `apply`). `splinely-cli export project.sply --output scene.svg` legt Audio und Runtime in eine Datei; der Audio-Start verlangt einen Benutzer-Klick.

Die [Aoi-Demo](../examples/anime-hello.html) nutzt eingebettetes Offline-Audio.

## State-Machine ohne sichtbare Knöpfe

Events können direkt von Szenenobjekten kommen: `click`, `pointerenter`, `pointerleave`, `pointermove`. Ein separater Button ist für lautlose Animationen nicht nötig. Die [braune Eule](../examples/brown-owl.html) demonstriert direkte Klicks, Hover, Pupillen-Tracking und Zustände, die Lider und Flügel steuern. Ein Vertrauens-/Start-Klick ist gegebenenfalls für Audio nötig, nicht für Zustandsmaschinen generell.

Typischer Aufbau:

1. `idle`: ruhige Figur, Feier-Effekt unsichtbar.
2. `attentive`: Maus über Figur, Augen folgen dem Zeiger.
3. `happy`: Figur angeklickt, Feier-Effekt sichtbar.
4. Rückkehr zu `idle`/`attentive` über weitere Events oder Clip-Ende.

Augen verfolgen den Zeiger nicht automatisch durch einen State allein: Der State aktiviert das Verhalten, ein Pointer-Handler berechnet begrenzte Pupillenpositionen in SVG-Koordinaten. Aktuell wird solches Verhalten über die eingebettete JS/API ergänzt; es ist kein fertiger visueller Rive-Constraint-Editor.

## Manchmal soll man nichts sehen

Für zustandsabhängig ausgeblendete Effekte `opacity = 0` verwenden und im nächsten State wieder auf `1` setzen. Die Ebene bleibt im Projekt vorhanden. `visible: false` ist dagegen ein dauerhaftes Sichtbarkeitsmerkmal, kein Wechsel-State.

Unsichtbar ist nicht automatisch unanklickbar: SVG-Hit-Testing und `pointer-events` bestimmen das. Klick/Hover daher an eine sichtbare, stabile Figur-Fläche binden; nicht an temporäre Herzen oder einzelne Pupillen. Eine bewusst vollständig unsichtbare Szene braucht einen weiterhin erreichbaren Auslöser, z. B. einen äußeren Bereich, Zeitablauf oder ein externes Event.

## Prüfgrenzen

Native Vorschau unterstützt das dokumentierte Editor-Verhalten, führt aber eingebetteten JS nicht aus. Pointer-Tracking und custom JS deshalb im interaktiven Browserexport prüfen. Ein Screenshot allein beweist weder Zustandswechsel noch Sicherheit; dafür echte Pointer-Interaktionen und Runtime-Diagnostik beobachten.

Ein unvollständiger Hinweis wie `/tmp/... keyEnter` ist kein reproduzierbarer Pentest-Bericht. Für einen Befund werden tatsächlicher Berichtspfad, Payload, erwartetes/beobachtetes Verhalten und der geprüfte Kontext benötigt. Testdaten und Berichtstexte dürfen nicht als Shell-Kommandos interpretiert werden.
