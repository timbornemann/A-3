# A^3 – Autonomous Agent Assistant

Dieses Verzeichnis ist die verbindliche Architektur- und Entwicklungsbaseline für A^3.

Der sichtbare Produktname besteht exakt aus den drei Zeichen **A^3**. Die ausgeschriebene Form lautet **Autonomous Agent Assistant**. In technischen Bezeichnern, in denen das Zeichen ^ ungeeignet ist, wird ausschließlich der Slug **a3** verwendet.

## Ziel

A^3 ist ein schneller, lokaler, plattformübergreifender Coding-Agent für Windows, Linux und macOS. Der Agent soll auch mit kleineren lokalen Modellen und begrenztem Kontext zuverlässig in großen Codebasen arbeiten. Die Qualität entsteht primär durch einen deterministischen Harness, evidenzgebundenes Gedächtnis, kontrollierte Werkzeuge und eine präzise Kontextzusammenstellung.

## Verbindlichkeit

Die Schlüsselwörter **MUSS**, **DARF NICHT**, **SOLLTE** und **KANN** sind normativ:

- **MUSS / DARF NICHT**: nicht verhandelbare Architekturregel
- **SOLLTE**: Standard; eine Abweichung benötigt eine dokumentierte Begründung im Pull Request
- **KANN**: zulässige Option

Bei einem Konflikt gilt folgende Reihenfolge:

1. Sicherheits- und Datenschutzregeln
2. akzeptierte ADRs
3. Architekturregeln
4. fachliche Detaildokumente
5. Entwicklungspläne
6. lokale Implementierungsdetails

Eine Änderung an einer akzeptierten Entscheidung benötigt ein neues ADR, das die alte Entscheidung ausdrücklich ersetzt.

Für den kleineren Agentenkern ist
[ADR-0119](adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md)
angenommen. Die dort genau benannten Teilablösungen und der
[Zielvertrag](SMALL_LOCAL_MODEL_AGENT_CORE.md) gelten für neue Verträge;
fortbestehende Sicherheitsregeln bleiben verbindlich. Implementierte Schema-,
IPC- und SourceGuided-Versionsbeschreibungen dokumentieren weiterhin die
Altbaseline. Nach dem [verifizierten P0](plans/22-P0_BASELINE_STATUS.md) folgen
Kandidaten-Testkompositionen mit isolierten Daten; erst P11 aus
[Plan 22](plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md) schaltet die Produktion um
und migriert aktive Nutzerdaten. Ein dokumentierter Zielvertrag ist keine
Behauptung bereits implementierter Produktfähigkeit.

## Dokumentenkarte

| Dokument | Zweck |
| --- | --- |
| [AGENTS.md](../AGENTS.md) | Dauerhafter Arbeitsauftrag und Verhaltensvertrag für Codex |
| [Produktanforderungen](PRODUCT_REQUIREMENTS.md) | Verbindlicher V1-Funktionsumfang und messbare Produktziele |
| [Architektur](ARCHITECTURE.md) | Systemaufbau, Komponenten, Abhängigkeiten und Laufzeitflüsse |
| [Architekturregeln](ARCHITECTURE_RULES.md) | Verbindliche Regeln für hochwertigen Code und klare Verantwortlichkeiten |
| [Domänenmodell](DOMAIN_MODEL.md) | Begriffe, Aggregate, Zustände und Invarianten |
| [Job-Laufzeit](JOB_RUNTIME.md) | Begrenzte Jobs, Fortschritt, Cancellation, Ereignisse und Shutdown |
| [Index und Projektkarte](INDEXING_AND_PROJECT_MAP.md) | Deterministische Analyse, Deep Map und inkrementelle Aktualisierung |
| [Memory und Context](MEMORY_AND_CONTEXT.md) | Evidenzgedächtnis, Task Ledger und Context Compiler |
| [Daten und Persistenz](DATA_AND_PERSISTENCE.md) | Datenmodell, Speicherorte, Migrationen und Suchindizes |
| [Sicherheit und Ausführung](SECURITY_AND_EXECUTION.md) | Trust Boundary, Freigaben, Prozess- und Dateisystemregeln |
| [IPC-Protokoll](IPC_PROTOCOL.md) | Versionierte Grenztypen zwischen WebView und Rust-Kern |
| [Qualitätsgates](QUALITY_GATES.md) | Tests, Leistungsbudgets und Definition of Done |
| [Continuous Integration](CI.md) | Lokale Gates, GitHub-Actions-Jobs, Plattformmatrix und Lizenzbericht |
| [ADRs](adrs/README.md) | Einzelne, nachvollziehbare Architekturentscheidungen |
| [Entwicklungspläne](plans/README.md) | Reihenfolge, Arbeitspakete und Abnahmekriterien |
| [Kleiner Agentenkern](SMALL_LOCAL_MODEL_AGENT_CORE.md) | Angenommene Zielverträge für Chat, Projektfragen und kleinschrittige lokale Agentenarbeit; Umsetzung nach Plan 22 |

## Nicht-Ziele der ersten stabilen Version

- Kein verteiltes Multi-Agent-System
- Kein zwingender Cloud-Dienst
- Kein autonomes Pushen, Mergen oder Veröffentlichen
- Keine unkontrollierte Shell
- Keine vollständige Kopie jeder Quelldatei in einer Vektordatenbank
- Keine IDE mit vollständigem Funktionsumfang
- Keine Unterstützung jeder Programmiersprache auf gleichem Qualitätsniveau

## Arbeitsweise

Codex beginnt bei [AGENTS.md](../AGENTS.md), wählt genau ein freigegebenes Arbeitspaket aus dem Masterplan und liest anschließend nur die dafür nötigen Detaildokumente und ADRs. Ein Arbeitspaket ist erst abgeschlossen, wenn seine Abnahmekriterien nachweisbar erfüllt sind.
