# Forma — Implementierungsplan

## Ziel

Forma ist ein einfacher Rust-Desktopeditor für SVG-Grafiken mit Keyframe-Animationen, State Machines, programmierbaren Interaktionen und Audio. Der interaktive Export soll als einzelne SVG-Datei offline ohne Server, API oder nachgeladene Bibliotheken funktionieren.

Dieser Plan beschreibt die Umsetzung; die Implementierung beginnt anschließend schrittweise. Der Name der App ist **Forma**.

## Leitlinien und Grenzen

- Einfache Oberfläche: Zeichenfläche, Eigenschaftenpanel, einklappbare Timeline; zusätzliche Tabs für Zustände und Code.
- Rust für Editor und Dokumentkern; eingebettetes JavaScript für die Browser-Runtime.
- Plugin-/ECS-Architektur nach Bevy-Vorbild, mit klaren Modulgrenzen.
- Projektformat und SVG-Export sind getrennt: das Projekt erhält alle bearbeitbaren Daten.
- Kein Netzwerkbedarf der mitgelieferten Runtime; alle benötigten Ressourcen werden im eigenständigen Export eingebettet.
- Interaktive SVGs benötigen einen Browserkontext, der SVG-Skripte erlaubt. Als `<img>` oder in vielen Bildbetrachtern laufen sie nicht. CSP und Browserrichtlinien können weitere Einschränkungen verursachen.
- Ohne Skriptausführung bleibt eine sinnvolle statische Grafik sichtbar.
- „Alle Möglichkeiten offen“ bedeutet Erweiterbarkeit und Quelltextzugriff, nicht vollständige Unterstützung jedes SVG-Features im ersten Release.

## 1. Architektur

### Stack

- Rust-Workspace.
- Bevy ECS und Plugin-Scheduling, mit egui-Oberfläche über eine kompatible Integration.
- Versioniertes Dokumentmodell mit Serde und JSON.
- JavaScript-Runtime ohne externe Laufzeitabhängigkeiten für SVG-Export.
- Rust-Tests für Modell und Auswertung; Browsertests für Export, Interaktion und Audio.

Konkrete Bibliotheksversionen werden beim Projektstart auf gegenseitige Kompatibilität geprüft und festgeschrieben. Rendering und Desktopintegration werden zuerst durch einen kleinen technischen Prototyp validiert.

### Datenfluss

```text
UI / Werkzeuge / Quelltextänderungen
                 |
              Commands
                 |
        Dokumentkern + History
                 |
           ECS-Projektion
                 |
 Animation / Zustände / Interaktion / Audio
                 |
         ausgewertete Darstellung
                 |
        Editorvorschau / SVG-Export
```

Das gespeicherte Dokument ist die einzige autoritative Quelle. ECS-Entitäten sind Laufzeitobjekte; stabile Dokument-IDs bleiben unabhängig von ECS-Entity-IDs. Vorschau und Playback verändern das Dokument nicht dauerhaft.

### Vorgesehene Workspace-Struktur

```text
crates/
  forma_document/    # Schema, IDs, Validierung, Migrationen
  forma_core/        # Commands, History, Geometrie
  forma_animation/   # Tracks, Interpolation, Transport
  forma_behavior/    # State Machines, Events, Variablen
  forma_audio/       # Assets, Cues, Transportregeln
  forma_svg/         # Import, Export, Quelltexterhaltung
  forma_editor/      # Desktopapp, ECS-Plugins, egui
runtime/
  src/               # eingebettete Browser-Runtime und öffentliche API
  tests/             # Browser- und Konformitätstests
fixtures/
  projects/          # Referenzprojekte
  svg/               # SVG-Import-/Exportfälle
  conformance/       # gemeinsame Rust-/JS-Auswertungsfälle
```

## 2. Plugin-/ECS-System

| Plugin | Verantwortung |
| --- | --- |
| ScenePlugin | Formen, Gruppen, Darstellung und Transformationen |
| EditorPlugin | Werkzeuge, Auswahl, Inspector und Shortcuts |
| HistoryPlugin | Commands, Transaktionen, Undo/Redo |
| TimelinePlugin | Tracks, Clips, Keyframes und Transport |
| InteractionPlugin | Pointer, Touch, Tastatur, Fokus und Drag |
| StateMachinePlugin | Zustände, Guards, Aktionen und Variablen |
| AudioPlugin | Assets, Audio-Cues und Speech-Cues |
| CodePlugin | Codeansicht, Diagnostik und Runtime-API |
| SvgPlugin | Import, Export und SVG-Quelltext |

Plugins können Komponenten, Systeme, Ereignisse, Inspector-Felder und serialisierbare Erweiterungen registrieren. Rust-Plugins werden zunächst statisch eingebunden. Dynamische native Plugins sind kein MVP-Ziel.

Ein exportierbares Plugin muss eine Browserimplementierung oder eine Übersetzung in unterstützte Runtime-Daten liefern. Bevy-Systeme werden nicht automatisch im Browser ausgeführt. Nicht exportierbare Funktionen führen zu verständlichen Diagnosen, nicht zu stillem Datenverlust.

Systemphasen: Eingaben sammeln → Events und Zustände verarbeiten → Animation auswerten → Werte zusammensetzen → Darstellung aktualisieren. Audio-Scheduling folgt derselben Transportzeit, nutzt aber die Audio-Uhr.

## 3. Dokument und Bearbeitung

Das Projekt enthält:

- `schemaVersion` und Migrationen.
- Szenenknoten mit stabilen IDs, Gruppen, Geometrie, Stil und Transformationen.
- SVG-Definitionen und eingebettete Assets.
- Animationsclips, Tracks und Keyframes.
- State Machines, Variablen, Events und Aktionen.
- Audio- und Speech-Cues.
- Nutzercode und erhaltene SVG-Quelltextbereiche.
- Plugin-Daten mit Namensraum und Version.

Alle abgeschlossenen Bearbeitungen laufen über Commands. Ein Drag oder eine Keyframe-Verschiebung erzeugt eine Undo-Transaktion. Autosave, Laden und atomisches Speichern gehören zum Fundament.

Geometrie unterstützt Matrixinversion, Bildschirm-/SVG-/Objektkoordinaten, Bounding Boxes, Hit-Testing und später Snapping sowie Bézierbearbeitung.

## 4. Keyframe-Editor

### MVP

- Tracks pro Objekt und Eigenschaft.
- Keyframes hinzufügen, verschieben, kopieren und löschen.
- Position, Rotation, Skalierung, Deckkraft und Farbe.
- Lineare, diskrete und kubische Bézier-Interpolation.
- Benannte Clips, Play/Pause, Seek/Scrubbing und Loop.
- Eigene Audio-Tracks.

Die Auswertung ist deterministisch: identische Eingaben und Zeit ergeben identische Eigenschaftswerte. Rust-Vorschau und JavaScript-Runtime verwenden gemeinsame Konformitätsfälle.

Wertpriorität im MVP: Basiswerte → Zustands-Overrides → Clipwerte → explizite Runtime-Overrides. Konkurrierende Clips verwenden dokumentierte Prioritäten; komplexes additives Blending folgt später. Runtime-Overrides können ausdrücklich aufgehoben werden.

Später: Bewegung entlang von Pfaden, kompatibles Pfad-Morphing, erweiterter Kurveneditor und Crossfading. Beliebige Pfade sind nicht automatisch morph-kompatibel.

## 5. Interaktionen und State Machines

Editor-Werkzeugzustände und exportiertes Grafikverhalten bleiben getrennt.

Die Grafik-Runtime unterstützt zunächst:

- Klick, Doppelklick, Pointer rein/raus, runter/bewegen/loslassen.
- Drag mit Pointer Capture und SVG-Koordinaten.
- Touch über Pointer Events, Mausrad und Tastatur/Fokus.
- Timer, benutzerdefinierte Events und Clip-Ende.
- Zustände, Übergänge, Guards und typisierte Variablen.
- Aktionen für Clipsteuerung, Eigenschaften, Events und Audio.

Der visuelle Zustandseditor bietet einfache Knoten und Verbindungen, eine Vorschau und ein Eventprotokoll. Ereignisreihenfolge, Übergangspriorität und Abbruch laufender Animationen werden spezifiziert und getestet. Grenzen für unmittelbare Übergangsschleifen verhindern blockierende Runtime-Zyklen.

## 6. Unabhängige Audio-API

Audio hängt nicht an einem bestimmten Grafikobjekt. Assets und Cues sind eigenständige Dokumentdaten.

### Audiodateien

- Import einer lokalen Datei mit ID, MIME-Typ und Bytes.
- Cue mit Zeit, Asset-ID, Offset, Lautstärke und optionalem Ende.
- Eigenständiger Export: Bytes eingebettet im SVG, beispielsweise Base64 in einem Datenblock; Dekodierung über Web Audio.
- Optionaler Export mit externen Dateien, klar als nicht eigenständig gekennzeichnet.
- Codec-Unterstützung wird für Zielbrowser geprüft; Dateigröße und Speicherlimits werden angezeigt.

### Speech

- Cue mit Text, Sprache, Rate, Tonhöhe und Lautstärke.
- Browser-Sprachausgabe als zusätzliche Funktion.
- Stimmen, Offlineverfügbarkeit und Startlatenz sind systemabhängig und nicht garantiert.
- Präzise, offline reproduzierbare Sprache verwendet eingebettete Aufnahmen.

### Transportregeln

- Zentrale Transportzeit für Grafik und Audio; Audio-Uhr für geplante Audiostarts.
- Vorladen/Dekodieren vor dem Start und klare Ladefehler.
- Pause stoppt Quellen und bewahrt Position; Fortsetzen plant neue Quellen.
- Seek startet überlappende Audio-Cues am richtigen Offset, wenn Wiedergabe aktiv ist.
- Scrubbing löst standardmäßig weder kurze Sounds noch Speech aus.
- Loop plant Audio neu; Speech wird bei Seek/Pause gestoppt und nicht rückwirkend nachgeholt.
- Cues werden beim Überschreiten ihres Zeitpunkts genau nach festgelegten Regeln ausgelöst, auch bei ausgelassenen Renderframes.
- Audiofreigabe nach Nutzerinteraktion, gegebenenfalls über einen Start-Button. Keine Garantie absolut verzögerungsfreier Ausgabe.

## 7. Code und SVG-Quelltext

### Geplante öffentliche JavaScript-API

```js
forma.on("button", "click", async () => {
  await forma.audio.unlock();
  forma.clip("intro").play();
});

forma.timeline.at(1.5, () => forma.audio.play("hello"));
forma.speech.say("Hallo!", { lang: "de-DE" });
forma.machine("main").send("OPEN");
forma.set("circle", { opacity: 0.5 });
forma.variable("speed").set(2);
```

Die API wird versioniert und dokumentiert. Eventabonnements und Zeit-Callbacks besitzen Abmeldefunktionen und definierte Regeln bei Seek und Loop.

### Quelltextbearbeitung

- SVG/XML-Ansicht und JavaScript-Ansicht.
- XML-Validierung vor Übernahme.
- Unterstützte SVG-Änderungen werden in das Modell übernommen.
- Unbekannte Elemente/Attribute bleiben soweit möglich als rohe XML-Bereiche erhalten.
- Änderungen an IDs und Referenzen werden geprüft.
- Konflikte zwischen visueller Bearbeitung und Quelltext werden angezeigt.
- Ein Quelltextmodus erhält nicht visuell interpretierbare Inhalte, ohne vollständige Bearbeitbarkeit vorzutäuschen.

Beliebiges SVG und JavaScript lassen sich nicht allgemein verlustfrei in einen visuellen Editor zurückübersetzen. Automatische vollständige Roundtrips werden deshalb nicht versprochen.

## 8. Export und Sicherheit

Exportprofile:

1. Statisches SVG eines gewählten Zeitpunkts.
2. Eigenständiges interaktives SVG mit Runtime, Verhalten, Code und Assets.
3. Optional SVG mit externen Assets.

Die Runtime wird pro SVG gekapselt und unterstützt mehrere Instanzen ohne globale Namenskollisionen. XML, Daten und Script-Inhalte werden korrekt escaped. Eingebettete Fonts und Bilder benötigen geeignete Rechte und Browserunterstützung.

Standardimport führt keinen fremden Code aus. Skripte und gefährliche URLs werden im sicheren Importmodus entfernt oder deaktiviert. Bewusst aktivierter Code wird getrennt behandelt; untrusted Vorschauen benötigen eine isolierte Browserumgebung. Frei ausführbarer JavaScript-Code kann selbst Netzwerkaufrufe durchführen; Offlinefähigkeit der Runtime ist keine Sicherheitsgarantie für Nutzercode.

SVG-Komplexitäts-, Ressourcen- und Dateigrößenlimits sowie klare Fehlermeldungen schützen den Editor.

## 9. Implementierungsphasen und Abnahmekriterien

### Phase 0 — Technischer Prototyp

- Rust-Workspace und kompatible Bevy-/egui-Versionen festlegen.
- Fenster, einfache Form, Eigenschaftenpanel und ECS-Projektion.
- Ein offline geöffnetes SVG mit eingebettetem Klickhandler und Audiofreigabe testen.
- Vorschau-Rendering und Browser-Teststrategie entscheiden.

**Abnahme:** Desktopfenster funktioniert; Export läuft in den vorgesehenen Browserkontexten ohne Netzwerk.

### Phase 1 — Dokument und Minimaleditor

- Schema, stabile IDs, Commands und History.
- Rechteck, Kreis, Auswahl, Verschieben, Skalieren und Rotation.
- Laden, Speichern, Autosave und statischer Export.

**Abnahme:** Projekt-Roundtrip und Undo/Redo bestehen automatisierte Tests.

### Phase 2 — Browser-Runtime und Keyframes

- Runtime-Grundgerüst und Exportdatenformat.
- Timeline, Eigenschaftstracks, Easing, Clips und Transport.
- Gemeinsame Rust-/JS-Konformitätsfälle.

**Abnahme:** Vorschau und Export ergeben zu vorgegebenen Zeiten dieselben Werte innerhalb festgelegter numerischer Toleranzen.

### Phase 3 — Interaktion und Zustände

- Pointer-/Tastaturereignisse, Drag und Koordinatenumrechnung.
- State-Machine-Modell, visueller Editor und Eventprotokoll.
- Clips und Zustandswechsel verbinden.

**Abnahme:** Klick-, Hover- und Drag-Beispiele funktionieren offline; Schleifen und Abbrüche sind getestet.

### Phase 4 — Audio

- Audioassets, eingebetteter Export, Cue-Tracks und Speech.
- Laden, Freigabe, Pause, Seek und Loop.

**Abnahme:** Eingebettete Aufnahme startet am geplanten Cue; Transporttests prüfen Offsets und vermeiden doppelte Auslösungen. Timing-Toleranzen werden im Browserprototyp gemessen und festgelegt.

### Phase 5 — Code und SVG-Quelltext

- Codeeditor, Runtime-API und Diagnostik.
- SVG-Validierung, Quelltexterhaltung und Konfliktanzeige.
- Sichere Vorschau für untrusted Projekte.

**Abnahme:** Eigener Code kann Clips, Zustände und Audio steuern; fehlerhafte Eingaben zerstören kein gespeichertes Projekt.

### Phase 6 — Erweiterung und Qualität

- Gruppen, Text, Bézierpfade, Snapping und fortgeschrittene Animation.
- Plugin-Beispiel mit Editor- und Exportintegration.
- Browsermatrix, Performance, Accessibility und reduzierte Bewegung.

**Abnahme:** Referenzprojekte, Offline-Tests und Regressionstests bestehen in dokumentierten Zielbrowsern.

## 10. Erstes durchgängiges Beispiel

Ein Kreis bewegt sich per Keyframes. Ein Klick aktiviert die State Machine und startet den Clip. Bei Sekunde 1,5 spielt eine eingebettete Sprachaufnahme. Ein optionales Codeskript verändert anschließend die Farbe. Das Projekt lässt sich speichern, erneut bearbeiten und als eine eigenständige SVG-Datei exportieren.

## Nächster Umsetzungsschritt

Phase 0 beginnen: Workspace anlegen, Abhängigkeiten prüfen und einen minimalen Desktop-/SVG-Runtime-Prototyp bauen. Erst nach dessen Validierung die vollständige Timeline und Audiointegration erweitern.
