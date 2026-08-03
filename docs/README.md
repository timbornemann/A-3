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

## Dokumentenkarte

| Dokument | Zweck |
| --- | --- |
| [AGENTS.md](../AGENTS.md) | Dauerhafter Arbeitsauftrag und Verhaltensvertrag für Codex |
| [Produktanforderungen](PRODUCT_REQUIREMENTS.md) | Verbindlicher V1-Funktionsumfang und messbare Produktziele |
| [Architektur](ARCHITECTURE.md) | Systemaufbau, Komponenten, Abhängigkeiten und Laufzeitflüsse |
| [Architekturregeln](ARCHITECTURE_RULES.md) | Verbindliche Regeln für hochwertigen Code und klare Verantwortlichkeiten |
| [Domänenmodell](DOMAIN_MODEL.md) | Begriffe, Aggregate, Zustände und Invarianten |
| [Index und Projektkarte](INDEXING_AND_PROJECT_MAP.md) | Deterministische Analyse, Deep Map und inkrementelle Aktualisierung |
| [Memory und Context](MEMORY_AND_CONTEXT.md) | Evidenzgedächtnis, Task Ledger und Context Compiler |
| [Daten und Persistenz](DATA_AND_PERSISTENCE.md) | Datenmodell, Speicherorte, Migrationen und Suchindizes |
| [Sicherheit und Ausführung](SECURITY_AND_EXECUTION.md) | Trust Boundary, Freigaben, Prozess- und Dateisystemregeln |
| [Qualitätsgates](QUALITY_GATES.md) | Tests, Leistungsbudgets und Definition of Done |
| [ADRs](adrs/README.md) | Einzelne, nachvollziehbare Architekturentscheidungen |
| [Entwicklungspläne](plans/README.md) | Reihenfolge, Arbeitspakete und Abnahmekriterien |

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
