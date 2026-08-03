# ADR-0006: Deterministischer Index vor LLM-Kartierung

Status: Accepted  
Datum: 2026-08-03

## Kontext

Ein kleines lokales Modell kann kein großes Repository vollständig lesen oder verlässlich aus freier Exploration rekonstruieren. Eine reine LLM-Zusammenfassung ist nicht reproduzierbar und veraltet unbemerkt.

## Entscheidung

- Jeder Projektanalyse geht ein deterministischer Fast Index voraus.
- Fast Index umfasst Discover, Hash, Parse, Link, Rank und atomisches Publish.
- Deep Map darf ausschließlich auf einem veröffentlichten Indexsnapshot arbeiten.
- Das LLM exploriert nur budgetiert und über Read-only-Werkzeuge.
- Deep-Map-Ergebnisse referenzieren konkrete Evidence IDs.
- Task Lens wird aus Ziel und Index dynamisch kompiliert.
- Der Agent darf ohne verfügbares LLM weiterhin Index und Karte durchsuchen.

## Konsequenzen

### Positiv

- reproduzierbare und inkrementelle Wahrheitsschicht
- kleinere, relevantere Prompts
- veraltete Aussagen können invalidiert werden

### Negativ

- Parser- und Graphadapter erfordern erheblichen Entwicklungsaufwand.
- semantische Beziehungen bleiben je Sprache teilweise unvollständig.

### Risiken und Gegenmaßnahmen

- falsche Sicherheit durch Parser — Edge Provider und Confidence sichtbar machen.
- Mappingkosten — Budgets, Coverage und Stopbedingungen.

## Verworfene Alternativen

- Repository komplett in Prompt — skaliert nicht.
- nur LLM-Agent lässt find und read ausführen — zu langsam und nicht reproduzierbar.
- nur Embeddingchunks — verliert strukturelle Beziehungen und Aktualitätsbeweis.

## Compliance

ProjectMapper benötigt SnapshotId eines vollständig veröffentlichten IndexRun. Tests lehnen Mapping auf Dirty oder Partial Index ab.

## Referenzen

- [Turso Code Indexing](https://docs.turso.tech/guides/code-indexing)

