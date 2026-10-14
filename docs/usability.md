# Forma: eine ruhigere Werkstatt

## Einstieg

- **Gestalten** startet mit Zeichenfläche und Eigenschaften. Die Zeitleiste ist zunächst verborgen; der Schalter **Zeitleiste** bleibt in jedem Arbeitsbereich erreichbar.
- **Animieren** öffnet die Zeitleiste und bietet Zeichenfläche, Skelett, Zustände und Audio/Sprache.
- **Erweitert** enthält JavaScript und den bewahrten SVG-Quelltext. Quelltext ist kein Zeichenwerkzeug.
- **Speichern**, **Abspielen/Pause**, **Vorschau** und **Forma Viewer öffnen** sind direkt erreichbar. Das Sternchen bei Speichern zeigt ungespeicherte Dokumentänderungen an.
- **Datei** bündelt Neu (Strg+N), Öffnen … (Strg+O), Zuletzt geöffnet, Speichern (Strg+S), Speichern unter … (Strg+Umschalt+S), SVG-Import und beide SVG-Exporte (Strg+E: interaktiv). Die Dialoge sind native System-Dateidialoge (XDG-Portal/GTK, `rfd`) und laufen in einem Hilfsthread, die Zeichenfläche bleibt bedienbar. **Pfade manuell eingeben** bleibt als Rückfallebene.
- Ein nie gespeichertes Projekt („Unbenannt“) fragt beim Speichern nach dem Ziel, statt still ins Arbeitsverzeichnis zu schreiben. Sein Autosave liegt unter `~/.local/share/forma/autosave/`. Zuletzt geöffnete Dateien stehen in `~/.config/forma/recent.txt`.
- Fenstertitel: `• datei.frm — Forma` bei ungespeicherten Änderungen. Beim Schließen mit ungespeicherten Änderungen: **Speichern / Nicht speichern / Abbrechen**.

Leere Zeichenflächen und fehlende Auswahl geben einen kurzen nächsten Schritt statt einer Anleitung in Absatzlänge. Werkzeughinweise stehen direkt beim Arbeiten; Details und Tastenkürzel sind in Hover-Hilfen. Ein dunkles Theme mit dezentem Petrol-Akzent und etwas größeren Abständen kommt ohne neue Abhängigkeiten aus.

## Werkzeuge und Gesten

**Auswahl / Pen / Pfadpunkte / Freihand** sind exklusive Auswahlknöpfe. Die internen Zustände bleiben `pen`, `edit_path_handles` und `freehand_mode`; es gibt keinen zweiten persistenten Werkzeugzustand.

- Auswahl: Formen anklicken und verschieben; Eigenschaften rechts bearbeiten.
- Pen: Anker klicken, für Tangenten ziehen. Enter beendet, Escape verwirft den Entwurf. Erneutes Auswählen von Pen behält den Entwurf.
- Pfadpunkte: Pfad auswählen, Anker/Tangenten ziehen; Doppelklick auf eine Kante fügt einen Anker ein. Shift koppelt Tangenten, Shift+Alt erhält die gegenüberliegende Länge; Escape verwirft die laufende Punktgeste.
- Freihand: Maus oder Tablet ziehen; Breite und Druckkontur erscheinen nur für dieses Werkzeug. Druck hängt weiterhin von gelieferten Touch-Force-Events ab. Ein Strich bleibt ein Rückgängig-Schritt.

Werkzeugwechsel beenden reale laufende Bearbeitung als Undo-Transaktion, verwerfen spekulative Pfad-/Stiftgesten und verlassen Wiedergabe/Vorschau. Auch laufende unabhängige Ebenenuhren und Überblendungen werden zurückgesetzt, damit Freihand nicht ausgewählt, aber blockiert ist. Das ändert keine Clips oder Dokumentgewichte. Ebenen können über die erweiterten Vorschauaktionen erneut gestartet werden.

Die bestehenden Pfadaktionen bleiben in **Pfadaktionen · schließen / fertig / verwerfen** erreichbar (anfangs geöffnet). Ihre internen Werkzeugschalter werden anschließend ebenfalls exklusiv normalisiert. Erfolgreiches Fertigstellen wechselt zu Pfadpunkte.

**Ansicht** enthält Lineale, Plugin-Hervorhebung, sichtbares Raster, Einrasten am Raster und die Einstellungen. Zoom und Einpassen bleiben sichtbar. Mitteltaste oder Leertaste+Ziehen verschieben nur die Ansicht; Pen-Entwürfe bleiben dabei erhalten. F/Home passt ein, +/- zoomt, 0 setzt 100 %. Kurzes Drücken/Loslassen der Leertaste schaltet Wiedergabe; eine Pan-Geste tut das nicht. Ctrl/Cmd+Z und Ctrl/Cmd+S bleiben erhalten.

## Zeitleiste, Befehlspalette und Bild-Export

- **Visuelle Zeitleiste:** Über den Abspielknöpfen zeigt eine Zeitleiste den gewählten Clip: Lineal mit rotem Abspielkopf (ziehen oder klicken zum Springen), eine Zeile pro Spur mit Schlüsselbild-Rauten. Raute ziehen verschiebt das Schlüsselbild (Zeitraster aus den Einstellungen, Standard 0,05 s; Alt: frei; nie über die Nachbarn hinaus), Doppelklick in eine Zeile fügt ein Schlüsselbild mit dem aktuellen Wert ein, Entf löscht das gewählte (Strg+Z stellt es wieder her). Ein Klick auf den Spurnamen wählt die Form.
- **Übergänge (Easing):** Rechtsklick auf eine Raute bietet Linear, Weich (ease), Beschleunigen, Abbremsen, Weich rein und raus und Sprung – für das Schlüsselbild oder die ganze Spur. Die Form der Raute zeigt den Übergang (Raute = linear, Quadrat = Sprung, rund = Kurve).
- **Zwiebelhaut** (Ansicht › Zwiebelhaut): Blasse Geister der animierten Formen vor (rötlich) und nach (bläulich) dem Abspielkopf; statische Formen werden nicht wiederholt. In den Einstellungen wählbar: an Schlüsselbildern oder in festen Zeitabständen, bis zu 5 Geister je Seite (weiter entfernte werden blasser), Deckkraft und beide Farben. Während der Wiedergabe ausgeblendet.
- **Aufnahme (Auto-Key):** „🔴 Aufnahme“ oben in der Zeitleiste (oder Befehlspalette „Aufnahme“). Solange sie an ist, wird jede Änderung an Position, Drehung, Größe, Farbe, Deckkraft oder Verlauf als Schlüsselbild beim Abspielkopf im gewählten Clip gespeichert; beim ersten Schlüsselbild einer Eigenschaft wird der alte Wert bei 0 s festgehalten. Ohne Clip wird „Animation“ angelegt.
- **Verlauf (Ansicht › Verlauf …):** Liste aller Rückgängig-Schritte mit Namen („Verschieben: Kreis“ …); ein Klick springt zu diesem Stand.
- **Befehlspalette (Strg+K, Datei › Befehlspalette):** Jeder Befehl per Suche: deutsche Namen oder englische Begriffe („union“, „align left“, „png“). ↑/↓ wählen, Enter ausführen, Escape schließen.
- **Tastenkürzel-Übersicht (F1, Datei › Tastenkürzel):** zeigt die aktuelle Belegung; „Tastenkürzel ändern …“ führt in die Einstellungen.
- **PNG und GIF:** Datei › „Bild als PNG exportieren“ speichert das aktuelle Bild in Dokumentgröße (Skalierung einstellbar), „Animation als GIF exportieren“ den gewählten Clip als endlos laufendes GIF (Standard: 20 Bilder/s, weißer Hintergrund, höchstens 1600 px breit; alles in den Einstellungen änderbar).

## Einstellungen (Strg+,)

Datei › Einstellungen, Ansicht › Einstellungen oder die Befehlspalette öffnen ein Fenster mit sechs Bereichen. Änderungen gelten sofort und werden in `~/.config/forma/settings.json` (bzw. `$XDG_CONFIG_HOME/forma`) gespeichert; „Alles zurücksetzen“ stellt die Standardwerte wieder her.

| Bereich | Optionen |
| --- | --- |
| Allgemein | Oberflächengröße (75–200 %), Erscheinungsbild Dunkel/Hell, Autosave-Abstand, Anzahl Rückgängig-Schritte |
| Zeichenfläche | Lineale, sichtbares Raster, Einrasten am Raster, Rastergröße, intelligente Hilfslinien, Fangabstand in Bildschirmpixeln |
| Bearbeiten | Pfeiltasten-Schritt klein/groß, Versatz beim Duplizieren und Einfügen |
| Animation | Zwiebelhaut (Modus, Anzahl davor/danach, Abstand, Deckkraft, Farben), Zeitraster der Zeitleiste (0 = frei) |
| Export | GIF-Bildrate, maximale GIF-Breite, GIF-Hintergrund, PNG-Skalierung |
| Tastenkürzel | Jeder Befehl der Befehlspalette lässt sich belegen: Kürzel anklicken, neue Taste(nkombination) drücken (Esc bricht ab). Eine schon vergebene Kombination wird beim anderen Befehl entfernt (Hinweis in der Statuszeile). „Leeren“ entfernt, „Standard“ stellt die Vorgabe wieder her; Suchfeld zum Filtern. |

Fest belegt bleiben Pfeiltasten, Entf/Rücktaste, Esc, Leertaste und Strg+C/X/V. Beides rendert wie der statische SVG-Export (ohne Skripte).

## Sprache

- **Spracheinsätze** (Animieren › Audio / Sprache): Text, Sprache (de-DE, en-US, …), Tempo, Tonhöhe, Lautstärke.
- **▶ Probehören** erzeugt die Stimme lokal und spielt sie über ffplay ab.
- **Als Audio einbetten** ersetzt den Browser-Spracheinsatz durch eine lokal erzeugte, aufbereitete Aufnahme zur selben Zeit (Piper, falls installiert; Flite für Englisch; sonst eSpeak NG). Die Aufnahme wird gefiltert, auf kurze Ränder geschnitten und auf gleiche Lautheit gebracht und klingt dann in jedem Browser gleich – Browser-Sprache nutzt dagegen die Systemstimmen (unter Linux meist eSpeak).
- Im Browser wählt die Runtime automatisch die natürlichste verfügbare Stimme der Sprache.

## Text, Farben und neue Projekte

- **Text direkt bearbeiten:** Doppelklick auf einen Text (Werkzeug Auswahl) öffnet ein Eingabefeld an Ort und Stelle; Klick daneben oder Strg+Enter übernimmt (ein Rückgängig-Schritt), Escape bricht ab.
- **Dokumentfarben:** Unter Darstellung › Füllung (Farbe) stehen alle im Projekt verwendeten Farben als Felder: Klick setzt die Füllung, Rechtsklick den Rand.
- **Neues Projekt (Strg+N):** Name und Format wählen – Standard, Full HD, Quadrat, Story, A4, Icon, Banner oder eigene Größe („Drehen“ tauscht Hoch-/Querformat). Ungespeicherte Arbeit wird vorher wie gewohnt abgefragt.

## Tastenkürzel

Standardbelegung; alles außer Pfeiltasten, Entf, Esc, Leertaste und Zwischenablage ist unter Einstellungen › Tastenkürzel änderbar.

| Taste | Wirkung |
| --- | --- |
| V · P · A · B | Auswahl · Pen · Pfadpunkte · Freihand |
| R · E · T | Rechteck · Kreis · Text in der Mitte der Ansicht einfügen |
| Entf / Rücktaste | Auswahl löschen (wie „Form löschen“, inkl. Spur-/Zustandsbezügen) |
| Strg+A | Alle sichtbaren, entsperrten Formen auswählen |
| Strg+G · Strg+Umschalt+G | Gruppieren · Gruppe lösen |
| Bild↑ / Bild↓ (Umschalt) | Eine Ebene (ganz) nach vorne / hinten |
| Umschalt+H · Umschalt+V | Horizontal · vertikal spiegeln |
| Strg+D · Alt+Ziehen | Duplizieren (versetzt um 10; wurde die letzte Kopie verschoben, wiederholt Strg+D genau diesen Abstand) · Kopie direkt verschieben |
| I | Pipette: nächster Klick überträgt Stil der angeklickten Form auf die Auswahl |
| Umschalt+2 | Auf Auswahl zoomen |
| Rechtsklick | Kontextmenü (Ausschneiden … Stil kopieren/einfügen, Anordnen, Sperren, Ausblenden) |
| Strg+C / Strg+X / Strg+V | Kopieren / Ausschneiden / Einfügen; die Form liegt als JSON in der Systemzwischenablage und lässt sich auch zwischen Forma-Fenstern einfügen |
| Pfeiltasten (Shift) | Auswahl um 1 (10) Einheiten verschieben, je ein Rückgängig-Schritt |
| Escape | Auswahl aufheben |
| Strg+Z · Strg+Umschalt+Z / Strg+Y | Rückgängig · Wiederholen |
| Strg+Mausrad / Touchpad-Zoom | Zum Mauszeiger zoomen (5 %–3200 %) |
| Mausrad · Mitteltaste · Leertaste+Ziehen | Ansicht verschieben |
| F / Pos1 · 0 · +/- | Einpassen · 100 % · Zoomstufe |
| Strg+K · F1 · Strg+, | Befehlspalette · Tastenkürzel-Übersicht · Einstellungen |

Beim Ziehen zeigt eine kleine Anzeige am Zeiger den Versatz (Δx/Δy), an Griffen die Skalierung in % bzw. den Winkel.

Gesperrte Ebenen werden von Löschen, Verschieben und Duplizieren nicht verändert. In Textfeldern gelten die Kürzel nicht.

## Auswahl, Gruppen und Anordnen

- **Klick** wählt eine Form, **Umschalt+Klick** fügt hinzu oder entfernt, **Ziehen auf freier Fläche** zieht ein Auswahlrechteck auf (berührte, sichtbare, entsperrte Formen; mit Umschalt zur Auswahl hinzu). In der Ebenenliste wirken Umschalt- und Strg-Klick genauso.
- Ziehen, Pfeiltasten, Löschen, Duplizieren, Kopieren und Einfügen wirken auf die ganze Auswahl, jeweils als ein Rückgängig-Schritt. Gesperrte Formen bleiben unverändert.
- **Gruppen** (Strg+G) lassen sich verschachteln: Gruppieren fasst die Auswahl unter ihrer gemeinsamen Gruppe zusammen, Strg+Umschalt+G löst die äußerste Ebene. Ein Klick auf eine Form wählt ihre äußerste Gruppe, **Strg+Klick** nur die einzelne Form. Die Ebenenliste zeigt einen Baum: Gruppen auf-/zuklappen, Klick wählt die ganze Gruppe, Doppelklick benennt sie um, Auge/Schloss wirken auf alle Mitglieder. Gruppenmitglieder liegen in der Ebenenreihenfolge zusammen; der SVG-Export erzeugt verschachtelte `<g>`-Elemente, der SVG-Import übernimmt `<g id>` als Gruppen.
- **Text in Pfad umwandeln** (Darstellung › In Pfad umwandeln): Buchstaben werden mit der lokalen Systemschrift zu bearbeitbaren Umrissen.
- Das Panel **Anordnen** rechts richtet aus (links/mittig/rechts/oben/mittig/unten; bei einer Form an der Zeichenfläche), verteilt ab drei Formen mit gleichen Abständen, spiegelt um die gemeinsame Mitte und ändert die Ebenenreihenfolge. Es arbeitet auf der Grundpose (ohne Clips), auch für an Knochen gebundene Formen.
- **Griffe auf der Zeichenfläche:** Acht Skaliergriffe und ein Drehknopf um die Auswahl. Umschalt hält die Proportionen bzw. rastet die Drehung in 15°-Schritten ein, Alt skaliert von der Mitte aus. Über den Anker hinaus ziehen spiegelt. Ist eine Form schräg gedreht, gibt es nur proportionale Eckgriffe (ungleichmäßiges Skalieren würde sie scheren). Ein Griff-Zug ist ein Rückgängig-Schritt.
- **Boolesche Operationen** (Anordnen-Panel, ab zwei Formen): Vereinigen, Abziehen (unterste minus die anderen), Schnittmenge, Ausschluss. Ergebnis ist ein Pfad mit dem Stil der untersten Form; Kurven bleiben Kurven. Text kann nicht kombiniert werden. Bewusst ohne Tastenkürzel, damit Strg++/Strg+- weiter zoomen.
- **+ Mehr** legt Sechseck, Dreieck, Stern oder Linie als bearbeitbaren Pfad an.
- **Morph** (Anordnen-Panel oder Rechtsklick, genau zwei Formen): Die zuerst gewählte Form verwandelt sich in 1 s in die zweite. Forma gleicht beide Umrisse an (gleiche Segmentzahl, passender Startpunkt und Umlaufsinn), legt einen Clip an und blendet die Zielform aus. Weitere Zwischenformen lassen sich als Schlüsselbilder der Eigenschaft „Pfadform (Morph)“ hinzufügen, solange sie dieselbe Struktur haben.
- **Verläufe animieren:** In der Zeitleiste die Eigenschaft „Verlauf“ wählen und „◼ Aktuellen Wert merken“; Richtung, Positionen und Farben der Stopps werden zwischen Schlüsselbildern interpoliert. Art (linear/radial) und Anzahl der Stopps bleiben gleich.
- **Intelligente Hilfslinien** (Ansicht, standardmäßig an): Beim Ziehen rasten linke/mittlere/rechte und obere/mittlere/untere Kante der Auswahl an denselben Linien anderer sichtbarer Formen und der Zeichenfläche ein (6 Bildschirmpixel); magentafarbene Linien zeigen das Einrasten. Der Zug wird immer vom Start aus berechnet, Rückgängig nimmt ihn als Ganzes zurück.
- **Manuelle Hilfslinien:** vom oberen Lineal (waagerecht) oder linken Lineal (senkrecht) auf die Zeichenfläche ziehen; vorhandene Linien lassen sich verschieben und durch Zurückziehen aufs Lineal löschen (Ansicht › Hilfslinien löschen entfernt alle). Formen rasten auch an ihnen ein. Sie werden im Projekt gespeichert (`guides`) und sind rückgängig machbar.
- **Exakte Maße:** Im Panel Anordnen (bei Mehrfachauswahl offen) X, Y, Breite und Höhe der Auswahl eintippen oder ziehen; „Proportional“ koppelt Breite und Höhe.
- **Verläufe:** Unter Darstellung › Füllung zwischen Farbe, linearem Verlauf (Richtung in Grad) und radialem Verlauf wählen; 2–16 Farbstopps mit Position. Verläufe stehen als `gradient` im Projekt, werden exportiert und von der Runtime nicht überschrieben (Füllfarb-Animationen wirken dann nicht).
- **Wiederherstellung:** Ist beim Öffnen (oder beim Start ohne Datei) eine automatische Sicherung neuer als die Datei, bietet Forma „Wiederherstellen“ oder „Verwerfen“ an. Speichern entfernt die Sicherung.
- Mehrfachauswahlen zeigen einen gestrichelten Gesamtrahmen. Der Inspektor bearbeitet weiterhin die zuerst angeklickte Form.
- Öffnen oder ein neues Projekt leeren den Rückgängig-Verlauf; man kann nicht versehentlich in die vorherige Datei zurückspringen.

## Ebenen

Die Ebenenliste zeigt oberste Ebenen zuerst mit Formsymbol, 👁 (sichtbar) und 🔒 (gesperrt). Ab neun Ebenen gibt es ein Filterfeld; die Liste rendert nur sichtbare Zeilen und scrollt bei Auswahl auf der Zeichenfläche automatisch zur Ebene. ⬆/⬇, Duplizieren und 🗑 wirken auf die ausgewählte Ebene.

## Geschwindigkeit

- Die Vorschau rastert nur den sichtbaren Ausschnitt in nativer Pixeldichte (schärfer beim Zoomen, keine Kosten für Bereiche außerhalb).
- Unveränderte Szenen (gleiches Dokument, Zeit, Zustand und Auswahl) werden nicht neu ausgewertet.
- Die Validierung indiziert Spurziele einmal pro Durchlauf statt pro Schlüsselbild.

## Progressive Offenlegung, keine entfernten Funktionen

- Dokumentgröße, Erweiterungen und technische Hinweise sind zugeklappt; Position und Drehung einer Auswahl sind zunächst geöffnet.
- **Mehrere Clips & Überblendung** verbirgt zunächst parallele Wiedergabe, manuelle Rücksetzung und einzelne Ebenenaktionen. Aktive Ebenenzeiten/-gewichte bleiben außerhalb der Klappe sichtbar.
- **Clip-Optionen & Mischgewicht** enthält Export-Wiederholung, Autostart, Ersetzen/Gewichtet und Gewicht. Clip-Anlegen, Zeitregler, Spuren und Schlüsselbilder bleiben erreichbar.
- Skelett, Zustandsgraph/JSON, Variablen, Ereignisse, Audio, Sprache, Erweiterungen, Ebenen, Pfadkonvertierung und Quelltextbearbeitung bleiben vorhanden.

## Dokument- und Vertrauensgrenzen

Die Umordnung ersetzt weder Dokumentvalidierung noch Undo, Import/Export oder die Gestenimplementierungen. Werkzeugwechsel überschreiben keine vorgemerkten JavaScript-, SVG- oder Zustands-JSON-Entwürfe und ändern die explizite JavaScript-Freigabe nicht. Dokumentwechsel/Undo über `sync_buffers()` widerrufen diese Freigabe weiterhin.

JavaScript läuft nur nach Freigabe im interaktiven Export/Viewer, nicht in der nativen Vorschau. Bewahrtes SVG wird als XML validiert und als Quelltext gespeichert, nicht in die Szene injiziert. Der native Viewer bleibt eine Momentaufnahme: zum Aktualisieren erneut öffnen. Sprache und zustandsgesteuertes Audio benötigen weiterhin Viewer/Browser; native Audiovorschau bleibt für globale Audioeinsätze verfügbar.

## Prüfung und verbleibende Grenzen

Neue Headless-egui-Tests in `crates/forma_editor/src/app_ui_tests.rs` prüfen echte Werkzeug-/Vorschauknopf-Klicks, Datei-Menü und progressive Offenlegung, Pfad-Fertigstellen/Abbrechen, alle Werkzeugwechsel, Entwurfserhalt, Undo und JS-Freigabe sowie den Wechsel aus unabhängiger Ebenenwiedergabe/Überblendung.

Prüfbefehle (mit `TMPDIR=/home/user/svg_editor/.work-tmp`):

```sh
cargo test -p forma_editor
cargo clippy -p forma_editor --all-targets -- -D warnings
```

Oberfläche, Statusmeldungen und Fehlermeldungen der Werkzeuge sind deutsch. Technische Validierungsmeldungen des Dokumentkerns und Manifestfehler von Drop-in-Plugins bleiben englisch, weil CLI und Skripte sie maschinenlesbar auswerten. Diese Module liegen außerhalb dieses UI-Änderungsbereichs; die Arbeitsbereichsnavigation ist konsistent deutsch. Die Headless-Tests ersetzen keinen visuellen Desktop-Test, keinen echten Tablet-/Drucktest und keinen WebView- oder Audio-Hardwaretest. Ein vorhandener ffplay/SDL-Integrationstest ist standardmäßig ignoriert.
