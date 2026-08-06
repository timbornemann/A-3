# Daten und Persistenz

Status: verbindliche Baseline  
Stand: 2026-08-06

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
  slash-separierte Rohbytes; sie müssen normalisiert, traversierungsfrei und verlustlos sein. Eine
  read-only Projektion rekonstruiert den wirksamen aktuellen Pfad-/Content-Hash-Stand aus der
  unveränderlichen Delta-Kette. Sie validiert die Generation-/Parent-Kette sowie Pfad-, Änderungsart-
  und Hashwerte erneut an der Adaptergrenze und bleibt über einen Appneustart erhalten.
- Pro Worktree darf höchstens ein `building`-IndexRun bestehen. Er kann ohne Veröffentlichung als
  `failed` oder `cancelled` enden oder ausschließlich zusammen mit einem vollständigen, exakt
  passenden `IndexPublication` atomar auf `published` wechseln. Die run-gebundenen Datei-, Symbol-,
  Kanten-, Kandidaten-, Rank-, Modul-, Membership- und Repository-Card-Zeilen werden vor dem
  Statuswechsel in derselben Transaktion geschrieben.
  Leser rekonstruieren nur den jüngsten veröffentlichten Run in einer konsistenten Read-Transaktion.
  Ein erfolgreicher Ersatz entfernt die regenerierbaren Projektionszeilen älterer Runs noch in
  derselben Transaktion in Batches von höchstens 1.024 Zeilen. Ein Rollback stellt damit auch die
  vorherige Projektion wieder her; Snapshot- und IndexRun-Metadaten bleiben erhalten.
  Breite Symbol-, Kanten-, Kandidaten- und Rankzeilen werden über parametrisierte Mehrzeilen-Inserts
  mit höchstens 30.000 SQL-Parametern und 1.024 Zeilen pro Cancellation-Checkpoint geschrieben.
- Knowledge-Schema V9 veröffentlicht ausschließlich einen vom deterministischen R9-Verifier
  erzeugten `VerifiedModuleCardBatch`. Eine `IMMEDIATE`-Transaktion speichert Cards, Feldgruppen,
  Feldwerte, feldgenaue Evidence IDs, Claims, Claim-Evidence und strukturierte Prädikate gemeinsam
  mit der vollständigen File-, Symbol- oder Graphkanten-Provenienz. Run und Snapshot müssen dem
  jüngsten atomar publizierten Index entsprechen; ein zweiter Publish, eine fremde Evidence ID oder
  ein unvollständiger Lexical-Marker wird vor Mutation abgelehnt. Dieselbe Transaktion füllt
  `card_fts` und aktualisiert dessen erwartete Zeilenzahl. Cancellation, Deadline und höchstens 64
  monotone Progressereignisse werden bis unmittelbar vor Commit geprüft. Nach erfolgreichem Commit
  bleibt der Receipt erfolgreich, auch wenn Cancellation erst danach eintrifft.
- Die dev-only Suite `a3-storage-contract-tests` prüft Katalog, Snapshot-Ketten, Linked-Worktree-
  Isolation, Publish, Rebuild und IndexRun-Übergänge ausschließlich über die Application-Ports. Der
  libSQL-Adapter liefert nur eine Factory für temporäre App-Data-Roots; engine-spezifische Migration-,
  Crash-, Korruptions- und Schema-Negativtests bleiben getrennt. Jeder weitere Storageadapter muss
  dieselbe Suite ausführen.
- Der Desktop-Composition-Root öffnet `catalog.db` im privaten Tauri-App-Data-Verzeichnis und injiziert
  denselben Store in Open-, Recent- und Index-Use-Case. Nach einem erfolgreichen Project Open besitzt
  ein begrenzter Koordinator den Worktree-Watcher, reicht serialisierte Refresh-Jobs an den
  Scheduler weiter und leert dessen begrenzten Ereigniskanal. Beim Project Open wird die zugehörige
  `knowledge.db` innerhalb dieses privaten Roots geöffnet. Die WebView erhält keine DB-Verbindung und
  keinen autoritativen gespeicherten Pfad.
- Für einen eindeutigen Umzugskandidaten zeigt der privilegierte Desktop-Adapter einen nativen Dialog
  mit „reconciliieren“, „separat öffnen“ und „abbrechen“. Nach Bestätigung persistiert der Adapter
  zuerst die exakte Absicht, verschiebt dann `projects/<alte WorktreeId>` atomar innerhalb des privaten
  `projects`-Verzeichnisses, schreibt die Knowledge-Identität transaktional um und aktualisiert den
  Katalog zuletzt. Vor jedem Schritt werden Kandidat, Revision, Quell- und Zielzustand erneut geprüft;
  ein vorhandenes Ziel wird nie überschrieben.

Discovery und Hashing bilden den bestätigenden Vorlauf des Fast Index. Der lokale
Adapter liefert eine versionierte, deterministisch sortierte Projektion relevanter tracked und
untracked Dateien, liest deren Inhalt innerhalb fester Einzel- und Gesamtgrenzen vollständig und
bildet BLAKE3-basierte `FileRevision`s. Der Snapshot-Builder vergleicht sie mit dem aus der
persistierten Delta-Kette rekonstruierten Dateistand, erzeugt ausschließlich bei Inhalts-, HEAD-,
Schema- oder Adapteränderungen die exakt nächste Generation und liefert sie für ein atomisches
Append. Im inkrementellen Pfad übernimmt der Snapshot-Builder unveränderte Revisionen aus der
persistierten Baseline und hasht nur neue oder vom Watcher gemeldete Pfade; ein sichtbares
Eventverlustsignal erzwingt den Vollscan. Parser und Graph erzeugen den vollständigen
deterministischen Publish-Input; Knowledge-Schema V8 veröffentlicht die daraus abgeleiteten Datei-,
Symbol-, Kanten-, Kandidaten-, Rank-, Exact-, FTS-, Modul- und Repository-Card-Projektionen atomar.
Knowledge-Schema V9 ergänzt danach ausschließlich deterministisch verifizierte Module Cards,
Claims und ihre vollständige Evidence-Provenienz.
Die Reconciliation entscheidet trotz persistierter Evidenz nie
selbstständig: Sie benötigt einen eindeutigen Kandidaten und die privilegierte native Bestätigung.

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

Discovery V1 liest optional ein strikt begrenztes Schema:

~~~toml
[discovery]
ignore = ["private/**", "fixtures/generated/"]
~~~

Die Muster verwenden Gitignore-Syntax, sind aber nur ausschließend; `!`-Negationen und unbekannte
Konfigurationsfelder sind ungültig. Eingebaute Secret-, Vendor-, Generated-, Binary- und
Größenregeln können nicht aus dem Repository aufgehoben werden. Benutzerweite Git-Konfiguration
außerhalb des ausgewählten Worktrees wird nicht automatisch gelesen.

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
- unresolved_edges
- module_projections
- modules
- module_manifests
- module_members
- module_membership_evidence
- module_central_symbols
- module_entrypoints
- module_tests
- repository_card_entrypoints
- parse_diagnostics
- ranking_projections

### Suche und Semantik

- exact_search_projections
- exact_search_symbols
- exact_search_manifests
- lexical_search_projections
- symbol_fts
- path_fts
- card_fts
- semantic_cards
- semantic_card_snapshots
- embedding_profiles
- embeddings
- retrieval_policies

### Knowledge und Evidenz

- module_cards
- module_card_fields
- module_card_field_values
- module_card_field_evidence
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
- Der wirksame Dateistand ist die jeweils jüngste Änderung pro Pfad; `Upsert` setzt den aktuellen
  Content Hash, `Delete` entfernt den Pfad und behält den vorherigen Hash zur Invalidation.
- Pro Worktree existiert höchstens ein laufender `building`-IndexRun; seine Sequenz ist lückenlos und
  worktree-lokal.
- File Revision ist über WorktreeId, normalisierten Pfad und Content Hash eindeutig.
- Veröffentlichte Indexdaten besitzen SnapshotId und IndexRunId.
- Die Exact-Search-Projektion besitzt pro veröffentlichtem Run genau einen Versionsmarker mit
  erwarteter Symbol- und Manifestanzahl. Qualifizierte Namen decken exakt alle veröffentlichten
  Symbole ab; Manifestzeilen referenzieren die aktuelle File Revision desselben Runs.
- Die Lexical-Search-Projektion besitzt pro veröffentlichtem Run genau einen Versionsmarker mit
  erwarteter Symbol-, Pfad- und Card-Anzahl. `symbol_fts` und `path_fts` decken exakt die aktuelle
  Run-Projektion ab; nach R9-Publish deckt `card_fts` exakt die publizierten Cards dieses Runs ab.
- Graphabfragen lesen ausschließlich `file_revisions`, `symbols`, `symbol_edges` und qualifizierte
  Namen desselben jüngsten veröffentlichten Runs. Seeds und jedes erreichte Ziel müssen in dieser
  vollständigen Run-Projektion vorhanden sein; historische oder gelöschte IDs werden nicht
  aufgelöst.
- EvidenceRef besitzt eine typabhängige, validierte Nutzlast. Persistierte Card-Evidence behält
  zusätzlich die vollständige File Revision, Symbolrevision oder Graphkante des Verify-Zeitpunkts.
- Claim-Evidence ist many-to-many.
- Fact, Observation und Hypothesis sowie `Active` und Confidence werden in getrennten Spalten
  gespeichert; Schema-Checks verhindern, dass Architekturabsicht als Fact persistiert wird.
- Verifizierte Cards, Claims und Evidence sind dauerhaft. Ein Rebuild entfernt ihre regenerierbare
  `card_fts`-Projektion, aber nicht ihre Herkunft; R11 übernimmt deren spätere Invalidierung.
- Embedding ist über SemanticCardId, ModelProfileId und BodyHash eindeutig.
- Eine Semantic Card besitzt pro Snapshot genau eine Body-Revision; derselbe Body darf über
  Snapshots hinweg denselben regenerierbaren Cacheeintrag wiederverwenden.
- Persistierte Profilmetadaten müssen vollständig zur abgeleiteten ModelProfileId passen; ein
  Provider-, Modell- oder Dimensionswechsel erzeugt daher einen getrennten Cachekorridor.
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

Knowledge-Schema V6 verwendet den eingebetteten FTS5-Trigram-Tokenizer. Untrusted Querytext wird
zuerst in begrenzte alphanumerische Tokens und daraus in ausschließlich abgeleitete, gequotete
Trigram-Klauseln transformiert; der vollständige Ausdruck bleibt ein gebundener Parameter. FTS
liefert nur den Kandidatenkorridor. Die finale Gewichtung, Mindestschwelle und stabile Reihenfolge
erfolgen deterministisch im Adapter. Pfade bleiben im Treffer als verlustlose Bytes erhalten; nur
ihre Suchprojektion ist UTF-8 beziehungsweise eine deterministische Prozentkodierung.

## Vektoren

Embeddings werden nur für normalisierte Semantic Cards und ausgewählte Symbole gespeichert.

Pflichtmetadaten:

- Provider und Modell
- Dimensionszahl
- Datentyp und Quantisierung
- Normalisierung
- Body Hash
- Erstellzeit

Knowledge-Schema V7 speichert den kanonischen Cardkörper, seine Snapshot-Zuordnung, alle
vektorformenden Profilfelder und den normalisierten Float32-Vektor in portabler
Little-Endian-Darstellung. Der exakte Cachelookup verwendet ausschließlich
`(SemanticCardId, ModelProfileId, BodyHash)` und validiert Dimension, Endlichkeit und L2-Norm auch
beim Lesen erneut.

Die libSQL-Capability wird für die konkrete Profildimension mit einer isolierten In-Memory-
`FLOAT32(dim)`-Tabelle und einem `libsql_vector_idx(..., 'metric=cosine')` geprüft. Bei verfügbarer
Erweiterung werden höchstens 4.096 snapshot- und profilgebundene Cachezeilen in eine kurzlebige
DiskANN-Kandidatenprojektion übernommen. Die Projektion liefert ausschließlich Kandidaten; die
endgültige Cosine-Normalisierung, Reihenfolge und Trunkierung erfolgen deterministisch im Rust-
Adapter. Schlägt Capability oder Projektion fehl, vergleicht derselbe begrenzte Korridor linear.
Ein abgeschnittener Korridor oder ein Resultlimit wird im `VectorSearchResult` sichtbar markiert.

Der Vektorindex ist optional. Bei fehlender Unterstützung bleiben Exact Search, FTS und Graph
unverändert nutzbar; auch semantische Kandidaten bleiben über den begrenzten linearen Fallback
verfügbar.

## Migrationen

Das implementierte Knowledge-Schema V9 ergänzt V8 um `module_cards`, `module_card_fields`,
`module_card_field_values`, `module_card_field_evidence`, `evidence_refs`, `claims`,
`claim_evidence` und `claim_relations`. Alle Identitäten bleiben an den verifizierenden
`source_index_run_id` und `SnapshotId` gebunden. Der Herkunfts-Run ist bewusst kein
Delete-Fremdschlüssel: Fast-Index-Rebuilds dürfen die dauerhaften Claims nicht vernichten.
Snapshots bleiben referenziell geschützt; alle abhängigen Cardtabellen besitzen dagegen strikte
komposite Schlüssel. Ein getesteter V8→V9-Rollback lässt die V8-Datenbank unverändert. Die
Publisher-Contractprobe belegt atomaren SQL-Fehlerrollback, Cancellation vor Mutation,
Progressfehler, maximal 64 monotone Fortschrittsereignisse, getrennte Fact-/Hypothesis-
Persistenz, exakte FTS-Zeilenzahl, Duplicate-Rejection und Erhalt der Claims nach Index-Rebuild.

Das implementierte Knowledge-Schema V8 ergänzt V7 um die vollständige run-gebundene
`ModuleProjection`. Ein Marker hält `ModulePolicyVersion`, Datei-/Symbol-/Modul-/Membershipzahlen,
Sprachmaske und Repository-Card-Trunkierung. Primärmodule, Manifestrevisionen, genau eine primäre
Membership pro Symbol, zusätzliche Graph-Memberships mit aktuellen `EvidenceRef`s sowie begrenzte
Zentral-, Entrypoint- und Testlisten werden in strikt typisierten Tabellen gespeichert. Reader
rekonstruieren die Domainaggregate und prüfen Snapshot, Counts, Symbolabdeckung, aktuelle
File Revisions und die Zugehörigkeit jeder Graph-Evidence erneut.

V8 verwendet für seine Projektionshierarchie absichtlich keine tiefen Delete-Cascades. Replacement
und Rebuild löschen die Kindtabellen in referenziell sicherer Reihenfolge und begrenzten Batches,
bevor Modulmarker, Symbole oder File Revisions entfernt werden. Migration aus jeder unterstützten
Vorgängerversion, fehlgeschlagener V7→V8-Upgrade, atomarer Rollback, Replacement, Reopen und Rebuild
sind getestet. Index-Schema V4 erzwingt beim nächsten Compilerlauf die neue vollständige Projektion;
ein Publisher kann Graph und Ranking nicht mehr ohne passende Module und Repository Card sichtbar
machen. Ein aus V7 migrierter veröffentlichter Run mit Index-Schema V1 bis V3 bleibt für seine
vorhandenen Exact-/FTS-/Graphkanäle lesbar, wird aber nicht als vollständiger `PublishedIndex`
ausgegeben; erst der erzwungene V4-Rebuild liefert wieder die vollständige Projektion. Fehlt der
Modulmarker dagegen bei einem V4-Snapshot, gilt die Datenbank als inkonsistent.

Das implementierte Knowledge-Schema V7 ergänzt V6 um `semantic_cards`,
`semantic_card_snapshots`, `embedding_profiles` und `embeddings`. Alle Tabellen sind strikt,
referenzieren existierende Snapshots und trennen Cachezeilen durch Card-, Body- und Profil-ID.
V7 besitzt einen getesteten Vorwärtsupgrade- und Rollbackpfad; der Rebuild löscht ausschließlich
diese regenerierbaren Tabellen in referenziell sicherer Reihenfolge, einzeln committeten begrenzten
Batches und mit determiniertem Row-Progress. Ein Abbruch kann damit ohne ungültigen Zwischenzustand
idempotent fortgesetzt werden.

Knowledge-Schema V6 ergänzt V5 um `lexical_search_projections`, `symbol_fts`,
`path_fts` und das für R5 vorbereitete, noch leere `card_fts`. Exact- und Lexical-Projektion werden
gemeinsam mit File Revisions, Graph und Ranking in derselben Publish-Transaktion sichtbar. Ein aus
einem älteren Schema migrierter, bereits veröffentlichter Run besitzt absichtlich keinen neueren
Projektionsmarker und liefert den kanalbezogenen Zustand `ProjectionUnavailable`, statt
unvollständige Treffer vorzutäuschen. Index-Schema V3 erzwingt beim nächsten Compilerlauf eine
Neupublikation der lexikalischen Projektion.

R3 benötigt keine zusätzliche Migration: Knowledge-Schema V4 speichert bereits jede aufgelöste
Kante mit kanonischer Sequenz, typisierten Endpunkten, Relation, Provider, Confidence, Resolution,
Snapshot und `EvidenceRef`. Der Graph-Reader validiert zusätzlich den Exact-Projektionsmarker des
Runs, weil Symbolziele ihre containment-abgeleiteten qualifizierten Namen als Domainobjekt tragen.

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
