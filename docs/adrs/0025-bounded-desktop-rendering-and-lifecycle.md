# ADR-0025: Begrenztes Desktop-Rendering und projektgebundener UI-Lebenszyklus

Status: Accepted

Datum: 2026-08-13

Entscheider: Tim Bornemann

## Kontext

Plan 06/U10 verlangt, dass A^3 auch während eines Indexbursts reaktionsfähig bleibt, große
Repositories nur in begrenzten Ausschnitten rendert und bei einem Projektwechsel keine alten
Listener oder Buffer behält. Die bestehende Core-Grenze liefert Repository- und Modulbäume bereits
cursorpaginiert sowie Modulgraphen, Agentenaktivität und Inspektionsdaten mit festen Obergrenzen.
Die WebView konnte Seiten jedoch kumulativ und damit ohne eigene Retentiongrenze anhängen. Ein
Diff-Hunk durfte außerdem bis zu 128.000 streng validierte Zeilen auf einmal in den DOM abbilden.

Agent Workspace, Settings, Inspektor, Approval Center und Graphdarstellung wurden bisher zusammen
mit der initialen Shell gebündelt. Der 500-Millisekunden-Statuspoller besaß zwar einen
Unmount-Cleanup, verhinderte aber weder überlappende Abfragen noch späte UI-Commits aus einem zuvor
aktiven Projekt. Diese Präsentationsprobleme rechtfertigen keine breiteren IPC-Payloads, keine
zweite fachliche Zustandsquelle und keine Verlagerung von Core-Invarianten in das Frontend.

## Entscheidung

- Die initiale WebView lädt nur die interaktive Produktshell und unmittelbar sichtbare
  Kernprojektionen. Agent Workspace, Settings, Inspektor, Approval Center und die eigentliche
  Graphdarstellung werden als getrennte Vite-Chunks erst bei Sichtbarkeit oder einer ausdrücklichen
  Nutzeraktion importiert. Jeder Lazy-Zustand besitzt eine zugängliche Lade-, Fehler- und
  Wiederholungsdarstellung; ein Importfehler darf keine leere Fläche erzeugen.
- Repository- und Modulbaum behalten jeweils höchstens eine serverseitig begrenzte Seite im
  Renderzustand. Vorwärts- und Rückwärtsnavigation verwendet nur validierte Cursorhistorie des
  aktuellen Verzeichnisses beziehungsweise Elternmoduls. Ein Wechsel der Ebene, Publikation oder
  des Projekts verwirft diese Cursorhistorie. Das Frontend konstruiert keine ungebundene
  Vollbaumprojektion.
- Große gleichförmige Zeilenlisten werden gefenstert. V1 virtualisiert insbesondere Diff-Zeilen
  mit fester gemessener Zeilenhöhe, kleinem Overscan, Gesamtzeilenangabe und korrekten
  `aria-rowindex`-/`aria-rowcount`-Metadaten. Nur der sichtbare Ausschnitt plus Overscan liegt im
  DOM; fachliche Daten bleiben unverändert und begrenzt.
- Poll- und Eventbursts laufen durch genau einen vom App-Mount besessenen UI-Scheduler. Er erlaubt
  je Pollquelle höchstens eine laufende Abfrage, merkt höchstens eine weitere Ausführung vor und
  fasst gleichartige Rendercommits innerhalb eines Animation Frames nach dem Latest-Wins-Prinzip
  zusammen. Die WebView interpretiert diese Präsentationsbatchinggrenze nicht als fachliches
  Ereignisjournal.
- Der Scheduler besitzt eine monotone Projektgeneration. Jede asynchrone projektbezogene Abfrage
  bindet ihren Commit an die beobachtete Generation. Projektwechsel und Unmount erhöhen die
  Generation, verwerfen ausstehende Renderbuffer und rufen alle zugehörigen Listener-, Timer- und
  Beobachter-Cleanups genau einmal auf. Core-eigene Inspection- und Approval-Buffer bleiben gemäß
  ADR-0021 und ADR-0022 zusätzlich im Composition Root an denselben Projektlebenszyklus gebunden.
- Performanceaussagen verwenden reproduzierbare Messungen. Ein synthetischer Indexburst prüft
  Batching, DOM-Obergrenze und die 100-Millisekunden-Blockadegrenze. Buildartefakte dokumentieren
  initiale und lazy Chunks. Idle-RAM wird ohne Modellserver an einem nativen Desktopprozess auf der
  dokumentierten Messmaschine erfasst und getrennt von den deterministischen Unit-Verträgen
  ausgewiesen.
- U10 erweitert weder privilegierte Tauri-Capabilities noch persistente Schemas. Es gibt keine
  WebView-Dateisystem-, SQL-, Prozess- oder Providergrenze und keine Hintergrund-Netzwerkaktivität.

## Konsequenzen

### Positiv

- Große Inspektionsdaten erzeugen nur eine kleine konstante DOM-Menge; Baumdaten wachsen nicht mit
  der Anzahl besuchter Folgeseiten.
- Nicht sichtbare Produktflächen belasten Startzeit und initiales JavaScript nicht.
- Ein langsamer Poll kann keinen unbegrenzten Request- oder Rendersturm auslösen, und ein
  Projektwechsel trennt alte asynchrone Arbeit sichtbar von der neuen Projektion.
- Die vorhandenen Core-, Evidence- und IPC-Autoritäten bleiben unverändert.

### Negativ

- Der erste Aufruf einer großen Fläche benötigt einen zusätzlichen lokalen Chunkimport.
- Virtualisierte Diff-Zeilen haben in V1 eine feste einzeilige Darstellung und horizontalen
  Overflow statt beliebig hoher umgebrochener Zeilen.
- Rückwärtsnavigation in Bäumen hält eine kleine Cursorhistorie, aber keine bereits gerenderten
  Seiteninhalte.

### Risiken und Gegenmaßnahmen

- Ein Chunk kann nicht geladen werden — die Fläche zeigt einen benannten Fehler mit explizitem
  Retry und lässt die übrige Shell bedienbar.
- Fensterung kann Screenreader-Kontext verlieren — Gesamtzahl, absolute Zeilenindizes und ein
  sichtbarer Bereichshinweis begleiten die native Scrollregion.
- Ein später Promise-Callback schreibt in das neue Projekt — jeder projektbezogene Commit prüft
  die Schedulergeneration; Verträge simulieren Wechsel bei ausstehendem Poll und ausstehendem
  Renderframe.
- Cursorhistorie passt nicht mehr zur Publikation — Indexlauf, Snapshot, Parent und Requestcursor
  werden wie bisher streng geprüft; jede Inkompatibilität verwirft Seite und Historie.
- Synthetische jsdom-Zeit wird als reale Browserleistung missverstanden — der Unit-Test belegt nur
  Algorithmus und DOM-Grenze; ein realer Browserprofil-Lauf misst zusätzlich Long Tasks und
  Interaktionslatenz.

## Verworfene Alternativen

- Alle Daten laden und nur per CSS verstecken — reduziert weder DOM- noch Retentionkosten.
- Eine Frontend-Vollbaum- oder Vollgraphkopie — widerspricht den begrenzten, evidenzgebundenen
  Core-Projektionen und wächst mit dem Repository.
- Polling durch ungebremste Tauri-Events ersetzen — Events brauchen dieselbe Ownership- und
  Batchinggrenze und würden die IPC-Oberfläche ohne fachlichen Bedarf verbreitern.
- Einen Worker oder Node-Hintergrundprozess für UI-Rendering ausliefern — widerspricht ADR-0002 und
  löst DOM-Kosten nicht.
- Nur Bundlegröße als Performancebeweis verwenden — belegt weder Reaktionszeit noch RAM oder
  Cleanup.

## Compliance

- Unit-Verträge prüfen Latest-Wins-Batching, höchstens einen laufenden Poll, genau einen
  vorgemerkten Poll, Generationstrennung sowie idempotenten Listener-/Timer-Cleanup.
- Component-Tests prüfen eine konstante Obergrenze gerenderter Diff-Zeilen, absolute ARIA-Indizes,
  Scrollfensterung und den Wechsel zwischen vorwärts und rückwärts paginierten Baumseiten.
- Decoder- und Integrationsverträge behalten die bestehenden Cursor-, Payload- und Graphgrenzen;
  der Graph rendert höchstens den ausdrücklich angeforderten begrenzten Ausschnitt.
- Produktionsbuild und Browserprofil weisen getrennte Lazy-Chunks, Indexburst-Blockaden und die
  Anzahl gerenderter Knoten aus. Die native RAM-Messung dokumentiert Prozess, Plattform,
  Warm-up, Modellserverausschluss und Peak-/Idle-Wert.

## Referenzen

- [ADR-0002](0002-tauri-rust-svelte-desktop.md)
- [ADR-0014](0014-cross-platform-release-and-quality.md)
- [ADR-0021](0021-bounded-agent-inspection.md)
- [ADR-0022](0022-task-bound-approval-center.md)
- [ADR-0024](0024-semantic-design-tokens-and-accessible-themes.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Quality Gates](../QUALITY_GATES.md)
- [Desktop Product U10](../plans/06-DESKTOP_PRODUCT.md#u10-frontend-performance)
