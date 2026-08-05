# Plan 02: Storage, Projektidentität und Fast Index

Ziel: A^3 kann einen Worktree sicher öffnen, einen transaktionalen deterministischen Index erstellen und Änderungen inkrementell übernehmen.

Relevante ADRs: 0004, 0005, 0006, 0015, 0016, 0017

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

Status: Completed

- [x] Git tracked und untracked relevante Dateien erfassen
- [x] .gitignore, globale und .a3-Ignore-Regeln
- [x] Binary-, Secret-, Vendor-, Generated- und Größenklassifikation
- [x] Manifest-, Build-, Test- und CI-Dateien erkennen
- [x] deterministisch sortierte DiscoveryResult

Verifizierter Abschluss vom 2026-08-04: Der neue Feature-Adapter `a3-repo-index` vereinigt alle im
Git-Index geführten vorhandenen regulären Dateien mit relevanten untracked Dateien aus einem
isolierten `gix`-Dirwalk. Repository-lokale `.gitignore`- und `.git/info/exclude`-Regeln,
ausschließende Gitignore-Muster aus dem strikt begrenzten `[discovery].ignore`-Schema in
`.a3/project.toml` sowie versionierte, nicht übersteuerbare A^3-Sicherheitsdefaults werden mit klarer
Priorität angewendet. Secret-Pfade und hochsichere Credential-Signaturen, Binärdateien, Vendor- und
Generated-Bäume, Symlinks/Reparse-Points, Spezialdateien und Dateien über 4 MiB gelangen nicht in das
Ergebnis. Zulässige Dateien werden für die Klassifikation höchstens bis 16 KiB gelesen; ein
instrumentierter Reader-Test belegt, dass übergroße Dateien gar nicht geöffnet werden. Domain und
Application bleiben frei von Git-, TOML- und OS-Typen, der Adapter revalidiert Worktree Root und Git
Common Directory, prüft Cancellation und emittiert höchstens begrenzte monotone Fortschrittswerte.
Reale Git-Fixtures decken tracked trotz Ignore, untracked, `.gitignore`, `info/exclude`, A^3-Ignore,
alle Sicherheitsklassen, Rollen, deterministische Wiederholung, ungültige Negation und eine
Windows-Junction außerhalb des Worktrees ab. Die reproduzierbare manuelle S3-Baseline entdeckte auf
der lokalen Windows-Entwicklungsmaschine 200 gemischte Dateien mit 100.000 LOC in 416 ms; sie ist kein
Messwert für den vollständigen Fast Index und begründet keinen Beschleunigungsclaim.

Akzeptanz:

- ignorierte und geheime Fixtures gelangen nicht in den Index;
- gleiche Eingabe ergibt gleiche sortierte Ausgabe;
- große Dateien werden nicht vollständig geladen.

## S4 Snapshot und Hashing

Abhängigkeiten: S3

Status: Completed

- [x] BLAKE3-Hashing
- [x] FileRevision und Snapshot
- [x] HEAD plus Worktree Generation
- [x] Delta für add, modify, delete und rename candidate
- [x] unveränderte Dateien überspringen

Verifizierter Abschluss vom 2026-08-04: `RepositoryFileState`, `FileRevision` und `SnapshotDelta`
bilden im Domain-Layer eine kanonisch nach verlustfreien Repository-Pfadbytes sortierte
Inhaltsprojektion. Der lokale `a3-repo-index`-Adapter validiert Repository und Worktree erneut, führt
die versionierte Discovery aus und streamt jede zugelassene Datei in höchstens 64-KiB-Blöcken durch
BLAKE3. Die Obergrenzen betragen 4 MiB pro Datei und 8 GiB pro Beobachtung; Cancellation wird vor
jedem Read geprüft. Dateihandle-Metadaten, HEAD und Git-Index-Checksum werden vor und nach der
Beobachtung verglichen, sodass ein währenddessen veränderter Worktree kontrolliert verworfen wird.
Das Delta unterscheidet Add, Modify und Delete und meldet nur eindeutige contentgleiche
Delete-/Add-Paare als Rename-Kandidaten. Eine domänengetrennte BLAKE3-ID bindet Worktree, Parent,
monotone Generation, HEAD, Discovery-Policy, Indexschema, Adapterrevisionen und kanonische
Änderungen. Ohne Inhalts-, HEAD-, Schema- oder Adapteränderung entsteht keine neue Generation. Der
`KnowledgeIndexStore` rekonstruiert den aktuellen Dateistand aus der unveränderlichen Delta-Kette und
validiert beschädigte persistierte Pfade erneut. Reale Git- und Dateisystemtests decken mtime-only,
gleich große Inhaltsänderungen, HEAD-only, Add/Modify/Delete, eindeutige und mehrdeutige Renames,
Indexänderung während des Hashings, Neustart und Cancellation ab. Die manuelle lokale S4-Baseline
beobachtete 200 gemischte Dateien mit 100.000 LOC einschließlich Discovery und vollständigem Hashing
in 481 ms; Parser, Graph, Ranking und Publish sind darin nicht enthalten und daraus wird kein
allgemeiner Beschleunigungsclaim abgeleitet.

Akzeptanz:

- mtime-only Änderung erzeugt keine neue Revision;
- Inhaltsänderung mit gleicher Größe wird erkannt;
- Löschungen und Umbenennungen invalidieren alte Referenzen.

## S5 LanguageAdapter Contract

Abhängigkeiten: S4

Status: Completed

- [x] versionierter Adapter-Contract
- [x] gemeinsame Symbol-, Edge- und Diagnostic-Typen
- [x] Tree-sitter-Parserpool
- [x] Parse-Limits und Cancellation
- [x] gemeinsame Golden-Contract-Suite

Verifizierter Abschluss vom 2026-08-04: Domain und Application definieren einen versionierten V1-
Vertrag für exakte `FileRevision`-Eingaben, sprachneutrale Symbole, syntaktische Kanten, sichere
Diagnostics und sichtbare Byte-/Region-Coverage. Der Adapter-Port trägt seine konkrete Sprach-,
Adapter- und Grammatikrevision; dieselbe Revision wird durch den Contract-Test in
`SnapshotCompatibility` übernommen. Der lokale Tree-sitter-Unterbau verwendet einen auf 64 Instanzen
begrenzten, wiederverwendbaren Parserpool mit Backpressure, 500-ms-Lease-Wartezeit, zwei Sekunden
Parsezeit, kooperativer Cancellation, begrenztem Fortschritt sowie festen Grenzen für Eingabe,
Baumknoten, Tiefe und Ergebnisartefakte. Quellen werden vor dem Parsen erneut in 64-KiB-Blöcken gegen
ihren BLAKE3-Hash geprüft. Syntaxfehler sind erfolgreiche partielle Parse-Ergebnisse und vergiften
weder den Parser noch nachfolgende Dateien. Das dev-only Workspace-Crate
`a3-language-adapter-contract-tests` prüft für jeden Adapter dieselben Golden-, Determinismus-,
Fehlerisolations-, Coverage-, Cancellation-, Größen-, Hash-, Pfad- und Revisionsverträge. Ein
Tree-sitter-JSON-Probeadapter bindet diese Suite, ohne einen der geplanten Produktadapter S6 bis S8
vorwegzunehmen. Die reproduzierbare Release-Messung auf der lokalen Windows-Entwicklungsmaschine
parste 100.000 strukturelle Zeilen beziehungsweise 1.600.002 Bytes direkt in 317 ms und über den
vollständig begrenzten Pool-Pfad in 369 ms; sie ist kein Messwert für den vollständigen Fast Index.

Akzeptanz:

- Parserfehler einer Datei brechen nicht den gesamten Index;
- partielle Coverage ist sichtbar;
- Adapterversion fließt in Snapshotkompatibilität ein.

## S6 Rust-Adapter

Abhängigkeiten: S5

Status: Completed

- [x] Module, Funktionen, Structs, Enums, Traits, Impls und Methoden
- [x] use, mod, pub und Re-Exports
- [x] syntaktische Calls
- [x] Tests und Cargo-Manifeste
- [x] main-, lib- und bin-Einstiegspunkte
- [x] Golden Fixtures

Verifizierter Abschluss vom 2026-08-04: Der Produktadapter
`rust-tree-sitter-0.24.2-cargo-v1-contract-v1` implementiert den unveränderten V1-Contract für
Rust-Quelldateien und `Cargo.toml`. Er extrahiert Module, Funktionen, Methoden, Structs, Felder,
Enums, Varianten, Traits, Implementierungen, Typaliase, Konstanten und Statics samt Sichtbarkeit,
Signaturen, Dokumentationsbereichen und Test- beziehungsweise Einstiegspunktrollen. `mod`, `use`,
öffentliche Deklarationen und Re-Exports sowie syntaktische Calls, Makroaufrufe, Trait-Bounds und
Implementierungen erzeugen evidenzgebundene Beziehungen. Cargo-Pakete, Workspaces, explizite
Targets, Abhängigkeiten und sichere relative Target-Pfade werden ohne Persistenz- oder
Infrastrukturtypen außerhalb des Adapters abgebildet. Nicht expandierte Makro-Tokenbäume und nicht
unterstützte Call-Target-Formen werden als partielle Coverage ausgewiesen; ein Makroname bleibt als
syntaktischer Call erhalten. Cargo-Manifeste sind auf 256 KiB begrenzt, ungültiges TOML und ungültiges
UTF-8 bleiben sichtbare partielle Ergebnisse. Gemeinsame Contract-Goldens, reale Rust-/Cargo-
Fixtures sowie fokussierte Fehler-, Wiederverwendungs-, Rollen-, Pfad- und Grenztests sichern das
Verhalten. Die reproduzierbare Release-Messung auf der lokalen Windows-Entwicklungsmaschine
verarbeitete 100.000 strukturelle Rust-Zeilen beziehungsweise 1.480.000 Bytes mit 20.001 Symbolen
und 60.001 Beziehungen: direktes Tree-sitter benötigte 249 ms, der vollständige Adapterpfad 445 ms.
Die Messung ist kein Wert für den vollständigen Fast Index und begründet keinen allgemeinen
Performanceclaim.

## S7 TypeScript-/JavaScript-Adapter

Abhängigkeiten: S5

Status: Completed

- [x] Funktionen, Klassen, Interfaces, Types und Methoden
- [x] imports, exports und Re-Exports
- [x] Calls und Konstruktoren, soweit syntaktisch bestimmbar
- [x] Testframework-Heuristiken
- [x] package.json und Workspace-Manifeste
- [x] Golden Fixtures

Verifizierter Abschluss vom 2026-08-05: Der Produktadapter
`typescript-javascript-ts-0.23.2-js-0.25.0-json-0.24.8-package-v1-contract-v1` verwendet die
dedizierten Tree-sitter-Grammatiken für JavaScript/JSX, TypeScript und TSX und verarbeitet `.js`,
`.jsx`, `.mjs`, `.cjs`, `.ts`, `.mts`, `.cts` und `.tsx`. Er extrahiert Funktionen, Generatoren,
Klassen, Interfaces, Typaliase, Enums, Namespaces, Variablen, Methoden, Felder und Varianten samt
Sichtbarkeit, Signaturen, JSDoc-Bereichen sowie Test- und Einstiegspunktrollen. ES-Imports,
ES-Exports, Re-Exports, CommonJS-`require` und -Exports, Heritage-Kanten sowie statisch bestimmbare
Calls und Konstruktoren bleiben mit Quellbereichevidenz erhalten; dynamische Ziele werden nur mit
reduzierter Confidence und sichtbarer partieller Coverage ausgegeben. Heuristiken für Jest, Vitest,
Mocha, Node, Bun und Deno erkennen Suites und Tests, ohne aus Dateinamen außerhalb einer von der
Discovery gelieferten Testrolle Fakten abzuleiten.

Der Manifestpfad parst `package.json` mit einer dedizierten JSON-Grammatik und erzeugt begrenzte
Beziehungen für Dependencies, Dev-Dependencies, Scripts, Workspaces, Exports, Browser-Mappings,
Binärziele und sichere repository-relative Einstiegspfade. Export-Patterns bleiben ungelöste
Build-Kandidaten; Pfadtraversal, falsche JSON-Formen und ungültige Kodierung erzeugen partielle
Coverage. `pnpm-workspace.yaml` unterstützt bewusst nur die begrenzte skalare Top-Level-
`packages`-Sequenz und kennzeichnet komplexe YAML-Formen als nicht unterstützt. `package.json` ist
auf 512 KiB, das pnpm-Manifest auf 256 KiB begrenzt. Gemeinsame Contract-Goldens, ein lizenziertes
TypeScript-Monorepo-Fixture sowie Dialekt-, Syntaxfehler-, CommonJS-, TSX-, Rollen-, Manifest-,
Pfad-, Größen- und Wiederverwendungstests sichern das Verhalten.

Die reproduzierbare Release-Messung auf der lokalen Windows-Entwicklungsmaschine verarbeitete
100.000 strukturelle TypeScript-Zeilen beziehungsweise 1.740.000 Bytes mit 20.001 Symbolen und
60.001 Beziehungen. Ein Zwischenstand mit quadratischer globaler Contains-Duplikatsuche benötigte
1.782 ms über den Adapterpfad; nach deren Entfernung benötigte direktes Tree-sitter 197 ms und der
vollständige begrenzte Adapterpfad 399 ms. Die Messung ist kein Wert für den vollständigen Fast Index
und begründet keinen allgemeinen Performanceclaim.

## S8 Python-Adapter

Abhängigkeiten: S5

Status: Completed

- [x] Module, Funktionen, Klassen und Methoden
- [x] imports und Exports
- [x] Calls mit sichtbarer Unsicherheitskennzeichnung
- [x] pytest- und unittest-Erkennung
- [x] pyproject-, setup- und requirements-Metadaten
- [x] Golden Fixtures

Verifizierter Abschluss vom 2026-08-05: Der Produktadapter
`python-tree-sitter-0.25.0-pyproject-setup-requirements-v1-contract-v1` bindet die offizielle
Tree-sitter-Python-Grammatik 0.25.0 und verarbeitet `.py`- und `.pyi`-Dateien. Er extrahiert Module,
synchrone und asynchrone Funktionen, Klassen, Methoden und PEP-695-Typaliase samt Signaturen,
konventionsbasierter Sichtbarkeit, Docstring-Bereichen sowie Test- und Einstiegspunktrollen. Direkte,
aliasierte, relative, Future- und From-Imports, explizites statisches `__all__`, konventionell
öffentliche Moduldeklarationen und Basisklassen erzeugen evidenzgebundene Kandidaten. Da Python
Aufrufziele laufzeitdynamisch sind, tragen direkte Calls höchstens 7.000, Attribut-Calls 6.000 und
Subscript- beziehungsweise dynamische Attribut-Calls 4.000 Confidence-Basispunkte. Nicht stabil
darstellbare verschachtelte Call-Ziele und Wildcard-Bindungen erzeugen sichtbare partielle Coverage.
pytest-Fixtures, Marks, `test_`-Deklarationen und -Klassen sowie `unittest.TestCase`-Klassen und deren
Testmethoden werden über begrenzte Syntax- und Pfadheuristiken erkannt.

`pyproject.toml` unterstützt PEP-621-Abhängigkeiten, optionale Gruppen, Build-Systeme, Scripts,
Plugin-Entrypoints, pytest-Konfiguration und statische Poetry-Abhängigkeiten, Gruppen und Scripts.
`setup.py` bleibt ausführbarer, nicht autoritativer Python-Code; nur statische Argumente eines
sichtbaren `setup`-Calls werden mit 8.500 Confidence als Projekt-, Dependency-, Package- und
Entrypoint-Metadaten interpretiert. Ein begrenzter `setup.cfg`-Teilparser verarbeitet die entsprechenden
Metadata-, Options-, Extras- und Entrypoint-Sektionen. Requirements-Dateien unterstützen PEP-508-
Paketnamen, Test-/Dev-Rollen, credential-freie Direct-Reference-Namen sowie sichere lokale Include-
und Constraint-Pfade, ohne Index-URLs oder Zugangsdaten in Artefakte zu übernehmen. Dynamische oder
komplexere Formen bleiben explizit partiell. `pyproject.toml` und Requirements sind auf 512 KiB,
`setup.cfg` auf 256 KiB begrenzt; Python-Quellen verwenden das gemeinsame 4-MiB-Contract-Limit.
Gemeinsame Contract-Goldens, ein lizenziertes Python-Package-Fixture sowie Syntax-, Import-, Export-,
Confidence-, pytest-, unittest-, Manifest-, Traversal-, Kodierungs-, Abbruch-, Fortschritts- und
Größentests sichern das Verhalten.

Die reproduzierbare Release-Messung auf der lokalen Windows-Entwicklungsmaschine verarbeitete
100.000 strukturelle Python-Zeilen beziehungsweise 1.750.000 Bytes mit 50.001 Symbolen und 150.001
Beziehungen. Direktes Tree-sitter benötigte 308 ms, der vollständige begrenzte Adapterpfad 620 ms.
Die Messung ist kein Wert für den vollständigen Fast Index und begründet keinen allgemeinen
Performanceclaim.

## S9 Graph Linker und Rank

Abhängigkeiten: S6 bis S8

Status: Completed

- [x] stabile SymbolIds
- [x] Contains, Defines, Imports, Exports, Calls und Tests
- [x] Manifest-, Config- und Buildbeziehungen
- [x] unresolved Edge Candidates
- [x] Einstiegspunkt- und Zentralitätsprojektion
- [x] RankingPolicy-Version

Verifizierter Abschluss vom 2026-08-05: `SymbolId` V1 ist ein domänengetrennter BLAKE3-Digest über
verlustfreie Pfadbytes, Content Hash, Sprache, Adapterrevision, Contract-Version und lokale Symbol-ID.
Er bleibt damit bei identischer Parse-Evidenz und jedem Re-Rank stabil, wechselt aber bei geändertem
Inhalt oder Adaptervertrag. Der Linker akzeptiert nur Parse-Ergebnisse, deren Dateirevision im
effektiven Snapshotzustand und deren Adapterrevision im Snapshot exakt vorhanden ist. Jede kanonische
Kante bindet ihren Snapshot sowie eine `EvidenceRef` aus Pfad, Content Hash und Quellbereich und
behält Relationstyp, ursprünglichen Provider, gekappte Confidence und die sichtbare Auflösungsart.

Direkte lokale Symbole und vorhandene Adapter-Dateiziele werden exakt aufgelöst. Eindeutige relative
TypeScript-/JavaScript-Module, Python-Pakete, relative Python-Imports und Entrypoints sowie
Rust-`crate`-, `self`-, `super`- und Modulpfade erhalten eine konservative sprachspezifische
Auflösung. Einfache Calls dürfen nur ein eindeutiges Symbol derselben Datei treffen; globale
Eindeutigkeit wird nur für `Extends` und `Implements` verwendet. Linkerauflösungen sind je nach
Evidenzklasse auf 9.500, 9.000 oder 8.500 Confidence-Basispunkte begrenzt. Fehlende, mehrdeutige,
externe oder laufzeitdynamische Ziele werden nicht als Graphkante ausgegeben, sondern bleiben als
getrennter `UnresolvedEdgeCandidate` mit Grund und vollständiger Evidenz erhalten. Manifest-, Test-,
Config- und Buildbeziehungen durchlaufen denselben Vertrag.

RankingPolicy V1 benötigt ausschließlich den fertigen `LinkedGraph`; ein Re-Rank liest oder parst
keine Quelldatei. Die ganzzahlige, reproduzierbare Projektion kombiniert Einstiegspunkt-, Export-,
Test-, Manifest-/Build-, In-/Out-Degree- und modulübergreifende Zentralitätssignale und löst
Scoregleichstände über `SymbolId` auf. Linken ist auf 250.000 Dateien/Parses, 1.000.000 Symbole,
jeweils 2.000.000 Kanten und Kandidaten, zehn Sekunden und 64 Progressereignisse begrenzt; Rank ist
auf 1.000.000 Symbole, 2.000.000 Kanten, fünf Sekunden und ebenfalls 64 Ereignisse begrenzt. Beide
Pfade prüfen Cancellation spätestens nach 1.024 Arbeitseinheiten.

Ein selbst erstelltes gemischtes Rust-/Cargo-, TypeScript- und Python-/pyproject-Fixture sichert den
normalisierten Graph- und Rank-Golden-Digest, Struktur-, Import-, Export-, Call-, Test-, Config- und
Buildbeziehungen, sprachspezifische Auflösung,
dynamische und mehrdeutige Kandidaten, deterministische Wiederholung bei geänderter Eingabereihenfolge,
stale Hashes, falsche Adapterrevisionen, Evidenzzugehörigkeit, leere Eingaben, Ressourcenlimits,
Cancellation und Progressfehler. Die reproduzierbare Release-Messung auf der lokalen
Windows-Entwicklungsmaschine verwendete das 100.000 strukturelle Zeilen beziehungsweise 1.750.000
Bytes große Python-Fixture mit 50.001 Symbolen, 100.001 aufgelösten Kanten und 50.000 ungelösten
Kandidaten. Link benötigte 299 ms und Rank 57 ms. Parsing war nicht Teil dieser beiden Messwerte;
die Messung ist kein Wert für den vollständigen Fast Index und begründet keinen allgemeinen
Performanceclaim.

Akzeptanz:

- Kanten tragen Provider, Confidence und Evidence;
- ungelöste dynamische Calls werden nicht als sichere Facts ausgegeben;
- Re-Rank benötigt kein Re-Parse.

## S10 Atomisches Publish

Abhängigkeiten: S2, S9

Status: Completed

- [x] File Delta transaktional schreiben
- [x] alte Symbole und Kanten entfernen oder superseden
- [x] IndexRun erst nach vollständigem Commit veröffentlichen
- [x] Crash vor Publish simulieren
- [x] Rebuild regenerierbarer Tabellen

Verifizierter Abschluss vom 2026-08-05: Der neue domänenreine `IndexPublication`-Vertrag akzeptiert
nur einen `LinkedGraph` und eine `RankProjection` desselben Snapshots mit exakt derselben Symbolmenge.
Knowledge-Schema V4 ergänzt run-gebundene `file_revisions`, `symbols`, `symbol_edges`,
`unresolved_edges` und `ranking_projections`. Der libSQL-Adapter rekonstruiert den effektiven
Dateistand innerhalb einer `IMMEDIATE`-Transaktion aus der unveränderlichen Snapshot-Delta-Kette,
vergleicht Pfad und Content Hash mit dem vollständigen Graph und prüft Run-Snapshot sowie
RankingPolicy-Version. Erst nachdem alle Datei-, Symbol-, Evidenz-, Kandidaten- und erklärbaren
Rankzeilen geschrieben wurden, setzt das letzte Statement den aktiven Run von `building` auf
`published`. Der vollständige Leser wählt den jüngsten veröffentlichten Run und rekonstruiert den
typisierten Graph samt Ranking in einer konsistenten Read-Transaktion.

Jede Zeile trägt ihre `IndexRunId`; ältere veröffentlichte Graphen bleiben damit vollständig
superseded und können nicht mit dem neuen Sichtstand vermischt werden. Publish und vollständiges
Lesen sind auf die bestehenden S9-Ressourcengrenzen begrenzt, prüfen Cancellation spätestens nach
1.024 Zeilen, melden höchstens 64 monotone Progressereignisse und haben ein festes Fünf-Minuten-
Timeout. Der Rebuild verweigert einen aktiven `building`-Run und löscht ausschließlich
run-gebundene, regenerierbare Indexzeilen in 4.096-Zeilen-Batches sowie anschließend die IndexRuns.
Snapshots, Worktree-Bindung und fremde nicht regenerierbare Tabellen bleiben unangetastet.

Die gemeinsame Storage-Contract-Suite deckt vollständigen Roundtrip über die Application-Grenze,
Appneustart, File-Modify und Delete, Supersede, falsche Snapshot-/Policy-Bindung, Linked-Worktree-
Isolation, Rebuild und den Erhalt der Snapshotkette ab. Engine-spezifische Tests erzwingen per
abbrechendem Datenbanktrigger einen Fehler unmittelbar vor dem sichtbaren Statuswechsel und belegen,
dass alle neuen Zeilen zurückgerollt werden, der letzte veröffentlichte Index lesbar bleibt und der
fehlgeschlagene Run weiterhin kontrolliert abgeschlossen werden kann. Weitere Negativtests sichern
Cancellation vor Mutation, Progress-Ausfall, höchstens 64 Progresswerte, den Erhalt einer simulierten
Tasktabelle beim Rebuild sowie atomare V3→V4-Migration und Rollback bei einem Schemakonflikt.

Akzeptanz:

- Leser sehen nie halben neuen Index;
- Crash lässt letzten veröffentlichten Snapshot intakt;
- Taskdaten überleben Rebuild.

## S11 File Watcher und Incremental Index

Abhängigkeiten: S10

Status: Completed

- [x] plattformneutraler Watcher
- [x] Debounce und Change Coalescing
- [x] Bestätigung über Status und Hash
- [x] begrenzte Jobqueue
- [x] Indexprogress und Cancellation
- [x] Full-Rescan-Fallback bei Eventverlust

Verifizierter Abschluss vom 2026-08-05: Ein besitzender, standardbibliotheksbasierter Polling-Watcher
verwendet die isolierte Git-Discovery und vergleicht Kandidatensatz, HEAD, Index-Checksum sowie
plattformgekapselte Dateimetadaten. Er pollt im V1-Profil alle 100 ms, entprellt 200 ms, schließt
Bursts spätestens nach 750 ms und hält höchstens einen fertigen Batch. Pfade werden kanonisch
koalesziert. Initialbeobachtung, Repository-Metadatenwechsel, Beobachtungsfehler und Queue-Überlauf
werden als typisierte Full-Rescan-Gründe sichtbar; der Worker besitzt Shutdown und Join.

Der inkrementelle Snapshot-Builder bestätigt jeden Batch erneut über den vollständigen
Git-Discovery-Pfadsatz und BLAKE3, übernimmt aber unveränderte Revisionen aus der dauerhaften
Baseline. Der zustandsbehaftete Drei-Sprachen-Compiler behält nur einen exakt zum Parent passenden
Parse-Cache, entfernt Deletes und parst ausschließlich geänderte oder neue unterstützte Dateien.
Nach Neustart wird der Cache einmal vollständig aufgewärmt; ein bereits identischer Publish wird
nicht dupliziert. Link und Rank bleiben vollständig deterministisch. Der Application-Use-Case führt
Snapshot-Append, eindeutigen IndexRun, Failure/Cancel-Abschluss und atomisches Publish zusammen.
Der Desktop-Composition-Root startet ihn nach Project Open über einen besitzenden Koordinator und
die vorhandene begrenzte Scheduler-Queue, ohne die WebView-Capability zu erweitern.

Reale Git-/Filesystem-/libSQL-Tests belegen genau einen Hash und Parse bei einer Ein-Datei-Änderung,
einen konsistenten gemeinsamen Add-/Modify-/Delete-/Rename-Burst, gleich große Inhaltsänderungen,
Secret-Ausschluss, Restart/Warmup, Eventverlust-Fallback, Progress, Cancellation unter 500 ms und
Watcher-Shutdown unter 500 ms. Die bestehende CI-Matrix ist für native Läufe auf Windows, Linux und
macOS konfiguriert; verifizierte Ergebnisse für diesen Commit stehen noch aus. Die reproduzierbare
30-Sample-Release-Messung auf Windows 11, Ryzen 9 5900XT und
NVMe verwendete 200 Rust-Dateien mit 100.000 LOC und maß den gesamten Pfad vom Write über Debounce,
Discovery, Hash, Parse, Link, Rank bis libSQL-Publish: P50 1.202 ms, P95 1.305 ms, Watcher-P95 389 ms
und Refresh-/Publish-P95 922 ms. Die zunächst gemessenen 15.286 ms wurden durch begrenzte
Mehrzeilen-Inserts und transaktionale Retention supersedeter Indexprojektionen reduziert.

Akzeptanz:

- Ein-Datei-Änderung parst nicht das gesamte Repo;
- Burst-Änderungen führen zu einem konsistenten Delta;
- P95-Ziel aus QUALITY_GATES.md wird gemessen.

## Gate M2/M3

- [x] Project Open funktioniert nach Neustart
- [x] alle drei strukturellen Sprachadapter bestehen Contract und Golden Tests
- [x] 100.000-LOC-Fixture gemessen
- [x] inkrementelle add, modify, delete und rename Szenarien grün
- [x] kein Secret-Fixture in DB oder Logs
- [ ] Windows-, Linux- und macOS-Smoke für Watcher und Pfade
