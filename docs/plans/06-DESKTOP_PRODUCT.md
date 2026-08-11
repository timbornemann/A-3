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
- [x] stale und NeedsReview sichtbar
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

Bei dieser Teilabnahme wurden Rustfmt, 28 Desktoptests einschließlich fünf Lifecycle-Randfällen,
sowie sämtliche
Workspace-Tests mit allen Features, Workspace-Clippy mit Warnings denied, Rustdoc, 50
Frontendtests, Formatter, ESLint, Svelte-Typecheck, Produktionsbuild, Tooltests, 47
Markdown-Dateien mit 74 lokalen Links, Dependency-/Lizenzbericht und der native
Tauri-Release-Build ohne Bundle.

Abnahme vom 2026-08-11: Der letzte U3-Schnitt projiziert die Lebenszyklen der jeweils neuesten
Module Card pro Modul gegen den aktuellen atomar veröffentlichten Run. `Current`, `Stale` und
`NeedsReview` werden mit exakten verlustfreien Zählern und höchstens fünf typisierten Ursachen
sichtbar. Storage-Contracts belegen direkte und ein-Hop-Invalidierung, entfernte Module ohne
Remapauftrag, das Verdrängen historischer stale Cards nach Neupublikation und die Ablehnung einer
fehlenden Lifecycle-Zeile. Der pfadlose IPC- und TypeScript-Contract lehnt unbekannte Felder,
inkonsistente Summen, falsche Reihenfolge und illegale Status/Ursachen-Paare ab; die Storage-Abfrage
läuft nicht im 500-ms-Polling.

Der Abschlussnachweis umfasst Rustfmt, sämtliche Workspace-Tests mit allen Features, Workspace-
Clippy über alle Targets und Features mit Warnings denied, Rustdoc, 30 Desktop-Libtests plus zwei
Desktop-Binary- und drei Desktop-Integrationstests, 54 Frontendtests, Formatter, ESLint,
Svelte-Typecheck, Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74 lokalen Links,
Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle. Damit ist U3 vollständig
verifiziert und abgeschlossen.

## U4 Project Map

Abhängigkeiten: Gate M5

- [x] Repository- und Modulbaum
- [x] Abhängigkeitsgraph mit begrenzter Knotenzahl
- [x] Entry Points, Tests und Runtime Flows
- [x] Module Card
- [x] Evidence Inspector
- [x] Confidence, Coverage und Freshness
- [x] Suche und Task-Lens-Umschaltung

Akzeptanz:

- jede Aussage kann zu Evidence navigieren;
- Hypothesen sind visuell klar von Facts getrennt;
- große Projekte zeigen progressive Details statt unbrauchbarem Vollgraph.

Abnahme Repository- und Modulbaum vom 2026-08-11: Der Repository-Baum liest Root und
Unterverzeichnisse progressiv aus genau einer atomaren Indexpublikation. Jede Seite enthält
höchstens 100 direkte, verlustfrei byteweise geordnete Kinder; Dateien tragen den exakten
veröffentlichten ContentHash, Verzeichnisse eine exakte Nachfahren-Dateizahl. Der Modulbaum liest
aus derselben Publikationsgrenze ausschließlich deterministische primäre Manifest- und
Pfadgrenzen. Root und nächster primärer Nachfahre bilden eine echte Hierarchie; transitive
Nachfahren und Graph-Communities erscheinen nicht als Kinder. Die exakte Community-Anzahl bleibt
als getrenntes Zusatzsignal sichtbar.

Jeder Modulbaumknoten trägt aktuelle repräsentative und gegebenenfalls Manifest-`FileRevision`-
Evidence, exakte Manifest-/Datei-/Symbolzahlen sowie bounded Central-/Entrypoint-/Testzahlen mit
expliziter Trunkierung. Eine historische Publikation ohne V8-Projektionsmarker bleibt als
`projectionUnavailable` von einer gültigen leeren Projektion unterscheidbar. Beide Bäume verwenden
stabile exklusive Cursor, Cancellation und ein Zwei-Sekunden-Limit. Folgeseiten werden nur mit
identischer Run-, Snapshot-, Eltern-/Verzeichnis- und Cursorbindung angehängt. Opake Hex-Tokens und
stabile Modul-IDs bleiben von sicheren Anzeigen getrennt und gewähren weder Live-Dateisystem-,
Source-, Projekt- noch Pfadautorität; kein Baumread läuft im 500-ms-Statuspolling.

Der Abschlussnachweis umfasst Rustfmt, sämtliche Workspace-Tests mit allen Features,
Workspace-Clippy über alle Targets und Features mit Warnings denied, Rustdoc, 34
Desktop-Libtests plus zwei Desktop-Binary- und drei Desktop-Integrationstests, 69 Frontendtests,
Formatter, ESLint, Svelte-Typecheck, Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74
lokalen Links, Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle.

Abnahme Abhängigkeitsgraph vom 2026-08-11: Von einem explizit ausgewählten aktuellen primären
Manifest- oder Pfadmodul liest A^3 ausschließlich die direkte relationsspezifische Nachbarschaft
des jüngsten atomar publizierten V8-Stands. Graph-Communities, `Contains` und `Defines` werden nicht
als Abhängigkeitsknoten oder -kanten ausgegeben. Symbolendpunkte benötigen genau eine primäre
Membership; Dateiendpunkte werden nur bei einer über alle strukturellen Symbole eindeutigen
Primärmodulzuordnung verwendet. Fehlende Dateizuordnungen bleiben als eigener Zähler sichtbar,
widersprüchliche Projektionen werden abgelehnt.

Das Gesamtknotenlimit liegt bei 1 bis 100 einschließlich Zentrum. Nachbarn werden nach der im
kanonischen Präfix beobachteten Evidencezahl und stabiler `ModuleId` gewählt; maximal 256
relationsspezifische Gruppen werden angezeigt. Der Adapter inspiziert höchstens 4.096 nach
`edge_sequence` geordnete zentrumsinzidente Kanten. Knoten-, Gruppen- und Quelltrunkierung,
beobachtete Nachbar-/Gruppenzahlen sowie ungemappte Kanten sind getrennt sichtbar. Ein real
publiziertes 4.097-Kanten-Contract-Fixture belegt die Quell- und Gruppengrenzen.

Jede sichtbare Gruppe trägt eine vollständige aktuelle repräsentative `GraphEdge` mit stabiler
Evidence-ID, Revision, Range, Provider, Confidence und Link-Resolution; die UI kann diese Evidence
ohne Source- oder Dateisystemzugriff lokal aufklappen. Der streng versionierte Rust-/TypeScript-
IPC-Vertrag akzeptiert nur Zentrum-ID und Knotenlimit aus der WebView. Der Read ist cancellable,
auf zwei Sekunden begrenzt und läuft nur nach Auswahl, Aktualisierung oder erfolgreichem Publish,
nie im 500-ms-Statuspolling.

Der Abschlussnachweis umfasst Rustfmt, sämtliche Workspace-Tests mit allen Features,
Workspace-Clippy über alle Targets und Features mit Warnings denied, Rustdoc, 36
Desktop-Libtests plus zwei Desktop-Binary- und drei Desktop-Integrationstests, 76 Frontendtests,
Formatter, ESLint, Svelte-Typecheck, Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74
lokalen Links, Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle.

Abnahme Entry Points, Tests und Runtime Flows vom 2026-08-11: Für ein explizit ausgewähltes
aktuelles Primärmodul liest A^3 Entry Points und Tests gemeinsam aus genau einer kurzen
Transaktion des jüngsten atomar publizierten V8-Stands. Featurezeile, lückenloser Rang,
`SymbolRole`, aktuelle Symbolrevision und genau eine primäre Membership müssen übereinstimmen;
Communities sind keine auswählbaren Roots. Gespeicherte Anzahl, angefordertes Präfix und bereits
bei der Modulbildung entstandene Trunkierung bleiben getrennte Signale. Die UI startet mit je 20
Roots und erweitert nur auf explizite Anforderung bis zur festen Grenze von 256.

Ein Flow entsteht erst nach Rootauswahl und verwendet ausschließlich den bestehenden
R3-Graphadapter. Entry Points erlauben höchstens zwei ausgehende `Calls`-Hops, Tests genau eine
direkte ausgehende `Tests`-Kante; Relation, Richtung und Tiefe sind über IPC nicht frei wählbar.
Run, Snapshot, Primärmodul, Rolle und Root werden unmittelbar vor der Traversierung erneut
validiert und danach mit dem Graphresultat verglichen. Ein Replacement-Publish liefert
`publicationChanged`; stale Roots und ihre Evidence werden in der UI sofort ausgeblendet, statt
mit einem neuen Snapshot vermischt zu werden.

Jeder sichtbare Root und jedes eindeutige Ziel führt zu exakter aktueller Symbol- oder
Datei-Evidence; jeder Flow zeigt den vollständigen kürzesten Kantenpfad mit Revision, Range,
Provider, Confidence und Resolution. Die Oberfläche bezeichnet diese Daten ausdrücklich als
strukturelle Beobachtung, nicht als ausgeführte Laufzeitspur oder bewiesene Tatsachenbehauptung.
Progressive Rootpräfixe, der explizite Flow-Klick, höchstens 100 Ziele und 4.096 inspizierte Kanten
ersetzen einen unbrauchbaren Vollgraph. Kein Runtime-Read läuft im 500-ms-Statuspolling und die
WebView erhält weder Source-, Datei-, Shell-, SQL-, Provider- noch generische Graphbefugnisse.

Der Abschlussnachweis umfasst Rustfmt, sämtliche Workspace-Tests mit allen Features,
Workspace-Clippy über alle Targets und Features mit Warnings denied, Rustdoc, 38
Desktop-Libtests plus zwei Desktop-Binary- und drei Desktop-Integrationstests, 83 Frontendtests,
Formatter, ESLint, Svelte-Typecheck, Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74
lokalen Links, Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle. Der
Windows-libSQL-Contract mit 23 isolierten Fällen bestand nach der begrenzten Teardown-
Stabilisierung zweimal vollständig sowie im grünen Workspace-Gesamtlauf. Die lokale visuelle
Prüfung bestätigte Desktop- und 720-px-Layout ohne Horizontaloverflow oder Browser-Consolefehler.

Abnahme Module Card vom 2026-08-11: Für ein explizit ausgewähltes aktuelles Primärmodul liest A^3
die deterministisch jüngste dauerhafte Card gemeinsam mit der jüngsten atomaren Indexpublikation
in genau einer kurzen Deferred-Transaktion. Aktueller Run/Snapshot und historischer
Quell-Run/Snapshot bleiben getrennt. Die Auswahl folgt R11; V1-Schema, Mapperprofil, lückenlose
Wertindizes, Feldgrenzen, Gesamttext, eindeutige Claims, kanonische Evidence-IDs und
Claim-Evidence-Teilmengen werden an Storage-, Application-, IPC- und TypeScript-Grenze erneut
validiert. Cancellation und das feste Zwei-Sekunden-Limit schließen den Read kontrolliert ab.

Claim-Typ (`Fact`, `Observation`, `Hypothesis`), Confidence und effektiver Lifecycle sind
unabhängig sichtbar. `Stale` beziehungsweise `NeedsReview` werden zwingend auf jeden angezeigten
Claim propagiert, sodass ein historisch verifizierter Fact nie wie eine aktuelle Faktenquelle
erscheint. Reale libSQL-Fixtures belegen Current→Stale nach geänderter Evidence und ein-Hop-
NeedsReview bei direkter Abhängigkeitsänderung. Die UI lädt erst nach bewusster Modulauswahl,
entfernt alte Card-Daten während eines Reloads und läuft nicht im 500-ms-Statuspolling. Stabile
Evidence-IDs bleiben als Hooks erhalten; ihre sichere Auflösung und Navigation ist bewusst der
nachfolgenden offenen Teilaufgabe Evidence Inspector vorbehalten.

Die WebView darf nur Protokollversion und aktuelle primäre Modul-ID liefern. Card-, Claim-,
Evidence-, Run- und Snapshotanker stammen aus dem Core; Source-, Datei-, Shell-, SQL-, Provider-
oder generische Claim-Befugnisse werden nicht exponiert. Der Abschlussnachweis umfasst Rustfmt,
sämtliche Workspace-Tests mit allen Features, Workspace-Clippy über alle Targets und Features mit
Warnings denied, Rustdoc, 40 Desktop-Libtests plus zwei Desktop-Binary- und drei
Desktop-Integrationstests, 89 Frontendtests, Formatter, ESLint, Svelte-Typecheck,
Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74 lokalen Links,
Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle. Der Windows-libSQL-
Contract mit 23 isolierten Fällen bestand zweimal vollständig sowie im grünen Workspace-
Gesamtlauf. Der Browser-QA-Lauf bestätigte explizite Auswahl, dominante Stale-Darstellung,
Desktop- und 720-px-Layout ohne Horizontaloverflow sowie eine warnungs- und fehlerfreie Konsole.

Abnahme Evidence Inspector vom 2026-08-11: Jede evidenzgebundene Aussage der sichtbaren
deterministisch neuesten Module Card führt über ihre stabile Claim-Evidence-ID zu genau einer
typisierten File-, Symbol- oder Graphkanten-Provenienz. Der atomare libSQL-Read validiert aktuelle
Publikation, aktuelles Primärmodul, R11-Cardauswahl, historische Quellanker, Evidence-
Mitgliedschaft und die aus der gespeicherten Payload erneut abgeleitete ID gemeinsam. Ein
Replacement-Publish oder eine Card-Ersetzung liefert `selectionChanged`; erfundene oder fremde IDs
werden nicht als Such- oder Existenzkanal verwendet.

Aktuelle Evidence muss exakt im jüngsten Published Index auflösen. Nur eine stale Card darf ihre
beim R9-Publish dauerhaft gespeicherte historische Payload als dominant markierte stale Provenienz
zeigen; `NeedsReview` bleibt vom Evidence-Zustand getrennt und verlangt weiterhin aktuelle
Evidence. Graphpayloads bewahren Relation, Endpunkte, Revision, Range, Provider, Confidence und
Resolution. Die UI lädt erst nach einem Evidence-Klick, verwirft Inspectorzustand bei Card-Reload
oder Auswahlwechsel sofort und zeigt keinen Source-Inhalt. WebView-Anker sind opake Hooks, keine
Datei-, Graph-, SQL-, Shell- oder Providerbefugnisse.

Der Abschlussnachweis umfasst Rustfmt, den seriell grünen Workspace-Test mit allen Features,
Workspace-Clippy über alle Targets und Features mit Warnings denied, Rustdoc, 42 Desktop-Libtests
plus zwei Desktop-Binary- und drei Desktop-Integrationstests, 93 Frontendtests, Formatter, ESLint,
Svelte-Typecheck, Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74 lokalen Links,
Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle. Der Windows-libSQL-
Contract mit 23 isolierten Fällen bestand zweimal vollständig; die neuen Verträge decken Current-
File/Symbol/Graph, historische stale Graph-Evidence, Current-Evidence auf `NeedsReview`,
Cancellation, fremde IDs und Selectionwechsel ab. Der Browser-QA-Lauf bestätigte den vollständigen
Klickpfad, dominante Stale-Darstellung, 1280- und 720-px-Layout ohne Horizontaloverflow sowie eine
warnungs- und fehlerfreie Konsole.

Abnahme Confidence, Coverage und Freshness vom 2026-08-11: Das Application-Read-Model leitet die
Coverage jeder sichtbaren Module Card ausschließlich aus den tatsächlich ausgelieferten
verifizierten, evidenzgebundenen Feldern und `ModuleCardSchema::V1` ab. Gesamt-, acht Muss- und vier
Soll-Felder tragen exakte Zähler, ganzzahlige Basispunkte und kanonische Missing-Field-Listen. Es
entsteht weder eine neue Persistenzprojektion noch eine Datei-, Graph-, Modell- oder generische
Read-Capability; Coverage bleibt an denselben atomaren, cancellable und auf zwei Sekunden
begrenzten Card-Read gebunden.

Der strikte Rust-/TypeScript-IPC-Vertrag revalidiert Feldmenge, Zähler, Prozentwerte, Muss-/Soll-
Zuordnung, Lücken und Reihenfolge. Confidence bleibt eine numerische Einschätzung und kann weder
Claim-Klassifikation noch Lifecycle ändern; Freshness stammt ausschließlich aus `Current`, `Stale`
oder `NeedsReview`. Die UI zeigt alle drei Signale gemeinsam, aber unverrechnet. Fehlende Felder
werden erst nach explizitem Öffnen progressiv sichtbar; Hypothesen behalten ihre eigenständige
visuelle Klassifikation und stale Facts bleiben weiterhin dominant als nicht aktuell markiert.

Der Abschlussnachweis umfasst Rustfmt, den seriell grünen Workspace-Test mit allen Features,
Workspace-Clippy über alle Targets und Features mit Warnings denied, Rustdoc, den realen
libSQL-Card-Publish-/Read-Contract, 42 Desktop-Libtests plus zwei Desktop-Binary- und drei
Desktop-Integrationstests, 94 Frontendtests, Formatter, ESLint, Svelte-Typecheck,
Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74 lokalen Links,
Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle. Der Browser-QA-Lauf
bestätigte Klickpfad und progressive Details, 1280- und exakt 720-px-breite Layouts ohne
Horizontaloverflow sowie eine warnungs- und fehlerfreie direkte App-Konsole.

Abnahme Suche und Task-Lens-Umschaltung vom 2026-08-11: Die Project-Map-Suche startet nur nach
bewusstem Submit und führt die bestehenden Exact- und Lexical-Adapter gegen dieselbe jüngste
atomare Publikation aus. Je Kanal gelangen höchstens 100 Kandidaten in R4; höchstens 20
deduplizierte Ziele behalten Run, Snapshot, typisierte native Begründung, normalisierte Scores und
exakte File- oder Declaration-Evidence. Projekt, Cursor und Limits bleiben Core-eigen. Unbekannte
Projektionsstände, gemischte Publikationen und widersprüchliche Ziele werden als vollständige
Zustände beziehungsweise Fehler behandelt statt teilweise gerendert. Source-Inhalt,
Dateisystemzugriff und Storagehandles überschreiten IPC nicht.

Die Task-Lens-Auswahl liest höchstens 20 aktuelle Goal Contracts in stabiler Reihenfolge. Nach
einer opaken `TaskId` werden aktueller Goal Contract und Task Ledger in genau einer kurzen
libSQL-Transaktion gelesen; fehlendes Ledger und Goal-Revisionsabweichung bleiben explizit. Vor
jeder Kompilierung lädt der Application-Use-Case beide Aggregate erneut, akzeptiert nur einen
weiterhin aktiven Plan-Schritt und leitet die UTF-8-sicher auf je 4 KiB begrenzten Goal-/Step-Seeds
selbst ab. Die WebView kann weder Seeds noch Projekt-, Pfad-, Ledger-, Run- oder Snapshotanker
erfinden.

Die bestehende R10-Pipeline kompiliert danach Exact → Lexical → Graph/Test → aktuelle Claims →
optional Semantic unter festen Kandidaten-, Token-, Fortschritts-, Cancellation- und
30-Sekunden-Grenzen. IPC und der unabhängige TypeScript-Decoder binden höchstens 64 L0–L3-Einträge,
128 aktuelle Claims und begrenzte Evidence gemeinsam an Task, Goal, Ledger/Store, Run, Snapshot und
Policy. Evidence wird progressiv geöffnet. Semantic erscheint ausdrücklich nur als Kandidat und
niemals als Beweis; eine Architekturabsicht ohne Evidence ist visuell als unbewiesene Hypothese
getrennt. Ein erfolgreicher Publish verwirft die sichtbare Lens.

Der Abschlussnachweis umfasst Rustfmt, sämtliche Workspace-Tests mit allen Features einschließlich
29 libSQL-Contracts und der realen No-Embeddings-App-Abnahme, Workspace-Clippy über alle Targets und
Features mit Warnings denied, Rustdoc, 44 Desktop-Libtests plus zwei Desktop-Binary- und drei
Desktop-Integrationstests, 106 Frontendtests, Formatter, ESLint, Svelte-Typecheck,
Produktionsbuild, vier Tooltests, 47 Markdown-Dateien mit 74 lokalen Links,
Dependency-/Lizenzbericht und den nativen Tauri-Release-Build ohne Bundle. Component-Tests belegen
den bewussten Tab-/Auswahl-/Compile-Pfad, progressive Evidence sowie die semantische und visuelle
Fact-/Hypothesis-Trennung. Damit sind die drei U4-Akzeptanzkriterien vollständig verifiziert und U4
ist abgeschlossen.

## U5 Agent Workspace

Abhängigkeiten: Gate M7

- [x] Goal Contract
- [x] Acceptance Criteria
- [x] Task Ledger und aktueller Step
- [ ] kompakte Conversation- und Action Timeline
- [ ] Context- und Budgetanzeige
- [ ] Pause, Cancel, Resume und Replan
- [ ] Fehler und Blocker

Akzeptanz:

- Ziel und aktueller Step bleiben stets sichtbar;
- Textantwort und echte Ausführungsaktion sind unterscheidbar;
- Cancel ist erreichbar und zeigt Abschlusszustand.

Abnahme des ersten U5-Schnitts am 2026-08-11: Der Agent Workspace liest den aktuellen
revisionierten Goal Contract aus dem aktiven Core-Worktree, legt eine vollständige Aufgabe mit
ausschließlich Core-generierten Task-/Kriterien-IDs atomar an und hängt materielle Nachfolger gegen
die sichtbar gebundene Vorgängerrevision an. Must und Should, Constraints, Non-Goals,
Nutzerentscheidungen, Abschlussprüfung und Revisionsgrund bleiben über den realen gemeinsamen
libSQL-Reopen-Vertrag erhalten. Strikte Rust-/TypeScript-IPC-, Redaction-, Error-Recovery- und
Component-Tests decken Grenzen, stale Editoren, erfundene IDs, UTF-8-Bytebudgets und die inaktive
Projektgrenze ab. Die reale Browser-Abnahme bestand im Desktop- und 390-Pixel-Layout; vollständige
Workspace-Tests, Clippy mit `-D warnings`, Rustdoc, `ci:frontend`, Link-/Dependency-Prüfung und der
Tauri-Release-Build ohne Bundle waren grün. Die direkte `getrandom`-Deklaration verwendet die
bereits transitiv aufgelöste Version als injizierte OS-Identitätsquelle; sie fügt kein neues Paket
zum Lockfile-Graph hinzu. Ledger, Run und Modellarbeit werden durch diesen Schnitt nicht gestartet.

Abnahme des Ledger-Schnitts am 2026-08-11: Der Workspace lädt das bereits durable Task Ledger
ausschließlich über den bestehenden worktreegebundenen Task-Lens-Read und zeigt Ledgerrevision,
optimistische Store-Version sowie alle aktiven Planschritte. Nur `InProgress`,
`AwaitingApproval`, `Verifying` oder `Blocked` kann den sichtbaren aktuellen Schritt stellen; Ziel
und Schritt bleiben in einem gemeinsamen Sticky Anchor erhalten. Component-Tests belegen dieselbe
Task-ID-Bindung und die gleichzeitige Sichtbarkeit. Fehlendes Ledger, Revisionsabweichung,
Projektwechsel und Readfehler bleiben explizit; die read-only UI erzeugt keinen erfundenen Plan.

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
