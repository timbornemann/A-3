# ADR-0007: Evidence Graph statt Vector-first Memory

Status: Accepted  
Datum: 2026-08-03

## Kontext

Coding-Aufgaben benötigen exakte Namen, Abhängigkeiten, Tests und Veränderungsbezug. Vektorähnlichkeit findet thematische Nähe, beweist aber keine Codebeziehung und kann wichtige exakte Treffer verdrängen.

## Entscheidung

A^3 verwendet einen relational gespeicherten Evidence Graph mit typisierten Knoten und Kanten. Retrieval kombiniert:

1. exakte Pfad- und Identifier-Suche;
2. FTS;
3. Graphtraversierung;
4. Test- und Taskbeziehungen;
5. fresh Claims;
6. optionale Vektorähnlichkeit.

Vektorähnlichkeit erzeugt Kandidaten, aber niemals Evidenz oder Fact-Status. Embeddings werden primär für Semantic Cards statt beliebige überlappende Rohcode-Chunks erzeugt.

## Konsequenzen

### Positiv

- präzise, erklärbare Treffer
- gute Nutzung von Codebeziehungen
- geringere Embeddingmenge und leichter Rebuild

### Negativ

- mehrere Retrievalkanäle und Fusion müssen evaluiert werden.
- Graphqualität hängt von LanguageAdaptern ab.

### Risiken und Gegenmaßnahmen

- zu starre deterministische Suche — semantische Kandidaten gezielt ergänzen.
- Popularitätsbias im Graph — Goal-, Test- und exakte Signale priorisieren.

## Verworfene Alternativen

- eine große Vektordatenbank als einziges Gedächtnis — nicht exakt und schwer zu invalidieren.
- nur grep — unzureichend für natürliche Aufgaben und Beziehungen.
- Graphdatenbankdienst — unnötige zusätzliche Infrastruktur.

## Compliance

Jeder Retrievaltreffer enthält SourceChannel und Erklärung. Fact-Erstellung aus reinem VectorHit wird im Typmodell verhindert.

