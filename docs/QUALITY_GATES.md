# Qualitätsgates und Definition of Done

Status: verbindliche Baseline  
Stand: 2026-08-05

## Grundsatz

Qualität ist eine überprüfte Eigenschaft. „Sieht korrekt aus“, erfolgreiche Kompilierung oder eine LLM-Einschätzung reichen nicht als Abschlussnachweis.

## Gate pro Änderung

### Rust

- cargo fmt --check
- cargo clippy --workspace --all-targets --all-features mit -D warnings
- relevante Unit- und Integrationstests
- cargo test --workspace --all-features
- Dokumentation für öffentliche APIs und Invarianten

### Frontend

- Formatter
- Linter ohne Warnungen
- TypeScript Typecheck
- Unit- und Component-Tests der Änderung
- Accessibility-Prüfung für neue Interaktionen

### Persistenz

- Migration von leerer DB
- Upgrade aus jeder unterstützten Vorgängerversion
- Rollback des Appstarts bei fehlgeschlagener Migration ohne Datenverlust
- Contract-Tests gegen temporäre DB
- Rebuild trennt regenerierbare und dauerhafte Daten korrekt

### Index und Retrieval

- Golden Fixture für Parseränderungen
- deterministische Wiederholung ergibt identische normalisierte Resultate
- Löschung, Umbenennung und Syntaxfehler getestet
- Graphzyklen terminieren; kürzeste Pfade, Hopgrenze, Resultlimit und Beziehungsevidenz sind getestet
- Retrieval-Eval zeigt keinen unbegründeten Recall-Rückgang
- keine stale Evidence in Facts

### Security Boundary

- Negativtests für Traversal, Symlinks und unerlaubte Roots
- ungültige IPC- und LLM-Payloads abgelehnt
- Approval- und Policy-Tests
- Secret-Redaction-Test
- Prozessabbruch und Outputlimit getestet

## Testpyramide

| Ebene | Zweck |
| --- | --- |
| Domain Unit | Invarianten und Zustandsübergänge |
| Property | Parser-, Pfad-, Hash- und Zustandskombinationen |
| Adapter Contract | gleiche Semantik je Provider oder Store |
| Golden Fixture | stabile Index- und Context-Ergebnisse |
| Integration | DB, Workspace, Modellstub und Controller |
| End-to-End | Desktop-Workflow auf kleinem Fixture-Repo |
| Evaluation | reale Coding-Aufgaben und Retrievalqualität |
| Platform Smoke | Windows, Linux und macOS |

Tests müssen offline und deterministisch laufen, außer explizit markierten optionalen Provider-Benchmarks.

## Referenz-Fixtures

Mindestens:

- kleines Rust-Workspace-Projekt
- TypeScript-Monorepo
- Python-Package
- gemischtes Repository mit generierten und ignorierten Dateien
- Repository mit Symlinks
- Repository mit absichtlichen Parsefehlern
- großes synthetisches Repository für Performance

Fixtures enthalten keine inkompatibel lizenzierten oder vertraulichen Quellen.

## Performancebudgets

Die Budgets gelten auf einer dokumentierten Referenzmaschine mit 8 CPU-Kernen, 32 GB RAM und NVMe; LLM-Server und Modellgewichte werden bei App-RAM separat ausgewiesen.

| Messung | Ziel für V1 |
| --- | ---: |
| Desktop bis interaktiv, warm | P95 ≤ 2 s |
| Idle-RAM ohne Modellserver | ≤ 200 MB |
| Fast Index, 100.000 LOC cold | P95 ≤ 30 s |
| Ein-Datei-Indexdelta | P95 ≤ 2 s |
| exakte oder FTS-Suche | P95 ≤ 100 ms |
| Context Compile ohne LLM | P95 ≤ 300 ms |
| UI-Interaktion während Indexlauf | keine sichtbare Blockade über 100 ms |
| Cancellation-Reaktion | ≤ 500 ms plus Prozessbeendigung |

Diese Zahlen sind Releaseziele. Wird ein Ziel nicht erreicht, braucht der Release eine dokumentierte Abweichung, Messdaten und einen konkreten Folgetask.

S11 besitzt dafür den reproduzierbaren ignorierten Release-Test
`incremental_index_performance::one_file_delta_meets_the_two_second_p95_target`. Das Fixture umfasst
200 Rust-Dateien und 100.000 LOC; jede der 30 Stichproben misst vom gleich großen Ein-Datei-Write über
Watcher-Debounce, Git-Discovery, BLAKE3-Bestätigung, Ein-Datei-Parse, vollständiges Link/Rank und
atomisches libSQL-Publish. Auf Windows 11 Pro, AMD Ryzen 9 5900XT, 32 GiB RAM und Samsung 970 EVO
Plus NVMe wurden am 2026-08-05 P50 1,202 s und P95 1,305 s gemessen; Watcher-P95 betrug 389 ms und
Refresh-/Publish-P95 922 ms. Die gemessene Ausgangsversion mit zeilenweisen SQL-Aufrufen lag bei
P95 15,286 s, ein erster 900-Parameter-Batch bei 14,493 s. Erst höchstens 30.000 Parameter,
1.024 Zeilen pro Cancellation-Checkpoint und transaktionale Retention supersedeter Projektionen
erreichten das Budget. Diese lokale Messung ersetzt nicht die abschließende V1-Referenzmessung auf
der oben definierten 8-Core-Maschine.

R1 besitzt den reproduzierbaren ignorierten Release-Test
`exact_search_performance::exact_symbol_search_meets_the_100_millisecond_p95_target`. Das Fixture
enthält 50.000 Symbole als Projektion von 100.000 strukturellen Zeilen. Auf derselben lokalen
Windows-11-Maschine wurden am 2026-08-05 für den vor R1 notwendigen vollständigen Index-Load mit
anschließendem Namensscan über fünf Samples P50 652,8 ms und P95 656,8 ms gemessen. Die
indexgestützte Exact Query über 30 Samples erreichte nach begrenztem Wiederverwenden vollständig
verifizierter, identitätsgebundener Datenbankhandles P50 37,0 ms und P95 39,7 ms. Die erste Messung
mit erneutem Open, Migration und Integritätsprüfung pro Query lag bei P50 554,0 ms und P95 570,5 ms.
Auch diese lokale Messung ersetzt nicht die abschließende V1-Referenzmessung.

R2 verwendet dasselbe Fixture und denselben Release-Test für eine absichtlich falsch geschriebene
FTS-Query. Die erste breite Trigram-`OR`-Messung lag bei P50 194,1 ms und P95 195,9 ms; eine reine
Reduktion auf 512 nachbewertete Kandidaten erreichte P50 169,1 ms und P95 201,8 ms und verfehlte das
Gate weiterhin. Die begrenzte Ein-Fehler-Abfrage mit zusätzlichem Endanker erreichte am 2026-08-05
über 30 Samples P50 34,9 ms und P95 35,3 ms. Der unveränderte vollständige Index-Load plus Scan lag
in diesem Lauf über fünf Samples bei P50 1,145 s und P95 1,189 s; Exact Search erreichte P50 38,3 ms
und P95 41,5 ms.

Modellmetriken werden separat erfasst:

- Time to First Token
- Prompt-Tokens
- Output-Tokens
- Tokens pro Sekunde
- Toolerfolg beim ersten Versuch
- Taskerfolg

## Retrieval- und Agentenevaluation

Ein versioniertes Eval-Set enthält:

- Symbol finden
- Architekturfrage beantworten
- Fehler lokalisieren
- kleinen Bug beheben
- API über mehrere Module ändern
- Test ergänzen
- Änderung nach zwischenzeitlichem User-Edit fortsetzen
- lange Aufgabe nach Context Compaction fortsetzen

Mindestbedingungen vor V1:

- keine stale Facts in 100 Prozent der Invalidierungstests;
- Goal Contract bleibt in 100 Prozent der Langlauf-Fixtures erhalten;
- keine Mutation außerhalb des erlaubten Roots;
- alle Muss-Aufgaben des Eval-Sets besitzen reproduzierbare Baselines;
- Qualitätswerte dürfen durch einen Release nicht unbemerkt sinken.

## Cross-Platform-Matrix

CI baut und testet:

- Windows x86_64
- Linux x86_64
- macOS Apple Silicon
- macOS x86_64, solange unterstützt und praktikabel

Plattformspezifische Installer werden auf der Zielplattform erzeugt und signiert, sobald Distributionsidentitäten verfügbar sind.

## Definition of Done

Ein Arbeitspaket ist Done, wenn:

- alle Akzeptanzkriterien nachweisbar erfüllt sind;
- Architekturregeln und relevante ADRs eingehalten sind;
- erforderliche Tests existieren und bestehen;
- relevante Performancebudgets gemessen sind;
- Fehler-, Abbruch- und Sicherheitswege getestet sind;
- Dokumentation und Schemas aktuell sind;
- finaler Diff keine fremden Änderungen, Secrets oder Debugreste enthält;
- Restunsicherheiten offen dokumentiert sind.

Ein Checklistenpunkt darf erst danach abgehakt werden.
