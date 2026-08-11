# Plan 06: Desktop Product und Nutzerworkflow

Ziel: Die technischen Fähigkeiten werden zu einer schnellen, verständlichen und zugänglichen A^3-Desktopanwendung.

Relevante ADRs: 0001, 0002, 0012, 0014

## U1 Information Architecture

Abhängigkeiten: Gate M7

Hauptbereiche:

- Projects
- Map
- Agent
- Settings

- [ ] Routen und Navigationszustand
- [ ] globaler Projekt-, Index-, Modell- und Runstatus
- [ ] leere, ladende, fehlgeschlagene und offline Zustände
- [ ] keine Fachlogik im Frontend
- [ ] Keyboard-first-Navigation

Akzeptanz:

- Nutzer erkennt stets aktives Projekt, Snapshot, Run und Modellzustand;
- Reload der WebView verliert keinen fachlichen Zustand;
- Navigation funktioniert ohne Maus.

## U2 Projects

Abhängigkeiten: Project Open

- [x] Repository öffnen
- [x] zuletzt verwendete Projekte
- [x] Worktree und Branch sichtbar
- [x] Indexstatus und letzter Snapshot
- [x] Storagegröße und Rebuild
- [x] Projekt sicher entfernen, ohne Repository zu löschen

Akzeptanz:

- „Projekt entfernen“ löscht nie Quellcode;
- Rebuild erklärt, welche Daten regenerierbar sind;
- ungültiger Pfad erhält konkrete Recovery.

Verifiziert am 2026-08-11: Der native Ordnerdialog öffnet ausschließlich einen validierten
Git-Worktree-Root; der Core hält die aktive Identität über WebView-Reloads und projiziert Branch,
Worktree, bounded Index-/Snapshotstatus sowie die verlustfreie private Storagegröße. Recent Projects
sind auf zehn validierte Einträge begrenzt. Der besitzende Indexmanager serialisiert Refresh und
einen cancellable Rebuild, der nur regenerierbare Projektionen entfernt und anschließend einen
vollständigen Rescan anfordert.

`remove_project` nimmt keine Identität und keinen Pfad aus der WebView an. Der zweistufig bestätigte
Use Case beendet Watcher und laufende Indexarbeit, entfernt atomar nur den exakten Recent-Eintrag
sowie offene Reconciliation-Absichten und behält Repository, Linked Worktrees, stabile `ProjectId`
und `projects/<WorktreeId>/knowledge.db`. Adapter-Contracts belegen Erhalt und Wiederöffnung. Die UI
validiert `CommandErrorV1` streng und ordnet nur stabile Codes konkreten Recovery-Schritten zu;
rohe Pfade und Adapterdetails bleiben unsichtbar.

Rustfmt, Workspace-Tests mit allen Features, Workspace-Clippy über alle Targets/Features mit
`-D warnings`, Rustdoc, 38 Frontendtests, Formatter, ESLint, Svelte-Typecheck, Produktionsbuild,
Tooltests, 47 Markdown-Dateien mit 74 lokalen Links und der Dependency-/Lizenzbericht sind grün.
Der vollständige Linux-`quality`-Job bestand lokal über
`act -j quality --pull=false -P ubuntu-22.04=a3-act-medium-rust:latest`. Der Windows-libSQL-Harness
isoliert nun auch Project-Catalog- und Knowledge-Contracts pro Test, wiederholt ausschließlich
`0xc0000005` höchstens zweimal und verlangt den Abschlussmarker nach der letzten Assertion.

## U3 Index Experience

Abhängigkeiten: Fast Index, Deep Map

- [x] Fast-Index-Fortschritt nach Phasen
- [x] Dateien, Symbole, Diagnostics und Coverage
- [x] Deep Map bewusst starten, pausieren, fortsetzen und abbrechen
- [x] Token-, Zeit- und Modellbudget vor Start
- [ ] stale und NeedsReview sichtbar
- [x] Indexfehler pro Datei statt globalem Blank State

Akzeptanz:

- UI bleibt während Cold Index responsiv;
- Nutzer kann mit veröffentlichtem alten Snapshot lesen, während neuer gebaut wird;
- Deep Map startet nicht unbemerkt GPU-lastig.

Teilabnahme vom 2026-08-11: Der pfadlose Fast-Index-Status projiziert die sechs festen Phasen aus
dem Core-eigenen in-memory Read-Model. Die getrennte begrenzte Übersicht hält den letzten
atomar veröffentlichten Snapshot während eines neuen Laufs lesbar und zeigt Dateien, Symbole,
bytegewichtete Coverage sowie sichere file-lokale Diagnostics statt eines globalen Blank States.

Deep Map besitzt nun einen expliziten, ebenfalls pfadlosen Produkt-Lifecycle. Vor dem Start zeigt
die UI ausschließlich ein durch Capability Probe verifiziertes Mapping-Profil samt Context- und
Outputlimit sowie Token-, Zeit- und Read-only-Toolbudget. Ohne später in U8 konfigurierten Executor
bleibt die Funktion sichtbar `unavailable` und startet keine Modell- oder GPU-Arbeit. Start,
Pause, Resume und Cancel laufen über den besessenen Scheduler; `Paused` wird erst nach terminaler
kooperativer Cancellation und einem plan-, snapshot- und budgetgebundenen Checkpoint sichtbar.
Resume erzeugt einen neuen Versuch ohne bestätigte Schritte zu wiederholen. Queued Work kann keinen
Pause-Checkpoint vortäuschen, Projektwechsel verwirft alten Zustand, und ein anderes
Checkpointbudget endet sicher als Fehler.

Verifiziert wurden Rustfmt, 28 Desktoptests einschließlich fünf Lifecycle-Randfällen, sämtliche
Workspace-Tests mit allen Features, Workspace-Clippy mit Warnings denied, Rustdoc, 50
Frontendtests, Formatter, ESLint, Svelte-Typecheck, Produktionsbuild, Tooltests, 47
Markdown-Dateien mit 74 lokalen Links, Dependency-/Lizenzbericht und der native
Tauri-Release-Build ohne Bundle. Offen innerhalb U3 bleibt ausschließlich die sichtbare
Stale-/NeedsReview-Projektion.

## U4 Project Map

Abhängigkeiten: Gate M5

- [ ] Repository- und Modulbaum
- [ ] Abhängigkeitsgraph mit begrenzter Knotenzahl
- [ ] Entry Points, Tests und Runtime Flows
- [ ] Module Card
- [ ] Evidence Inspector
- [ ] Confidence, Coverage und Freshness
- [ ] Suche und Task-Lens-Umschaltung

Akzeptanz:

- jede Aussage kann zu Evidence navigieren;
- Hypothesen sind visuell klar von Facts getrennt;
- große Projekte zeigen progressive Details statt unbrauchbarem Vollgraph.

## U5 Agent Workspace

Abhängigkeiten: Gate M7

- [ ] Goal Contract
- [ ] Acceptance Criteria
- [ ] Task Ledger und aktueller Step
- [ ] kompakte Conversation- und Action Timeline
- [ ] Context- und Budgetanzeige
- [ ] Pause, Cancel, Resume und Replan
- [ ] Fehler und Blocker

Akzeptanz:

- Ziel und aktueller Step bleiben stets sichtbar;
- Textantwort und echte Ausführungsaktion sind unterscheidbar;
- Cancel ist erreichbar und zeigt Abschlusszustand.

## U6 Diff und Verification

Abhängigkeiten: Patch und Verification Engine

- [ ] Datei- und Hunkübersicht
- [ ] Side-by-side und unified Diff
- [ ] Useränderungs- und Agentenänderungsmarkierung, soweit zuverlässig
- [ ] Test-, Build- und Diagnostic-Ergebnisse
- [ ] Evidence zu Step und Acceptance
- [ ] stale Verification sichtbar

Akzeptanz:

- Nutzer kann vor Freigabe genaue Pfade und Änderungen sehen;
- verkürzte Logs sind als verkürzt markiert und gezielt nachladbar;
- Done zeigt die Beweise pro Muss-Kriterium.

## U7 Approval Center

Abhängigkeiten: Policy Engine

- [ ] Aktion, Risiko, Scope und Begründung
- [ ] genaue ProcessSpec oder Dateipfade
- [ ] einmal erlauben, scopegebunden erlauben oder ablehnen
- [ ] Ablauf und Widerruf
- [ ] kein manipulativer Default

Akzeptanz:

- Zustimmung ist informiert und spezifisch;
- Ablehnung führt zu Replan oder sauberem Blocked;
- geheime Werte werden nicht angezeigt.

## U8 Settings und Model Health

Abhängigkeiten: ModelProfile

- [ ] lokaler Endpoint
- [ ] Provider Health und Capability Probe
- [ ] Modellprofile für Coding, Mapping und Embedding
- [ ] Kontext- und Ressourcenlimits
- [ ] Indexignore und sichere Command Allowlist
- [ ] Daten- und Privacy-Einstellungen

Akzeptanz:

- App ist ohne konfiguriertes Modell als Indexbrowser nutzbar;
- nicht lokaler Endpoint warnt deutlich;
- invalides Profil kann nicht für ausführbare Runs aktiviert werden.

## U9 Design System und Accessibility

- [ ] Farb-, Typografie-, Spacing- und Focus-Tokens
- [ ] Light und Dark Theme
- [ ] WCAG-konforme Kontraste
- [ ] Screenreader-Labels
- [ ] Reduced Motion
- [ ] skalierbare Schrift
- [ ] keine Information nur über Farbe

Akzeptanz:

- automatisierte Accessibilitychecks ohne kritische Befunde;
- Kernworkflow vollständig per Tastatur;
- 200-Prozent-Zoom ohne Funktionsverlust.

## U10 Frontend Performance

- [ ] große Editor- und Graphmodule lazy laden
- [ ] Listen virtualisieren
- [ ] paginierte IPC-Queries
- [ ] Eventbatching
- [ ] Renderingprofil für Indexburst
- [ ] Memory-Leak-Test bei Projektwechseln

Akzeptanz:

- UI-Blockade und Idle-RAM innerhalb QUALITY_GATES;
- Graph mit großem Repository rendert nur begrenzte Subsets;
- Projektwechsel lässt alte Listener und Buffers frei.

## Gate M8

- [ ] vollständiger Nutzerworkflow vom Open bis Done
- [ ] Accessibilitygate
- [ ] keine generische privilegierte Frontendcapability
- [ ] Performancebudget gemessen
- [ ] Fehler-, Offline- und Recoveryzustände vorhanden
- [ ] UX-Smoke auf Windows, Linux und macOS
