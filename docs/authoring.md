# Erweiterte Authoring-Werkzeuge

## Pfade

Im Canvas **Pen** aktivieren: Klick setzt einen Anker, Drag setzt eine kubische Tangente. **Close path** schließt den Entwurf; **Finish path** oder Enter validiert und legt genau einen Undo-Schritt an. Escape verwirft ihn. **Path handles** zeigt beim ausgewählten, sichtbaren und entsperrten Pfad quadratische Anker und runde Kontrollpunkte. Drag wird erst beim Loslassen committed; währenddessen nur eine Vorschau. Animation/Interact müssen ausgeschaltet sein.

M/L/Q/C/Z sind grafisch editierbar. Andere gültige SVG-Befehle werden beim grafischen Bearbeiten durch den SVG-Parser normalisiert; unbearbeiteter Pfadtext bleibt erhalten. Renderpräzision ist begrenzt. Pfade sind keine Meshes und werden nicht beliebig gemorpht. **Snap 10** rundet lokale Objektpositionen beim Ende eines Objekt-Drags, nicht einzelne Pfadpunkte oder Bone-Joints.

## States

Der States-Tab besitzt einen staged Graph-Editor: Knoten auswählen, Zustand hinzufügen/umbenennen, Transition auswählen und Quelle/Ziel/Event/Zielobjekt bearbeiten. Erst **Apply** ersetzt das validierte Projekt. Guards und Aktionen bleiben beim Bearbeiten von Übergängen erhalten; komplexe Definitionen weiter im JSON bearbeiten. Kanten sind eine Übersicht, keine frei ziehbaren Kabel. Ungespeicherter Machine-JSON-Text sperrt die Graph-Übernahme, neue Maschinen und referenzändernde Objektlöschung. Reload ist eine bewusste Verwerfung von Quelltextentwürfen.

## Bones / IK

Das Zwei-Bone-IK ist ein Authoring-Pose-Werkzeug, kein Echtzeit-Constraint. Voraussetzung: direktes Kind, kein lokaler x/y-Jointversatz. Unerreichbare Ziele werden auf den erreichbaren Radius begrenzt. Die entstandenen normalen Rotationen lassen sich keyframen; Browser benötigt keinen IK-Solver. Extreme Rotationen über ±1000000 Grad werden mit Fehler verweigert. Starre Bindungen, keine Skinning-Meshes.

## Native Audio

Im Audio-Tab **Enable / reload native audio**: eingebettete Assets werden außerhalb des GUI-Threads vorbereitet. Ein installiertes `ffplay` plus funktionierendes Ausgabegerät ist erforderlich. Unterstützt globale Projekt-Cues, seek/stop/pause und Überlappungen. Änderungen an Audioassets/Cues deaktivieren die alte Vorschau; erneut aktivieren. Grenzen: 32 MiB/Asset, 128 MiB Cache, 8 Stimmen, fünf Minuten Preview; Decoderprozesse haben Watchdogs und werden beim Stop/Drop beendet. Fehlende Geräte/Decoder führen zu Diagnose statt zum Editor-Absturz. Keine harte Decoder-RSS-Sandbox und keine Audio-Clock-Synchronitätsgarantie. Nutzercode läuft nicht; der Browserexport bleibt Referenz für das Timing.

## CLI-Showcases

`splinely-cli water-drop --help` und `splinely-cli random-figures --help` erklären die Generatoren. Die ausgelieferten HTML-Wrappers brauchen ihre benachbarte SVG. Zufallsfiguren sind per Seed reproduzierbar; `clip.end` führt automatisch zur nächsten Figur. Zustandsdauer richtet sich nach dem Clip.
