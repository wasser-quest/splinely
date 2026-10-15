# Barrierefreiheit

Dieses Dokument beschreibt, was Splinely für Bildschirmleser- und Tastaturnutzung tut – und was nicht.
**Keine WCAG-Konformitätsaussage.** Nichts hier wurde mit einem echten Bildschirmleser (NVDA, JAWS,
VoiceOver, Orca, TalkBack) geprüft; geprüft wurde nur, was Browser über ihren Barrierefreiheitsbaum
(Playwright `getByRole`/Aria-Snapshot) und über echte Tastatureingaben melden.

## Web-Player und SVG

Gilt für das exportierte interaktive SVG (`splinely-cli export`, Web-Player mit eingebetteter Runtime)
und das statische SVG. Barrierefreiheit ist **opt-in**: Ein Projekt ist „a11y-aktiv“, sobald es
mindestens eines hat – `description`, ein Knoten mit `a11y` oder einen Zustand mit `announce`. Ohne
solche Daten bleibt der Export **byte-identisch** zu vorher, und die Runtime fügt keine Attribute,
Listener oder Live-Region hinzu (`runtime/tests/perf-golden.test.cjs` und die eingefrorene
Referenz-Runtime bleiben unverändert grün).

### Daten im Projekt

| Feld | Ort | Bedeutung |
| --- | --- | --- |
| `name` | Projekt | wird als `<title>` exportiert (gab es schon; kein neues Feld) |
| `description` | Projekt, optional | wird als `<desc>` exportiert; ≤ 1000 Zeichen |
| `a11y` | Knoten, optional | `{label?, description?, role?, hidden?}`; unbekannte Schlüssel werden abgelehnt (`deny_unknown_fields`); ein leeres Objekt wird weggelassen |
| `announce` | Zustand einer Zustandsmaschine, optional | Text für die höfliche Live-Region; ≤ 200 Zeichen |

Regeln (Rust **und** JS prüfen gleich, streng): Texte nicht leer, ohne führende/folgende Leerzeichen,
ohne Steuerzeichen (auch U+FEFF/U+FFFE/U+FFFF); `label`/`announce` ≤ 200, `description` ≤ 1000
Zeichen; `role` nur aus `button`, `link`, `img`, `group`, `heading`; `hidden` ist ein Boolean. Ein
`hidden`-Knoten darf weder Label/Beschreibung/Rolle haben noch Ziel eines Zeiger-/Tastenereignisses
sein. Das binäre `.sply`-Format arbeitet auf dem JSON-Baum und braucht dafür keine Änderung
(Round-Trip-Tests in `crates/splinely_document/tests/accessibility.rs`).

CLI:

```sh
splinely-cli a11y scene.sply                       # Bericht: Beschreibung, Knoten, Ansagen, unbeschriftete interaktive Knoten
splinely-cli a11y scene.sply --title "Lampe" --description "Eine Lampe mit Schalter."
splinely-cli a11y scene.sply --node switch --label "Lampenschalter" --description "Schaltet die Lampe" --role button
splinely-cli a11y scene.sply --node deko --hidden true
splinely-cli a11y scene.sply --state-announce lamp.on="Lampe ist an"
splinely-cli check scene.sply                      # Warnung interactive_without_label je interaktivem Knoten ohne Label
```

`--title` setzt `name`. Mit `--node` bezieht sich `--description` auf den Knoten, ohne `--node` auf die
Szene. Leerer Text (`--label ""`) oder `--hidden false` löscht wieder.

### Was unterstützt wird

- **Wurzel-`<svg>`:** `role="img"` (statischer Export, oder Runtime-Export ohne interaktive Knoten),
  `role="group"` (Runtime-Export mit interaktiven Knoten oder Ansagen), `aria-labelledby` → `<title>`,
  `aria-describedby` → `<desc>`. Die IDs werden kollisionsfrei vergeben.
- **Dekorative Knoten** (`hidden`): `aria-hidden="true"`, nicht fokussierbar.
- **Interaktive Knoten** – Ziele von Zustandsmaschinen-Übergängen auf `click`, `dblclick`, `pointer*`,
  `wheel`, `keydown/keyup`, `focus/blur` (Zeit-Übergänge `after` zählen nicht) sowie von
  `interactions`-Bindungen in der Runtime: `tabindex="0"`, `role="button"` (überschreibbar über
  `a11y.role`), `aria-label` aus `a11y.label`, sonst Knotenname, sonst ID; optional
  `aria-description`. Die Runtime stellt beim Zerstören die ursprünglichen Attribute wieder her.
- **Tastaturaktivierung:** Enter (bei `keydown`, ohne Wiederholung, ohne Modifikatoren) und Leertaste
  (Standardaktion unterdrückt, Auslösung bei `keyup`) auf einem fokussierten Knoten senden dasselbe
  `click`-Ereignis wie der Zeiger (blubbernd, `detail: 0`, Koordinaten = Mitte des Knotens). Dieselben
  Übergänge, `on(...)`-Handler und Aktionen laufen. Tab-Reihenfolge = DOM-Reihenfolge.
- **Sichtbarer Fokusring:** ein SVG-Overlay (`<g data-splinely-focus-ring>`, doppelte Kontur
  Schwarz + Bernstein `#ffbf47`, im `forced-colors`-Modus Systemfarben `Canvas`/`Highlight`), nur bei
  Tastaturfokus (`:focus-visible`, Ersatz: Tastatur-Modalität), per CSSOM/Attributen gesetzt und damit
  CSP-sicher. Ein Mausklick zeigt den Ring nicht.
- **Hover-Entsprechung:** Knoten mit `pointerenter`/`pointerleave`-Übergängen erhalten diese beim
  Tastaturfokus/Verlassen ebenfalls.
- **Live-Region:** nur wenn mindestens ein Zustand `announce` hat: ein unsichtbares
  `<text role="status" aria-live="polite" aria-atomic="true" data-splinely-live>`; der Text wird bei
  jedem Zustandswechsel gesetzt (nicht für den Anfangszustand; bei gleichem Text wird kurz geleert, damit
  er erneut angesagt wird).
- **Reduzierte Bewegung:** `prefers-reduced-motion: reduce` bleibt wie zuvor wirksam (kein
  automatischer Start von Schleifen, keine Überblendungen, keine Zeiger-Folger-Bewegung); Tastatur und
  Ansagen funktionieren dann ebenso.
- **Native Rust-Runtime** (`splinely_runtime`, Editor-Vorschau): die Felder werden validiert und
  durchgereicht, haben aber **keine** Wirkung. Die Semantik ist **nur im Browser** vorhanden.

### Was nicht unterstützt wird

- Interaktionen, die nur in `customJs` (oder per `emit`) verdrahtet sind, erkennt Splinely nicht als
  interaktiv; solche Knoten bekommen weder `tabindex` noch Rolle. Manuell per `a11y.role`/Label lassen
  sie sich nicht fokussierbar machen – diese Beispiele (Cactus Runner, Sand / Accord, Brown Owl, Random
  Figures) erhalten daher nur Beschreibung und ausgeblendete Dekoration (Random Figures zusätzlich
  Ansagen).
- Kein Ziehen/Skalieren/Rad per Tastatur: Aktivierung bedeutet immer nur `click`.
- Keine Tastaturalternative für reine Hover-Effekte außer der Fokus-Entsprechung oben.
- Der Audio-Startknopf des Browser-Hosts (`data-splinely-host-control`) wurde nicht verändert.
- Keine Tastenkürzel, keine Fokusfalle, keine Fokussteuerung von außen; Tab-Reihenfolge ist nicht
  frei einstellbar (kein `tabindex` > 0).
- Der Fokusring-Kontrast über beliebigen Hintergründen ist nicht garantiert (doppelte Kontur
  mildert das, ersetzt aber keine Prüfung).
- `aria-description` wird nicht von allen Browsern/Bildschirmlesern ausgewertet; wichtige Information
  gehört ins Label oder in `announce`.
- Keine Untertitel/Transkripte/Audiodeskription für Audio-Assets; kein Alternativtext je Pfadabschnitt;
  Text in Knoten (`kind: text`) bleibt normaler SVG-Text.

### Nicht geprüft

- **Kein echter Bildschirmleser** (NVDA, JAWS, VoiceOver, Orca, TalkBack) wurde verwendet; ob
  Ansagen, Reihenfolge und Namen dort wie gewünscht klingen, ist unbekannt.
- Keine Prüfung gegen WCAG; **keine Konformitätsaussage** (A/AA/AAA).
- Safari und das WebKitGTK des nativen Viewers sind nicht getestet (nur Playwrights WPE-WebKit).
- Mobile/Touch-Screenreader-Gesten, Sprachsteuerung und Windows-Kontrastmodus auf echter Hardware
  (nur `forced-colors`-Emulation in der Matrix) sind nicht geprüft.
- Kontrast: nur die Textfarben der generierten Beispielseiten wurden rechnerisch geprüft
  (≥ 4,5:1 für Fließtext, Überschriften und Links in `cactus-runner`, `desert-stickfight`,
  `brown-owl`); die Grafik-Inhalte selbst sind nicht auf Kontrast geprüft (`splinely-cli check` warnt
  nur bei Textknoten mit geringem Kontrast).

### Prüfen

```sh
node --test runtime/tests/accessibility.test.cjs       # Fake-DOM: Attribute, Tastatur, Ring, Live-Region, reduzierte Bewegung
cargo test -p splinely_document --test accessibility    # Validierung, Round-Trip JSON/binär, Export
cargo test -p splinely-cli --test a11y_cli              # CLI a11y und check-Warnung
bash scripts/browser-matrix.sh --scenarios accessibility-web-player   # Chromium, Firefox, WebKit
```

Das Matrix-Szenario `accessibility-web-player` (`docs/browser-tests.md`) prüft in echten Browsern
Rollen und Namen, Tab-Reihenfolge, Enter/Leertaste, Fokusring (auch mit `forcedColors`-Emulation),
`aria-live`-Text, `prefers-reduced-motion` und, dass ein Projekt ohne a11y-Daten unverändert bleibt.

Beispiele: `examples/demo.*` ist die einzige mitgelieferte Beispielgrafik mit interaktivem Knoten
(`ball`, mit Label, Beschreibung und `announce` am Zustand `clicked`).

## Editor

Stand: Tastaturbedienung der Zeichenfläche und beschriftete Symbolknöpfe im Desktop-Editor. **Es wurde kein echter Bildschirmleser getestet, und es wird keine WCAG-Konformität behauptet.** Geprüft ist nur, was Unit-Tests und kopflose egui-Tests (`cargo test -p splinely_editor`) abdecken.

### Tastenfolge auf der Zeichenfläche

Die Zeichenfläche ist ein Tastaturziel: **F6** fokussiert sie (auch aus einem Textfeld heraus), ein Mausklick darauf ebenfalls, und mit **Tab** von anderen Bedienelementen erreicht man sie auch. Die Tasten wirken nur im Arbeitsbereich *Zeichenfläche* mit dem Auswahlwerkzeug (nicht in der Vorschau, nicht in offenen Dialogen) und **nie, solange ein Textfeld Fokus hat** – dort bleiben Tab, Pfeile, `?` und Entf. normale Texteingabe. Die Übersicht in der App (**F1** oder **?**, Knopf **?** neben *Datei*, *Datei › Tastenkürzel*) wird aus der Tastenbelegung und der Tabelle der festen Tasten erzeugt und kann deshalb nicht von dieser Liste abweichen; sie ist maßgeblich.

| Taste | Wirkung |
| --- | --- |
| F6 | Zeichenfläche fokussieren |
| Tab / Umschalt+Tab | nächste / vorherige sichtbare, entsperrte Form in Zeichenreihenfolge (hinten → vorn), am Ende Umbruch; eine Gruppe ist eine einzige Station (wählt alle ihre Formen) |
| Enter | Gruppe betreten (Tab läuft dann durch deren direkte Kinder) · bei Text: Texteditor öffnen (wie Doppelklick) |
| Umschalt+Enter | Gruppe verlassen (die Gruppe bleibt ausgewählt) |
| Pfeiltasten | Auswahl um den kleinen Schritt verschieben (Einstellung, Standard 1); mit Umschalt der große Schritt (Standard 10) – dieselben Einstellungen wie bisher |
| Esc | erster Druck: Auswahl (und Gruppenebene) aufheben; zweiter Druck: Zeichenfläche verlassen |
| Entf. / Rücktaste | Auswahl löschen (unverändert) |
| F1 oder ? | Tastenkürzel-Übersicht |

Versteckte, gesperrte und leere Gruppen werden übersprungen; ein leeres Dokument ändert nichts und meldet das in der Statuszeile.

### Fokusring und Statuszeile

- Wer per Tastatur navigiert, sieht um die ausgewählte Form (bzw. Gruppe) einen **schwarz-gelben Ring** (5 px schwarz unter 2,5 px Gelb `#FFD60A`), deutlich dicker als die dünnen Auswahlgriffe. Ohne Auswahl umrahmt er die Zeichenfläche. Nach einem Mausklick ist er verborgen.
- Die **Statuszeile** (unten) nennt bei jedem Tab-Schritt Name, Typ und Position, z. B. „Kreis 1 (Kreis) · Position x 10.0, y 20.0 · 2 von 4“, bei Gruppen Name und Anzahl der Formen; nach einem Verschieben „Verschoben: …“ mit der neuen Position. Die Position ist Anker plus Verschiebung der Form (ohne Drehung/Skalierung).
- Dieselbe Beschreibung steht als egui-`WidgetInfo` an der Zeichenfläche (siehe AccessKit).

### AccessKit-Status: **nicht aktiviert**

Der Editor nutzt `eframe`/`egui` 0.31.1. Dessen Feature `accesskit` zieht `accesskit_winit 0.23` (mit `accesskit_unix` u. a.) nach. Das ist weder in `Cargo.lock` noch im lokalen Cargo-Cache vorhanden; im Cache liegen nur `accesskit 0.21.1` und `accesskit_winit 0.29.2` (für egui 0.33, mit 0.31 nicht kompatibel). Der Versuch in einer Scratch-Kopie scheiterte offline mit `no matching package named accesskit_unix found … required by package accesskit_winit v0.23.0`. Deshalb bleibt das Feature aus, `Cargo.toml`/`Cargo.lock` sind unverändert. Folge: **Der Editor spricht keine Barrierefreiheits-Schnittstelle des Betriebssystems an**; Bildschirmleser sehen die Zeichenfläche und ihre Formen nicht. Beschriftungen setzen wir trotzdem über `Response::widget_info`; egui meldet sie als Ausgabeereignisse (`OutputEvent::FocusGained/Clicked`), aber ohne AccessKit liest sie kein Hilfsprogramm. Aktivierung wäre mit einem Upgrade auf egui ≥ 0.33 (mit Netz) oder dem Mitliefern der Crates möglich und danach mit einem echten Bildschirmleser zu prüfen.

### Beschriftet und nicht beschriftet

Beschriftet (Hover-Text *und* `widget_info`-Name): Ebenen-Panel – Auge und Schloss je Ebene und je Gruppe, Auf-/Zuklappen, eine Ebene nach vorn/hinten, Ebene löschen; Eigenschaften – Farbstopp entfernen, Randstärke −/+; Panels – Löschen-Knöpfe (×) für Schlüsselzeilen, Mausverfolgung und Audio-Cues; Mischraum – Beispiel entfernen; Werkzeugleiste – Knopf **?** (Tastenkürzel). Knöpfe mit Text (Abspielen, Aufnahme, Rechteck, Kreis …) hatten schon eine sichtbare Beschriftung.

**Nicht** per Tastatur bedienbar/beschriftet: Transformationsgriffe (Skalieren/Drehen) und Pfadpunkte/Tangenten bleiben Mausarbeit, nur Verschieben per Pfeiltasten geht; die Zeitleiste (Schlüssel/Rauten, Zeitlineal) und der Zustandsgraph (Knoten und Verbindungen) sind selbst gezeichnet und nur mit der Maus erreichbar, ihre Listen- und Formularfelder sind normale egui-Widgets; Lineale, Hilfslinien, Zeichenwerkzeuge (Pen/Freihand) und Drag-&-Drop sind Mausgesten. Dunkelheits-/Kontrastwerte des Themes wurden nicht gegen Kontrastgrenzen gemessen.

### Ehrliche Grenzen

- Kein echter Bildschirmleser, keine Tastatur-only-Sitzung am echten Fenster, keine IME-/Layout-Tests (`?` wird als getipptes Zeichen erkannt, also layoutunabhängig gedacht, aber nur mit synthetischen Ereignissen geprüft). Dass ein zweiter Esc den Fokus von der Zeichenfläche löst, folgt aus dem egui-Verhalten und ist nicht eigens getestet.
- Die Tab-Abdeckung gilt für Formen auf der Zeichenfläche; der Wechsel zwischen den Panels nutzt egui-Standardfokus (Tab/Umschalt+Tab außerhalb der Zeichenfläche), dessen Reihenfolge nicht angepasst wurde. Eine Fokusfalle gibt es nicht: Esc, Esc verlässt die Zeichenfläche.
- Übersetzungen der neuen Texte (en/es/fr/ja/zh) wurden nicht von Muttersprachlern geprüft.
