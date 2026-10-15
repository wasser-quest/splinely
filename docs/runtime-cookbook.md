# Runtime ohne Rätsel

Die Browser-Runtime bleibt kompatibel. Eine Animation besteht aus **Clips**, States steuern Verhalten, die **Timeline** ist die globale Uhr für Cues. Die native Rust-Runtime hat einen anderen Host-Vertrag: siehe [native-runtime.md](native-runtime.md).

## Einen Clip bedienen

In einem direkt geöffneten Splinely-SVG ist die Instanz `document.documentElement.splinely`; in eigenen Handlern wird `splinely` übergeben. Bei mehreren SVGs immer die Instanz der jeweiligen SVG verwenden, nicht die globale Komfort-Aliasvariable.

```js
const clip = splinely.clip('duel'); // vorhandene Clip-ID aus dem Projekt
clip.play();                   // Start bzw. Fortsetzen
clip.pause();                  // Pose behalten
clip.seek(0).play();            // Clip von vorn, kein vollständiger Szenenreset
clip.stop();                   // Beitrag entfernen und Clipzeit zurücksetzen
```

**Wichtig:** `timeline.pause()` pausiert auch Clips. `timeline.play()` startet nur die globale Uhr, nicht sämtliche pausierten Clips. Für Fortsetzen die gewünschten Clips ausdrücklich `play()`en. Fades laufen auch bei pausierten Clip-Uhren weiter; das ist beabsichtigtes Verhalten. Ein echter Szenen-Neustart muss auch States, Variablen, explizite Eigenschaften und Gewichte zurücksetzen: dafür einen eigenen Restart-Handler/Event verwenden.

## Was läuft gerade?

```js
const state = splinely.snapshot();
console.log(state.time, state.playing, state.clips, state.states);
console.log(state.audio, state.diagnostics);
splinely.on('error', ({error}) => console.warn(error.message));
```

`snapshot()` liefert neue, JSON-kompatible Daten ohne DOM-/Audiohandles. Es verändert keine Uhr und löst keine Callbacks aus; die Werte sind die zuletzt ausgewerteten Runtime-Werte. Auch nach `destroy()` bleibt eine Diagnose mit `destroyed: true` möglich. `reportError(error)` meldet eigene Hostfehler über denselben Error-Kanal und die auf 100 Einträge begrenzte Historie. Export-Hostfehler benutzen diesen Kanal; Audio-Unlock-Fehler werden nicht nochmals dupliziert.

## Im HTML anzeigen

Ein SVG mit Runtime als echte SVG-Dokumentinstanz einbetten, **nicht als `<img>`**:

```html
<object id="movie" type="image/svg+xml" data="../examples/stickfight.svg">
  SVG nicht verfügbar — bitte die SVG direkt oder im Splinely Viewer öffnen.
</object>
<p id="status">Animation wird geladen …</p>
<script>
const movie = document.getElementById('movie');
const status = document.getElementById('status');
movie.addEventListener('load', () => {
  try {
    const f = movie.contentDocument?.documentElement?.splinely;
    if (!f) throw new Error('Runtime nicht verfügbar');
    status.textContent = 'Bereit';
    f.on('error', ({error}) => { status.textContent = error.message; });
  } catch (_) {
    status.textContent = 'SVG direkt oder im Splinely Viewer öffnen; Scripting/CSP und Same-Origin-Zugriff prüfen.';
  }
});
</script>
```

Zugriff aus dem HTML funktioniert nur, wenn die Browser-Isolation dies erlaubt. Besonders `file://` ist browserabhängig; die bereitgestellten eigenständigen Showcases oder der Viewer vermeiden zusätzliche Dateien. Isolation/CSP nicht abschalten, um einen Fehler zu verstecken. Ein fehlendes `load`-Event benötigt in einer Produktionsoberfläche zusätzlich einen Lade-Timeout.

## Leistung und Ruhezustand

Die Runtime rendert nur, wenn sich etwas geändert hat: laufende Clips, Überblendungen, Follower, `set()`, Variablen, Zustandswechsel oder Seeks. Ein ruhender Film (keine laufende Zeit, keine Änderung) kostet pro `tick()` praktisch nichts und schreibt keine DOM-Attribute; auch bei laufenden Clips werden nur Attribute geschrieben, deren Wert sich seit dem letzten Schreiben geändert hat. Ergebnis, `snapshot()`, `bones()` und Diagnosen bleiben dabei identisch zur bisherigen Runtime (Golden-Test `runtime/tests/perf-golden.test.cjs`).

Konsequenzen für Hosts:

- Die Runtime überschreibt animierte Attribute nicht mehr in jedem Frame. Wer ein animiertes Attribut von außen am DOM ändert, sieht die Änderung bis zur nächsten echten Wertänderung bestehen; dauerhafte Eingriffe gehören in `splinely.set(id, {…})`.
- `overrides` eines Zustands werden beim ersten Gebrauch gelesen; Zustände zur Laufzeit zu ändern wird nicht unterstützt (das Projekt ist nach dem Laden unveränderlich).
- `splinely.sourceProject` liefert das ursprünglich übergebene bzw. einmal aus `<metadata>` gelesene Projekt-Objekt. Hosts, die dieselben Daten brauchen, sollten es wiederverwenden statt das JSON erneut zu parsen (`splinely.sourceProject || JSON.parse(…)`); nicht verändern.

## Audio und Vertrauen

Audio aus einer echten Nutzeraktion freigeben: `await splinely.audio.unlock()`. Der Export bietet dafür seinen Startknopf. Kein automatisches Audio beim Laden versprechen.

`customJs` bleibt ohne explizite Exportfreigabe Daten. Aktive fremde SVGs sind nicht vertrauenswürdig; weder Browser-Runtime noch Viewer sind eine Sandbox. Die native Rust-Runtime führt `customJs` nicht aus: Hosts zeichnen Nodes, liefern Eventziele und behandeln Audio-Ausgaben selbst.
