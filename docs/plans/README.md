# Entwicklungspläne

Diese Pläne zerlegen A^3 in überprüfbare vertikale Arbeitspakete. Sie sind für die schrittweise Umsetzung mit Codex gedacht.

## Reihenfolge

1. [Masterplan](00-MASTER_PLAN.md)
2. [Foundation](01-FOUNDATION.md)
3. [Storage und Fast Index](02-STORAGE_AND_FAST_INDEX.md)
4. [Retrieval und Project Map](03-RETRIEVAL_AND_PROJECT_MAP.md)
5. [Memory und Agent Harness](04-MEMORY_AND_AGENT_HARNESS.md)
6. [Editing und Verification](05-EDITING_AND_VERIFICATION.md)
7. [Desktop Product](06-DESKTOP_PRODUCT.md)
8. [Hardening und Release](07-HARDENING_AND_RELEASE.md)
9. [Fast-Index-Ablauf- und Wertanalyse](08-FAST_INDEX_FLOW_ANALYSIS.md)
10. [Zusammenhängende Desktop-Arbeitsumgebung](09-COHESIVE_DESKTOP_UI.md)
11. [Verbindliche Rechercheführung](10-RESEARCH_WORK_STATE.md)

## Ausführungsregeln für Codex

- Genau ein Arbeitspaket gleichzeitig auf In Progress setzen.
- Erst Abhängigkeiten und referenzierte ADRs lesen.
- Vor Implementierung Goal, Akzeptanz, Non-Goals und Prüfweg notieren.
- Mit einem kleinen End-to-End-Slice beginnen; keine Sammlung leerer Interfaces erzeugen.
- Checkbox erst nach objektiver Verifikation setzen.
- Bei Architekturkonflikt neues ADR vorschlagen und pausieren.
- Performancepakete benötigen Messung vorher und nachher.
- Sicherheitsgrenzen benötigen Negativtests.
- Nach jedem Meilenstein den Gate-Abschnitt vollständig ausführen.

## Status

Die Checkboxen bilden den geplanten Ausgangszustand ab. Nach Übernahme in das echte Repository werden sie dort als Fortschrittsquelle gepflegt.

## Schätzung

Die Pläne verwenden keine Zeitversprechen. Jeder Abschnitt endet mit einem technischen Gate und kann unabhängig reviewt werden.
