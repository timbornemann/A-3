# Plan 02: Storage, Projektidentität und Fast Index

Ziel: A^3 kann einen Worktree sicher öffnen, einen transaktionalen deterministischen Index erstellen und Änderungen inkrementell übernehmen.

Relevante ADRs: 0004, 0005, 0006, 0015, 0016

## S1 Projektpfad und Identität

Abhängigkeiten: Gate M1

Status: Completed

- [x] Ordnerauswahl über schmalen Tauri-Command
- [x] PathPolicy mit Canonicalization und Symlinktests
- [x] Git Common Directory, Worktree Root, HEAD und Unborn erkennen
- [x] RepositoryId und WorktreeId erzeugen
- [x] Worktree-Umzug erkennen und Reconciliation anbieten

Verifizierter Abschluss vom 2026-08-04: Die privilegierte Git-Inspektion leitet eine stabile
`WorktreeAnchorId` aus dem repository-relativen Metadatenpfad ab. A^3 bietet ausschließlich einen
eindeutigen Kandidaten mit gleichem Anchor und gleicher `RepositoryId` oder gleichem vorhandenen
Remote-Fingerprint an. Ein nativer Dialog erlaubt Reconciliation, separates Öffnen oder Abbruch,
ohne die WebView-Capability oder IPC V1 zu erweitern. Die bestätigte Übernahme persistiert einen
crash-resumierbaren Intent, verschiebt die private Worktree-Ablage atomar ohne Überschreiben, schreibt
Repository-/Worktree-Identität samt abhängigen Snapshots und IndexRuns transaktional um und schließt
den Katalog zuletzt ab. Repository- und Linked-Worktree-Umzug, Restart/Resume, separate Öffnung,
Mehrdeutigkeit, veraltete Vorschläge, belegte Ziele und Datenkontinuität sind durch Domain-,
Workspace-, Application-, Adapter- und gemeinsame Storage-Contract-Tests abgedeckt.

Akzeptanz:

- normale Repos, Worktrees, Repos ohne Remote und Unborn Repos funktionieren;
- Pfad außerhalb des gewählten Roots ist nicht implizit erlaubt;
- gleiche Identität bleibt über Appneustart stabil.

## S2 libSQL Storage Foundation

Abhängigkeiten: S1

Status: Completed

- [x] KnowledgeStore-Port aus konkreten Use Cases ableiten
- [x] catalog.db und knowledge.db öffnen
- [x] Migration Runner
- [x] Foreign Keys und pragmatische sichere DB-Konfiguration
- [x] Project-, Snapshot- und IndexRun-Repository
- [x] Storage-Contract-Suite
- [x] korrupte oder neuere DB sicher behandeln

Verifizierter Teilstand vom 2026-08-04: Der lokale Adapter öffnet `catalog.db` an einem typisierten App-Data-Pfad und leitet `projects/<WorktreeId>/knowledge.db` ausschließlich aus der validierten Worktree-Identität ab. Beide Datenbanktypen werden bei Bestand zunächst read-only geprüft, vorwärtsgerichtet und transaktional migriert und mit derselben kontrollierten Connection-Policy betrieben. Knowledge-Schema V1 bindet jede Worktree-Datenbank an `RepositoryId` und `WorktreeId`; V2 normalisiert diese Identität in `repositories` und `worktrees` und ergänzt unveränderliche Snapshot-Ketten, geordnete Pfad-/Hash-Änderungen, Sprachadapterrevisionen und monotone IndexRun-Sequenzen. V3 erlaubt die kontrollierte Identitätsumschreibung über kaskadierende Fremdschlüssel. Linked Worktrees bleiben getrennt. Der zusammengesetzte `KnowledgeStore` prüft die Knowledge-Datenbank vor der atomaren Katalogregistrierung, sodass ein Knowledge-Fehler weder ein erfolgreiches Open-Ergebnis noch neue Recency erzeugt. Der ergänzende `KnowledgeIndexStore` akzeptiert nur die exakt nächste Snapshot-Generation mit unmittelbarem Parent, serialisiert einen `building`-IndexRun pro Worktree und erlaubt vor S10 nur `failed` oder `cancelled`. Katalogschema V3 trennt stabile Projekte, Repository-Beobachtungen und Worktrees, persistiert Anchor-, Remote-, HEAD- und Pfadevidenz sowie Reconciliation-Intents und liefert eine auf zehn Einträge begrenzte Most-recent-first-Projektion. Neustart, Öffnungsreihenfolge, HEAD-Aktualisierung, Linked Worktrees, Repositories ohne Remote, Unborn HEAD, ungültige persistierte Projektionen, widersprüchliche Worktree-Zuordnung, Snapshot-Roundtrip, Generation-/Parent-Konflikte, nicht-UTF-8-Pfade, Run-Lifecycle und fehlgeschlagene V1→V2-/V2→V3-Migrationen sind durch Adapter-Contract- und Regressionstests abgedeckt. Das dev-only Workspace-Crate `a3-storage-contract-tests` prüft dieselben Katalog-, Snapshot-, Worktree-Isolations-, Reconciliation- und IndexRun-Verträge ausschließlich über die Application-Ports; der libSQL-Adapter bindet es über eine schmale Factory an und behält engine-spezifische Negativtests separat.

Akzeptanz:

- leerer Start und Wiederöffnung bestehen;
- fehlgeschlagene Migration verliert keine Bestandsdaten;
- Application enthält weder SQL noch libSQL-Typen.

## S3 Discovery

Abhängigkeiten: S1, S2

- [ ] Git tracked und untracked relevante Dateien erfassen
- [ ] .gitignore, globale und .a3-Ignore-Regeln
- [ ] Binary-, Secret-, Vendor-, Generated- und Größenklassifikation
- [ ] Manifest-, Build-, Test- und CI-Dateien erkennen
- [ ] deterministisch sortierte DiscoveryResult

Akzeptanz:

- ignorierte und geheime Fixtures gelangen nicht in den Index;
- gleiche Eingabe ergibt gleiche sortierte Ausgabe;
- große Dateien werden nicht vollständig geladen.

## S4 Snapshot und Hashing

Abhängigkeiten: S3

- [ ] BLAKE3-Hashing
- [ ] FileRevision und Snapshot
- [ ] HEAD plus Worktree Generation
- [ ] Delta für add, modify, delete und rename candidate
- [ ] unveränderte Dateien überspringen

Akzeptanz:

- mtime-only Änderung erzeugt keine neue Revision;
- Inhaltsänderung mit gleicher Größe wird erkannt;
- Löschungen und Umbenennungen invalidieren alte Referenzen.

## S5 LanguageAdapter Contract

Abhängigkeiten: S4

- [ ] versionierter Adapter-Contract
- [ ] gemeinsame Symbol-, Edge- und Diagnostic-Typen
- [ ] Tree-sitter-Parserpool
- [ ] Parse-Limits und Cancellation
- [ ] gemeinsame Golden-Contract-Suite

Akzeptanz:

- Parserfehler einer Datei brechen nicht den gesamten Index;
- partielle Coverage ist sichtbar;
- Adapterversion fließt in Snapshotkompatibilität ein.

## S6 Rust-Adapter

Abhängigkeiten: S5

- [ ] Module, Funktionen, Structs, Enums, Traits, Impls und Methoden
- [ ] use, mod, pub und Re-Exports
- [ ] syntaktische Calls
- [ ] Tests und Cargo-Manifeste
- [ ] main-, lib- und bin-Einstiegspunkte
- [ ] Golden Fixtures

## S7 TypeScript-/JavaScript-Adapter

Abhängigkeiten: S5

- [ ] Funktionen, Klassen, Interfaces, Types und Methoden
- [ ] imports, exports und Re-Exports
- [ ] Calls und Konstruktoren, soweit syntaktisch bestimmbar
- [ ] Testframework-Heuristiken
- [ ] package.json und Workspace-Manifeste
- [ ] Golden Fixtures

## S8 Python-Adapter

Abhängigkeiten: S5

- [ ] Module, Funktionen, Klassen und Methoden
- [ ] imports und Exports
- [ ] Calls mit sichtbarer Unsicherheitskennzeichnung
- [ ] pytest- und unittest-Erkennung
- [ ] pyproject-, setup- und requirements-Metadaten
- [ ] Golden Fixtures

## S9 Graph Linker und Rank

Abhängigkeiten: S6 bis S8

- [ ] stabile SymbolIds
- [ ] Contains, Defines, Imports, Exports, Calls und Tests
- [ ] Manifest-, Config- und Buildbeziehungen
- [ ] unresolved Edge Candidates
- [ ] Einstiegspunkt- und Zentralitätsprojektion
- [ ] RankingPolicy-Version

Akzeptanz:

- Kanten tragen Provider, Confidence und Evidence;
- ungelöste dynamische Calls werden nicht als sichere Facts ausgegeben;
- Re-Rank benötigt kein Re-Parse.

## S10 Atomisches Publish

Abhängigkeiten: S2, S9

- [ ] File Delta transaktional schreiben
- [ ] alte Symbole und Kanten entfernen oder superseden
- [ ] IndexRun erst nach vollständigem Commit veröffentlichen
- [ ] Crash vor Publish simulieren
- [ ] Rebuild regenerierbarer Tabellen

Akzeptanz:

- Leser sehen nie halben neuen Index;
- Crash lässt letzten veröffentlichten Snapshot intakt;
- Taskdaten überleben Rebuild.

## S11 File Watcher und Incremental Index

Abhängigkeiten: S10

- [ ] plattformneutraler Watcher
- [ ] Debounce und Change Coalescing
- [ ] Bestätigung über Status und Hash
- [ ] begrenzte Jobqueue
- [ ] Indexprogress und Cancellation
- [ ] Full-Rescan-Fallback bei Eventverlust

Akzeptanz:

- Ein-Datei-Änderung parst nicht das gesamte Repo;
- Burst-Änderungen führen zu einem konsistenten Delta;
- P95-Ziel aus QUALITY_GATES.md wird gemessen.

## Gate M2/M3

- [ ] Project Open funktioniert nach Neustart
- [ ] alle drei strukturellen Sprachadapter bestehen Contract und Golden Tests
- [ ] 100.000-LOC-Fixture gemessen
- [ ] inkrementelle add, modify, delete und rename Szenarien grün
- [ ] kein Secret-Fixture in DB oder Logs
- [ ] Windows-, Linux- und macOS-Smoke für Watcher und Pfade
