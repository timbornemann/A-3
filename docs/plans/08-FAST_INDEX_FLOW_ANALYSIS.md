# Plan 08: Ablauf- und Wertanalyse mit „Abläufe“

Status: In Progress

Grundlage: vom Nutzer bestätigter Implementierungsplan vom 2026-09-05 und
[ADR-0045](../adrs/0045-fast-index-function-flow-analysis.md).

## Ziel und Abnahme

Fast Index integriert Ausführungsreihenfolge, einzelne Aufrufstellen, Variablen
und Wertflüsse über mehrere Funktionen für Rust, JS/TS und Python. Eigener
Hauptbereich „Abläufe“ mit aufklappbaren Schritten, Suche, Quellen und
Variablenverfolgung. Ask, Plan und Agent lesen dieselben aktuellen Belege.

Das Fixture A ruft B und danach C auf; B ruft ebenfalls C auf; C startet ein
lokales Node-/Python-Skript X; X ruft D auf. Beide C-Aufrufe müssen getrennte
Aufrufstellen und Wertkontexte behalten. Eine Änderung in X invalidiert jeden
davon abhängigen Beleg. Bedingungen, Rekursion, dynamische Ziele und
Nebenläufigkeit dürfen keine erfundene tatsächliche Ausführungsfolge erzeugen.

## Arbeitspakete

- [x] F1 Gemeinsame Domain-Artefakte und validierte, begrenzte Parserausgabe.
- [x] F2 TypeScript-Schnitt: Parser, Linker, atomische Persistenz und lesbarer Ablauf.
- [x] F3 Rust/Python, Imports/Aliase, Kontrollfluss und gemeinsame Golden-Verträge.
- [x] F4 Lokale Werte und aufrufbezogene Verfolgung über Funktionsgrenzen.
- [x] F5 Node-/Python-Skriptübergänge und unterstützte Argumentübergaben.
- [x] F6 Versionierte Application-/IPC-/Agent-Reads und sichere Source-Vorschau.
- [x] F7 Eigene lazy UI „Abläufe“, Suche, Schritte, Werte, Breadcrumb und Kartenlink.
- [x] F8 Migration, Neustart, Rebuild, konkurrierendes Publish und Security-Verträge.
- [ ] F9 Browser-/Accessibility-/Plattformprüfungen und Leistungsmessungen.
- [ ] F10 Vollständige Qualitätsgates und finaler Diff-/Dokumentationsreview.

## Feste Grenzen und Nicht-Ziele

Dieselbe Fast-Index-Publikation und Knowledge-Datenbank; dieselben sechs Phasen.
Keine Skriptausführung oder Modellarbeit durch Analyse/Navigieren. Kein Shell-
oder PowerShell-Parser, Laufzeittracking, allgemeiner Taint-Analyzer oder
vollständiger Typchecker. Unveränderte Dateien verwenden ihre Parse-Artefakte
erneut. Umfangreiche Funktionsdaten werden gezielt gelesen.

4.096 zusätzliche Elemente/Funktion, 2.000.000/Lauf, 50 Ergebnisse/Antwort,
acht Aufrufkontexte, 4.096 untersuchte Beziehungen und zwei Sekunden/Abfrage.
Kürzung und unbekannte Wirkungen bleiben eigenständige Zustände.

## Prüfungen

Domain-/Adapter-Goldens, echte libSQL-/Filesystem-/Controller-Verträge,
Source-/IPC-Negativtests, Frontend-Decoder, Components und Accessibility.
Vorher/Nachher: 100.000 LOC kalt ≤30 s P95, Delta ≤2 s P95, Suche ≤100 ms P95,
Context ≤300 ms P95, UI ≤1.500 DOM-Knoten und keine Blockade >100 ms.
Abschluss mit den verbindlichen Rust- und Frontend-Gates. Checklisten werden
erst nach objektiver Prüfung abgehakt; offene Nachweise bleiben ausdrücklich offen.

## Arbeitsstand

2026-09-05: F1–F8 implementiert und funktional geprüft. Keine Releaseabnahme:
F9 und F10 bleiben wegen der unten genannten Nachweise und Gate-Abweichung offen.
Die unterstützte konservative Teilmenge und alle sichtbaren Analysegrenzen stehen
in [FAST_INDEX_FLOWS](../FAST_INDEX_FLOWS.md).

### Implementierung und Nachweise

- Domainvalidierung begrenzt IDs, Elternregionen, Quellbereiche, Abhängigkeiten,
  Aufrufstellen und Prozessziele. Das Lesebudget bleibt nach Erschöpfung gesperrt.
- Zehn `function_flow_features`-Tests prüfen die drei Sprachen, Reihenfolge,
  Überschattung, Wertversionen, Abbrüche, separate Handler/Finally, Deferred,
  Analysegrenzen und konservative Bibliotheks-/Skriptauflösung.
- Drei `function_flow_publication`-Verträge verwenden reale Dateisysteme, Parser,
  Linker, libSQL und Application-Reads: getrennte Herkunft und Verwendung über
  beide C-Kontexte bis D in Node/Python, sichere Aliasauflösung, Reopen,
  Refresh/Rebuild und Deep-Map-Inspect mit Quellenbelegen.
- Indexschema V6, Adapterrevisionen und private V34-Flowpersistenz sind umgestellt.
  Die bestehenden gemeinsamen Graph-/Deep-Map-Goldens wurden geprüft und angepasst.
  Migration V33→V34, Rollback, immutable Zeilen, abgebrochener atomarer Publish,
  Cancellation und beschädigte Bodies haben eigene Regressionstests. Die
  bestehenden konkurrierenden Publish-Verträge laufen unverändert mit.
- `ExploreFunctionFlows` liest gezielt, prüft gespeicherte Ziele gegen den aktuellen
  Graphen und verifiziert den Publikationsanker erneut. AgentAction V4 und
  AskResearchDecision V4 nutzen denselben Reader; Ask und Plan teilen den Researcher.
  Historische Schemas bleiben erhalten. Der echte Read-only-Agent-Controller prüft
  die neue Aktion und verweigert Ergebnisse nach einem noch nicht indexierten Edit.
  Ask/Plan sichern alle Quellen und prüfen sie auch bei bereits gelesenen Dateien live.
- Strikte Rust-/TypeScript-Contracts prüfen den pfadlosen IPC, Source-Schritt-IDs,
  fremde Felder, gemischte Runs, widersprüchliche Ziele und Kontextpfade. UI-Tests
  prüfen verzögerte Antworten, Wechsel, Quellen und beidseitige Kartenverknüpfung.
- Der Hauptbereich „Abläufe“ ist lazy, begrenzt auf 50 Zeilen, mit aufklappbaren
  Schritten, Aufrufpfad, Werten und Quellen. Analyse und Bedienung starten weder
  Skripte noch Modelle und erzeugen keinen zweiten Index.

### Lokale Browserprüfung

Windows, Chromium-In-App-Browser; echte `FlowWorkspace`-Komponente mit synthetischem
50-Schritte-/50-Werte-/50-Trace-Fixture. Bei 1280×720 maximal 800 DOM-Knoten und
50 ms längster beobachteter Long Task. Bei 680×760 kein horizontaler Überlauf;
keine Schaltfläche unter 44 Pixel Höhe. „Hinein“ per Tastatur, Breadcrumb,
Wertverfolgung, Quellen und sichtbare Unknown-/Truncated-Zustände geprüft.
Dieser Komponentenlauf ersetzt keinen nativen plattformübergreifenden Desktop-Smoke.

### Messungen und Abweichungen

Lokale Windows-Release-Messungen, offline und ohne parallele Testläufe. Das bestehende
Delta-Fixture enthält 200 Dateien und 100.000 LOC; 30 Samples vom Write bis Publish.
Ausgangsstand: `0f07c7c85cb790549ef9d7cc70c9314a3b6059b9`.

| Messung (P95) | Unveränderter Ausgangsstand | Upgrade | Budget |
| --- | ---: | ---: | ---: |
| Ein-Datei-Delta | 818,572 ms | 1.067,7953 ms (final) | 2.000 ms |
| Vollständiger Index, leerer Analysecache | nicht separat gemessen | 1.211,1741 ms | 30.000 ms |
| Gezielter Flow-Read | zuvor nicht vorhanden | 16,3023 ms | 2.000 ms |
| Exact Search | 48,2691 ms | 44,0475 ms | 100 ms |
| FTS | 40,912 ms | 41,0823 ms | 100 ms |
| Task Lens | 398,2676 ms | 389,7352 ms | 300 ms |
| Vollständiger Context Compile | 602,2532 ms | 601,4486 ms | 300 ms |

Der erste Delta-Nachlauf lag bei 967,1719 ms P95 (etwa 18 Prozent mehr Zeit), der
finale bei 1.067,7953 ms (etwa 30 Prozent mehr Zeit); beide bleiben im Budget.
Finales Delta-P50: 888,1361 ms, Watcher-P95: 437,7247 ms, Refresh-P95: 690,9967 ms.
Kein Geschwindigkeitsgewinn wird behauptet. Ein weiterer ruhiger Upgrade-Lauf ergab für
Exact/FTS 68,8876/60,5979 ms und Context 569,7755 ms: Streuung, kein Beleg einer
Verbesserung. Der Context-Test scheitert ausdrücklich am bestehenden 300-ms-Gate.
Das identische Release-Fixture wurde zusätzlich im sauberen temporären Worktree
des Ausgangscommits ausgeführt und scheitert dort ebenfalls. Die Vergleichskopie
wurde nach Prüfung auf fremde Änderungen entfernt; der Hauptworktree blieb erhalten.

Reproduzieren (aus dem Repository, Performanceprüfungen nicht parallel ausführen):

```powershell
cargo test -p a3-repo-index --test incremental_index_performance --release --offline --locked -- --ignored --nocapture --test-threads=1
cargo test -p a3-storage-libsql --test exact_search_performance --release --offline --locked -- --ignored --nocapture --test-threads=1
```

`cold_index_and_flow_reads_keep_existing_budgets` ergänzt fünf komplette
Indexaufbauten mit jeweils neuer Datenbank und neuem Compiler sowie 30 gezielte
Flow-Reads. „Cold“ bedeutet hier ohne Parse-/DB-Cache, nicht geleerten OS-Dateicache.
Finaler Stand einschließlich Ziel-Revalidierung gegen den aktuellen Graphen:
Cold-P50 1.172,2987 ms/P95 1.211,1741 ms; Read-P50 14,7093 ms/P95 16,3023 ms.
Beide ignorierten Index-Performancetests wurden separat im Releaseprofil ausgeführt
und bestanden. Diese lokalen Messungen ersetzen nicht die Referenzmaschinenabnahme.

### Qualitätsgates und verbleibende Abnahme

Rust-Gesamtlauf `cargo test --workspace --all-features --offline --locked`:
1.026 bestanden, null fehlgeschlagen, 13 explizit ignorierte optionale Prüfungen.
Die separat ausgeführten Performanceprüfungen sind oben ausgewiesen.
`cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
und `cargo fmt --all --check` bestehen. `git diff --check` und `pnpm check:links`
bestehen ebenfalls. Finale Änderungen auf Architekturgrenzen, unbeabsichtigten
Scope, Secrets und generierte Reste geprüft; keine fremden Ausgangsänderungen.

Frontend-Gesamtlauf `pnpm ci:frontend`: bestanden (Formatter, Lint mit null Warnungen,
Typecheck null Fehler/Warnungen, 66 Testdateien mit 356 bestandenen und 14
übersprungenen Tests, fünf zusätzliche Werkzeugtests, Produktionsbuild).
Engine-Warnung: Node 25.6.1 statt gepinntem 24.14.0; pnpm 11.9.0 entspricht dem Pin.
Der Build weist weiterhin auf große Chunks hin. Keine Pakete nachinstalliert.

Offene Freigabenachweise:

- F9: native Windows-WebView-End-to-End-Abnahme, Linux/macOS-Plattformmatrix und
  Referenzmaschinenmessung; bisherige Browserprüfung ist ein Komponentenfixture.
- F10: Frontend-Gates mit Node 24.14.0 wiederholen. Die vorhandene gebündelte
  Version 24.19.0 erfüllt den exakten Pin ebenfalls nicht.
- F10: separater Folgeschritt für den vorbestehenden Context-/Task-Lens-Budgetfehler:
  das obige 50.000-Symbole-Fixture profilieren, wiederholte Datenbank-/Indexreads
  lokalisieren und unter unveränderten Freshness-Verträgen auf ≤300 ms P95 bringen.
  Keine opportunistische Änderung am Context-Compiler in diesem Upgrade; keine
  stillschweigende Ausnahme oder Releasefreigabe.
