# Qualitätsgates und Definition of Done

Status: verbindliche Baseline  
Stand: 2026-08-03

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

