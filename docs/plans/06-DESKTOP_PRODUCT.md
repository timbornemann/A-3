# Plan 06: Desktop Product und Nutzerworkflow

Ziel: Die technischen Fähigkeiten werden zu einer schnellen, verständlichen und zugänglichen A^3-Desktopanwendung.

Relevante ADRs: 0001, 0002, 0012, 0014, 0029

## U1 Information Architecture

Abhängigkeiten: Gate M7

Hauptbereiche:

- Projects
- Map
- Agent
- Settings

- [x] Routen und Navigationszustand
- [x] globaler Projekt-, Index-, Modell- und Runstatus
- [x] leere, ladende, fehlgeschlagene und offline Zustände
- [x] keine Fachlogik im Frontend
- [x] Keyboard-first-Navigation

Akzeptanz:

- Nutzer erkennt stets aktives Projekt, Snapshot, Run und Modellzustand;
- Reload der WebView verliert keinen fachlichen Zustand;
- Navigation funktioniert ohne Maus.

Verifiziert am 2026-08-13: Vier native Links bilden `Projects`, `Map`, `Agent` und `Settings` auf
geschlossene URL-Fragmente ab. Unbekannte Fragmente fallen auf Projects zurück; direkte Reloads
stellen die Route wieder her und lesen Projekt-, Index-, Modell- und Agentzustand erneut aus den
bestehenden Core-Projektionen. Die Route besitzt keine Projekt-, Snapshot-, Run- oder
Modellautorität. Agent und Settings laden weiterhin erst bei Sichtbarkeit oder ihrer ausdrücklichen
Route als lokale U10-Chunks.

Eine ständig sichtbare Statusleiste zeigt Projekt, laufende Indexphase beziehungsweise
veröffentlichten Snapshot, verifiziertes Mapping-Modell und den im Agent Workspace ausgewählten
Run. Kein Projekt, kein Snapshot, kein ausgewählter Run, nicht verifiziertes Modell, Loading und
Readfehler sind eigene textuelle Zustände und werden nie nur farblich vermittelt. Der Agent-
Callback projiziert ausschließlich den bereits taskgebundenen `AgentActivityV1`-Controllerzustand;
Fachauswahl, Persistenz und Recovery bleiben im Rust-Kern.

Component-Verträge prüfen Hash-Restore über Remount, erneutes Core-Laden, aktuelle Navigation,
Fokusübergabe, alle vier Globalwerte und die Agent-Runprojektion. Der reale Chromium-Smoke bestätigte
`#agent` mit fokussiertem Ziel, direkten Reload auf `#settings`, null doppelte IDs, null horizontale
Überbreite sowie verständliche globale Fehler-/Offlinezustände ohne Tauri-Backend. Die Navigation
verwendet native Links, besitzt keine positive Tabreihenfolge und erfüllt gemeinsam mit dem U9-
DOM-/Fokusgate Keyboard-first. Formatter, ESLint, Svelte-Typecheck ohne Warnung, 204 Frontendtests,
vier Tooltests, Produktionsbuild und die Markdown-Linkprüfung bestehen. Damit sind alle drei
U1-Akzeptanzkriterien objektiv nachgewiesen und U1 ist abgeschlossen.

UX-Überarbeitung vom 2026-08-13: Die vier URL-gebundenen Hauptbereiche erscheinen nicht mehr als
aufeinanderfolgende One-Page-Sektionen. Eine feste primäre App-Sidebar, eine kompakte globale
Statusprojektion im schmalen Seitenkopf und genau ein scrollbarer Inhalts-Viewport bilden jetzt
eine zusammenhängende Desktop-Shell. Bereichstitel, Projekt, Index, Modell und Run belegen dabei
eine gemeinsame Toolbar statt zweier übereinanderliegender Leisten. `Projects` trennt
Projektverwaltung und Indexübersicht von der eigenen `Map`-Arbeitsfläche; `Agent` und `Settings`
bleiben eigenständige, lokal lazy geladene Views. Ein Routewechsel aktualisiert sichtbaren
Seitentitel, Fokusziel und `data-workspace-area`, ohne fachliche Autorität in die WebView zu
verschieben. Der Bestätigungszustand für das nicht destruktive Entfernen eines Projekts wird als
benannter nativer Modal-Dialog angezeigt.

Vereinfachungsrunde vom 2026-08-14: Die dauerhaft sichtbare zweite Kontextspalte und die redundante
Fußleiste entfallen. Projects ist bei aktivem Worktree ein kompakter Launcher: Pfad und Branch
stehen neben direkten Einstiegen in Project Map und Agent sowie den Aktionen `Projekt verwalten`
und `Anderen Worktree auswählen`. Technische Projekt- und Analysedaten liegen in einem
einzigen scrollbaren Modal mit den drei eindeutigen Tabs `Übersicht`, `Code-Analyse` und `Optionen`; die
Hauptfläche enthält keine aufklappbaren Detailsektionen mehr. Map rendert jeweils nur
`Recherche`, `Explorer`, `Modul` oder `Mapping` und innerhalb von Modul genau eine Detailansicht.
Agent startet mit dem Plan und trennt Aktivität, Review und Vertragsdetails in ausdrückliche Tabs.
Settings trennt Allgemein, Modelle, Projekt und Datenschutz. Das redundante `Ansicht`-Sprungmenü,
der `Lokal`-Mikrostatus und die dauerhaft sichtbare Desktop-Core-Diagnose sind entfernt. Farbschema
und technische App-Metadaten liegen stattdessen unter Allgemein; Version, Protokoll und Plattform
bleiben bis zum Öffnen von `Über A^3` eingeklappt. Die passive Recent-Projects-Projektion wird nicht
mehr gerendert: Der akzeptierte V1-IPC-Vertrag erlaubt weder Auswahl noch erneutes Öffnen und eine
nicht interaktive Liste widerspricht dem Launcher-Zweck. Der bounded Core-Katalog bleibt dabei
unverändert; bis zu einem gesondert entschiedenen sicheren Auswahl-Use-Case erfolgt der Wechsel
weiter über den nativen Ordnerdialog. Rein wiederholende Abschnittstitel und -labels werden
ebenfalls nicht mehr gerendert. Damit bleibt jede relevante Aktion erreichbar, während große
Nebenflächen weder gleichzeitig sichtbar noch nur per CSS im DOM versteckt werden.

Die angepassten Component- und M8-Workflow-Verträge durchlaufen die neuen Tabs und Modale explizit.
Formatter, ESLint, Svelte-Typecheck ohne Warnung, 208 Frontendtests, fünf Tooltests und der
Produktionsbuild bestehen. Der reale Chromium-Smoke bestätigt die kompakte Dark-Theme-Shell bei
1.280 × 720 ohne horizontale Dokumentüberbreite.

Projektkatalog-Erweiterung vom 2026-08-22: ADR-0029 ersetzt die passive Recent-Projektion in der
Projects-Fläche durch einen interaktiven, Core-eigenen Katalog. Das aktive Projekt bleibt als
Launcher oben; darunter stehen Suche, feste 25er-Seiten, Vor-/Zurücknavigation, ID-gebundener
Wechsel, natives Hinzufügen und bestätigtes nicht destruktives Entfernen. Beim Prozessstart wird
ausschließlich der zuletzt erfolgreich aktivierte Eintrag ohne WebView-Parameter wiederhergestellt.
Fehlende oder identitätsveränderte Roots bleiben als Recovery sichtbar und lösen keinen Fallback
auf ein anderes Projekt aus.

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

Verifiziert am 2026-08-22: Der native Ordnerdialog öffnet ausschließlich einen validierten
Git-Worktree-Root; der Core hält die aktive Identität über WebView-Reloads und Prozessneustarts und
projiziert Branch, Worktree, bounded Index-/Snapshotstatus sowie die verlustfreie private
Storagegröße. Der dauerhafte Katalog speichert unbegrenzt viele Einträge, liest aber nur feste
25er-Seiten mit opaken Cursorn und optionaler FTS-Suche über die sichere Root-Anzeige. Der
besitzende Indexmanager serialisiert Refresh und einen cancellable Rebuild, der nur regenerierbare
Projektionen entfernt und anschließend einen vollständigen Rescan anfordert.

UX-Verfeinerung vom 2026-08-22: `Projekt verwalten` priorisiert Projektname, Branch und einen
verständlichen Zustand der lokalen Code-Analyse. Worktree-, Snapshot- und Speicherangaben bleiben
für Supportfälle in geschlossenen technischen Details erreichbar. Die Seite `Code-Analyse`
erklärt Zweck und Fortschritt in Alltagssprache, fasst Dateien, Symbole, Abdeckung und Hinweise
zusammen und zeigt einzelne Dateihinweise erst auf Wunsch; Parsercodes und Bytebereiche werden
nicht als Endnutzerinformation dargestellt. `Optionen` trennt den sicher regenerierbaren
Analyse-Neuaufbau sichtbar vom nicht destruktiven Entfernen. Dessen zweite Bestätigung erfolgt als
klarer Schritt im selben Dialog und nennt vor der Ausführung ausdrücklich, welche Ordner, Dateien
und lokalen A^3-Projektdaten erhalten bleiben.

Component- und Accessibility-Regressionen prüfen die drei Ansichten, laufenden Fortschritt,
standardmäßig geschlossene Technik- und Dateihinweise, den Analyse-Neuaufbau sowie die eingebettete
Bestätigung ohne zweiten Dialog. Der native Windows-Tauri-Smoke bestätigte Übersicht,
Code-Analyse, Optionen und Bestätigung mit einem realen aktiven Projekt. Formatter, ESLint,
Svelte-Typecheck, alle 220 Frontendtests, fünf Tooltests, Produktionsbuild und Markdown-Linkprüfung
sind grün.

Die Map bietet denselben pfadlosen Fast-Index-Neuaufbau zusätzlich direkt in ihrer Command-Leiste
an. Sie verfolgt dafür ausschließlich den vorhandenen begrenzten Index-Aktivitätsstatus; weder
Projektpfad noch Worktree- oder Job-ID werden von der WebView gewählt.

Bugfix-Abnahme vom 2026-08-29: ADR-0035 und Knowledge-Schema V28 trennen die dauerhafte monotone
Index-Laufkoordinate von den regenerierbaren `index_runs`. Ein expliziter Rebuild mit unverändertem
Snapshot erhält dadurch einen neuen deterministischen Publikationsanker; historische Cards bleiben
unverändert am alten Anker und Deep Map wechselt auf `Ready`. Storage-CAS, Reopen, Migration,
Überlauf, der vollständige Fast-Index-Pfad und die Map-Aktion sind durch Regressionstests gedeckt.

`activate_catalog_project` und `remove_catalog_project` nehmen ausschließlich eine zuvor gelistete
`worktreeId`, aber nie einen Pfad an. Aktivierung revalidiert Root, Repository- und Worktree-
Identität und schreibt die neue Reihenfolge erst nach erfolgreichem Runtime-Wechsel. Der
bestätigte Removal-Use-Case beendet beim aktiven Eintrag Watcher und laufende Indexarbeit, entfernt
atomar nur den exakten Katalogeintrag sowie offene Reconciliation-Absichten und behält Repository,
Linked Worktrees, stabile `ProjectId` und `projects/<WorktreeId>/knowledge.db`. Adapter-Contracts
belegen Erhalt und Wiederöffnung. Die UI validiert `CommandErrorV1` streng und ordnet nur stabile
Codes konkreten Recovery-Schritten zu; rohe Pfade und Adapterdetails bleiben unsichtbar.

Rustfmt, Workspace-Tests mit allen Features, Workspace-Clippy über alle Targets/Features mit
`-D warnings`, Rustdoc, 44 Frontend-Testdateien mit 220 Tests, Formatter, ESLint,
Svelte-Typecheck, Produktionsbuild, fünf Tooltests, 57 Markdown-Dateien mit 159 lokalen Links und
der Dependency-/Lizenzbericht sind grün. Der native Windows-Tauri-Release-Build und UX-Smoke
bestätigen das reale A^3-Fenster; Browser-QA bei 1.280 × 720 und 680 × 760 bestätigt die responsive
Projects-Fläche ohne horizontale Überbreite.
Der vollständige Linux-`quality`-Job bestand lokal über
`act -j quality --pull=false -P ubuntu-22.04=a3-act-medium-rust:latest`. Der Windows-libSQL-Harness
isoliert nun auch Project-Catalog- und Knowledge-Contracts pro Test, wiederholt ausschließlich
`0xc0000005` höchstens zweimal und verlangt den Abschlussmarker nach der letzten Assertion.

## U3 Index Experience

Abhängigkeiten: Fast Index, Deep Map

- [x] Fast-Index-Fortschritt nach Phasen
- [x] Dateien, Symbole, Diagnostics und Coverage
- [x] Deep Map bewusst starten, pausieren, fortsetzen und abbrechen
- [x] feste Schnell-/Standard-/Gründlich-Budgets vor Start
- [x] Deep-Map-Details als verständliches Live-Informationssystem für Plan, Cards und Atlas-Wirkung
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

Deep Map besitzt nun einen expliziten, ebenfalls pfadlosen Produkt-Lifecycle. Die dauerhafte,
einzeilige Leiste bietet ausschließlich Schnell, Standard und Gründlich mit Core-eigenen festen
Token-, Zeit- und Read-only-Toolbudgets. Modell- und Laufdetails liegen im gemeinsamen rechten
Inspector. Ohne vollständig komponierten Executor
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
- [x] kompakte Conversation- und Action Timeline
- [x] Context- und Budgetanzeige
- [x] Pause, Cancel, Resume und Replan
- [x] Fehler und Blocker

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

Abnahme des Activity-Schnitts am 2026-08-11: Aus dem aktiven oder deterministisch letzten retained
Step-Versuch wählt der Application-Use-Case den Agent Run selbst; die WebView sendet ausschließlich
die bereits ausgewählte `TaskId`. Goal-, Ledger- und Run-Anker werden nach dem Journalread erneut
geprüft, gleichzeitige Änderungen ergeben `activityChanged`, und höchstens die jüngsten 64
zusammenhängenden inhaltsfreien Events überschreiten IPC. Der Workspace zeigt endlichen
Controllerzustand, terminalen Zustand, Snapshot und letztes sichtbares Context-Pack sowie harte
Grenze und durable Nutzung aller sechs Run-Budgetdimensionen. Aktuelle Ledger-Blocker und
fehlgeschlagene, verweigerte oder abgebrochene Events sind sichtbar. Modellantwort und
Aktionsauswahl heißen ausdrücklich „keine/noch keine Ausführung“; nur ein separates `ToolAction`
wird als echte Ausführungsaktion dargestellt. Application-, Protokoll-, Command-, TypeScript- und
Component-Tests decken Ableitung, 64er-Grenze, geschlossene Formen, monotone Sequenzen, Blocker,
Budget, Aktionsunterscheidung und sichtbaren terminalen Zustand ab. Der Schnitt besitzt keinerlei
Mutation oder Run-Steuerung; Pause, Cancel, Resume und Replan bleiben der nächste U5-Schnitt.

Teilabnahme der Recovery-Steuerung am 2026-08-13: `query_agent_task_recovery` und
`control_agent_task_run` leiten den einzigen steuerbaren Run aus der ausgewählten `TaskId` und dem
aktiven retained Ledger-Versuch ab. Die WebView kann weder Run, Snapshot, Step, Event-ID noch Zeit
wählen und muss Resume, Replan oder Cancel gegen die exakt sichtbare Ledgerrevision und
-Storeversion senden. Der bestehende H11/E8-Pfad revalidiert Published Snapshot, stale Evidence,
Unknown-Mutationsdisposition, Ledger und Runsequenz und committed die Wirkung atomar. Resume bleibt
bei stale Evidence oder Unknown gesperrt; Replan öffnet invalidierte Arbeit kontrolliert, Cancel
bleibt erreichbar und wird als terminaler Activity-Zustand neu geladen. Strikte Protocol-,
Command-, TypeScript- und Component-Tests belegen geschlossene Formen, fail-closed Controls und die
Task-Bindung. Die Tauri-Capability enthält nun auch alle bereits registrierten Project-Map-,
Task-Lens-, Agent-Goal- und Agent-Activity-Commands explizit; direkte privilegierte Plugins bleiben
ausgeschlossen. Der U5-Gesamtpunkt bleibt offen, bis Pause an einen tatsächlich besitzenden,
kooperativ abbrechbaren Agent-Runtimepfad angebunden ist.

Abnahme des vollständigen U5-Agent-Workspace am 2026-08-13: ADR-0020 trennt den flüchtigen
`Idle|Queued|Running|Pausing|Paused|Cancelling|Succeeded|Failed|Cancelled`-Produktlifecycle vom
unveränderten ADR-0010-Controller. Der Desktop-Composition-Root besitzt einen begrenzten
`AgentRunManager`; Mount und Polling starten keine Arbeit. Während eines live besessenen Workers
läuft keine H11-Neustartinspektion, sondern nur eine content-freie, Task- und Ledger-gebundene
Runtimeprojektion. Pause ist ausschließlich aus `Running` möglich und bestätigt `Paused` erst nach
terminaler Scheduler-Cancellation, Executor-Acknowledgement und einer weiterhin nichtterminalen
H11/E8-Revalidierung. Queued Work kann keinen Checkpoint vortäuschen.

Resume und Replan verwenden zuerst den bestehenden atomaren Published-Snapshot-/Ledger-/Run-CAS
und dürfen erst danach einen neuen Scheduler-eigenen Versuch mit strikt neuerer Ledger-
Storeversion einreihen. Cancel eines lebenden Workers stoppt ihn zuerst und committed danach gegen
die exakt sichtbaren Anker; aus einem bestätigten `Paused`-Zustand liefert derselbe H11/E8-Pfad
direkt den dauerhaften `Applied/Cancelled`-Abschluss. Projektwechsel, Entfernung und Shutdown
quieszen besessene Arbeit ohne einen Nutzer-Cancel vorzutäuschen. Ohne verifizierte Agent-
Executor-Capability bleibt die Funktion sicher unavailable und erzeugt keine Modellarbeit.

Sieben deterministische Manager-Tests belegen Startfreiheit vor der expliziten Aktion,
Queued-/Running-Grenzen, Pause, Resume als neuen Job, exakte Cancel-Anker, extern committed Cancel,
Projektwechsel und Scheduler-Join beim Shutdown. Rust-/TypeScript-IPC- und Component-Contracts
prüfen `runtimeOwned`, `pausing`, erst anschließend `paused`, alle vier Controls, strikt geschlossene
camelCase-Formen sowie dauerhafte UI-Endzustände. Workspace-Clippy über alle Targets/Features mit
Warnings denied, die geänderten Rust-Suites, 130 Frontendtests, Formatter, ESLint,
Svelte-Typecheck, Produktionsbuild, Rustdoc mit Warnings denied, 48 Markdown-Dateien mit 85 lokalen
Links, Dependency-/Lizenzbericht und der native Tauri-Release-Build ohne Bundle sind grün. Der
vollständige serielle Windows-Workspace-Lauf bestand alle fachlichen Suites; nur der bereits
dokumentierte native libSQL-Teardown verlor einmal nach erfolgreichem Contractprozess mit
`0xc0000005`. Der exakte isolierte Snapshot-Contract bestand unmittelbar danach vollständig.
Die abschließende vollständige Wiederholung bestand danach ohne Ausfall. Damit sind alle drei
U5-Akzeptanzkriterien objektiv nachgewiesen und U5 ist abgeschlossen.

## U6 Diff und Verification

Abhängigkeiten: Patch und Verification Engine

- [x] Datei- und Hunkübersicht
- [x] Side-by-side und unified Diff
- [x] Useränderungs- und Agentenänderungsmarkierung, soweit zuverlässig
- [x] Test-, Build- und Diagnostic-Ergebnisse
- [x] Evidence zu Step und Acceptance
- [x] stale Verification sichtbar

Akzeptanz:

- Nutzer kann vor Freigabe genaue Pfade und Änderungen sehen;
- verkürzte Logs sind als verkürzt markiert und gezielt nachladbar;
- Done zeigt die Beweise pro Muss-Kriterium.

Abnahme U6 vom 2026-08-13: Der taskgebundene Inspector verbindet die exakte bounded E3-
Patchprojektion mit der dauerhaft neu ausgewerteten Verification-Inspection. Dateioperationen,
verlustfreie Pfadbytes, kontrollierte Pfadanzeige, vollständige Hash-/Byteanker, gemeinsame
koordinatengeprüfte Hunkzeilen und die vier verlässlichen Provenienzzustände werden im Core
erzeugt; Unified und Side-by-side rendern dasselbe strikt decodierte Zeilenmodell. Die WebView
sendet weder Pfade noch Run-, Step-, Snapshot-, Process-, Verification-Spec- oder Evidence-IDs für
die Übersicht. Test-, Build-, Diagnostic-, Lint-, Format- und Commandresultate bleiben zunächst
content-frei. Ein Logread ist eine bewusste, inspection-revisions- und recordgebundene Aktion mit
exaktem Bytecursor; `pageTruncated` bietet gezieltes Nachladen, `sourceTruncated` kennzeichnet
dauerhaft verworfenen Overflow, und Redaction kann keinen Text ausliefern. Das Mindestlimit von
vier Byte garantiert auch an einem vier Byte langen UTF-8-Skalar bounded Paging-Fortschritt.

Der dauerhafte Teil zeigt Goal-, Ledger- und Published-Snapshotanker, Step-/Attemptstatus, alle
fünf typisierten Evidence-Varianten, Semantikergebnis und Freshness. Stale Steps und Artefakte
bleiben sichtbar, liefern aber niemals einen Done-Beweis. Done erscheint nur, wenn jedes
Muss-Kriterium exakte Step- und Evidence-IDs eines abgeschlossenen, bestandenen und frischen
Versuchs besitzt. Der Workspace bindet den Inspector an dieselbe ausgewählte dauerhafte TaskId;
Projektwechsel und Shutdown entfernen flüchtige Patch- und Logdaten.

Die sechs Application-Inspector-Tests, der IPC-Grenztest, der Capability-Negativtest, die strikten
TypeScript-Decoder- und Component-Tests, Formatter, ESLint, Svelte-Typecheck, 152 Frontendtests,
Produktionsbuild sowie Clippy über alle Workspace-Targets und -Features mit Warnings denied sind
grün. Ein vollständiger serieller Windows-Workspace-Lauf traf einmal nach fachlich erfolgreichem
Contractprozess den bereits dokumentierten nativen libSQL-Teardown mit `0xc0000005`; der exakt
betroffene Contract bestand unmittelbar isoliert und die anschließende vollständige
Workspace-Wiederholung ohne Ausfall. Damit sind alle drei U6-Akzeptanzkriterien objektiv
nachgewiesen und U6 ist abgeschlossen.

## U7 Approval Center

Abhängigkeiten: Policy Engine

- [x] Aktion, Risiko, Scope und Begründung
- [x] genaue ProcessSpec oder Dateipfade
- [x] einmal erlauben, scopegebunden erlauben oder ablehnen
- [x] Ablauf und Widerruf
- [x] kein manipulativer Default

Akzeptanz:

- Zustimmung ist informiert und spezifisch;
- Ablehnung führt zu Replan oder sauberem Blocked;
- geheime Werte werden nicht angezeigt.

Abnahme U7 vom 2026-08-13: Das taskgebundene Approval Center verbindet einen bereits dauerhaft
persistierten exakten Request mit einer flüchtigen vollständigen E3-Patch- oder E4-ProcessSpec-
Präsentation. Der Core revalidiert Task, Goal, Ledger, Run, Step, Snapshot, Request und optionalen
Grant. Die WebView erhält Aktion, abgeleitete Klasse/Risiko, Scope, Policy-Grund, Ablauf sowie
verlustfreie Pfadbytes oder getrennte argv-Tokens, CWD, ausschließlich Env-Namen, Limits,
Execution Mode, Plan-/Netzwerkbindung und Specification-ID. Request-, Grant-, Run-, Process-,
Policy- und Event-IDs bleiben Core-eigen.

ADR-0022 konkretisiert den Planpunkt „scopegebunden erlauben“ als exaktes aktions- und
scopegebundenes `AllowOnce`, nicht als breiten wiederverwendbaren Modus. GrantStored startet keine
Mutation; erst die getrennte Continue-Aktion kann die interne Grant-ID an einen neuen
scheduler-eigenen Versuch übergeben, sodass Revoke bis dahin möglich bleibt. Deny committed den
wartenden Step als `Blocked`, den Run als `Failed` und `ApprovalDenied` atomar ohne Toolwirkung;
Replan oder Cancel bleiben über U5 verfügbar. AllowOnce und Deny sind neutral, nicht vorausgewählt,
und die Bestätigung bleibt bis zur expliziten Wahl deaktiviert.

Der vollständige Rust-Workspace-Test über alle Targets und Features, darunter alle acht
mutierenden Agent-Harnessfälle, 62 Desktop-Core-Tests und 29 libSQL-Shared-Contracts, ist grün.
Workspace-Clippy mit Warnings denied, Rustdoc mit Warnings denied und rustfmt sind ebenfalls grün.
Protocol-V1 besteht 45 Tests; die Desktop-IPC-/Capability-Grenze lehnt nicht kanonische oder
autoritätstragende Eingaben ab. Prettier, ESLint, Svelte-Typecheck ohne Befund, 159 Frontendtests
und der Produktionsbuild bestehen. Die lokale Markdown-Prüfung bestätigt 49 Dateien und 110
gültige Links. Damit sind alle drei U7-Akzeptanzkriterien objektiv nachgewiesen und U7 ist
abgeschlossen.

## U8 Settings und Model Health

Abhängigkeiten: ModelProfile

- [x] lokaler Endpoint
- [x] Provider Health und Capability Probe
- [x] Modellprofile für Coding, Mapping und Embedding
- [x] Kontext- und Ressourcenlimits
- [x] Indexignore und sichere Command Allowlist
- [x] Daten- und Privacy-Einstellungen

Akzeptanz:

- App ist ohne konfiguriertes Modell als Indexbrowser nutzbar;
- nicht lokaler Endpoint warnt deutlich;
- invalides Profil kann nicht für ausführbare Runs aktiviert werden.

Abnahme U8 vom 2026-08-13: ADR-0023 ist vollständig als getrennte globale und projektbezogene
Settings-Grenze umgesetzt. Der Katalog persistiert append-only V1-Snapshots mit CAS und
modellfreiem Nullzustand. Endpointkonfiguration ist credential-frei, kanonisch und unterscheidet
lokales Loopback von sichtbar blockiertem Remote. Reads starten keine Provider-, Netzwerk- oder
GPU-Arbeit; ein Endpointwechsel invalidiert alle vorherigen Kandidaten und Probe-Evidence.
Explizite, abbrechbare und begrenzte Probes können Coding und Mapping nur nach real verifiziertem
Structured Output sowie Embedding nur nach einem endlichen Vektor mit beobachteter gültiger
Dimension aktivieren. Nicht verifizierte, fehlgeschlagene, abgebrochene, stale oder remote
Profile bleiben nicht ausführbar.

Die Oberfläche zeigt Rollenstatus, Health, Kontext-/Output-/Parallelitäts-/Batchlimits und die
fail-closed Privacygrenzen. Projektbezogenes Indexignore bleibt read-only und verwendet nur die
bereits autorisierten Git-, Safety- und `.a3/project.toml`-Quellen. Die sichere Command-Allowlist
wird aus dem jüngsten Published Index rekonstruiert und kann nur mit sichtbarer Katalogrevision,
Store-CAS und bekannten IDs bestätigt werden; die WebView liefert weder Pfade noch argv. Ohne
einen separat vollständig komponierten Laufzeit-Executor bleibt die jeweilige Funktion
unavailable und behauptet keine nur durch Settings vorgetäuschte Ausführbarkeit. Der Desktop bindet
ein ausführbares Mapping-Profil inzwischen an den produktiven Deep-Map-Executor; der Agent bleibt
bis zur Komposition seines eigenen Executors davon unabhängig unavailable.

Bugfix-Abnahme vom 2026-08-23: Ein persistiertes, durch Structured Output verifiziertes
Mapping-Profil wird beim Start ohne WebView-Parameter aus Settings und Credential Store wieder an
Deep Map gebunden. Provider-, Credential- und Probe-Änderungen erneuern diese Bindung, verwerfen
inkompatible Pause-Checkpoints und aktualisieren dadurch Statusleiste und Startfreigabe aus derselben
Core-Projektion. Der produktive Lauf verwendet den publizierten Index, bounded read-only Exploration,
modellgestützte Claim-Proposals, Evidence-Verifikation und die atomare Module-Card-Publikation.

Deep-Map-Laufzeitkorrektur vom 2026-08-23: Ollama erhält pro Request nur noch den konservativ
benötigten, durch das Profil begrenzten operativen Kontext statt stets des maximal konfigurierten
Fensters; Explorer und Claim-Proposer besitzen je eine 120-Sekunden-Deadline. Claim-IDs werden im
Core deterministisch aus Card, Feld und Werteindex erzeugt, dem Modell nur zum Kopieren übergeben
und nach der Antwort exakt geprüft. Der Manager verwirft terminale Fehlerursachen nicht mehr,
sondern projiziert ausschließlich geschlossene, content-freie Kategorien. Die Mapping-Oberfläche
erklärt daraus Ursache und nächsten sicheren Schritt, und deaktivierte Pause-, Resume- und
Cancel-Controls zeigen keinen falschen Ladecursor mehr.

Scheduler-eigene Desktop-Jobs betreten für ihre asynchronen Index-, Deep-Map- und Agent-Futures nun
die Tauri-eigene Tokio-Laufzeit. Dadurch besitzt insbesondere der produktive Ollama-Request im
Deep-Map-Worker den erforderlichen Reactor-Kontext und endet nicht mehr als abgefangener Panic mit
einem widersprüchlichen Schedulerzustand. Nur ein tatsächlich plan- oder budgetfremder Checkpoint
wird als `invalidCheckpoint` projiziert; sonstige Lifecycle-Widersprüche bleiben als
`progressUnavailable` getrennt sichtbar.

Explorer- und Claim-Structured-Output-Schemas sind außerdem phasengenau an die Core-Autorität des
aktuellen Requests gebunden. Inspect und Proposal sind nicht mehr gleichzeitig auswählbar;
Proposal- und Claim-IDs, Feldreihenfolge, Aussagen und beobachtete Evidence-Mengen sind geschlossen.
Ein V1-Proposal bleibt auf einen kompakten zusammengefassten Wert und einen relevanten Beleg je
Feld begrenzt. Die bis zu 100 erlaubten Observation-IDs werden einmalig per Schema-Definition
geteilt, sodass auch der Maximalfall unter der 64-KiB-Providergrenze bleibt und kleine lokale
Modelle keine hunderte IDs umfassende Ausgabe erzeugen müssen.

Budgetierte Feldfragmente desselben Moduls werden vor Claim-Erzeugung deterministisch zu einer
vollständigen, Core-identifizierten Module Card zusammengeführt. Verifikation lässt höchstens eine
publizierbare Card je Modul zu und entspricht damit dem atomaren Storagevertrag. Die internen
Fortschrittszähler des anfänglichen und des Verifier-Indexreads werden nicht als globale
Scheduler-Totals weitergereicht; dadurch kann der anschließende Publisher seine eigene monotone
Progressreihe beginnen, ohne fälschlich als Storagefehler abgewiesen zu werden.

Rustfmt, der vollständige Rust-Workspace-Test über alle Targets und Features, Workspace-Clippy
mit Warnings denied sowie Rustdoc mit Warnings denied sind grün. Der Windows-libSQL-Harness
belegt den zuvor betroffenen Index-Contract mit Abschlussmarker und höchstens zwei ausschließlich
für `STATUS_ACCESS_VIOLATION` erlaubten frischen Wiederholungen. Prettier, ESLint,
Svelte-Typecheck, 177 Frontendtests und Produktionsbuild bestehen. Desktop-Component- und
Boundarytests belegen den modellfreien Indexbrowser, die nicht nur farbliche Remote-Warnung,
strikte Requests, Probe/Cancel, Rollenprofile, Privacy sowie die stale-sichere Projektsettings-
Bestätigung. Damit sind alle drei U8-Akzeptanzkriterien objektiv nachgewiesen und U8 ist
abgeschlossen.

Erweiterung vom 2026-08-14: ADR-0026 ersetzt die technische Endpointkarte durch eine kompakte,
seitennavigierte Settings-Arbeitsfläche mit getrennten Bereichen für Allgemein, Provider,
Modelle, Projekt, Datenschutz und Info. Ollama wird als typisierte aktive Providerverbindung über
Modale angelegt, bearbeitet und entfernt. Eine ausschließlich explizite, abbrechbare Abfrage liest
den begrenzten lokalen Modellkatalog und stellt Coding-, Mapping- und Embeddingmodelle als
Dropdown bereit. Der Katalog bleibt flüchtig und die bestehende Capability-Probe bleibt die
einzige Aktivierungsgrenze; Appstart und Settings-Read erzeugen weiterhin keinen Netzwerkzugriff.

Erweiterung vom 2026-08-20: ADR-0028 ergänzt Google Gemini über den ausschließlich zulässigen
Google-Origin. API-Keys werden über eine one-way Settings-Capability angenommen und nur im nativen
OS-Keyring gespeichert; Katalogschema v5 enthält ausschließlich Lifecycle und monotone Generation.
Die dreiphasigen Store-/Delete-Übergänge sperren inkonsistente Zustände als `RecoveryRequired`, und
Providerwechsel löschen einen Gemini-Key vor der neuen Konfiguration. Gemini-Discovery ist
paginiert und begrenzt, Streaming wertet Candidate, Thought-, Block- und Finish-Zustände streng aus,
strukturierte Ausgabe verwendet `responseJsonSchema`, und abgekündigte Embedding-IDs sind nicht
fest codiert. Discovery und Probe bleiben explizite Nutzeraktionen; Speichern allein kontaktiert
Google nicht.

Erweiterung vom 2026-08-27: ADR-0032 ergänzt OpenAI über den ausschließlich zulässigen Origin
`https://api.openai.com`. Der native Adapter verwendet die Responses API mit `store: false`,
deaktivierten Provider-Tools, striktem JSON Schema und begrenztem SSE-Parser sowie Models- und
Embeddings-API. OpenAI-Keys durchlaufen denselben one-way CAS-/OS-Keyring-Lifecycle wie Gemini,
bleiben aber unter der Provider-ID `openai` isoliert. Die Settings-Oberfläche zeigt Remote- und
Kostenhinweis, lädt Modelle nur nach ausdrücklichem Klick und aktiviert Coding oder Mapping erst
nach einer echten Structured-Output-Probe sowie Embeddings erst nach einer validierten
Dimensionsprobe. Der produktive Deep-Map-Composition-Root rekonstruiert OpenAI ausschließlich aus
revisionsgebundenem Endpoint, Credential und ausführbarem Mappingprofil.

Abnahme der OpenAI-Erweiterung vom 2026-08-27: Neunzehn Provider-Unittests, dreizehn Ollama-,
dreizehn Gemini- und neun OpenAI-Offline-Contracts sind grün; die beiden echten Provider-Smokes
bleiben ignoriert und ausdrücklich opt-in. Der vollständige Rust-Workspace-Test, Rustfmt,
Workspace-Clippy und Rustdoc mit verweigerten Warnungen sowie Prettier, ESLint, Svelte-Typecheck,
225 Frontendtests, Tooltests, Produktionsbuild und Markdown-Linkprüfung bestehen. Ein unabhängiger
Windows-libSQL-Contract benötigte nach einem `STATUS_ACCESS_VIOLATION` einen isolierten grünen
Wiederholungslauf; der anschließend wiederholte vollständige Workspace-Test war grün.

OpenAI-Produktionsschema-Korrektur vom 2026-08-29: Die kleine Capability-Probe verwendete nur ein
einfaches Enum-Schema und konnte deshalb ein Modell aktivieren, obwohl die ersten echten Deep-Map-
und AgentAction-Requests weiterhin A^3-interne Schlüssel wie `const`, `oneOf`, `prefixItems` und
`uniqueItems` unverändert übertrugen. Der OpenAI-Dialektadapter übersetzt nun `const` nach einem
Einzelwert-Enum und `oneOf` nach `anyOf`, verdichtet feste Tuple-Items unter unveränderten exakten
Arraygrenzen und entfernt ausschließlich Hinweise, die der unveränderte strikte Decoder und die
Domain-Invarianten nach jeder Antwort erneut prüfen. Unbekannte Schlüssel, optionale Objektfelder
und fremde Referenzen bleiben vor dem Netzwerkzugriff abgelehnt. Offline-Contracts decken die
vollständigen Explorer-, Claim- und AgentAction-Schemas sowie einen erfolgreichen schemafreien
Agent-Chat ab. Ein fehlgeschlagener Deep-Map-Status öffnet jetzt per Klick eine sichere Erklärung
mit Ursache, betroffenem Provider/Modell, nächstem Schritt und stabilem Diagnosecode; rohe
Providerantworten und Secrets bleiben ausgeschlossen.

UX-Korrektur vom 2026-08-22: Die zweite vertikale Settings-Sidebar wird durch eine kompakte
horizontale Bereichsnavigation ersetzt. Provider und Modelle bilden nun den gemeinsamen Bereich
`KI & Modelle` mit dem sichtbaren Ablauf Provider verbinden und dabei die Modellliste einmal
ausdrücklich laden, danach Rollen einzeln zuordnen. Direkte Startpunkte für Ollama und Google
Gemini verkürzen die Ersteinrichtung; der Button `Verbinden und Modelle laden` macht die einmalige
Katalogabfrage als Teil desselben bewussten Nutzerklicks sichtbar. Settings-Reads und das spätere
Bearbeiten ohne `Modelle aktualisieren` erzeugen weiterhin keinen Providerzugriff. Gespeicherte
Rollenprofile bleiben nach Reload sichtbar, auch wenn der begrenzte Katalog absichtlich flüchtig
ist; nur `Modelle aktualisieren` fragt spätere neue Auswahlwerte ab. Der Gemini-Dialog nimmt den
API-Key gleichzeitig mit der Verbindung über den bestehenden one-way Command entgegen; eine spätere
Bearbeitung zeigt ausschließlich einen festen Sternchen-Platzhalter als content-freien
Konfigurationsstatus. Rollen- und Providerstatus besitzen eine kleine zugängliche Statushilfe: Sie
erklärt insbesondere, dass `Capability fehlt` eine fehlende Live-Verifikation der strukturierten
JSON-Antwort bedeutet, und nennt sichere nächste Schritte, ohne rohe Providerfehler oder Secrets
sichtbar zu machen.

Gemini-Kompatibilitätskorrektur vom 2026-08-25: Die echte Structured-Output-Probe reserviert nun
256 statt 32 Outputtokens. Aktuelle Gemini-2.5-/3.x-Modelle verwenden standardmäßig internes
Thinking; das frühere Budget konnte deshalb enden, bevor selbst das kleine sichtbare JSON-Ergebnis
ausgegeben wurde, und markierte ein tatsächlich schemafähiges Modell fälschlich als nicht
verifiziert. Der weiterhin ignorierte, ausdrücklich netzwerkgebundene Live-Smoke bevorzugt
`gemini-3.7-flash`, prüft zusätzlich einen vorhandenen `gemini-pro-latest`-Alias und führt für
beide Capability-Probe, Streaming und ein produktives Schema aus, ohne Key oder Providerantwort zu
persistieren oder auszugeben.

Deep-Map-Gemini-Korrektur vom 2026-08-25: Der produktive Explorer-Request enthält strengere
JSON-Schema-Eigenschaften als die kleine Capability-Probe. Insbesondere `uniqueItems` wurde vom
A^3-Gemini-Adapter noch vor dem Netzwerkzugriff als unbekannt abgelehnt; deshalb konnte ein korrekt
verifiziertes `gemini-flash-latest` beim bewussten Deep-Map-Start sofort als `modelRejected`
scheitern. Der begrenzte Dialektadapter entfernt nun ausschließlich lokal erneut geprüfte,
Google-seitig nicht unterstützte Hinweise und Schema-Metadaten und übersetzt die diskriminierten
`oneOf`-Varianten nach `anyOf`. Der strikte Explorer-/Claim-Decoder und die Domain-Invarianten
bleiben nach jeder Antwort autoritativ. Offline-Contracts decken beide vollständigen
Produktionsschemas und den exakten Wire-Dialekt ab; der opt-in Live-Smoke sendet zusätzlich die
erste produktionsnahe Deep-Map-Anfrage mit synthetischen Inhalten über `gemini-flash-latest`.

Deep-Map-Verfügbarkeitskorrektur vom 2026-08-25: Explorer und Claim-Proposer verwenden nun einen
gemeinsamen providerneutralen Stream-Collector. Ein echter Connect-/Body-Abbruch oder ein vom
Adapter korrekt als transient normalisierter HTTP-Status wartet cancellation-fähig eine Sekunde
und wiederholt dieselbe sichere Structured-Output-Anfrage exakt einmal; Teiloutput wird verworfen
und beide Versuche bleiben innerhalb der ursprünglichen Requestdeadline. Ablehnung, ungültige
Antwort, Timeout, Cancellation und Endpoint-Deny werden nicht wiederholt. Gemini und Ollama
klassifizieren dafür 408, 429 und retry-fähige 5xx getrennt von dauerhaften 4xx-/501-Fehlern. Die
Fehlerkarte nennt bei erschöpftem Retry Google Gemini, Ollama oder eine spätere Provider-ID aus der
validierten Core-Projektion und enthält keine fest verdrahtete Ollama-Hilfe mehr. Der bereits im
Workspace vorhandene Tokio-Timer genügt für den nicht blockierenden Backoff; es kam kein neues
externes Paket hinzu.

Gemini-Produktionsschema-Korrektur vom 2026-08-25: Die kleine Capability-Probe deckte weiterhin
nicht Geminis dokumentiertes Komplexitätslimit für reale Deep-Map-Schemas ab. Bereits spezialisierte
Explorer- und Claim-Requests übertrugen unbenutzte `$defs` sowie viele positionsgebundene,
strukturell gleiche `prefixItems`; Google kann solche großen oder tiefen Schemas als
`INVALID_ARGUMENT` ablehnen. Der Gemini-Dialektadapter behält nun nur transitiv erreichbare
Definitionen und verdichtet gleichförmige Tuple-Items zu einem gemeinsamen `items`-Schema mit
vereinigten Enums und unveränderten exakten Arraygrenzen. Die positionsgenaue ID-, Feld- und
Evidence-Bindung bleibt durch den unveränderten strikten Decoder autoritativ. Zusätzlich werden
Gemini-Fehlerobjekte innerhalb eines HTTP-200-SSE-Streams nach ihrem numerischen Status
klassifiziert: 408, 429 und retry-fähige 5xx erreichen den gemeinsamen begrenzten Retry, dauerhafte
4xx bleiben Ablehnungen und Providertexte verlassen den Adapter nicht.

## U9 Design System und Accessibility

- [x] Farb-, Typografie-, Spacing- und Focus-Tokens
- [x] Light und Dark Theme
- [x] WCAG-konforme Kontraste
- [x] Screenreader-Labels
- [x] Reduced Motion
- [x] skalierbare Schrift
- [x] keine Information nur über Farbe

Akzeptanz:

- automatisierte Accessibilitychecks ohne kritische Befunde;
- Kernworkflow vollständig per Tastatur;
- 200-Prozent-Zoom ohne Funktionsverlust.

Abnahme U9 vom 2026-08-13: ADR-0024 definiert eine einzige semantische V1-Tokenquelle für Farbe,
Typografie, Spacing, Radien, Fokus und Controlgrößen. System, Hell und Dunkel verwenden dieselben
Rollen; die Auswahl ist textuell und per `aria-pressed` sichtbar, folgt ohne explizite Wahl der
OS-Präferenz und legt keinen zweiten dauerhaften Settings-Store neben ADR-0023 an. Ein
automatisierter Sourcevertrag erfasst auch alle gekapselten Svelte-Styles und lehnt jede Rohfarbe
außerhalb der Tokenquelle ab. Alle normativen Text-/Flächenpaare erreichen in Light und Dark
rechnerisch mindestens WCAG AA 4,5:1.

Der DOM-Contract belegt Main-Landmark, Skip-Link, eindeutige IDs, lückenlose Überschriften,
benannte native Controls, keine positiven `tabindex`-Werte und nicht nur farbliche Zustände.
Project-Map-Modi sind eine benannte native Togglegruppe; Formulare, Details, Auswahl-, Approval-
und Run-Controls bleiben über Standardtastatur bedienbar. Der zentrale 3-Pixel-Fokusring ist in
der echten Layout-Engine sichtbar. Controls sind mindestens 44 CSS-Pixel hoch, Schrift bleibt in
relativen Einheiten skalierbar, und Reduced Motion schaltet Smooth Scrolling aus sowie Animationen
und Transitions auf einen einzelnen praktisch sofortigen Durchlauf.

Der Browser-Smoke schaltete System, Light und Dark in der realen Layout-Engine, fand und behob eine
helle scoped Agentfläche im Dark Theme und bestätigte nach der Regression null horizontale
Dokumentüberbreite bei von 1.265 auf 640 CSS-Pixel halbierter Breite als reproduzierbares
200-Prozent-Reflow-Äquivalent. Agent Workspace und Settings blieben vollständig lesbar. Prettier,
ESLint, Svelte-Typecheck ohne Warnung, 186 Frontendtests, vier Tooltests und der Produktionsbuild
bestehen; die Markdown-Prüfung bestätigt 52 Dateien und 129 lokale Links. Damit sind alle drei
U9-Akzeptanzkriterien objektiv nachgewiesen und U9 ist abgeschlossen.

Die UX-Überarbeitung verwendet dieselben semantischen Rollen mit einer neutraleren Desktop-Palette,
einer durchgehend serifenlosen UI sowie zentralen Radien von 0,25, 0,35 und 0,5 rem. Numerische
Komponentenradien wurden auf diese Quelle vereinheitlicht; 44-CSS-Pixel-Controls, der 3-Pixel-Fokus,
Reduced Motion und die Kontrastverträge bleiben unverändert. Der reale Browserlauf bestätigte Dark
und Light bei 1.280 × 720 sowie null horizontale Dokumentüberbreite bei 720 × 600 und 640 × 600.
Die Browserkonsole blieb ohne Warnung oder Fehler.

Das kanonische Schwarz-Weiß-Logo unter `docs/logo` speist über den reproduzierbaren Desktop-
`icons`-Befehl den vollständigen Tauri-Icon-Satz. Die Bundle-Konfiguration bindet die nativen PNG-,
ICO- und ICNS-Varianten explizit ein, sodass Anwendungsfenster, Taskleiste und durch spätere
Plattformpakete erzeugte Verknüpfungen dieselbe A^3-Identität verwenden. Das WebView-Dokument nutzt
die kleinste generierte PNG-Variante als Favicon; die Installer-Erzeugung bleibt davon unabhängig
und weiterhin deaktiviert.

Die Abnahme erzeugte den Satz über den öffentlichen `icons`-Befehl neu, bestand drei gezielte
Icon-Vertragstests sowie den vollständigen Frontend-Gate mit 209 Tests und baute die native
Windows-Release-EXE ohne Bundle. Das aus dieser EXE extrahierte 32-Pixel-Icon stimmt pixelgenau mit
der generierten PNG-Variante überein.

## U10 Frontend Performance

- [x] große Editor- und Graphmodule lazy laden
- [x] Listen virtualisieren
- [x] paginierte IPC-Queries
- [x] Eventbatching
- [x] Renderingprofil für Indexburst
- [x] Memory-Leak-Test bei Projektwechseln

Akzeptanz:

- UI-Blockade und Idle-RAM innerhalb QUALITY_GATES;
- Graph mit großem Repository rendert nur begrenzte Subsets;
- Projektwechsel lässt alte Listener und Buffers frei.

ADR-0025 bindet die U10-Optimierungen an begrenzte Core-Projektionen und den UI-Lebenszyklus.
Agent Workspace, Settings, Inspektor, Approval Center und Graphdarstellung liegen in getrennten
lokalen Lazy-Chunks; der initiale JavaScript-Chunk sank von 420,17 kB roh/117,63 kB gzip auf
279,29 kB roh/77,81 kB gzip. Repository- und Modulbaum behalten nur eine serverseitig auf 50
Einträge begrenzte Seite mit validierter Cursorhistorie. Ein 128.000-Zeilen-Diff rendert durch
Fensterung höchstens 20 Zeilen mit absoluten ARIA-Indizes; der Modulgraph rendert höchstens den
angeforderten 50-Knoten-Ausschnitt und zeigt alle Core-Trunkierungen weiter an.

Der Mount-gebundene `UiScheduler` fasst 10.000 gleichartige Commits auf genau einen Rendercommit
zusammen, lässt pro Pollquelle höchstens einen laufenden und einen vorgemerkten Lauf zu und verwirft
alte Frames über eine monotone Projektgeneration. Unit- und Component-Verträge prüfen zusätzlich
idempotenten Timer-/Observer-Cleanup, Vor-/Zurück-Paging und das Freigeben von Graphauswahl sowie
Evidence beim Projektwechsel.

Das reale Chromium-151-Profil über 30 Samples mit je 10.000 Indexereignissen ergab Enqueue-P95
1,1 ms, Interaktions-P95 1,3 ms und null Long Tasks. Das native Releaseprofil über 15 Sekunden
Warm-up und 30 Ein-Sekunden-Samples maß für den vollständigen achtteiligen A^3-/WebView2-
Prozessbaum 121,475 MiB privaten residenten Idle-Median und 122,734 MiB Sample-Peak; ein beobachteter
`ollama`-Prozess blieb durch die Prozessbaumgrenze getrennt. Damit bestehen die 100-ms- und
200-MB-Budgets lokal. Formatter, ESLint, Svelte-Typecheck ohne Warnung, 197 Frontendtests, vier
Tooltests, der Produktions- und native Releasebuild sowie 53 Markdowndateien mit 139 lokalen Links
bestehen. Alle U10-Akzeptanzkriterien sind objektiv nachgewiesen und U10 ist abgeschlossen.

Der Produktionsbuild nach der UX-Überarbeitung hält Agent Workspace, Settings, Inspector, Approval
Center und Graph weiterhin in getrennten lokalen Lazy-Chunks. Der initiale Chunk liegt nun bei
293,90 kB roh beziehungsweise 82,17 kB gzip; die vorhandenen U10-Budgetclaims werden dadurch nicht
als neue Messung ersetzt. Eine zusätzliche native Tauri-Prüfung mit aktivem Worktree belegt die
getrennten Projects-, Map-, Agent- und Settings-Arbeitsbereiche, den einzeiligen globalen
Statusstreifen, den begrenzten Settings-Inhalt, das Zurücksetzen der Arbeitsfläche bei Seitenwechsel
und den sicheren Abbruch des modalen Entfernungsdialogs.

## U11 Visuelle Code Map

Abhängigkeiten: U3, U4, U9, U10, ADR-0030

- [x] eine map-first Arbeitsfläche statt Recherche-, Explorer-, Modul- und Mapping-Unterseiten
- [x] begrenzter deterministischer Architektur-Atlas mit Mappingstatus und Relationsrouten
- [x] gemeinsame Code-Suche und Task-Lens-Fokus
- [x] kontextabhängiger Inspector mit Entry Points, Tests, Claims und Evidence
- [x] evidence-gebundene begrenzte Source-Vorschau
- [x] minimalistische Deep-Map-Leiste und dauerhaftes content-freies Laufjournal
- [x] responsive und vollständig tastaturbedienbare Map bei begrenztem DOM

Akzeptanz:

- Nutzer erkennt beim Öffnen Module, Abhängigkeiten, Mappingstatus und Begrenzungen ohne eine
  technische Unterseite auswählen zu müssen;
- Suche, Task Lens, Moduldetails, Runtime-Flows und Evidence fokussieren dieselbe Kartenfläche;
- Deep Map zeigt fünf verständliche Phasen, aktuelle Erkundung, modulgruppierten Plan,
  ausschließlich veröffentlichte Cards und die exakte Atlas-Wirkung; es startet weiterhin nur
  explizit und ein Current-Index verursacht keine erneuten Modellkosten;
- Source-Vorschau akzeptiert keine freien Pfade und liefert ausschließlich aktuellen,
  evidence-gebundenen Plain Text innerhalb der festen ADR-0030-Grenzen;
- Übersicht, Fokus und paginiertes Laufjournal überschreiten weder Core-, IPC-, DOM- noch
  100-Millisekunden-Interaktionsgrenzen.

Nicht-Ziele sind ein Vollgraph, Force-Simulation, Code-Stadt, rohe Modelltranskripte,
Zwischenpublikation unbestätigter Claims, vollständige Dateien oder eine allgemeine
WebView-Dateisystemfähigkeit. Checklistenpunkte werden erst nach den objektiven U11-Verträgen und
dem vollständigen Quality Gate abgehakt.

Lokale U11-Abnahme vom 2026-08-27: Das reproduzierbare Browserprofil
`apps/desktop/performance/u11-map-atlas.html` rendert die produktive `MapWorkspace` mit 64 Regionen,
128 Routen und einem 32-Ereignis-Feed. Unter Chrome 151 auf Windows lagen die synchronen
Main-Thread-Zeiten bei 3,7 ms für Mount, 0,7 ms für Auswahl, 0,2 ms für Zoom und 0,2 ms für Pan;
der Feed-Commit benötigte 19,8 ms. Es trat kein Long Task auf, und die fokussierte Ansicht blieb mit
660 DOM-Knoten unter der festgelegten 1.500-Knoten-Grenze. Die responsive Browserprüfung bestand
bei 1.440 × 900, 720 × 520 und 680 × 760 flächenfüllend ohne Außenradius, Außenrahmen oder
Horizontaloverflow, mit 3-Pixel-Fokus, Drawer-Inspector und ohne sichtbares Bedienelement unter
44 × 44 Pixel. Der Produktionsbuild trennt die Map in einen
42,64-kB-JavaScript-/23,80-kB-CSS-Lazy-Chunk; der initiale JavaScript-Chunk beträgt 201,69 kB.
Diese Messung belegt das aktuelle Budget, behauptet aber keine Verbesserung gegenüber der
entfernten Legacy-Oberfläche. Der native Windows-Smoke bestand mit einem echten 976 × 719 großen
WebView2-Fenster und einem 50.500-Byte-Screenshot.

Deep-Map-Livekorrektur vom 2026-09-03: Ein angenommener Start öffnet das Informationssystem und
wählt den neu erzeugten Lauf auch dann, wenn zuvor ein alter Lauf betrachtet wurde. Dashboard,
Module, Verlauf und aufgeklappte Planschritte werden nicht überlappend aktualisiert; die konkrete
sichere Aktivitätsklasse wird in einem verständlichen Satz erklärt. Ein temporär blockierter
Publikations-Read überblendet keinen Manager-Lifecycle mehr als falsches `Fehlgeschlagen`, sodass
kompakte Leiste, Gesamtzustand und spätere Card-Publikation denselben Lauf konsistent abbilden. Die
laufenden Map-, Journal- und Card-Zugriffe verwenden getrennte Transaktionskontexte; damit kann ein
Publish keine Live-Abfrage durch eine bereits aktive Transaktion derselben Connection ausschließen.

## U12 Progressiver Code Atlas

Abhängigkeiten: U11, ADR-0025, ADR-0030, ADR-0031

- [x] semantischer Zoom Projekt → Modul → Datei → Typ/Symbol mit Breadcrumb
- [x] verschachtelte, deterministische Regionen und begrenzte Architekturrouten
- [x] getrennte Caller-/Callee-, Test- und Datenfluss-Szenen
- [x] vollständige feste 50er-Inventarseiten im Inspector
- [x] exakt evidence-gebundene Claim-Badges und verifizierte Modulhinweise
- [x] aktuelle File-, Symbol- und Relationsevidence in der sicheren Source-Vorschau
- [x] responsive, tastaturbedienbare Karte innerhalb der U10-DOM- und Interaktionsbudgets

Akzeptanz:

- Nutzer können ohne Unterseiten vom Projekt in ein Modul, von dort in eine Datei und weiter zu
  einem Typ oder zentralen Symbol zoomen; Klick wählt, Enter, Doppelklick oder `Öffnen` vertieft,
  Escape und Breadcrumb navigieren zurück; Selbstrelationen eines Namespace-/Modulsymbols
  erzeugen weder auf Dateiebene einen Self-Parent noch auf Symbolebene ein dupliziertes Zentrum,
  und ein abgewiesener Detail-Read bietet neben dem gebundenen Retry immer den sicheren Rückweg zur
  weiterhin aktuellen Projektübersicht;
- Projekt-, Modul-, Datei- und Symbolebene halten die festen Grenzen 64/128, 32 Dateien,
  48 Symbole sowie Zentrum plus 31 direkte Nachbarn ein; 16 Boundary-Stubs und 31 Flow-Ziele sind
  zusätzliche feste Obergrenzen; werden mehr Boundary-Stubs gezählt als gerendert, melden sowohl
  Boundary- als auch Relationsmetadaten die Kürzung konsistent zur tatsächlich gelieferten Szene;
- die Standardkarte zeigt ausschließlich `Imports`, `Exports`, `Implements`, `Extends`, `Builds`
  und `Configures`; `Calls`, `Tests` und `Reads/Writes` öffnen getrennte evidenzgebundene Flows;
- sämtliche ausgelieferten Gesamtzahlen, nicht zugeordneten oder ungelösten Ziele,
  Inspektionsgrenzen und Trunkierungen bleiben sichtbar; die WebView erhält niemals einen
  Vollgraph;
- die Karte routet die 24 stärksten Übersichtskanten und bei Auswahl bis zu 32 relevante inzidente
  Kanten über deterministische, rechtwinklige Korridore; vollständige Relationszahlen bleiben im
  Read-Model und Inspector erhalten;
- die Kartenfläche lässt sich zusätzlich zu den Scrollleisten durch Ziehen verschieben und per
  Mausrad am Zeigerpunkt zwischen 50 und 200 Prozent zoomen; die nichtgrafische Zusammenfassung
  bleibt als viewport-festes Fenster unten links unabhängig von Pan und Zoom erreichbar;
- der Desktop-Inspector bildet mit der Karte ein horizontales Split-Panel und lässt sich per Maus
  oder Tastatur zwischen 320 und 720 Pixeln verbreitern; auf schmalen Ansichten bleibt er ein
  kantiger Drawer. Identität, verständliche Statusangaben, relevante Kennzahlen und Primäraktionen
  stehen vor progressiv geöffneten Verbindungen, Inventaren, Flows und technischen Nachweisen;
- Inventare halten genau eine feste 50er-Seite im Renderzustand und akzeptieren nur einen aktuellen,
  publikations- und scopegebundenen Cursor;
- File-, Symbol-, Kanten- und Boundary-Auswahlen tragen nur dann Claim-Badges, wenn ihre aktuelle
  Evidence-ID exakt in einem aktuellen verifizierten Claim vorkommt;
- Source-Vorschauen verwenden ausschließlich eine zuvor vom Core ausgegebene aktuelle File-,
  Symbol- oder Relationsevidence-Auswahl und behalten sämtliche ADR-0030-Grenzen;
- Navigation, Suche, Task Lens und Polling starten keine Modellarbeit. Deep-Map-Inhalte erscheinen
  erst nach Verifikation und atomarem Publish.

Nicht-Ziele sind ein Vollgraph, Force-Simulation, Code-Stadt, Laufzeittelemetrie,
Netzwerkauflösung externer Pakete, neue Sprachadapter, vollständige Dateien in der WebView oder
fachlich persistierter Karten-/Zoom-/Filterzustand. Checklistenpunkte werden erst nach den
objektiven U12-Verträgen, dem reproduzierbaren Browserprofil und dem vollständigen Quality Gate
abgehakt.

Lokale U12-Abnahme vom 2026-08-27: Das aktualisierte Browserprofil
`apps/desktop/performance/u11-map-atlas.html` rendert die produktive progressive `MapWorkspace` mit
64 Modulen, 128 Relationsgruppen, 32 Dateien, 48 Symbolen, 31 Flow-Zielen und 32 Deep-Map-
Ereignissen. Im aufgewärmten Chrome 151 auf Windows lagen die jeweils schlechteren Messwerte aus
720 × 520 und 680 × 760 bei 1,8 ms Mount, 0,3 ms Auswahl, 0,5 ms semantischem Zoom, 0,1 ms Pan und
26,5 ms Feed-Commit. Es trat kein Long Task auf; die komplexere Drawer-Ansicht blieb mit 499
DOM-Knoten unter der 1.500-Knoten-Grenze. Beide Mindestgrößen füllten die verfügbare Fläche exakt
und blieben ohne Außenrahmen, Außenradius, Horizontaloverflow oder Browserfehler; bei 680 Pixeln
öffnete der 390-Pixel-Inspector als kantiger Drawer. Der native Produktionsbuild trennt die Map in
einen 63,52-kB-JavaScript-/18,26-kB-CSS-Lazy-Chunk; der initiale JavaScript-Chunk beträgt 152,66 kB.
Der native Windows-Smoke bestand mit einem echten 976 × 719 großen WebView2-Fenster, 35
Stichprobenfarben und einem 52.407-Byte-Screenshot. Die Messung belegt das aktuelle Budget, aber
behauptet keine Verbesserung gegenüber U11.

## U13 Chatbasierter Agent Workspace

Abhängigkeiten: ADR-0033, Gate M8

- [x] eine Agent-Seite mit Sessionverlauf, Conversation und Inspector
- [x] projektlokale revisionierte Sessions und inhaltsfreie Layoutpräferenzen
- [x] geschlossene `Ask`-, `Plan`- und `Agent`-IPC-Verträge
- [x] serverseitige Suche, Archivierung, Rename und Presentation Delete
- [x] Scheduler-Eigentum, Fortschritt, Cancellation und Projektwechsel-Quiesce für Conversationjobs
- [x] evidence-gebundener Ask-Lauf mit ausschließlich read-only Werkzeugen
- [x] nachvollziehbarer adaptiver Ask-Rechercheweg mit aktueller TODO-/FIXME-Suche
- [x] persistente verwendete und zusätzlich bereitgestellte Ask-Quellen mit sicherer Vorschau
- [x] gemeinsame endliche Mehr-Runden-Recherche für Ask, Plan und Agent-Vorbereitung
- [x] pro Nachricht wählbare Tiefe, öffentliche Arbeitsnotizen und explizite Fortsetzung
- [x] Core-gesteuerte Evidence-Vertiefung, abschnittsweise Dateireads und begrenztes Retry
- [x] priorisierter Recherchekontext mit deterministischer Zielauflösung und adaptiver Reserve
- [x] ruhige Chatposition und fortsetzbarer Modell-/Diagrammabschluss nach begrenztem Repair
- [x] modusgefilterte Slash-Command-Palette mit Tastatursteuerung und festen Profilen
- [x] evidence-gebundene Diagramme mit sicherem lokalen Rendern und nativem SVG-/PNG-Export
- [x] Mermaid-sichere Flowchart-Labels mit V32-Kompatibilität und evidence-gebundener Neuerzeugung
- [x] sichtbare nächste Moduswahl mit erneuter Planfreigabe nach unterbrochener Agent-Kontinuität
- [x] begrenzte dauerhafte FIFO für Folgenachrichten mit Pause und expliziter Wiederaufnahme
- [x] taskgebundene Agentenlauf-Seitenleiste ohne doppelte Rechercheprojektion
- [x] exakte Planfreigabe materialisiert Goal, Ledger und Run
- [x] produktiver AgentRunExecutor führt typisierte Actions bis Approval, Recovery oder Review aus
- [x] adaptiver, schrittweise verifizierter Agent-Arbeitsplan mit begrenztem Replan
- [x] Component-, Accessibility-, responsive und native UX-Verifikation

Akzeptanz:

- Ask kann Repositoryfragen beantworten, ohne eine mutierende Capability zu besitzen;
- einfache Fragen bleiben bei einem Modellturn; nur offene Evidence-Lücken lösen innerhalb des
  gewählten festen Standard-/Gründlich-Profils weitere begrenzte Read-only-Runden aus;
- eine vom Modell ausgewiesene Evidence-Lücke oder eine noch nicht gelesene eindeutig genannte
  Indexdatei kann keine vorschnelle Antwort abschließen; große Dateien werden bei Bedarf über
  weitere gebundene Seiten gelesen und transiente Fehler setzen begrenzt am betroffenen Schritt
  fort;
- jeder laufende und abgeschlossene Turn zeigt in Ask, Plan und Agent-Vorbereitung verständlich
  Suche, Task-Lens-Auswahlgrund, öffentliche Befunde, offene Lücken, Vollständigkeit und die für das
  Ergebnis angegebenen Quellen; neue Live-Schritte
  erscheinen nacheinander in einer verbundenen Timeline mit genau einem sichtbaren aktiven Schritt;
- Plan hält Rückfragen und jede vollständige Planrevision im Verlauf und startet erst nach Review;
- Agent bindet jede Ausführung an Goal, Ledger, Run, Snapshot und aktuelle Evidence und zeigt
  Activity, Inspection, Approval und Verification in derselben Session;
- Follow-ups behalten den projektlokalen Zusammenhang, während eine neue Session ohne fachlichen
  Altzustand beginnt;
- Verlauf und Inspector sind tastaturbedienbar, begrenzt, persistent anpassbar und bei schmalen
  Fenstern als Drawer vollständig nutzbar.

Nicht-Ziele sind ein offener Chat-Loop, rohe Chain-of-Thought- oder Providertranskripte,
WebView-Datei-/Shell-/SQL-Zugriff, automatische Veröffentlichung sowie das Löschen von Auditdaten.

Abnahme U13 vom 2026-08-29: Der neue Workspace ersetzt die internen Agent-Unterseiten durch eine
projektlokale Conversation mit Sessionverlauf und einem gemeinsamen Inspector für Fortschritt,
Änderungen, Verification und Approval. Ask liest ausschließlich aktuelle, hashgebundene
Repository-Evidence; Plan trennt Rückfragen von unveränderlichen Planrevisionen; erst die exakte
Review-Aktion materialisiert Goal, Ledger und Run. Der produktive Executor verwendet den
vorhandenen Context Compiler, die typisierten Read-/Mutation-Actions, zentrale Policy, Approval,
Reindex und Acceptance bis zu einem autoritativen terminalen Zustand. Die Component- und
IPC-Tests decken die drei Modi, exakte Revisionen, den vollständigen Open-bis-Done-Workflow und
begrenztes textuelles Markdown ab. Die visuelle Browserprüfung bestand bei 1440 × 900, 1024 × 768
und 720 × 800 ohne Horizontaloverflow; Verlauf und Inspector wurden bei schmaler Breite als
vollständig nutzbare Drawer geprüft. Der native Windows-Smoke bestand mit dem aktuellen
Releasebinary in einem 1296 × 839 großen WebView2-Fenster, 35 Stichprobenfarben und einem
65.764-Byte-Screenshot. Die Windows-Aufnahme wartet dabei explizit auf den WebView-Paint und lehnt
eine bloße schwarze Titelleiste bei der manuellen Sichtkontrolle als ungültige Abnahme ab.

OpenAI-Laufzeitkorrektur vom 2026-08-29: Schemafreie Ask-/Plan-Antworten und strukturierte
AgentAction-Ausgaben laufen beide über den nativen Responses-Adapter. Ablehnung, ungültige Antwort,
Timeout und Nichterreichbarkeit bleiben bis zur Conversation-Grenze getrennte geschlossene
Fehlerklassen. Die Session zeigt dazu sichere konkrete Abhilfe statt des bisherigen pauschalen
`nicht verfügbar`, ohne Providertexte, Promptinhalte oder Credentials zu übernehmen.

Erweiterung vom 2026-09-03 nach ADR-0037: Ask bindet sich vor dem ersten Modellturn an einen
veröffentlichten Index, liest Symbol- und Span-Treffer an ihrer relevanten Stelle und durchsucht bei
konkreten TODO-/FIXME-Fragen bis zu 2.000 sicher lesbare aktuelle Indexdateien. Das strikt
validierte Antwortschema erlaubt entweder eine belegte Antwort oder genau eine weitere Runde mit
höchstens vier geschlossenen Read-only-Aktionen und turnweit einem Reparaturversuch. Knowledge V30
speichert nur inhaltsfreie Rechercheevents, File-Revisions und Zitatbezüge; Antwort, Zitate und
terminales Event committen atomar. Die aufklappbare Live-Karte bleibt nach Abschluss als
„Recherche & Quellen“ erhalten, unterscheidet vollständige von begrenzten Suchen und sperrt
Source-Vorschauen nach einem Indexwechsel. Rohes Chain-of-Thought, Providertranskripte, Prompts,
Modellrohantworten und persistierter Quelltext bleiben ausgeschlossen.

Timeline-Erweiterung vom 2026-09-03: Die append-only V30-Ereignisse werden während eines aktiven
Ask-Turns in höchstens 900 Millisekunden schrittweise eingeblendet. Eine verbundene Statuslinie
kennzeichnet erledigte, aktive, fehlgeschlagene und abgebrochene Arbeit auch textuell; nur der
aktuelle sichere Stand wird per `aria-live` angekündigt. Reduced Motion überspringt Staffelung und
Animation. Nach einem frisch beobachteten Abschluss bleibt der terminale Zustand 700 Millisekunden
sichtbar und die Karte klappt anschließend einmalig ein, ohne spätere Nutzerinteraktion zu
überschreiben. Persistenz und V1-Reads bleiben unverändert.

Mehr-Runden-Erweiterung vom 2026-09-03 nach ADR-0038: `Standard` und `Gründlich` wählen pro
Nachricht einen festen endlichen Entscheidungs-, Read- und Zeitrahmen. Der gemeinsame
Application-Controller kann Evidence-Lücken über mehrere sequenzielle Such-, Source-, Relations-
und Verzeichnisreads schließen, dedupliziert identische Aktionen und beendet zwei stagnierende
Runden ehrlich mit einer expliziten Fortsetzungsaktion. Knowledge V31 speichert öffentliche
Arbeitsnotizen mit ihren Source-Ketten für Ask, Plan und Agent-Vorbereitung; V30 bleibt als Legacy
lesbar. Der Composer, Conversation-Block und Inspector verwenden dieselbe progressive Timeline.
Rohes internes Denken bleibt ausgeschlossen.

Recherchekontext-Korrektur vom 2026-09-05 nach ADR-0044: Eindeutig genannte aktuelle Dateien
werden vor der aktuellen, auf zwölf gelesene Ziele begrenzten Task Lens aufgebaut; historische
Quellen folgen mit höchstens acht Einträgen nur nachrangig und werden bei expliziten Dateizielen
nicht pauschal übernommen. Der Modellkontext zeigt die Core-aufgelöste Zuordnung aus Anfrage,
Repositorypfad und `S`-Quelle. Spätere adaptive Reads besitzen reservierten Kontextplatz;
dateihaltige `searchIndex`-Aktionen lesen das eindeutige Ziel direkt. Die vorhandenen Standard-,
Gründlich-, Stagnations- und Sicherheitsgrenzen bleiben unverändert.

Fortsetzungs- und Schleifenkorrektur vom 2026-09-05: Eine explizite Recherchefortsetzung behält
das ursprüngliche Ziel und übernimmt bevorzugt die revalidierten Quellen und öffentlichen
Arbeitsnotizen des letzten Abschnitts. Sie führt bei vorhandenen Quellen nicht erneut die breite
Basisrecherche aus. Normale Fortsetzungsnachrichten zeigen im Chat nur „Recherche fortsetzen“
statt die Frage mehrfach auszuschreiben; die ursprüngliche Frage bleibt dauerhaft rekonstruierbar.
Pro Modellrunde wird nur ein aktuelles Evidence-Paket kompiliert; identische Reads blähen den
Kontext nicht auf und zählen nicht als neuer Fortschritt. Eine letzte Auswertung nach Stagnation
und bereits validierte Teilantworten bleiben als zitierter Zwischenstand mit ausdrücklicher
Fortsetzungsaktion erhalten. Die Recherche beginnt keine neuen Budgets ohne Nutzeraktion.

Abschluss- und Scrollkorrektur vom 2026-09-05: Die Conversation positioniert einen ausdrücklich
gestarteten oder geöffneten Turn einmalig, folgt danach aber nicht mehr jeder progressiven
Höhenänderung des Rechercheblocks. Manuelles Scrollen und jede Pointer-, Touch- oder
Wheelinteraktion behalten deshalb stabil die Nutzerposition. Erschöpfte strukturierte Repairs
enden mit erhaltenen Quellen in `AwaitingContinuation`; die Diagrammphase reserviert eine zweite
Formatierungsentscheidung und bewahrt bei erneutem Fehler die bereits validierte Antwort samt
Quellen, statt den ganzen Lauf als fehlgeschlagen zu markieren.

Diagramm-Folgeturn-Korrektur vom 2026-09-05: Ein veröffentlichtes Diagramm bleibt dauerhaft an der
zugehörigen Antwort montiert. Das Polling einer späteren Ask-Recherche invalidiert weder seine
Artefaktprojektion noch startet es den lokalen Mermaid-Renderer erneut. Historische
Rechercheprojektionen erhalten ebenfalls keinen fremden Live-Refresh. Damit verschwindet kein
großer Inhaltsblock oberhalb des laufenden Turns und der Nachrichten-Viewport behält auch in der
Folge `/diagram` → normaler Ask seine Höhe und Nutzerposition.

Slash-Command-Erweiterung vom 2026-09-04 nach ADR-0039: Ein `/` öffnet den vom Core gelieferten,
modusgefilterten Katalog. Hauptauftrag und höchstens zwei Linsen erscheinen als entfernbare Chips;
die feste Tiefe kann für diese Nachricht nicht überschrieben werden. Der Rust-Core validiert
erneut, bevor die dauerhaft sichtbare Nachricht und ihr besessener Job entstehen. `/diagram`
zeigt bis zu drei aus aktueller Evidence Core-kompilierte Diagramme inline. Mermaid wird lokal lazy
im Strict Mode geladen und das SVG zusätzlich sanitisiert. SVG-/PNG-Export wählt das Ziel nur im
nativen Rust-Dialog, validiert den gerenderten Inhalt erneut und gibt der WebView keinen Pfad.
Knowledge V32 hält Command-Aufruf und Artefaktquellen über Reopen, während Presentation Delete sie
entfernt und ein Indexwechsel den historischen Stand sichtbar kennzeichnet.

Diagramm-Renderkorrektur vom 2026-09-05: Flowchart-Kantenbeschriftungen werden deterministisch
quotiert, sodass Methodensignaturen und Klammern nicht mehr als Mermaid-Syntax gelesen werden.
Eine eng auf die frühere Core-Ausgabe begrenzte Präsentationsnormalisierung hält bereits
persistierte V32-Artefakte darstellbar. Falls Mermaid danach weiterhin einen echten Parsefehler
meldet, startet die sichtbare Neuerzeugungsaktion einen normalen evidence-gebundenen
`/diagram`-Turn aus dem ursprünglichen Auftrag; rohe Mermaid-Ausgabe bleibt außerhalb der
Modell- und Ausführungsgrenze.

Arbeitsweg-Beruhigung vom 2026-09-04 nach ADR-0040: Der neueste Turn behält vom ersten Live-Event
bis zur Antwort dieselbe Rechercheinstanz. Timeline und Inspector verwenden eine gemeinsame
progressive Projektion mit Vorbereitung, nummerierten Runden und Abschluss. Der redundante
Statuskasten entfällt; erfolgreiche Läufe klappen nach sichtbarer Antwort einmalig ruhig ein.
Antwortmarker zeigen Core-zugeordnete Dateinamen und Zeilen, öffnen die sichere Vorschau und die
Quellenliste trennt verwendete von zusätzlich gefundenen Quellen. Kohärente Projektionsreads
verhindern, dass bekannte Quellen bei einem parallelen Poll fälschlich als leer erscheinen.

Scroll- und Quellendichte-Korrektur vom 2026-09-04: Der Conversation-Viewport folgt wachsenden
Live-Schritten nur, solange er am Ende steht. Aufwärts-Scrollen oder eine Touch-Geste löst die
Bindung sofort; erst das bewusste Zurückkehren ans Ende aktiviert sie erneut. Browser-eigenes
Scroll-Ankern ist in diesem Viewport deaktiviert, damit Timeline-Wachstum keinen wechselnden Anker
erzeugt. Inline-Zitate bleiben vollständig zuordenbar, erscheinen aber typografisch zurückhaltend.
Verwendete Quellen werden als kompakte, umbrechende Verweise gezeigt; zusätzliche Quellen bleiben
hinter einer gezählten Disclosure, bis der Nutzer sie öffnet.

Live-Projektionskorrektur vom 2026-09-04: Sobald ein Rechercheweg für einen Turn sichtbar ist,
bleibt dieser letzte vollständige Stand bei `updating`, vorübergehend fehlenden Daten und
verkürzten oder nicht append-only Poll-Ergebnissen unverändert im DOM. Nur eine monotone
Erweiterung darf Timeline und Session-Tail ersetzen. Dadurch kann ein paralleler Read den Block
nicht mehr ausblenden, auf einen Schritt verkürzen oder den Conversation-Viewport periodisch in
der Höhe verschieben. Quellenpages werden vor dem sichtbaren Austausch vollständig zusammengesetzt;
die Live-Timeline bleibt bis zum terminalen Einklappen als dieselbe Komponenteninstanz erhalten,
ohne eine gemessene Layout-Höhe zu konservieren. Eine manuell gelöste Scrollbindung wird nur nach
einer bewussten Abwärtsbewegung bis ans Ende wieder aktiviert, nicht durch programmatisches Scrollen
oder ein schrumpfendes Layout.

Modus- und Queue-Erweiterung vom 2026-09-04 nach ADR-0041: Die stets sichtbare Stufenleiste trennt
den laufenden Modus vom Ziel der nächsten Nachricht. Weitere validierte Nachrichten werden in
Knowledge V33 begrenzt und FIFO-geordnet vorgemerkt, nach Erfolg automatisch verarbeitet und an
menschlichen Haltepunkten beziehungsweise nach Fehler oder Abbruch angehalten. Nach Neustart ist
eine ausdrückliche Fortsetzung nötig. Ein Rückwechsel aus Agent entfernt die Ausführbarkeit des
früheren Plans; der nächste Agent-Auftrag endet zunächst an einer neuen Planfreigabe. Recherche
bleibt ausschließlich im Chat. Fortschritt, Änderungen und Review erscheinen in der rechten
Seitenleiste erst mit einer materialisierten Task und der Header ordnet Menü, Laufsteuerung und
Seitenleistenschalter ohne Überlagerung an. Terminale Agent-Runs belegen den Planstart nicht mehr;
die kurze Abschlussübergabe wartet auf den noch auslaufenden Conversationbesitzer.

Agentstart-Recovery-Korrektur vom 2026-09-04: Planfreigabe, Task-Materialisierung und
AgentRun-Status verwenden nun ausschließlich zum jeweiligen Eintrag passende Anker; insbesondere
tragen Aktivitäts- und Abschlussmeldungen niemals die nur für Planartefakte zulässige
Planrevision. Der Planstart veröffentlicht vor der Materialisierung einen sichtbaren
Vorbereitungsschritt. Nach einem Prozessabbruch werden laufende Vorbereitungen ohne gebundenen
Task beim nächsten Projektstart deterministisch beendet; eine unterbrochene Planfreigabe kehrt
mit unveränderter Planrevision zu `AwaitingPlanReview` zurück. Damit bleiben weder unsichtbare
`Running`-Sessions noch ein wirkungsloser Abbruch zurück. Wurde Task, Ledger und Run bereits
atomar angelegt, bevor die Session-Verknüpfung scheiterte, übernimmt ein erneuter Planstart genau
diese index-, modell- und research-gebundenen Anker statt einen konkurrierenden zweiten Task zu
erzeugen. Nur ein unveränderter, noch ausführbarer `Execute`-Stand darf übernommen werden.
Plan- und Agent-Vorbereitung verwenden außerdem denselben geschlossenen `QUESTION:`-/`PLAN:`-
Vertrag: Eine tatsächlich blockierende Richtungsentscheidung wird als konkrete Rückfrage sichtbar
und benötigt keine erfundene Quellenangabe; nur ein vollständig strukturierter und aktuell belegter
Plan darf Task, Ledger und Run materialisieren.

Adaptive-Arbeitsplan-Erweiterung vom 2026-09-04 nach ADR-0042: Normale Plan- und Agentantworten
materialisieren ihre geordneten Änderungs- und Testpunkte als einzelne dauerhafte Todos. Der
Agentenlauf arbeitet sie sequenziell ab, kann nach neuer Evidence ein zusätzliches Planlücken-Todo
einschieben und offene Nachfolger als neue Ledger-Revision ersetzen. Die Arbeitsplanansicht zeigt
den aktuellen Schritt, den verständlichen Status, erledigte Schritte und die Planrevision in
topologischer Reihenfolge; eine angepasste Revision wird ohne technische Store-ID erklärt.

Abnahme der Slash-Command-Erweiterung vom 2026-09-04: 61 Frontend-Testdateien mit 294 bestandenen
Tests prüfen zusätzlich Palette, Tastaturnavigation, fail-closed Katalog-Retry, Chips, feste Tiefe,
lokales Lazy Rendering, Sanitizer, Render-Retry und path-freien Export. Formatcheck, ESLint,
Svelte-Typecheck mit 0 Fehlern/0 Warnungen, Produktionsbuild und Linkprüfung sind grün. Der Build
hält Mermaid außerhalb des initialen Chunks; der Agent-Workspace bleibt ein separater Lazy-Chunk.

## Gate M8

- [x] vollständiger Nutzerworkflow vom Open bis Done
- [x] Accessibilitygate
- [x] keine generische privilegierte Frontendcapability
- [x] Performancebudget gemessen
- [x] Fehler-, Offline- und Recoveryzustände vorhanden
- [ ] UX-Smoke auf Windows, Linux und macOS

Teilabnahme M8 vom 2026-08-13: Das reproduzierbare E9-Coding-Eval führt fünf reale lokale
Fixtures durch Command-Discovery, Goal/Ledger/Run, zentrale Policy, exaktes AllowOnce, Patch,
atomaren Reindex, kataloggebundenen Offline-Test, frische Verification-Evidence und Acceptance bis
zum dauerhaft gespeicherten `Done`. Der ergänzende Desktop-Vertrag beginnt bei `noProject`, öffnet
den Worktree erst nach dem expliziten Klick, liest den Core-Projektzustand neu und navigiert dann
zum sessiongebundenen Agent Workspace. Terminaler Run, globales `Done` und die Goal-/Ledger-
gebundenen exakten frischen Step-/Evidence-IDs jedes Muss-Beweises bleiben über Session,
Fortschritt und Review gemeinsam erreichbar; die UI erzeugt keinen eigenen Abschlusszustand.

Das U9-Accessibilitygate, die real gemessenen U10-Budgets sowie die vorhandenen textuellen Lade-,
Fehler-, Offline-, Stale- und Recoveryzustände bestehen unverändert. Der exakte
`main-capability`-Contract erlaubt ausschließlich die gelisteten schmalen Produktcommands und
weiterhin keine generische Dialog-, Datei-, Shell-, SQL-, Prozess-, Provider- oder
Netzwerkfähigkeit. Der neue native Plattformharness ist lokal auf Windows gegen das unveränderte
Releasebinary mit einem echten 976 × 719 großen WebView2-Fenster, 132 Stichprobenfarben und einem
185.399-Byte-PNG bestanden. Der vollständige lokale `act`-Matrixjob auf Linux x86_64 bestand
sämtliche Workspace-Tests, den Releasebuild und den echten WebKitGTK-Smoke mit einem visuell
geprüften 960 × 680 großen Fenster, Bildstandardabweichung 0,119231 und einem 75.913-Byte-PNG. M8
bleibt offen, bis derselbe committed Matrixjob zusätzlich auf echten macOS-ARM64- und
macOS-x86_64-Runnern grün ist und deren prozessgebundene Artefakte vorliegen.
