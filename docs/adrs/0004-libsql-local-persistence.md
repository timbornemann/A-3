# ADR-0004: Lokale libSQL-Persistenz hinter KnowledgeStore

Status: Accepted  
Datum: 2026-08-03

## Kontext

A^3 benötigt relationale Zustände, Graphkanten, Volltextsuche, Transaktionen und optional Vektorsuche in einer eingebetteten lokalen Datenbank. Turso bietet libSQL sowie eine neue Rust-Neuentwicklung an.

## Entscheidung

- V1 verwendet lokal eingebettetes libSQL.
- Alle Zugriffe erfolgen über den KnowledgeStore-Port.
- SQL und Row-Typen bleiben im Adapter.
- FTS ist primäre Textsuche in der Wissensbasis.
- DiskANN-Vektorsuche ist optional und sekundär.
- Cloud-Sync ist deaktiviert und nicht Teil des V1-Pfads.
- Die neue Turso-Engine wird erneut bewertet, wenn benötigte FTS- und ANN-Funktionen nicht mehr experimentell beziehungsweise Roadmap sind und Migrationstests bestehen.

## Konsequenzen

### Positiv

- bewährte SQLite-Kompatibilität
- Transaktionen, FTS und ANN in einer lokalen Datei
- klare Migrationsmöglichkeit über Port und Contract-Suite

### Negativ

- native libSQL-Buildabhängigkeiten können Packaging erschweren.
- A^3 nutzt zunächst nicht die neuen MVCC-Eigenschaften der Turso-Neuentwicklung.

### Risiken und Gegenmaßnahmen

- Anbieterbindung — nur standardnahe SQL-Kernpfade; spezielle FTS- und Vektorfunktionen in Capability-Methoden kapseln.
- Enginewechsel — gemeinsame Store-Contract-Suite und exportierbares logisches Schema.

## Verworfene Alternativen

- neue Turso-Engine sofort — FTS ist aktuell experimentell und ANN-Indexierung noch Roadmap.
- reine SQLite plus separate Vektor-DB — zusätzliche Zustands- und Transaktionsgrenze.
- große externe Vector DB — unnötiger Dienst und falsches primäres Retrievalmodell.

## Compliance

Application-Crates dürfen weder libSQL importieren noch SQL enthalten. Alle Storageadapter bestehen dieselbe Contract-Suite.

## Referenzen

- [libSQL Überblick](https://docs.turso.tech/libsql)
- [libSQL Vektorindex](https://docs.turso.tech/features/ai-and-embeddings)
- [Turso Rust SDK](https://docs.turso.tech/sdk/rust/reference)
- [Turso Database Status und Roadmap](https://github.com/tursodatabase/turso)

