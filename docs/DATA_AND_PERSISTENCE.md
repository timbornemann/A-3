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
- Knowledge-Schema V11 ergänzt den dauerhaften Task-Zielanker. `tasks` hält pro Worktree nur
  Erstellzeit und den aktuellen Revisionszeiger; `goal_contract_revisions` sowie die geordneten
  Acceptance-Criteria-, Constraint-, Non-Goal- und User-Decision-Zeilen sind append-only. Initiale
  Erstellung und jede Compare-and-Append-Revision laufen in einer `IMMEDIATE`-Transaktion. Der
  Adapter rekonstruiert beim Lesen ausschließlich Domain-Typen, validiert Sequenzen und Grenzen
  erneut und gibt weder Rows noch SQL an Application oder UI weiter.
- Knowledge-Schema V12 ergänzt pro Task genau ein materialisiertes, revisionsgebundenes Task Ledger.
  Schrittdefinitionen, Abhängigkeiten, erwartete Evidenz, Versuche, Verifikationen, Stale-Ursachen
  und Replan-Historie liegen in normalisierten relationalen Tabellen. Jede Ersetzung läuft in einer
  `IMMEDIATE`-Transaktion mit monotoner Store-Version; ein veralteter Writer oder der Versuch,
  bestehende Definitionen, terminale Versuche oder Replans umzuschreiben, wird vor Mutation
  abgelehnt. Leser rekonstruieren und validieren den vollständigen Domain-Aggregatzustand innerhalb
  einer konsistenten Read-Transaktion.
- Knowledge-Schema V13 ergänzt `agent_runs` als aktuelle relationale Laufprojektion und
  `run_events` als append-only Auditjournal. Runstart sowie jeder folgende Eventappend aktualisieren
  Event und Materialisierung atomar in einer `IMMEDIATE`-Transaktion. Der Tail wird per
  Sequenz-Compare-and-Swap geschützt; Snapshot, Goal-Contract-Revision und Task-Ledger-Revision
  werden vor Mutation erneut validiert. Materialisierter Zustand wird ohne Journal-Replay gelesen.
- Knowledge-Schema V14 ergänzt jeden neuen `agent_runs`-Datensatz um `ModelProfileId` und
  `ModelProfileVersion`. Beide Felder sind gemeinsam gesetzt oder gemeinsam leer; Insert- und
  Update-Trigger verhindern halbe Referenzen. Ausschließlich aus V13 migrierte Bestandsruns dürfen
  als expliziter Legacyfall ohne Profilbezug lesbar bleiben. Der Storage-Adapter erstellt keinen
  neuen Legacyrun und validiert ID-Länge sowie unterstützte Schemaversion erneut beim Lesen.
- Knowledge-Schema V15 ergänzt unveränderliche Turn-, Token-, Action-, Zeit- und Repair-Limits
  sowie materialisierte kumulative Verbrauchszähler in `agent_runs`. Jeder neue
  `model_interaction`-Event trägt seine Prompt-/Outputtokens, die optionale einzige Actionklasse
  und den Verbrauch des einmaligen Repairpfads. Checks und Trigger verhindern ungültige
  Kardinalitäten, halbe Turn-Charges, Charges auf anderen Eventtypen und spätere Budgetänderungen.
- Knowledge-Schema V16 ergänzt `tool_runs` und `tool_evidence`. Ein normalisierter Read-Tool-Lauf
  wird gemeinsam mit seinem `tool_action`-Event atomar geschrieben und bleibt über ToolRunId,
  Runsequenz, Ergebnisstatus und -digest sowie Vorher-/Nachher-Snapshot nachvollziehbar. Evidence
  besteht ausschließlich aus geordneten, content-adressierten File- oder Source-Span-Locators;
  Suchanfrage, Source-Text und begrenzte Toolvorschau werden nicht persistiert. Fremdschlüssel und
  ein Trigger binden jeden Toollauf an genau sein typisiertes Journal-Event.
- Knowledge-Schema V17 ergänzt `tool_run_attempts` als content-freie Lifecycle-Projektion. Vor
  jedem Read-Toolaufruf wird ein `in_flight`-Versuch mit Run-, Snapshot- und logischer ToolRunId
  committed. Nur der atomare Toolresultat-/Journal-Commit darf ihn auf den Ergebnisstatus setzen;
  Grenzfehler enden explizit als `failed`, `cancelled` oder `denied`. Beim Neustart werden
  verbliebene Versuche als `interrupted` abgeschlossen, während ein Retry derselben logischen
  ToolRunId eine monotone neue Versuchsnummer erhält.
- Knowledge-Schema V18 ergänzt `approval_requests`, `approval_grants` und `policy_decisions`.
  Requests und Entscheidungen sind append-only; Grants besitzen ausschließlich den Übergang
  `active` nach `consumed` oder `revoked`. Trigger binden jede Entscheidung sowie Grant und
  Widerruf an ihr typisiertes `approval_recorded`-RunEvent und prüfen Run, Action-Fingerprint,
  Scope-Digest, Klasse, Risiko und Zeitfenster erneut. Entscheidung, optionaler Request,
  Runprojektion, Event und optionaler One-time-Verbrauch werden gemeinsam per Runsequenz-CAS in
  einer `IMMEDIATE`-Transaktion committed. Grant und Widerruf besitzen je eine eigene atomare
  User-Audit-Transaktion.
- Knowledge-Schema V19 ergänzt `command_allowlist_revisions` und
  `command_allowlist_entries`. Jede Benutzerbestätigung bindet Worktree, monotonen CAS-Stand,
  exakten Katalog-Digest, Zeitpunkt und eine kanonische nicht leere Teilmenge von höchstens 256
  Command-IDs. Revisionen und Einträge sind append-only; ausschließlich die kontrollierte
  Worktree-Reconciliation darf ihre Worktree-ID über Fremdschlüssel-Cascade umschreiben.
- Knowledge-Schema V20 ergänzt Must-/Should-Kriterien, Goal-revisionsgebundene Step-Mappings,
  operationale VerificationSpec-Zeilen und immutable `verification_evidence`. Getrennte strikte
  Variantentabellen speichern content-freie Command-/Test-/Diagnostic-/Diff-/UserConfirm-Semantik
  sowie kanonische Present-/Absent-Abhängigkeiten. Diff-Quellen unterscheiden strikt autorisierte
  Patchresultate von geordneten Published-Index-Vergleichen. Stable IDs werden beim Reopen aus allen
  gespeicherten Feldern erneut abgeleitet; ein gleiches Evidence-ID-Append ist nur bei exakt
  identischem Artifact idempotent. Append und Acceptance-Read sind abbrechbar, besitzen ein
  Zeitlimit und prüfen bei mengenabhängigen Zeilen feste Checkpoints. Acceptance liest Evidence
  und den weiterhin aktuellen Published Index begrenzt und in konsistenten Transaktionen.
- Die dev-only Suite `a3-storage-contract-tests` prüft Katalog, Snapshot-Ketten, Linked-Worktree-
  Isolation, Publish, Rebuild, IndexRun-Übergänge, Policy-/Approval-Lifecycle, die
  projektbezogene Command-Allowlist und alle fünf Verification-Evidence-Varianten ausschließlich
  über die Application-Ports. Der libSQL-Adapter liefert nur eine Factory für temporäre
  App-Data-Roots; engine-spezifische Migration-, Crash-, Korruptions- und Schema-Negativtests
  bleiben getrennt. Jeder weitere Storageadapter muss dieselbe Suite ausführen.
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
- goal_contract_constraints
- goal_contract_non_goals
- goal_contract_user_decisions
- task_ledgers
- task_steps
- task_step_dependencies
- task_step_expected_evidence
- task_step_attempts
- task_step_attempt_evidence
- task_step_verifications
- task_step_verification_evidence
- task_step_stale_evidence
- task_ledger_replans
- task_ledger_replan_retirements
- task_ledger_replan_additions
- agent_runs
- run_events
- context_packs
- tool_runs
- verification_runs
- approval_requests
- approval_grants
- policy_decisions
- command_allowlist_revisions
- command_allowlist_entries

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
- Jeder Task besitzt bei Erstellung atomar eine valide initiale Goal-Contract-Revision. Der
  aktuelle Zeiger darf nur auf die exakt nächste, zeitlich nicht rückläufige Revision wechseln;
  alte Revisionen und ihre geordneten Inhalte bleiben unverändert lesbar. Ein konkurrierender
  Writer mit veraltetem Vorgänger wird abgelehnt.
- RunEvent ist über RunId und Sequenz eindeutig und append-only. Sequenz eins ist ausschließlich
  `RunStarted`; Zustands- und Ledgerübergänge besitzen vollständig typisierte Vorher-/Nachherwerte.
- `agent_runs` und der neu angehängte `run_events`-Datensatz wechseln in derselben Transaktion.
  Ein veralteter Writer verliert den Compare-and-Swap und hinterlässt weder Event noch Teilzustand.
- Jeder neue `agent_runs`-Datensatz referenziert eine vollständige `ModelProfileId` zusammen mit
  genau der unterstützten Profilschemaversion. Ein migrierter Legacyrun hat beide Werte `NULL`;
  gemischte Nullzustände sind durch Trigger und Adaptervalidierung ausgeschlossen.
- Das am Runstart gewählte Budget ist unveränderlich. Turn-, Action- und Repairzähler werden mit
  genau dem zugehörigen `model_interaction`-Event in derselben Sequenz-CAS-Transaktion erhöht;
  `action_count` und `repair_count` können nie größer als `turn_count` sein.
- Run-Event-Payloads enthalten nur geschlossene Codes, grobe Outcomes, content-freie
  Redaktionsmetadaten und den Digest dieser sicheren Struktur. Freitext, Modelloutput, Tooloutput,
  externe Fehlertexte und Secretwerte sind in diesem Schema nicht darstellbar.
- Jeder privilegierte Policyversuch besitzt genau eine unveränderliche `policy_decisions`-Zeile und
  genau ein passendes content-freies RunEvent. `ApprovalRequired` referenziert einen exakt
  metadata- und zeitgleichen Request; `ApprovalGranted` referenziert einen noch aktiven Grant mit
  identischem Run, Action-Fingerprint, Scope, Klasse und Risiko.
- Approval-Requests gelten höchstens 24 Stunden. Grants dürfen nur innerhalb des Requestfensters
  entstehen und nur vor ihrem exklusiven Ablauf verbraucht oder widerrufen werden. Consumption
  referenziert genau die erlaubende Decision; Grant und Widerruf referenzieren genau ihr explizites
  User-Audit-Event. Terminale Grants und append-only Requests/Decisions können nicht umgeschrieben
  oder gelöscht werden.
- Jede Command-Allowlist-Revision ist append-only und pro Worktree monoton. Ihre Einträge sind
  ordinal lückenlos, eindeutig und auf 256 begrenzt. Ein veralteter erwarteter Revisionsstand
  rollt die gesamte Bestätigung zurück; ein aktueller Command-Katalog akzeptiert sie nur bei
  identischem Worktree, Katalog-Digest und bestätigter Command-ID.
- Context Pack speichert keine Secrets und kann nach Retention-Policy komprimiert werden.

## Transaktionen

Eine einzelne DB-Transaktion darf:

- ein File Delta samt Symbolen und Kanten anwenden;
- Invalidationen für dieses Delta markieren;
- einen vollständigen Indexlauf veröffentlichen;
- einen Task-Schritt samt Verification abschließen;
- genau einen RunEvent anhängen und die zugehörige Run-Materialisierung aktualisieren;
- genau eine PolicyDecision, ihren optionalen ApprovalRequest, das Audit-Event und den optionalen
  Approval-Verbrauch per Runsequenz-CAS committen;
- genau einen Approval-Grant oder Widerruf zusammen mit seinem User-Audit-Event committen;
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

Das implementierte Knowledge-Schema V20 ergänzt V19 um die typed Verification Engine. Die
Migration erweitert bestehende Acceptance Criteria rückwärtskompatibel mit `requirement = must`,
bindet neue Step-Criterion-Mappings an exakt dieselbe Goal-Revision und persistiert operationale
Specs getrennt von lesbaren Legacy-Spezifikationen. Elf neue normalisierte Tabellen bewahren
Mappings, Specs, schema-versionierte Header, Processmetadaten, strukturierte Testfälle,
Diagnosezahlen, strikt disjunkte Patch-/Published-Index-Diffquellen, Diffpfade,
Userbestätigungen und Present-/Absent-Freshness-Abhängigkeiten. Der
gemeinsame
Adaptervertrag belegt Timeout und Cancellation ohne Teilwrite, idempotentes Append, Reopen aller
Varianten und beider Diffquellen, Soll-Nichtblockierung,
Acceptance und gezielte Stale-Erkennung nach einer neuen Indexpublikation. Migrationstests decken
leeres Schema, jeden Vorgänger bis V19 und vollständigen Rollback eines fehlgeschlagenen
V19→V20-Upgrades ab.

Das implementierte Knowledge-Schema V19 ergänzt V18 um die dauerhafte projektbezogene
Command-Allowlist. `command_allowlist_revisions` und `command_allowlist_entries` sind append-only,
streng worktreegebunden und erlauben nur der kontrollierten Identitäts-Reconciliation ein
Worktree-ID-Cascade. Der gemeinsame Adaptervertrag belegt leeren Anfangszustand, Confirmation,
Reopen, monotone Revision, vollständigen CAS-Rollback und Worktree-Isolation. Migrationstests
decken leeres Schema, jeden Vorgänger bis V18, den vollständigen Rollback eines fehlgeschlagenen
V18→V19-Upgrades sowie die allein zulässige Reconciliation-Cascade ab.

Das implementierte Knowledge-Schema V18 ergänzt V17 um die zentrale Policy- und
Approval-Persistenz. `approval_requests` und `policy_decisions` sind unveränderlich und
append-only; `approval_grants` erlaubt nur einen einmaligen terminalen Übergang. Relationale Checks,
Fremdschlüssel und Trigger binden Requests, Grants, Entscheidungen sowie Grant-/Widerruf-Events an
denselben Run und erzwingen exakte Action-/Scope-Metadaten und das ursprüngliche Ablaufzeitfenster.
Der gemeinsame Adaptervertrag belegt Reopen, Pfad-Mismatch ohne Grantverbrauch, einmalige
Consumption, Widerruf, restriktive Workspace-Policy und vollständigen Rollback eines veralteten
Run-CAS. Migrationstests decken leeres Schema, jeden Vorgänger bis V17 und den vollständigen
Rollback eines fehlgeschlagenen V17→V18-Upgrades ab.

Das implementierte Knowledge-Schema V17 ergänzt V16 um dauerhafte Toolversuche und atomare
Run-Recovery. Bestehende V16-Toolläufe werden als abgeschlossener erster Versuch übernommen; ein
partieller Unique Index erlaubt je logischer ToolRunId höchstens einen laufenden Versuch. Die
Recovery liest Run und Ledger nach Reopen, vergleicht abgeschlossene Verification-Evidence mit dem
jüngsten atomar veröffentlichten Index und committed Resume, Replan oder Cancel in einer kurzen
`IMMEDIATE`-Transaktion. Diese Transaktion vergleicht Published Snapshot, Ledger-Store-Version und
RunEvent-Sequenz, bevor Ledgerprojektion, Runmaterialisierung und Recovery-Event gemeinsam sichtbar
werden. Der gemeinsame Adaptervertrag simuliert Appabbruch, Interrupted-Markierung, Retry,
Snapshotwechsel, stale Step-Reopen sowie Snapshot- und Run-CAS-Rollback; der getestete V16→V17-
Fehlerfall lässt Schema und `user_version` vollständig auf V16.

Das implementierte Knowledge-Schema V16 ergänzt V15 um die dauerhafte, bounded Tool-Evidence.
Der libSQL-Adapter schreibt Tool-Event, Runprojektion, Toolmetadaten und höchstens 100 typisierte
Evidence-Locators in einer `IMMEDIATE`-Transaktion. Ein separater atomarer Action-Port ersetzt bei
Ledger-mutierenden `UpdateLedger`-Ergebnissen zusätzlich die Task-Ledger-Projektion und hängt die
resultierende Controllertransition mit zwei Compare-and-Swap-Ankern in derselben Transaktion an.
Adaptervertrag und Repositorytest belegen Reopen, Konflikt-Rollback und die Abwesenheit roher
Toolvorschauen; der getestete V15→V16-Fehlerfall rollt vollständig auf V15 zurück.

Das implementierte Knowledge-Schema V15 ergänzt V14 um unveränderliche Controllerbudgets,
kumulative Verbrauchszähler und vollständige per-Turn-Charges. Der gemeinsame Adaptervertrag
belegt die atomare Aktualisierung von Event und Nutzung sowie ihre exakte Wiederherstellung nach
Reopen. Migrationstests decken leeres Schema, jeden Vorgänger bis V14 und den vollständigen
Rollback eines fehlgeschlagenen V14→V15-Upgrades ab.

Das implementierte Knowledge-Schema V14 ergänzt V13 um den dauerhaften ModelProfile-Bezug jedes
neuen Agentenlaufs. Die Migration erhält bestehende Runprojektionen mit einem expliziten
Legacy-Nullpaar und installiert Guards gegen partielle Referenzen. Domain- und Adaptertests belegen
Profil-ID/Schemaversion nach Reopen, journalunabhängiges Lesen des Legacyfalls und die Ablehnung
eines einzelnen gesetzten Felds. Der generische Upgradevertrag migriert weiterhin jede unterstützte
Vorgängerversion bis V20 in einer eigenen atomaren Migration.

Das implementierte Knowledge-Schema V13 ergänzt V12 um `agent_runs` und `run_events`. Strikte
Checks erzwingen die Startsequenz, geschlossene Event-, State-, Outcome- und Redaction-Werte,
kontiguierliche Ledgerrevisionen sowie vollständige optionale Feldgruppen. Fremdschlüssel binden
jeden Run an Task, konkrete Goal-Contract-Revision, Task Ledger und aktuellen Snapshot; jedes Event
referenziert einen vorhandenen Snapshot. Der gemeinsame Adaptervertrag prüft atomaren Start,
Linked-Worktree-Isolation, konkurrierende Sequenz-CAS-Appends, Paging, Redaction, deterministischen
Export und exakte Wiederherstellung nach Reopen. Der getestete V12→V13-Fehlerfall rollt vollständig
auf V12 zurück.

Das implementierte Knowledge-Schema V12 ergänzt V11 um die relationale Task-Ledger-Projektion.
Strikte Checks und Fremdschlüssel begrenzen IDs, Texte, Sequenzen, Statuswerte und
Verifikationsformen. Create und Replace schreiben immer die vollständige Projektion atomar; Replace
verwendet Compare-and-Swap auf einer separaten Store-Version und bewahrt die append-only
Versuchs- und Replan-Historie. Der gemeinsame Adaptervertrag prüft Worktree-Isolation,
Konfliktablehnung, fehlgeschlagene und erfolgreiche Verifikation, transitive Evidence-Invalidierung,
Replan sowie die exakte Rekonstruktion nach Reopen. Der getestete V11→V12-Fehlerfall rollt
vollständig auf V11 zurück.

Das implementierte Knowledge-Schema V11 ergänzt V10 um `tasks`, `goal_contract_revisions`,
`acceptance_criteria`, `goal_contract_constraints`, `goal_contract_non_goals` und
`goal_contract_user_decisions`. Strikte Checks spiegeln die Domainlängen und die Grenze von 64
geordneten Einträgen; Fremdschlüssel schützen Task, Vorgänger und aktuellen Revisionszeiger. Die
initiale zyklische Task-/Revision-Bindung wird ausschließlich innerhalb einer deferred-FK-
Transaktion hergestellt. Der getestete V10→V11-Fehlerfall rollt vollständig auf V10 zurück;
Migrationstests decken weiterhin jede unterstützte Vorgängerversion ab.

Das implementierte Knowledge-Schema V10 ergänzt V9 um `module_card_lifecycle`, `claim_lifecycle`,
`evidence_invalidations` und `module_remap_queue`. Historische Card-, Claim- und Evidence-Zeilen
bleiben unverändert auditierbar; separate Lifecyclezeilen verhindern, dass eine alte Published-
Repräsentation nach einer Invalidierung erneut sichtbar wird. Der neue Index-Run, direkte
Evidence-Prüfung, Card-/Claim-Übergänge und Queue-Updates werden in derselben `IMMEDIATE`-
Transaktion publiziert. Bestehende Queueeinträge werden auf den neuen Ziel-Run weitergeschrieben,
entfernte Module gelöscht und eine erfolgreich ersetzte Card atomar aus der Queue genommen.

Der Queue-Reader validiert aktuellen Run und Snapshot, Priorität/Grund, stabile Reihenfolge,
Modul-Eindeutigkeit sowie ein Limit von höchstens 256 Einträgen. Ein getesteter V9→V10-Rollback
lässt die V9-Datenbank unverändert; Migrationstests decken weiterhin jede unterstützte
Vorgängerversion ab.

Wiederholte Indexmutationen verwenden einen separaten, identitätsgeprüften und auf vier Worktrees
begrenzten Adaptercache. Read- und Mutationshandles bleiben getrennt; die bestehende
Ein-Mutation-pro-Worktree-Regel sowie `IMMEDIATE`-Transaktionen bleiben die
Serialisierungsgrenzen. Reconciliation verwirft beide Cacheklassen, bevor Storage verschoben oder
umgebunden wird.

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
- Strukturierte RunEvents selbst werden nicht gekürzt: Die Retention-Policy
  `PreserveAuditEvents` behält das content-freie Auditjournal vollständig. Rohe Modell-, Tool- und
  Fehlertexte gelangen nie in das Journal und müssen deshalb dort nicht nachträglich gelöscht
  werden. Ein Rebuild oder Cleanup darf weder `agent_runs` noch `run_events` entfernen.
- `a3.run-journal.jsonl` V2 exportiert zusätzlich die unveränderlichen Budgets, den kumulativen
  Verbrauch und die content-freien per-Turn-Charges. Es schreibt eine Headerzeile und danach exakt
  einen kanonischen, schema-versionierten JSON-Datensatz pro Event. Der Export ist auf 10.000
  Events und 8 MiB begrenzt, paginiert Storagezugriffe mit höchstens 256 Events, unterstützt
  Cancellation und monotonen Progress und enthält ausschließlich dieselben content-freien Felder
  wie die DB.
- Ein Rebuild darf keine Task-Historie oder Decisions verlieren.
