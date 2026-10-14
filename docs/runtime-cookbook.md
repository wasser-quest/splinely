# Runtime ohne Rätsel

Die Browser-Runtime bleibt kompatibel. Eine Animation besteht aus **Clips**, States steuern Verhalten, die **Timeline** ist die globale Uhr für Cues. Die native Rust-Runtime hat einen anderen Host-Vertrag: siehe [native-runtime.md](native-runtime.md).

## Einen Clip bedienen

In einem direkt geöffneten Forma-SVG ist die Instanz `document.documentElement.forma`; in eigenen Handlern wird `forma` übergeben. Bei mehreren SVGs immer die Instanz der jeweiligen SVG verwenden, nicht die globale Komfort-Aliasvariable.

```js
const clip = forma.clip('duel'); // vorhandene Clip-ID aus dem Projekt
clip.play();                   // Start bzw. Fortsetzen
clip.pause();                  // Pose behalten
clip.seek(0).play();            // Clip von vorn, kein vollständiger Szenenreset
clip.stop();                   // Beitrag entfernen und Clipzeit zurücksetzen
```

**Wichtig:** `timeline.pause()` pausiert auch Clips. `timeline.play()` startet nur die globale Uhr, nicht sämtliche pausierten Clips. Für Fortsetzen die gewünschten Clips ausdrücklich `play()`en. Fades laufen auch bei pausierten Clip-Uhren weiter; das ist beabsichtigtes Verhalten. Ein echter Szenen-Neustart muss auch States, Variablen, explizite Eigenschaften und Gewichte zurücksetzen: dafür einen eigenen Restart-Handler/Event verwenden.

## Was läuft gerade?

```js
const state = forma.snapshot();
console.log(state.time, state.playing, state.clips, state.states);
console.log(state.audio, state.diagnostics);
forma.on('error', ({error}) => console.warn(error.message));
```

`snapshot()` liefert neue, JSON-kompatible Daten ohne DOM-/Audiohandles. Es verändert keine Uhr und löst keine Callbacks aus; die Werte sind die zuletzt ausgewerteten Runtime-Werte. Auch nach `destroy()` bleibt eine Diagnose mit `destroyed: true` möglich. `reportError(error)` meldet eigene Hostfehler über denselben Error-Kanal und die auf 100 Einträge begrenzte Historie. Export-Hostfehler benutzen diesen Kanal; Audio-Unlock-Fehler werden nicht nochmals dupliziert.

## Im HTML anzeigen

Ein SVG mit Runtime als echte SVG-Dokumentinstanz einbetten, **nicht als `<img>`**:

```html
<object id="movie" type="image/svg+xml" data="../examples/stickfight.svg">
  SVG nicht verfügbar — bitte die SVG direkt oder im Forma Viewer öffnen.
</object>
<p id="status">Animation wird geladen …</p>
<script>
const movie = document.getElementById('movie');
const status = document.getElementById('status');
movie.addEventListener('load', () => {
  try {
    const f = movie.contentDocument?.documentElement?.forma;
    if (!f) throw new Error('Runtime nicht verfügbar');
    status.textContent = 'Bereit';
    f.on('error', ({error}) => { status.textContent = error.message; });
  } catch (_) {
    status.textContent = 'SVG direkt oder im Forma Viewer öffnen; Scripting/CSP und Same-Origin-Zugriff prüfen.';
  }
});
</script>
```

Zugriff aus dem HTML funktioniert nur, wenn die Browser-Isolation dies erlaubt. Besonders `file://` ist browserabhängig; die bereitgestellten eigenständigen Showcases oder der Viewer vermeiden zusätzliche Dateien. Isolation/CSP nicht abschalten, um einen Fehler zu verstecken. Ein fehlendes `load`-Event benötigt in einer Produktionsoberfläche zusätzlich einen Lade-Timeout.

## Audio und Vertrauen

Audio aus einer echten Nutzeraktion freigeben: `await forma.audio.unlock()`. Der Export bietet dafür seinen Startknopf. Browser-Stimmen sind optional und nicht offline garantiert; lokal eingebettetes WAV benötigt keine Stimme. Kein automatisches Audio beim Laden versprechen.

`customJs` bleibt ohne explizite Exportfreigabe Daten. Aktive fremde SVGs sind nicht vertrauenswürdig; weder Browser-Runtime noch Viewer sind eine Sandbox. Die native Rust-Runtime führt `customJs` nicht aus: Hosts zeichnen Nodes, liefern Eventziele und behandeln Audio-/Speech-Ausgaben selbst.
