# ADR-0024: Semantische Designtokens und zugängliche Desktop-Themes

Status: Accepted

Datum: 2026-08-13

Entscheider: Tim Bornemann

## Kontext

Plan 06/U9 verlangt ein konsistentes Designsystem, Light und Dark Theme, WCAG-nahe Kontraste,
Tastatur- und Screenreader-Bedienbarkeit, Reduced Motion und skalierbare Schrift. Die bisherige
Desktopoberfläche enthielt viele komponentenlokale Rohfarben und einzelne Fallbackpaletten. Eine
globale Farbumschaltung konnte diese gekapselten Svelte-Styles nicht zuverlässig erfassen und
erzeugte im Dark Theme helle Flächen mit hellem Text.

ADR-0023 macht den lokale Katalog zur einzigen dauerhaften Settings-Autorität. Eine zweite
WebView-Persistenz nur für Darstellung würde nach Reload oder Appneustart einen unabhängigen,
nicht revisionsgebundenen Zustand schaffen. U9 benötigt außerdem keine privilegierte Capability
und darf die bestehende IPC-Grenze nicht verbreitern.

## Entscheidung

- Eine zentrale CSS-Datei definiert semantische V1-Tokens für Farbe, Typografie, Spacing,
  Radien, Mindestgröße interaktiver Controls und Fokus. Komponenten dürfen keine Rohfarben oder
  voneinander abweichende Farb-Fallbacks besitzen.
- Light und Dark verwenden dieselben semantischen Tokennamen. `prefers-color-scheme` bestimmt den
  Systemmodus; die expliziten Modi `light` und `dark` setzen ausschließlich ein `data-theme` am
  Dokumentroot.
- Die Theme-Wahl bleibt in U9 flüchtiger Präsentationszustand. Es gibt weder `localStorage` noch
  Cookie, Repositorydatei oder einen neuen IPC-Settingspfad. Eine spätere dauerhafte Darstellung
  muss den revisionsgebundenen Settings-Snapshot aus ADR-0023 erweitern.
- Normale Text-/Flächenpaare der semantischen Palette müssen in beiden Themes mindestens WCAG AA
  4,5:1 erreichen. Information benötigt zusätzlich Text, Icon, ARIA-Zustand oder strukturelle
  Semantik und darf nie nur durch Farbe vermittelt werden.
- Interaktionen verwenden native Links, Buttons, Inputs, Selects, Textareas, Details oder
  Fieldsets. ARIA ergänzt native Semantik, ersetzt sie nicht. Toggle-Gruppen verwenden sichtbare
  Beschriftung und `aria-pressed`; Livezustände verwenden die bereits vorhandenen begrenzten
  Status- und Alertregionen.
- Alle fokussierbaren Elemente erhalten einen sichtbaren, tokenisierten `:focus-visible`-Ring.
  Der primäre Inhalt ist über einen Skip-Link erreichbar. Interaktive Controls sind mindestens 44
  CSS-Pixel hoch und die DOM-Reihenfolge bleibt die Tastaturreihenfolge; positive `tabindex`-Werte
  sind verboten.
- Textgrößen und Abstände verwenden relative Einheiten. Die Kernoberfläche muss bei einer auf die
  Hälfte reduzierten CSS-Viewportbreite als reproduzierbares 200-Prozent-Reflow-Äquivalent ohne
  horizontale Dokumentüberbreite bedienbar bleiben.
- `prefers-reduced-motion: reduce` deaktiviert Smooth Scrolling und reduziert Animationen und
  Transitions auf einen einzelnen praktisch sofortigen Durchlauf. Kritische Zustandsänderungen
  dürfen nicht von Bewegung abhängen.

## Konsequenzen

### Positiv

- Gekapselte Komponenten bleiben in beiden Themes kontraststark und können keine unabhängige
  Palette mehr einschleusen.
- Theme, Fokus, Touch-Ziele und Reflow lassen sich deterministisch ohne Backend- oder
  Privilegienänderung testen.
- Der Systemmodus folgt der OS-Präferenz, ohne eine zweite dauerhafte Settingsquelle zu schaffen.

### Negativ

- Die explizite Theme-Auswahl fällt nach einem Reload auf System zurück.
- Neue visuelle Zustände benötigen zuerst einen passenden semantischen Token statt einer lokalen
  Farbe.
- Automatisierte Kontrast- und DOM-Verträge ersetzen nicht die Plattform-Smokes der nativen
  WebViews.

### Risiken und Gegenmaßnahmen

- Eine scoped Komponente führt erneut eine Rohfarbe ein — der Designsystemtest extrahiert jeden
  Svelte-Styleblock und lehnt Hex-, RGB- und benannte Rohfarben ab.
- Ein kontrastarmes Token erreicht mehrere Komponenten — der Gate-Test berechnet die relativen
  Luminanzen aller normativen Text-/Flächenpaare für Light und Dark.
- Visuelle Zoomfehler bleiben in jsdom unsichtbar — U9 verlangt zusätzlich einen Browser-Smoke
  mit halbierter CSS-Viewportbreite, gemessener Überbreite und visueller Kontrolle.
- ARIA wird ohne vollständiges Widgetverhalten eingesetzt — native Controls und einfache
  Toggle-Gruppen sind Standard; komplexere Widgets benötigen einen eigenen Keyboard-Contract.

## Verworfene Alternativen

- Komponentenlokale Light-/Dark-Paletten — driften auseinander und ließen den gefundenen
  Kontrastfehler zu.
- Theme in `localStorage` speichern — erzeugt eine zweite, nicht revisionsgebundene
  Settingsautorität außerhalb ADR-0023.
- Theme im Rust-Core als neue IPC-Capability — U9 braucht keine privilegierte Grenze; eine spätere
  Persistenz gehört in den bestehenden Settings-Snapshot.
- Nur Systemtheme ohne explizite Wahl — erfüllt den U9-Produktplan nicht.
- Nichtnative klickbare Container — verschlechtern Tastatur- und Screenreader-Semantik.

## Compliance

- Token-Tests prüfen vollständige Light-/Dark-Semantik, WCAG-AA-Kontrast, skalierbare
  Schriftgrößen, Reduced Motion und das Verbot komponentenlokaler Rohfarben.
- Component-Tests prüfen benannte native Controls, eindeutige IDs, lückenlose Überschriften,
  Skip-Link, keine positiven `tabindex`-Werte und textuell beziehungsweise per ARIA erkennbare
  Auswahlzustände.
- Der Browser-Smoke prüft beide Themes, sichtbaren 3-Pixel-Fokus, 44-Pixel-Controls und bei
  halbierter CSS-Viewportbreite keine horizontale Dokumentüberbreite.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0014](0014-cross-platform-release-and-quality.md)
- [ADR-0023](0023-local-settings-and-model-activation.md)
- [Produktanforderungen](../PRODUCT_REQUIREMENTS.md)
- [Quality Gates](../QUALITY_GATES.md)
- [Desktop Product U9](../plans/06-DESKTOP_PRODUCT.md#u9-design-system-und-accessibility)
