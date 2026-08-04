# Daten und Persistenz

Status: verbindliche Baseline  
Stand: 2026-08-04

## Entscheidung

V1 verwendet lokal eingebettetes libSQL über einen KnowledgeStore-Port. Cloud-Synchronisation ist nicht Teil von V1. Die neue Turso-Engine darf später als zweiter Adapter evaluiert werden.

Begründung:

- libSQL ist vollständig SQLite-kompatibel und von Turso als belastbare Wahl für mission-kritische Nutzung beschrieben;
- FTS und ein nativer DiskANN-Vektorindex stehen heute zur Verfügung;
- die neue Turso-Engine ist technisch attraktiv, aber Full-Text Search ist derzeit experimentell und schnelle approximative Vektorindizes stehen laut Projekt-Roadmap noch aus;
- der Port verhindert eine fachliche Bindung an eine Engine.

## Implementierter Storage-Unterbau

Der S2-Unterbau liegt im Infrastruktur-Crate `a3-storage-libsql`:

- Ein typisiertes `StorageLayout` nimmt ausschließlich einen absoluten, kanonisierten App-Data-Root an und begrenzt den globalen Katalog auf `catalog.db` innerhalb dieses Roots. `ProjectStorageLayout` leitet daraus ausschließlich über die validierte `WorktreeId` den Pfad `projects/<WorktreeId>/knowledge.db` ab. Verzeichnisse, Datenbankdatei und kanonische Elternbeziehungen werden geprüft; Symlinks und falsche Dateitypen werden abgelehnt. App-Data innerhalb des ausgewählten Worktrees ist unzulässig.
- Der Adapter verwendet die stabile libSQL-Version 0.9.29 ausschließlich mit dem lokalen `core`-Feature. Remote-, Replikations- und Synchronisationsfunktionen sind nicht aktiviert.
- Ein vorhandener Katalog oder eine vorhandene Worktree-Datenbank wird zunächst mit `SQLITE_OPEN_READ_ONLY` auf Schema-Version und Integrität geprüft. Eine unbekannt neuere Version wird vor Connection-Konfiguration oder Migration abgelehnt und nicht verändert.
- Schreibende Connections erzwingen `foreign_keys = ON`, WAL-Journaling, `synchronous = NORMAL`, einen Busy-Timeout von fünf Sekunden und `trusted_schema = OFF`; die Werte werden nach dem Setzen zurückgelesen.
- `PRAGMA user_version` ist die monotone Schema-Version. `schema_migrations` hält zusätzlich Name und versionierten BLAKE3-Checksum jeder Migration fest.
- Jede Migration läuft in einer eigenen `IMMEDIATE`-Transaktion. Ein fehlgeschlagener Migrationskörper wird explizit zurückgerollt.
- Knowledge-Schema V1 persistiert genau eine unveränderliche Bindung aus `RepositoryId` und
  `WorktreeId`. Knowledge-Schema V2 normalisiert diese Bindung vorwärtskompatibel in `repositories`
  und `worktrees` und ergänzt unveränderliche `snapshots`, `snapshot_adapter_revisions`,
  `snapshot_changes` und `index_runs`. Knowledge-Schema V3 ersetzt die Identitäts-Fremdschlüssel durch
  `ON UPDATE CASCADE`, damit ausschließlich der kontrollierte Reconciliation-Pfad Repository- und
  Worktree-ID samt abhängigen Snapshots und IndexRuns transaktional umschreiben kann. Jede
  Wiederöffnung prüft sowohl die V1-Bindung als auch die normalisierte Projektion gegen die frisch
  inspizierte Projektidentität. Linked Worktrees erhalten weiterhin getrennte Datenbanken.
- Katalogschema V3 normalisiert stabile `projects`, veränderliche `repository_observations` und
  `recent_worktrees`. Letztere speichern außerdem die `WorktreeAnchorId`. Dauerhafte
  `worktree_reconciliations` halten den exakt bestätigten Quell-/Zielzustand und den Status
  `prepared` oder `completed`, sodass ein Abbruch zwischen Dateisystem- und DB-Schritt sicher
  fortgesetzt wird. Eine normale Projekterkennung oder ein Reconciliation-Abschluss aktualisiert
  Katalog und Öffnungsreihenfolge in genau einer `IMMEDIATE`-Transaktion.
- `ProjectId`, `RepositoryId`, `WorktreeId`, `WorktreeAnchorId` und Remote-Fingerprints werden als
  32-Byte-Werte gespeichert. Autoritative Pfade werden auf Windows als UTF-16LE und auf Unix als rohe
  OS-Bytes gespeichert. Die separat gespeicherte Anzeigeprojektion ist kontrollzeichenfrei und auf
  32.768 Zeichen begrenzt.
- Der aus den konkreten Open- und Recent-Project-Use-Cases abgeleitete `KnowledgeStore`-Port nimmt nur
  Domain-/Application-Typen an. Der zusammengesetzte libSQL-Adapter öffnet und prüft zuerst die
  identitätsgebundene `knowledge.db` und aktualisiert erst danach den globalen Katalog atomar. Ein
  Knowledge-Fehler erzeugt deshalb weder ein erfolgreiches Open-Ergebnis noch einen neuen
  Recent-Eintrag. Scheitert erst der Katalogschreibvorgang, darf die bereits gebundene leere
  Worktree-Datenbank bestehen bleiben; sie wird bei der nächsten Öffnung erneut vollständig geprüft.
  Weder libSQL-Typen noch SQL verlassen den Adapter.
- Der schmale `KnowledgeIndexStore` ergänzt dieselbe Application-Grenze um Snapshot- und
  IndexRun-Persistenz, ohne bestehende Project-Open-Verbraucher zu verbreitern. Snapshots werden nur
  als exakt nächste Worktree-Generation mit dem unmittelbaren Parent akzeptiert und samt kanonisch
  geordneten Adapterrevisionen und Pfadänderungen atomar angehängt. Repository-Pfade sind relative,
  slash-separierte Rohbytes; sie müssen normalisiert, traversierungsfrei und verlustlos sein.
- Pro Worktree darf höchstens ein `building`-IndexRun bestehen. Der aktuelle Port kann diesen Lauf nur
  als `failed` oder `cancelled` beenden. Ein Übergang zu `published` ist bewusst nicht verfügbar, bis
  S10 Indexdaten und Sichtbarkeit in derselben Adaptertransaktion committen kann.
- Die dev-only Suite `a3-storage-contract-tests` prüft Katalog, Snapshot-Ketten, Linked-Worktree-
  Isolation und IndexRun-Übergänge ausschließlich über die Application-Ports. Der libSQL-Adapter
  liefert nur eine Factory für temporäre App-Data-Roots; engine-spezifische Migration-, Korruptions-
  und Schema-Negativtests bleiben getrennt. Jeder weitere Storageadapter muss dieselbe Suite ausführen.
- Der Desktop-Composition-Root öffnet `catalog.db` im privaten Tauri-App-Data-Verzeichnis und injiziert
  denselben Store in beide Use Cases. Beim Project Open wird die zugehörige `knowledge.db` innerhalb
  dieses privaten Roots geöffnet. Die WebView erhält keine DB-Verbindung und keinen autoritativen
  gespeicherten Pfad.
- Für einen eindeutigen Umzugskandidaten zeigt der privilegierte Desktop-Adapter einen nativen Dialog
  mit „reconciliieren“, „separat öffnen“ und „abbrechen“. Nach Bestätigung persistiert der Adapter
  zuerst die exakte Absicht, verschiebt dann `projects/<alte WorktreeId>` atomar innerhalb des privaten
  `projects`-Verzeichnisses, schreibt die Knowledge-Identität transaktional um und aktualisiert den
  Katalog zuletzt. Vor jedem Schritt werden Kandidat, Revision, Quell- und Zielzustand erneut geprüft;
  ein vorhandenes Ziel wird nie überschrieben.

Discovery, Hashing und die eigentlichen regenerierbaren Index- und Faktendaten sind noch nicht
implementiert. Die Reconciliation entscheidet trotz persistierter Evidenz nie selbstständig: Sie
benötigt einen eindeutigen Kandidaten und die privilegierte native Bestätigung.

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
- repository_observations
- recent_worktrees
- worktree_reconciliations

### Projektidentität und Snapshots

- repositories
- worktrees
- snapshots
- snapshot_adapter_revisions
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
- Snapshots bilden pro Worktree eine lückenlose, unveränderliche Generationenkette; Parent und
  Generation werden vor jedem Append gegen den letzten persistierten Snapshot geprüft.
- Snapshot-Pfadänderungen und Adapterrevisionen sind innerhalb eines Snapshots eindeutig und werden
  in kanonischer Reihenfolge rekonstruiert.
- Pro Worktree existiert höchstens ein laufender `building`-IndexRun; seine Sequenz ist lückenlos und
  worktree-lokal.
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
