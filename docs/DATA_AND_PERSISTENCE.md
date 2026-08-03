# Daten und Persistenz

Status: verbindliche Baseline  
Stand: 2026-08-03

## Entscheidung

V1 verwendet lokal eingebettetes libSQL über einen KnowledgeStore-Port. Cloud-Synchronisation ist nicht Teil von V1. Die neue Turso-Engine darf später als zweiter Adapter evaluiert werden.

Begründung:

- libSQL ist vollständig SQLite-kompatibel und von Turso als belastbare Wahl für mission-kritische Nutzung beschrieben;
- FTS und ein nativer DiskANN-Vektorindex stehen heute zur Verfügung;
- die neue Turso-Engine ist technisch attraktiv, aber Full-Text Search ist derzeit experimentell und schnelle approximative Vektorindizes stehen laut Projekt-Roadmap noch aus;
- der Port verhindert eine fachliche Bindung an eine Engine.

Quellen:

- [libSQL](https://docs.turso.tech/libsql)
- [libSQL AI and Embeddings](https://docs.turso.tech/features/ai-and-embeddings)
- [Turso Database Repository](https://github.com/tursodatabase/turso)
- [Turso Rust SDK Reference](https://docs.turso.tech/sdk/rust/reference)

## Speicherorte

A^3 schreibt den regenerierbaren Index standardmäßig nicht in das Repository.

~~~text
OS app data/A3/
├── catalog.db
├── logs/
└── projects/
    └── worktree-id/
        ├── knowledge.db
        ├── artifacts/
        └── backups/
~~~

Plattformpfade werden ausschließlich über Tauri- beziehungsweise OS-Path-APIs bestimmt.

Optional darf ein Repository folgende kontrollierte Dateien enthalten:

~~~text
.a3/
├── project.toml
└── rules.md
~~~

Diese Dateien sind menschenlesbare Projektkonfiguration und keine Runtime-Datenbank.

## Datenklassifikation

| Klasse | Beispiele | Regenerierbar | Standard-Sync |
| --- | --- | --- | --- |
| Catalog | Projektliste, UI-Einstellungen | teilweise | aus |
| Index | Dateien, Symbole, Kanten, FTS | ja | nie |
| Semantic Cache | Cards, Embeddings | ja | nie |
| Task State | Ziele, Schritte, Decisions | nein | aus |
| Audit | Run Events, Tooldigests | teilweise | aus |
| Secrets | Provider-Tokens | nein | verboten in DB |

Secrets werden über den jeweiligen OS-Schlüsselspeicher verwaltet.

## Logisches Schema

### Katalog

- schema_migrations
- app_settings
- model_profiles
- projects
- recent_worktrees

### Projektidentität und Snapshots

- repositories
- worktrees
- snapshots
- snapshot_changes
- index_runs

### Deterministischer Index

- files
- file_revisions
- symbols
- symbol_edges
- modules
- module_members
- entrypoints
- test_links
- manifests
- parse_diagnostics
- ranking_projections

### Suche und Semantik

- symbol_fts
- card_fts
- semantic_cards
- embeddings
- retrieval_policies

### Knowledge und Evidenz

- claims
- evidence_refs
- claim_evidence
- claim_relations
- decisions

### Aufgaben und Runs

- tasks
- goal_contract_revisions
- acceptance_criteria
- task_steps
- step_dependencies
- agent_runs
- run_events
- context_packs
- tool_runs
- verification_runs
- approvals

## Schlüsselinvarianten

- Fremdschlüssel sind aktiviert.
- Jede projektspezifische Zeile ist einem Worktree zugeordnet.
- File Revision ist über WorktreeId, normalisierten Pfad und Content Hash eindeutig.
- Veröffentlichte Indexdaten besitzen SnapshotId und IndexRunId.
- EvidenceRef besitzt eine typabhängige, validierte Nutzlast.
- Claim-Evidence ist many-to-many.
- Embedding ist über SemanticCardId, ModelProfileId und BodyHash eindeutig.
- RunEvent ist über RunId und Sequenz eindeutig und append-only.
- Context Pack speichert keine Secrets und kann nach Retention-Policy komprimiert werden.

## Transaktionen

Eine einzelne DB-Transaktion darf:

- ein File Delta samt Symbolen und Kanten anwenden;
- Invalidationen für dieses Delta markieren;
- einen vollständigen Indexlauf veröffentlichen;
- einen Task-Schritt samt Verification abschließen;
- einen Toolrun und sein Ergebnis protokollieren.

Sie darf keine Datei lesen, kein Modell aufrufen und keinen Prozess abwarten.

## FTS

FTS indiziert getrennte Felder und gewichtet Namen und Signaturen höher als freie Beschreibung:

- Symbolname
- qualifizierter Name
- Signatur
- Dateipfad
- Module-Card-Titel
- Module-Card-Zweck

Rohcode wird nicht pauschal vollständig in FTS dupliziert. Für gezielte Textsuche bleibt ein schneller Repository-Searcher zuständig.

## Vektoren

Embeddings werden nur für normalisierte Semantic Cards und ausgewählte Symbole gespeichert.

Pflichtmetadaten:

- Provider und Modell
- Dimensionszahl
- Datentyp und Quantisierung
- Normalisierung
- Body Hash
- Erstellzeit

Der Vektorindex ist optional. Bei fehlender Unterstützung bleibt die Funktion über exakte Suche, FTS und Graph vollständig nutzbar.

## Migrationen

- Nur Vorwärtsmigrationen
- monotone ganzzahlige Schema-Version
- jede Migration in eigener Transaktion, soweit die Engine dies unterstützt
- Backup vor destruktiver oder nicht trivial rückrollbarer Migration
- Tests von leerem Schema und unterstützten Upgradepfaden
- App startet bei neuerer unbekannter DB-Version ausschließlich read-only mit verständlichem Fehler
- keine automatische Datenlöschung zur Fehlerbehebung

## Retention und Wiederaufbau

- Index und Embeddings dürfen sicher gelöscht und aufgebaut werden.
- Task, Decisions und User-Evidence benötigen Backup vor Cleanup.
- Vollständige Toollogs können nach Policy gekürzt werden; Digest, Status, relevante Evidence und Verifikation bleiben.
- Ein Rebuild darf keine Task-Historie oder Decisions verlieren.

