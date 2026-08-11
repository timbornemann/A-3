# Indexierung und Projektkarte

Status: verbindliche Baseline  
Stand: 2026-08-05

## Grundsatz

Die Projektkarte ist kein frei formulierter Repository-Überblick des LLM. Sie ist eine versionierte Projektion aus:

1. deterministisch extrahierten Fakten;
2. graphbasierten Beziehungen;
3. LLM-erzeugten, evidenzgebundenen Module Cards;
4. expliziten Unsicherheiten und Aktualitätsstatus.

Die Karte muss ohne Vektorindex und ohne verfügbares LLM weiterhin nutzbar sein.

## Drei Indexmodi

| Modus | Auslöser | LLM | Ergebnis |
| --- | --- | --- | --- |
| Fast Index | automatisch beim Öffnen und bei Änderungen | nein | Dateien, Symbole, Kanten, FTS, Einstiegspunkte, Tests |
| Deep Map | bewusst gestartet oder nach Zustimmung | ja, budgetiert | Module Cards, Abläufe, Invarianten, Hypothesen |
| Task Lens | bei jeder Aufgabe und nach relevanten Änderungen | nur Context-Planung | kleiner zielbezogener Teilgraph |

## Fast Index

Phasen:

~~~text
DISCOVER → HASH → PARSE → LINK → RANK → PUBLISH
~~~

### Discover

- Git Common Directory, Worktree Root, HEAD und Status erkennen
- Ignore-Regeln aus Git, A^3-Konfiguration und sicheren Defaults zusammenführen
- Binärdateien, generierte Verzeichnisse, Vendor-Code und Größenlimits erkennen
- Manifest-, Build-, Test- und CI-Dateien klassifizieren
- Sprache je Datei bestimmen

Git-tracked Dateien werden standardmäßig berücksichtigt. Nicht getrackte Dateien können einbezogen werden, wenn sie nicht ignoriert sind. Secrets, bekannte Credential-Dateien und explizit ausgeschlossene Pfade werden nie indexiert.

Discovery V1 ist durch ADR-0017 konkretisiert: Repository-lokale Git-Ignores, ausschließende Muster
aus `.a3/project.toml` und nicht übersteuerbare sichere A^3-Defaults werden in einem begrenzten lokalen
Adapter zusammengeführt. Benutzerweite Git-Excludes außerhalb des freigegebenen Worktrees sind aus
Reproduzierbarkeits- und Pfadautoritätsgründen nicht implizit aktiv. Dateien oberhalb von 4 MiB werden
nicht geöffnet; von kleineren Kandidaten werden höchstens 16 KiB für Binary- und Secret-Erkennung
gelesen. Manifest-, Build-, Test- und CI-Rollen können überlappen. Die Sprachzuordnung folgt mit dem
versionierten `LanguageAdapter` in S5 und ist nicht Teil der S3-Pfadklassifikation.

Die nicht übersteuerbaren Pfad- und Präfixklassifikationen gehören zu `DiscoveryPolicy::v1` im
Domain-Kern. Git-Discovery und die sicheren E2-Dateiwerkzeuge verwenden damit dieselbe versionierte
Definition für Secret-, Binary-, Vendor- und Generated-Ausschlüsse, ohne dass der Workspace-
Adapter eine zweite abweichende Patternliste pflegt. Ein E2-Directory-Listing wird aus dem
aktuellen `PublishedIndex` projiziert und übernimmt dadurch auch repository-lokale Git-Ignores und
gültige ausschließende `.a3/project.toml`-Muster; die eingebauten Ausschlüsse werden am Toolrand
zusätzlich erneut geprüft.

### Hash

- BLAKE3 Content Hash pro relevanter Datei
- Metadaten nur als Änderungs-Hinweis, nie als Wahrheitsquelle
- unveränderte Content Hashes überspringen
- Löschungen und Umbenennungskandidaten erfassen

Snapshot V1 führt Discovery und Hashing als eine kohärente, abbrechbare Worktree-Beobachtung aus.
Jeder relevante Dateiinhalt wird vollständig und gepuffert in Blöcken von höchstens 64 KiB gehasht;
pro Datei gelten weiterhin 4 MiB und pro Lauf zusätzlich 8 GiB als feste Obergrenze. A^3 prüft den
geöffneten Dateihandle vor und nach dem Lesen sowie HEAD und Git-Index vor und nach der Beobachtung.
Eine zwischenzeitliche Änderung verwirft den gesamten Lauf statt einen gemischten Snapshot zu
erzeugen.

`RepositoryFileState` ordnet normalisierte Pfade byteweise ihren BLAKE3-Hashes zu. Das Delta zur
persistierten Vorgängergeneration unterscheidet Add, Modify und Delete; genau ein gelöschter und ein
hinzugefügter Pfad mit demselben Hash ergeben lediglich einen konservativen Rename-Kandidaten.
Mehrdeutige gleiche Inhalte werden nicht als Rename behauptet. Unveränderte Pfad-/Hash-Paare
erzeugen keinen Delta-Eintrag. Eine neue monotone Worktree-Generation entsteht nur, wenn sich
Dateiinhalt, HEAD, Indexschema oder eine Adapterrevision ändert. Die Snapshot-ID ist ein
domänengetrennter BLAKE3-Digest über Worktree, Parent, Generation, HEAD, Discovery-Policy,
Indexschema, geordnete Adapterrevisionen und das kanonische Delta.

### Parse

V1-Sprachen mit strukturellem Support:

- Rust
- TypeScript und JavaScript
- Python

Andere Textsprachen erhalten zunächst Datei-, FTS- und Manifest-Support. Ein LanguageAdapter liefert:

- Symbole und Signaturen
- Dokumentationsbereiche
- Imports und Exports
- syntaktisch erkennbare Aufrufe
- Testdefinitionen
- Parsefehler und Abdeckungsgrad

Tree-sitter ist die Baseline. LSP oder SCIP kann später als zusätzlicher Edge Provider eingesetzt werden; es ersetzt den lokalen Parser nicht.

Der versionierte V1-Contract bindet jeden Parse an die exakte `FileRevision` und prüft die übergebenen
Bytes erneut gegen deren BLAKE3-Hash. Gemeinsame Domain-Typen repräsentieren halboffene Byte-/
Positionsbereiche, file-lokale Symbol-IDs, sprachneutrale Symbolarten, syntaktische Beziehungen mit
Provider und Confidence sowie sichere Diagnostics ohne Quelltextauszüge. Ein Ergebnis enthält die
konkrete Sprach-, Adapter- und Grammatikrevision und eine explizite Coverage aus Gesamtbytes,
abgedeckten Bytes und unvollständigen Regionen. Syntaxfehler bleiben damit partielle Ergebnisse und
sind kein globaler Indexfehler.

Parser werden pro Grammatik in einem begrenzten, wiederverwendbaren Pool gehalten. V1 akzeptiert
höchstens 4 MiB pro Quelldatei, wartet höchstens 500 ms auf einen Parser, begrenzt den Parse auf zwei
Sekunden und prüft Baumgröße, Baumtiefe und Ergebnisanzahlen. Cancellation und monotone, begrenzte
Fortschrittsmeldungen gelten während Warten, Parsen und Baumprüfung. Jeder konkrete Adapter muss die
gemeinsame Golden-Contract-Suite bestehen; Sprachadapterrevisionen werden in der
Snapshotkompatibilität erfasst.

Der Rust-Adapter V1 bindet `tree-sitter-rust` 0.24.2 und verarbeitet `.rs`-Dateien sowie
`Cargo.toml`. Er liefert Rust-Deklarationen einschließlich Traits, Implementierungen und Methoden,
Sichtbarkeit, Dokumentationsbereiche, Test- und Einstiegspunktrollen sowie syntaktische Import-,
Export-, Call-, Implements- und Extends-Kandidaten. Cargo-Pakete, Workspaces, explizite Targets und
Abhängigkeiten ergänzen Manifestbeziehungen; relative Target-Pfade dürfen den Repository-Namensraum
nicht verlassen. Cargo-Manifeste sind auf 256 KiB begrenzt. Tree-sitter expandiert keine Makros:
Der Adapter erhält den Makronamen als syntaktischen Call, markiert den nicht strukturell analysierten
Tokenbaum aber explizit als unvollständige Coverage. Unsupported Call-Target-Formen, Syntaxfehler und
ungültige Kodierung werden ebenfalls sichtbar statt stillschweigend als vollständige Erkenntnis zu
gelten.

Der TypeScript-/JavaScript-Adapter V1 bindet die offiziellen Tree-sitter-Grammatiken für
JavaScript/JSX 0.25.0 sowie TypeScript/TSX 0.23.2 und erkennt die üblichen JS-, JSX-, MJS-, CJS-,
TS-, MTS-, CTS- und TSX-Endungen. Deklarationen einschließlich Interfaces, Typaliasen, Namespaces,
Klassenmitgliedern und Enumvarianten tragen Signatur-, Sichtbarkeits-, JSDoc-, Test- und
Einstiegspunktevidenz. ES- und CommonJS-Modulbeziehungen, Heritage sowie syntaktisch bestimmbare
Calls und Konstruktoren werden als Kandidaten ausgegeben; dynamische Formen bleiben mit reduzierter
Confidence und partieller Coverage sichtbar. Jest-, Vitest-, Mocha-, Node-, Bun- und Deno-Formen
werden über begrenzte Syntaxheuristiken erkannt.

`package.json` wird strukturell mit `tree-sitter-json` 0.24.8 bis 512 KiB analysiert. Dependencies,
Scripts, Workspaces, Binärziele, Exports und Browser-Mappings liefern Manifestbeziehungen, wobei nur
sichere repository-relative Dateiziele zu File-Kanten werden. Export-Patterns bleiben ungelöste
Build-Kandidaten. Für `pnpm-workspace.yaml` gilt eine absichtlich schmale Obergrenze von 256 KiB und
ein dokumentierter Teilparser für die skalare Top-Level-`packages`-Sequenz. Komplexere YAML-Formen,
ungültige Wertformen, Kodierungsfehler und unsichere Pfade erzeugen partielle Coverage statt
vermeintlich vollständiger Fakten.

Der Python-Adapter V1 bindet `tree-sitter-python` 0.25.0 und verarbeitet `.py` sowie `.pyi`.
Funktionen, Klassen, Methoden und Typaliase tragen Signatur-, Sichtbarkeits-, Docstring-, Test- und
Einstiegspunktevidenz. Imports, statisches `__all__`, konventionell öffentliche Moduldeklarationen
und Basisklassen werden strukturell erfasst. Python-Calls bleiben grundsätzlich Kandidaten: direkte
Namen, Attributzugriffe und dynamische Subscripts erhalten abgestufte Confidence; nicht stabil
darstellbare Ziele erzeugen partielle Coverage. pytest-Fixtures, Marks und Namenskonventionen sowie
`unittest.TestCase`-Vererbung liefern begrenzte Testrollen und -beziehungen.

`pyproject.toml` wird bis 512 KiB als PEP-621-/Poetry-/Build-/pytest-Metadatenquelle verarbeitet.
Statische `setup.py`-Argumente sind bewusst nur heuristische Kandidaten, weil die Datei ausführbarer
Python-Code bleibt. `setup.cfg` erhält einen 256-KiB-Teilparser für Package-, Dependency-, Extras- und
Entrypoint-Sektionen. Requirements-Dateien bis 512 KiB liefern Paket- und sichere lokale
Include-Beziehungen; Indexoptionen, dynamische Formen, unsichere Pfade und ungültige Kodierung werden
ohne Übernahme potentieller Credentials als partielle Coverage ausgewiesen.

### Link

Kantentypen in V1:

- Contains
- Defines
- Imports
- Exports
- Calls
- Implements
- Extends
- Reads
- Writes
- Configures
- Tests
- Builds
- Documents

Jede Kante trägt Provider, Confidence, Snapshot und EvidenceRef. Syntaktisch unsichere Calls bleiben als Kandidaten gekennzeichnet.

Graph Linker V1 erzeugt für jedes Adaptersymbol eine domänengetrennte BLAKE3-`SymbolId` aus den
verlustfreien Pfadbytes, dem Content Hash, Sprache, Adapterrevision, Contract-Version und lokaler
Symbol-ID. Die Identität ist damit für dieselbe Parse-Evidenz stabil und unabhängig von Snapshot-
und Rankingversion; geänderte Inhalte oder Adaptersemantik erzeugen absichtlich neue IDs. Vor dem
Linken müssen Parse-Revision, effektiver Dateistand und im Snapshot erfasste Adapterrevision exakt
übereinstimmen.

Direkte lokale Symbol- und vorhandene Adapter-Dateiziele werden unverändert übernommen. Der
konservative Resolver unterstützt eindeutige relative TypeScript-/JavaScript-Module, Python-Pakete,
relative Imports und `module:symbol`-Entrypoints sowie Rust-`crate`-, `self`-, `super`- und
Modulpfade. Ein einfacher Name darf nur auf ein eindeutiges Symbol derselben Datei zeigen; ein
global eindeutiger Name wird ausschließlich für `Extends` und `Implements` verwendet. Linkerbasierte
Auflösung kappt Confidence je nach Evidenzklasse bei 9.500, 9.000 oder 8.500 Basispunkten. Fehlende,
mehrdeutige und laufzeitdynamische Ziele bleiben als eigener `UnresolvedEdgeCandidate` mit Grund,
Provider, ursprünglicher Confidence, Snapshot und `EvidenceRef` erhalten. Dieser Typ kann nicht als
aufgelöste `GraphEdge` verwendet werden.

V1 begrenzt einen Linklauf auf 250.000 Dateien und Parse-Ergebnisse, 1.000.000 Symbole, jeweils
2.000.000 aufgelöste Kanten und ungelöste Kandidaten, zehn Sekunden sowie 64 Fortschrittsereignisse.
Cancellation und Timeout werden spätestens nach 1.024 Arbeitseinheiten erneut geprüft. Die
kanonische Ausgabe validiert alle Endpunkte, Dateihashes, Snapshotbindungen und die Zugehörigkeit der
Evidenz zum Quellknoten.

### Rank

Deterministische Signale:

- Einstiegspunkt
- öffentlicher Export
- In-Degree und Out-Degree
- Pfad- und Manifestnähe
- Testbezug
- Änderungsfrequenz, falls lokal verfügbar
- Modulzentralität

Die Rankingversion wird gespeichert. Änderungen am Algorithmus erzwingen keine Neu-Parse, aber eine neue Rank-Projektion.

RankingPolicy V1 arbeitet ausschließlich auf dem kanonischen `LinkedGraph` und verwendet keine
Parse-Ergebnisse. Sie projiziert Datei- und Symbolendpunkte über die `Defines`-Wurzel auf Module und
berechnet Modulzentralität mit plattformstabiler ganzzahliger Arithmetik. Der erklärbare Score addiert
5.000 Punkte für Einstiegspunkte, 2.000 für öffentliche oder exportierte Symbole, jeweils 1.500 für
Manifest-/Buildnähe und Testbezug, 200 je eingehender sowie 100 je ausgehender Kante bis maximal
4.000 Degree-Punkte und bis zu 3.000 Punkte aus der Modulzentralität. Gleichstände werden über die
stabile `SymbolId` aufgelöst. Ein Ranklauf akzeptiert höchstens 1.000.000 Symbole und 2.000.000
aufgelöste Kanten, läuft höchstens fünf Sekunden und meldet höchstens 64 Fortschrittsereignisse.

### Deterministische Modulbildung

Index-Schema V4 ergänzt den Rank-Schritt vor dem atomischen Publish um eine LLM-freie
`ModuleProjection`. `ModuleFormationPolicy::v1` verwendet als Primärsignal den jeweils nächsten
bestätigten Paketmanifest-Root. Als Paketgrenzen gelten die bekannten Descriptoren für Rust,
TypeScript/JavaScript, Python und den generischen Manifest-Support; Lockfiles,
`pnpm-workspace.yaml`, Requirements-Dateien und Dockerfiles erzeugen dagegen keine künstlichen
Paketgrenzen. Mehrere Descriptoren im selben Verzeichnis bilden eine gemeinsame Grenze. Liegt für
ein Symbol kein Manifest-Root auf seinem Pfad, wird die erste Pfadkomponente verwendet; Dateien im
Repository-Root gehören zur expliziten Repository-Root-Grenze. Dadurch gewinnt in Monorepos stets
das tiefste verschachtelte Paket, während manifestlose Repositories weiter vollständig abgedeckt
bleiben.

Jedes Symbol erhält genau eine primäre Membership. Ihre Evidence enthält immer die aktuelle
`FileRevision` des Symbols und bei manifestbasierten Modulen zusätzlich eine aktuelle bestätigte
Manifestrevision. Primäre `ModuleId`s werden mit `a3.module.primary.v1` allein aus dem kanonischen
Root abgeleitet. Für alle gerichteten Symbolkanten außer `Contains` und `Defines` berechnet der
Former ohne Rekursion die stark zusammenhängenden Komponenten. Komponenten mit mindestens zwei
Symbolen werden ergänzende Graphcommunities; ihr `ModuleId` entsteht mit
`a3.module.community.v1` aus den sortierten `SymbolId`s. Jede zusätzliche Membership bewahrt
mindestens einen aktuellen ein- oder ausgehenden Kantenbeleg innerhalb derselben Community und
ersetzt niemals die primäre Zugehörigkeit.

Pro Modul werden höchstens 16 zentrale Symbole sowie jeweils 256 belegte Entrypoints und Tests in
der bestehenden Rankreihenfolge gespeichert. Trunkierung bleibt typisiert sichtbar. Die
deterministische L0-Repository-Card enthält Primärmodule/Pakete, beobachtete Sprachfamilien,
höchstens 256 globale Entrypoints sowie exakte Datei- und Symbolzahlen. Formation ist auf 250.000
Module und 2.000.000 Memberships begrenzt, prüft Cancellation spätestens alle 1.024
Arbeitseinheiten, besitzt ein Fünf-Sekunden-Limit und meldet determinierten Phasenfortschritt.
Identische Eingaben ergeben identische Modul-IDs, Memberships, Evidence-Auswahl und Card.

### Publish

Ein Indexlauf wird erst sichtbar, wenn:

- alle Deltas erfolgreich in einer Transaktion committed wurden;
- Löschungen verarbeitet sind;
- abgeleitete Claims invalidiert sind;
- FTS und Graph dieselbe Snapshot-Version referenzieren.

Knowledge-Schema V4 speichert die vollständige effektive Dateiprojektion, Symbole, aufgelöste
Kanten, ungelöste Kandidaten und erklärbare Rankzeilen unter der jeweiligen `IndexRunId`. Der
Adapter rekonstruiert den Dateistand innerhalb derselben Transaktion aus der unveränderlichen
Snapshot-Delta-Kette und verlangt exakte Übereinstimmung mit dem vorbereiteten `LinkedGraph`.
Graphsnapshot, Ranking-Snapshot, RankingPolicy-Version und der aktive `building`-Run müssen ebenfalls
identisch sein. Erst nachdem alle run-gebundenen Zeilen geschrieben wurden, wechselt der letzte
Transaktionsschritt den Run auf `published`. Leser wählen ausschließlich den jüngsten veröffentlichten
Run und rekonstruieren dessen vollständiges typisiertes `PublishedIndex` in einer konsistenten
Read-Transaktion. Vor dem finalen Statuswechsel entfernt dieselbe Transaktion die regenerierbaren
Projektionszeilen älterer Runs in Batches von höchstens 1.024 Zeilen. Leser sehen bis zum Commit den
alten vollständigen Run und danach nur den neuen; Run-Metadaten und Snapshots bleiben historisch
erhalten. Ein Fehler oder Crash rollt auch die Retention zurück.

Knowledge-Schema V6 ergänzt denselben atomaren Sichtbarkeitswechsel um die versionierte
Lexical-Search-Projektion. Marker, Symbol-, Pfad- und derzeit leere Card-FTS-Zeilen werden vor dem
Statuswechsel geschrieben. Replacement-Publish und Rebuild entfernen FTS-Zeilen in denselben
begrenzten Batches wie Graph- und Exact-Projektionen; ein Leser kann deshalb nie Graph und FTS aus
verschiedenen Runs kombinieren.

Knowledge-Schema V8 nimmt Modulmarker, Module, Manifestbelege, primäre und zusätzliche
Memberships, Graph-Evidence sowie Zentral-/Entrypoint-/Test- und Repository-Card-Projektionen in
denselben Sichtbarkeitswechsel auf. Ein Read rekonstruiert die vollständige `ModuleProjection` und
prüft Snapshot, Counts, Symbolabdeckung, Manifestrevisionen und Graphbeziehungen erneut.

Index-Schema V5 und Knowledge-Schema V23 nehmen zusätzlich für jede exakte publizierte
`FileRevision` die Sprache, Adapterrevision, Parse-Coverage und kanonischen sicheren Diagnostics in
dieselbe Transaktion auf. Der Publish verlangt eine lückenlose Eins-zu-eins-Abdeckung der
Graphdateien; Reads validieren Hash, Parserzustand, Bereiche und Coverage erneut. Historische
Publikationen bis Index-Schema V4 bleiben als explizit generische Analyse lesbar. Dadurch ist ein
file-lokaler Parserfehler sichtbar, ohne den letzten vollständigen Index oder andere Dateien zu
verwerfen.
Replacement und Rebuild entfernen die neuen regenerierbaren Tabellen in expliziter FK-sicherer
Kind-vor-Eltern-Reihenfolge; tiefe Delete-Cascades gehören nicht zur V8-Löschsemantik.

Publish und vollständiges Lesen übernehmen Cancellation und Progress vom besitzenden Job, prüfen
spätestens nach 1.024 Zeilen erneut, emittieren höchstens 64 monotone Fortschrittswerte und brechen
nach fünf Minuten kontrolliert ab. Der Rebuild verweigert einen aktiven `building`-Run und entfernt
ausschließlich run-gebundene regenerierbare Indexzeilen in Batches von 4.096 Zeilen sowie anschließend
die IndexRun-Historie. Snapshots, Worktree-Identität und nicht regenerierbare Task-, Decision- oder
User-Evidence-Tabellen bleiben erhalten. Fehler, Cancellation, Timeout oder ein Crash vor dem finalen
Statuswechsel rollen die gesamte Mutation zurück; der zuvor veröffentlichte Index bleibt sichtbar.

### Progressiver Repository-Baum

U4 rekonstruiert den sichtbaren Repository-Baum nicht aus dem Live-Dateisystem und lädt dafür auch
keinen vollständigen `PublishedIndex`. Der schmale Application-Port liest aus den
`file_revisions` des jüngsten publizierten Runs je Anfrage höchstens 100 direkte Kinder. Eine
Read-Transaktion bindet Run, Snapshot, Einträge und exklusiven Cursor atomar; `limit + 1` macht eine
weitere Seite sichtbar, ohne sie auszuliefern. Root und Unterverzeichnisse werden aus denselben
relativen Pfadbytes gruppiert und strikt byteweise sortiert, sodass auch nicht als UTF-8 darstellbare
Namen verlustfrei navigierbar bleiben.

Ein Dateieintrag trägt die exakte aktuelle `FileRevision` über seinen ContentHash. Ein
Verzeichniseintrag ist ausschließlich ein aus mindestens einem aktuellen Nachfahren abgeleitetes
Präfix und trägt dessen exakte Dateizahl, aber keinen erfundenen Hash. Application- und
Frontendverträge lehnen indirekte Kinder, widersprüchliche Counts, falsche Cursor, nicht kanonische
Pfade und persistierte Korruption ab. Der libSQL-Read prüft Cancellation und besitzt ein festes
Zwei-Sekunden-Limit. Die Hexdarstellung an der IPC-Grenze ist ein opaker Indexschlüssel und gewährt
weder einen Source-Read noch Zugriff auf einen Betriebssystempfad.

### Progressiver Modulbaum

Der Modulbaum liest die bereits mit V8 atomar publizierten Tabellen `modules`, `module_projections`,
`module_members`, `module_manifests` sowie die drei begrenzten Featurelisten, ohne einen vollständigen
`PublishedIndex` zu rekonstruieren. Ein vorhandener `module_projections.module_count` markiert die
vollständige Projektion und muss exakt mit primären Manifest-/Pfadmodulen plus zusätzlichen
Graph-Communities übereinstimmen. Fehlt dieser Marker bei einem historischen publizierten Run, ist
die Projektion explizit nicht verfügbar; null aktuelle Module sind dagegen eine gültige leere
Projektion.

Primäre Module besitzen genau einen kanonischen Repository- oder Verzeichnisroot. Die Rootquery
liefert nur Module ohne primären Root-Vorfahren. Eine Childquery liefert nur Nachfahren des gewählten
primären Elternroots, zwischen denen kein weiterer primärer Root liegt. Gleiche primäre Roots,
zusätzliche statt primäre Membership-Kinds und widersprüchliche Manifestformen gelten als
persistierte Korruption. Graph-Communities werden niemals als Eltern oder Kinder interpretiert;
ihre exakte Gesamtzahl bleibt als Zusatzsignal erhalten.

Eine kurze Read-Transaktion bindet Run, Snapshot, Elternknoten, Counts, Boundary-Evidence, Seite und
Cursor atomar. Jede Seite enthält höchstens 100 nach `ModuleId` geordnete direkte Kinder; `limit + 1`
bestimmt, ob der letzte ausgelieferte Schlüssel als exklusiver Folgeseiten-Cursor erscheint. Pro
Knoten werden exakte Manifest-, distinct-File- und primäre Membership-Zahlen, die gespeicherten
bounded Central-/Entrypoint-/Test-Zahlen samt Trunkierungswahrheit sowie ein Child-Signal gelesen.
Eine vorhandene strukturelle Membership liefert eine repräsentative aktuelle `FileRevision`; eine
Manifestgrenze liefert zusätzlich ihre erste kanonische aktuelle Manifestrevision. Der Read ist
cancellable, auf zwei Sekunden begrenzt und öffnet weder Live-Dateisystem noch Source-Inhalt.

## Deep Map

Die Desktop-Grenze startet eine Deep Map ausschließlich nach der ausdrücklichen Aktion
`start_deep_map`. Vorher zeigt sie das live verifizierte Mapping-Profil mit Provider-, Modell- und
Profilidentität, Context- und Outputlimit sowie das gewählte Token-, Zeit- und Read-only-Toolbudget.
Die WebView wählt weder Projekt, Profil noch Job-ID. Ohne ein durch Capability Probe verifiziertes
Mapping-Profil bleibt die Funktion sichtbar `unavailable`; A^3 bleibt als Indexbrowser nutzbar und
startet weder Provider- noch GPU-Arbeit. Die spätere U8-Konfiguration liefert den optionalen
Executor an den Composition Root, ohne diesen U3-Startvertrag zu verändern.

Pause verwendet kooperative Scheduler-Cancellation. Nur ein vollständiger, plan- und
snapshotgebundener `ExplorerCheckpoint` darf den Core-eigenen Zustand `Paused` erzeugen. Resume
startet einen neuen besessenen Versuch ab dem ersten unbestätigten Schritt unter dem unveränderten
Startbudget; Cancel verwirft den Checkpoint. Statuspolling liest nur das begrenzte in-memory
Read-Model und löst keine Exploration, Storage-Rekonstruktion oder Modellausführung aus.

Phasen:

~~~text
SEED → EXPLORE → PROPOSE → VERIFY → PUBLISH
~~~

### Seeds

Der deterministische Planner wählt:

- Paket- und Anwendungseinstiegspunkte
- zentrale öffentliche Symbole
- Manifest- und Konfigurationsknoten
- Test-Roots
- stark gekoppelte oder ungewöhnliche Bereiche
- noch nicht beschriebene Module

Der implementierte V1-Planner läuft vollständig ohne Modell und akzeptiert konstruktiv nur einen
`PublishedIndex`. Sein `ModuleCoverageSnapshot` muss exakt dieselbe Snapshot- und
`ModuleCardSchemaVersion` tragen; unbekannte oder doppelte Module werden abgelehnt. Module mit
vollständiger Muss-Coverage werden auch dann übersprungen, wenn andere Module noch offen sind.

Manifestrevisionen, Entrypoints, zentrale Symbole, Test-Roots, Graphcommunities und offene Module
werden mit einer ganzzahligen, versionierten Kombination aus Seed-Priorität, Muss-/Soll-Coverage
und erwarteter Informationsbreite geordnet. Überlappende Symbolseeds werden vor dem Ranking
vereinigt. Ein deterministisches Top-K hält höchstens 16.384 Kandidaten; stabile Module-, Ziel- und
Reason-Schlüssel lösen Gleichstände auf.

Jeder `ExploreStep` enthält das erwartete Coverage-Ergebnis, die genaue aktuelle Modul-, Manifest-
oder Symbolevidenz, die spätere feldgenaue Verifikationsmethode, `Planned`-Status sowie vorab
reservierte Token-, Zeit- und Toolkosten. Die Summen werden checked addiert und nur aufgenommen,
wenn alle drei Dimensionen innerhalb des `ExploreBudget` bleiben. V1 verwendet als interaktiven
Standard 32.000 Tokens, 120 Sekunden und 64 Read-only-Toolaufrufe; globale Grenzen verhindern
unbegrenzte Rekonstruktionen.

Die versionierte Gate-M4/M5-Abnahme indiziert die repo-eigenen Produkt-Fixtures
`rust-adapter`, `typescript-monorepo` und `python-package` jeweils als eigenständiges Repository
über den vollständigen Snapshot-, Compiler- und atomaren libSQL-Publishpfad. Die aktuelle V1-
Golden umfasst 25 veröffentlichte Dateien, 98 Symbole, sechs primäre Manifestmodule und 20
budgetgedeckte Planschritte. Für jedes Symbol wird genau eine primäre, aktuelle Membership
nachgewiesen; jeder Schritt löst sein Modul sowie seine Manifest- oder Symbolevidenz gegen dieselbe
Publikation auf. Leere Coverage endet für alle drei Sprachen mit `CoveragePlanned`, und zwei Pläne
derselben Publikation sind identisch. Der Contract läuft ohne Modell, Embeddings und Netzwerk mit
`cargo test -p a3-repo-index --test deep_map_fixture_acceptance --locked`.

### Exploration

Das LLM darf nur über typisierte Read-only-Werkzeuge explorieren. Jeder nächste Leseschritt benötigt einen erwarteten Informationsgewinn. Vollständiges rekursives Lesen ist verboten.

Eine Exploration endet, wenn zuerst eine Bedingung erfüllt ist:

- konfiguriertes Token-, Zeit- oder Toolbudget;
- alle Muss-Coverage-Ziele erreicht;
- drei aufeinanderfolgende Expansionen liefern keinen neuen hoch bewerteten Knoten;
- verbleibende Bereiche liegen unter dem Relevanzschwellwert;
- Benutzerabbruch.

`ExplorationStopPolicy::v1` bildet diese Fälle typisiert als Cancellation, Budget Exhaustion,
erreichte Muss-Coverage, drei aufeinanderfolgende Expansionen unterhalb des Nutzens und
verbleibenden Informationsgewinn unter 100 Basispunkten ab. Der statische Plan weist getrennt aus,
ob Coverage vollständig eingeplant, das Budget erschöpft, der Gain-Schwellwert unterschritten oder
kein geeigneter Seed vorhanden ist.

### Module Card

~~~text
ModuleCard
  id
  snapshot_id
  title
  paths
  purpose
  responsibilities
  public_surface
  entrypoints
  dependencies
  data_flows
  invariants
  tests
  risks
  open_questions
  evidence_ids
  confidence
  mapper_profile_version
  status
~~~

Jedes nicht leere fachliche Feld muss seine Evidence IDs behalten. Ein Summary-Feld ohne Quellen wird verworfen.

Das implementierte Schema V1 begrenzt das gesamte strukturierte Dokument vor Validierung auf 64
KiB, die vereinigte Evidenzmenge auf 512 IDs und alle zwölf Fachfelder zusätzlich separat nach
Itemzahl und UTF-8-Bytes. `ModuleCardId`, `ModuleId`, `SnapshotId`, Confidence,
`MapperProfileVersion` und Status sind verpflichtende Envelope-Daten. Der Planner erzeugt noch
keine Card-Inhalte; strukturierte Explorer-Ausgabe, Repair und Proposalbildung beginnen erst mit
R8, deterministische Claim-Verifikation und Publish erst mit R9.

### Read-only Explorer V1

Der R8-Explorer konsumiert ausschließlich einen unveränderlichen `ExplorePlan`. Sein eingebettetes
JSON Schema `deep-map-explorer-action-v1` erlaubt nur Inspect, Search und Propose, setzt auf jeder
Objektebene `additionalProperties: false` und kennt weder Schreib- noch Execute-Aktionen. Der
Runtime-Decoder ist zusätzlich bytegenau: Er akzeptiert genau ein JSON-Dokument bis 64 KiB,
kanonische 256-Bit-IDs in Lowercase-Hex, passende Querytypen, alle feldspezifischen Card-Grenzen und
keinen Text außerhalb des Dokuments. `serde_json` wird dafür als bereits workspaceweit exakt
gepinntes Parserfundament nun direkt in `a3-application` verwendet; die Standardbibliothek besitzt
keinen JSON-Parser, und es wurde keine neue externe Version in den Lockfile-Graph aufgenommen.

`ExplorerModelProvider` ist der für R8 vorgezogene neutrale Stub-Port der Providergrundlage. Er
erhält die Schemafassung, das statische JSON Schema, Run, Snapshot, aktuellen Schritt, erwartete
Felder und höchstens ein normalisiertes Werkzeugresultat. Er kennt keine Ollama-Payload, keinen
Endpoint und keine Credentials. Der vollständige allgemeine `ModelProvider`, das versionierte
`ModelProfile`, Streaming, Capability Probe, Endpoint Policy und Ollama-Adapter wurden später in
H4 und H5 ergänzt; der R8-Port bleibt bewusst die engere read-only Capability.

Die Capability `DeepMapReadTools` besitzt konstruktiv nur `inspect` und `search`. Ein Planschritt
darf genau den bereits reservierten einen Werkzeugaufruf ausführen. Inspect verwendet ausschließlich
das planbestimmte Ziel; Search ist auf Exact, Lexical und typisierte Graphpresets mit höchstens 100
Treffern begrenzt. Adapter müssen jedes Ergebnis auf einen kontrollierten Preview von höchstens 16
KiB und höchstens 100 kanonische Evidence IDs normalisieren. Es existiert an diesem Port keine
Methode zum Schreiben, Ausführen, Starten eines Prozesses oder Ändern von Git.

Jede Modellausgabe wird vollständig dekodiert und gegen aktuellen Schritt, Gain-Schwelle,
Werkzeugbudget und vorhandene Observation autorisiert, bevor ein Read stattfinden kann. Ein
Proposal muss alle vom Schritt erwarteten Felder enthalten; sämtliche Feld-Evidence-IDs müssen aus
dem tatsächlich zurückgegebenen aktuellen Werkzeugresultat stammen. Über den gesamten R8-Aufruf
ist höchstens eine Repair-Anfrage mit einer inhaltsfreien Fehlerklasse zulässig. Das ungültige
Original wird nie ausgeführt; auch eine ungültige Reparatur beendet den Lauf typisiert.

Cancellation wird vor und nach jeder Provider- und Werkzeuggrenze geprüft. Das Ergebnis enthält
auch bei Abbruch den run-, snapshot-, schema- und policygebundenen `ExplorerCheckpoint`. Darin
stehen ausschließlich lückenlos bestätigte Schritte. Resume startet anhand seiner Länge beim
ersten unbestätigten Schritt und wiederholt deshalb keine bestätigte Exploration. Vorschläge werden
in R8 weder persistiert noch veröffentlicht; Evidence-Auflösung, Claimstatus und Publish bleiben
R9 vorbehalten.

### Verify

Der Verifier prüft:

- existieren alle referenzierten Symbole und File Revisions?
- passen behauptete Imports, Exports, Tests und Aufrufkanten zum Graph?
- sind Pfade im Snapshot gültig?
- widersprechen sich Cards?
- welche Aussagen sind nicht deterministisch prüfbar?

Prüfbare Aussagen werden Fact oder Observation. Unprüfbare Aussagen bleiben Hypothesis. Widersprüche werden nicht durch Mehrheitsentscheid des LLM aufgelöst.

R9 stellt jeden Feldwert als separaten, typisierten Claim dar. Path-, Symbol- und Relationclaims
werden nicht aus dem Beschreibungstext erraten, sondern referenzieren kanonische Evidence IDs für
eine File Revision, ein strukturelles Symbol oder eine vollständige Graphkante. Der read-only
Resolver lädt den letzten atomar publizierten Index, verlangt exakt passende Run- und Snapshot-IDs
und gibt ausschließlich die angeforderten Evidence-Objekte zurück. Der model-freie Verifier prüft
die Objekte danach erneut gegen denselben `PublishedIndex`.

Das strukturierte Dokument `module-card-claims-v1` ist auf 64 KiB begrenzt, bindet sich explizit an
Card, Modul und Snapshot und verwendet auf jeder Objektebene `additionalProperties: false`. Seine
geschlossene Prädikat-Union kennt nur Path, Symbol, Relation, Observation und
ArchitecturalIntent; Endpunkte und Relationstypen sind ebenfalls typisiert. Der unabhängige
Runtime-Decoder akzeptiert genau ein JSON-Dokument, kanonische Lowercase-Hex-IDs, begrenzte Pfade,
Statements, Confidence und Evidence-Mengen und erzeugt nur bei vollständiger Feldwertabdeckung
einen `ModuleCardVerificationCandidate`. Das Schema enthält keine Tool- oder Execute-Capability.

Affirmative Pfad-, Symbol-, Import-, Export-, Call- und Testclaims werden nur bei exaktem Treffer
zu Fact. Direkt evidenzgebundene, aber nicht strukturell beweisbare Beschreibung wird Observation;
Architekturabsicht und negative Abwesenheitsclaims bleiben deutlich markierte Hypothesis.
Confidence und Claimstatus sind getrennte Werte. Opponierende strukturierte Claims stoppen den
Batch mit einem sichtbaren Widerspruchsbericht. Der Publish-Port akzeptiert konstruktiv nur den
durch diese Prüfung erzeugten `VerifiedModuleCardBatch`, niemals Proposal- oder Rohmodelltypen.

Der libSQL-Adapter publiziert diesen Batch in einer `IMMEDIATE`-Transaktion des Knowledge-Schemas
V9. Cards, Feldwerte, Field Evidence, Claims, strukturierte Prädikate, Claim-Evidence und die
vollständige aufgelöste Provenienz werden gemeinsam committed; ein SQL-Fehler lässt keine
Teil-Card sichtbar. Gleichzeitig erhält `card_fts` genau eine Zeile pro Card und der
Lexical-Projektionsmarker wird auf dieselbe Anzahl gesetzt. Der Publisher lehnt stale Run- oder
Snapshotgrenzen und einen zweiten Publish desselben Runs ab, prüft Cancellation sowie eine
30-Sekunden-Deadline und sendet höchstens 64 monotone Progressereignisse. Dauerhafte Claims und
Evidence überleben einen Rebuild des regenerierbaren Fast Index; ihre Invalidierung folgt in R11.

## Task Lens

Seeds:

- Goal Contract und aktueller Schritt
- explizite Pfade, Symbole und Identifier der Benutzeranfrage
- Compiler-, Test- und Laufzeitfehler
- zuletzt geänderte Dateien
- offene Hypothesen und fehlgeschlagene Verifikationen

Expansion:

1. exakte Pfad- und Symboltreffer;
2. lexikalische FTS-Treffer;
3. direkte Graphnachbarn;
4. zugehörige Tests, Konfiguration und öffentliche Schnittstellen;
5. evidenzgebundene Claims;
6. semantische Kandidaten als Ergänzung;
7. höchstens ein weiterer Graph-Hop für hoch bewertete Knoten.

Ergebnis ist ein kleiner, priorisierter Subgraph mit Module Cards und Quellenausschnitten. Die Task Lens ist temporär, aber ihre Policy-Version und Seed-Menge werden für Reproduzierbarkeit gespeichert.

Die implementierte infra-freie R10-Domainvorstufe `TaskLensPolicy::v1` bindet die kanonische
Seedmenge und eine R4-Fusionsausgabe an genau einen `PublishedIndex`. Sie selektiert grob nach
konkret über L0 Repository Card, L1 Modul, L2 Symbol und L3 File beziehungsweise Declaration Span.
Ein konfigurierbares Budget von 256 bis 32.768 Tokens, höchstens acht Module und 64 Einträge
begrenzen die Lens; die V1-Schätzung verwendet strukturellen Overhead plus einen konservativen
Byte-Counter. Zu große Details und vorgelagert trunkierte Kandidaten bleiben sichtbar.

Persistierte Claim-Projektionen werden vor Aufnahme nochmals gegen Run, Snapshot, Modul,
Classification und die vollständige aktuelle File-, Symbol- oder Graphkanten-Evidence geprüft.
Nicht aktuelle Claims werden gezählt und vollständig aus dem Faktenanteil ausgeschlossen. Der
domänenseparierte `LensDigest` umfasst Policy, Fusionpolicy, Budget, Seeds, Publikation, geordnete
Auswahl, aktuelle Claims und Trunkierungszustand; ein Indexdelta erzwingt deshalb eine neue Lens.

`CompileTaskLens` führt die read-only Kanäle fest als Exact, Lexical, Graph/Test, Claims und optional
Semantic aus. V1 begrenzt unter anderem 16 Exact Queries, 32 Lexical Tokens, vier Graphseeds,
100 Kandidaten je Kanal, 32 Fusionstreffer, 128 Claims und 20 semantische Kandidaten. Acht feste
Fortschrittszustände, kooperative Cancellation und eine standardmäßige 30-Sekunden-Gesamtdeadline
gelten einschließlich aller Adapteraufrufe. Semantic ist eine optionale Capability; ohne sie bleibt
die vollständige deterministische Lens verfügbar. Similarity erzeugt ausschließlich Kandidaten und
wird weder Evidence noch Fact.

Der libSQL-Claim-Adapter liest nur Claims des exakt übergebenen aktuellen Runs in Claim-ID-Reihenfolge
und macht eine Begrenzung als `truncated` sichtbar. Persistierte Evidence-Zeilen sind dabei keine
Autorität: Evidence IDs werden gegen die vollständigen typisierten Objekte des unveränderlichen
`PublishedIndex` aufgelöst und danach erneut durch die Domain geprüft. Vor und nach einem
Indexaustausch validiert der Adapter den neuesten publizierten Run in einer konsistenten
Read-Transaktion; eine alte Indexcapability wird abgelehnt. Ein auf einen Eintrag begrenzter,
identitätsgebundener Shared-Index-Cache vermeidet pro Lens eine tiefe Kopie von zehntausenden
Symbolen. Jeder Zugriff vergleicht zuvor den aktuellen dauerhaften Run; Publish ersetzt und Rebuild
entfernt den Eintrag.

## Inkrementelle Aktualisierung

Der implementierte V1-Watcher verwendet keine neue Plattform- oder Netzwerkabhängigkeit. Ein
besitzender Rust-Thread beobachtet über isolierte Git-Discovery den tracked/untracked Kandidatensatz,
HEAD, Index-Checksum und plattformspezifische, aber gekapselte Dateimetadaten. Er pollt alle 100 ms,
wartet 200 ms Ruhe, schließt einen dauernden Burst spätestens nach 750 ms und hält höchstens einen
fertigen Batch. Pfade werden byteweise sortiert und dedupliziert. Initialisierung, Queue-Überlauf,
HEAD-/Indexwechsel und unvollständige Beobachtung tragen einen typisierten Full-Rescan-Grund.

Der Application-Use-Case behandelt Pfadhinweise niemals als Fakten. Git-Discovery bestätigt den
aktuellen relevanten Pfadsatz, BLAKE3 bestätigt neue und gemeldete Inhalte; ein Full-Rescan hasht alle
relevanten Dateien. Ein exakter Cache des Parent-Snapshots behält unveränderte
`LanguageParseResult`s, entfernt gelöschte Pfade und parst nur geänderte oder neue von Rust,
TypeScript/JavaScript oder Python unterstützte Dateien. Fehlt der Cache nach einem Neustart, wird er
einmal begrenzt aus dem aktuellen Snapshot aufgewärmt, ohne einen bereits identisch veröffentlichten
Run zu duplizieren. Erst Link, Rank und atomisches Publish aktualisieren die sichtbare Sicht.

Der Desktop-Koordinator serialisiert diese Refreshes pro aktivem Worktree über die bestehende
begrenzte Jobqueue. Backpressure wird in einen Full Rescan umgewandelt; Cancellation und Fortschritt
laufen über den Job-Kontext. Projektwechsel und Shutdown beenden den Watcher, canceln einen aktiven
Job kooperativ und joinen alle besitzenden Threads.

Für U3 meldet derselbe Application-Control-Vertrag die sechs ADR-0006-Phasen als monotone
`0/6` bis `6/6`-Grenzen. Snapshot-Discovery und Hashing sowie Compiler-Parse, Link und Rank werden
am jeweiligen inneren Port auf die end-to-end Phasen abgebildet; rohe untergeordnete Totals werden
nicht in denselben Schedulerjob gemischt. Der Desktop stellt die aktuelle Phase über eine
leichtgewichtige, pfadlose V1-Statusabfrage dar. Vor dem atomaren Publish ersetzen laufende,
fehlgeschlagene oder abgebrochene Builds den letzten publizierten Index nicht, sodass dessen
Snapshot parallel weiter lesbar bleibt.

Die getrennte U3-Indexübersicht rekonstruiert nur auf ausdrückliche UI-Aktualisierung den letzten
`PublishedIndex`. Ein Application-Read-Model berechnet file- und symbolgenaue Zähler sowie
bytegewichtete Parse-Coverage, behält den publizierten Snapshotanker und begrenzt die Fehleransicht
auf 64 kanonische Dateien mit je acht sicheren Diagnostics. Pfade sind ausschließlich sanitierte,
maximal 512 Zeichen lange Anzeigeprojektionen; Source-Inhalt, Hashes und autoritative
Dateisystempfade überschreiten die IPC-Grenze nicht. Ein Dateifehler erscheint damit lokal, ohne
andere Indexdaten in einen globalen Blank State zu verwandeln.

Die separate U3-Freshness-Projektion liest innerhalb einer auf zwei Sekunden begrenzten
Deferred-Transaktion die Lebenszyklen der jeweils neuesten veröffentlichten Card jedes Moduls. Sie
liefert exakte Zähler für `Current`, `Stale`, `NeedsReview` und Gesamt sowie höchstens fünf positive,
kanonisch geordnete Ursachen. Die Projektion wird gemeinsam mit aktuellem `IndexRunId` und
`SnapshotId` zurückgegeben. `module-removed` bleibt damit sichtbar, obwohl dieser Zustand bewusst
keinen Remapqueue-Eintrag erzeugt; nach Veröffentlichung einer neu verifizierten Card wird deren
ältere stale Version nicht mehr als aktueller Modulzustand gezählt. Card-Inhalte, Claims, Evidence,
Source und Pfade sind nicht Teil dieser Desktop-Projektion.

~~~mermaid
flowchart TD
    Change["Bestätigtes Change Set"]
    Delta["Parse- und Graph-Delta"]
    Invalidate["Evidenz invalidieren"]
    Map["Betroffene Cards neu planen"]
    Lens["Task Lens aktualisieren"]

    Change --> Delta
    Delta --> Invalidate
    Invalidate --> Map
    Map --> Lens
~~~

Invalidierungsradius:

- geänderte File Revision;
- darin definierte oder entfernte Symbole;
- ein- und ausgehende Kanten;
- Claims mit direkter Evidenz;
- Cards des eigenen Moduls;
- Cards direkter abhängiger Module als NeedsReview;
- abgeschlossene Task-Schritte mit betroffener Verification Evidence.

R11 implementiert diesen Radius beim atomischen Index-Publish. Der libSQL-Adapter wählt pro Modul
genau die neueste Card desselben Worktrees, prüft ihre gespeicherten File-, Symbol- und
Graph-Evidence gegen den neuen vollständigen Run und behandelt inkompatible Snapshot-
Adapterrevisionen oder Mapperprofile als eigene Invalidierungsgründe. Direkte Treffer werden
`Stale`; nur ein Hop direkter Graphabhängiger wird `NeedsReview`. Unabhängige Cards bleiben
`Published` und ihre weiterhin aktuelle Evidence darf über Run-Grenzen wiederverwendet werden.

Die dauerhafte Remapqueue ist nach `Direct` vor `Dependent` und danach nach `ModuleId` geordnet.
Jeder Eintrag ist an den aktuellen Ziel-Run und -Snapshot gebunden; ein begrenzter read-only
Application-Port liefert höchstens 256 Einträge mit sichtbarer Trunkierung, Cancellation und
Zwei-Sekunden-Deadline. Eine neu verifizierte Card entfernt ihr Modul im selben Commit aus der
Queue. Task-Lens-Claim-Reads wählen nur die neueste `Published`-Card mit `Active`-Claim-Lifecycle,
lösen deren Evidence erneut gegen den aktuellen Run auf und liefern dadurch keine stale Facts.

## Hybride Suche

### Implementierter Exact-Kanal

Index-Schema V2 materialisiert während desselben atomaren Publishes einen versionierten
Exact-Search-Marker, discovery-bestätigte Manifestrevisionen und genau einen qualifizierten Namen pro
Symbol. Qualifizierte Namen werden ausschließlich aus eindeutigen, azyklischen
`Contains`-Beziehungen gebildet. Mehrfach-Eltern, Zyklen, fehlende Symbole oder ein Überschreiten der
16-KiB-Textgrenze brechen den Publish typisiert ab. Entrypoint und Test stammen aus den
adapterbelegten `SymbolRoles`; Manifest wird aus der bereits begrenzten Discovery-Rolle übernommen.

Der read-only Application-Port `KnowledgeSearchStore` liefert Pfad-, Name-, Signatur- und
Rollentreffer über eine Deferred-Read-Transaktion. Identifier werden case-sensitiv zuerst nach
qualifiziertem Namen, dann Simple Name und Signatur exakt sowie anschließend in derselben Reihenfolge
als Präfix verglichen. SQL-Parameter bleiben Daten; der Präfixbereich verwendet den nächsten gültigen
Unicode-Skalargrenzwert. Resultate sind über Matchklasse, verlustlose Pfadbytes, qualifizierten Namen
und `SymbolId` stabil sortiert und per snapshotgebundenem Keyset-Cursor auf höchstens 100 Treffer pro
Seite begrenzt. Jede Seite prüft Projektionsversion und Zeilenzahlen. Cancellation und ein fixes
Zwei-Sekunden-Read-Limit werden vor und zwischen Ergebniszeilen geprüft. Der Adapter hält höchstens
vier bereits vollständig verifizierte, identitätsgebundene Projektdatenbanken offen; rohe Handles
verlassen ihn nicht.

### Implementierter Lexical-Kanal

Index-Schema V3 materialisiert `symbol_fts` für Pfad, qualifizierten Namen, einfachen Namen und
Signatur sowie `path_fts` für jede aktuelle File Revision. `card_fts` und sein Zähler sind bereits
versioniert vorhanden, bleiben aber bis zur evidenzgebundenen Card-Erzeugung in R5 leer. Nicht-UTF-8-
Pfade behalten im Ergebnis ihre Originalbytes und erhalten nur für FTS eine deterministische
Prozentkodierung.

Eine validierte Query wird nicht als FTS-Syntax interpretiert. Der Adapter extrahiert aus höchstens
4 KiB Text maximal 32 alphanumerische beziehungsweise `_`-Tokens, erzeugt daraus gequotete Trigramme und bindet
den Ausdruck als Parameter. Für längere Tokens begrenzen höchstens acht verteilte Trigramme mit
einer begrenzten Ein-Fehler-Klausel den Kandidatenraum; pro Symbol- und Pfadklasse werden höchstens 512
`bm25`-Kandidaten vollständig dekodiert. Die finale ganzzahlige Gewichtung lautet Symbolname 10,
qualifizierter Name 8, Signatur 6 und Pfad 4. Treffer unter der festen Mindestschwelle entfallen.

Die Sortierung verwendet absteigenden Score, Zielklasse, verlustlose Pfadbytes, qualifizierten Namen
und `SymbolId`. Der Cursor bindet den vollständigen Schlüssel an Query, Run und Snapshot.
Projektionsmarker und tatsächliche Zeilenzahlen werden vor jeder Suche verglichen; Cancellation und
das Zwei-Sekunden-Limit werden vor und zwischen Zeilen geprüft.

### Implementierter Graph-Kanal

Der read-only `KnowledgeSearchStore` traversiert ausschließlich `symbol_edges` des jüngsten
atomar veröffentlichten Runs. Eine typisierte Query legt Seed, Richtung, genau eine Relation,
maximal ein oder zwei Hops sowie höchstens 100 Resultate fest. Für Callers, Callees, Imports,
Exports und Tests existieren eindeutige Presets; Testkanten bleiben durch
`SourceChannel::Test` von allgemeinen Graphkandidaten getrennt.

Der Adapter führt eine levelweise Breitensuche in kanonischer `edge_sequence` aus, merkt besuchte
Datei- und Symbolendpunkte und behält deshalb pro Ziel den ersten kürzesten Pfad. Ein Zyklus kann
weder den Seed erneut ausliefern noch die Suche verlängern. Pro Query werden höchstens 4.096 Kanten
unter kooperativer Cancellation und einem festen Zwei-Sekunden-Limit dekodiert; Result- oder
Kantenbegrenzung wird als `truncated` sichtbar. Seed, Ziele, qualifizierte Symbolnamen, Kanten und
Evidenz stammen aus derselben konsistenten Deferred-Read-Transaktion. Jeder Treffer gibt seinen
vollständigen `GraphEdge`-Pfad als maschinenlesbare Begründung zurück.

### Implementierte Retrieval Fusion

Exact, Lexical, Graph, Tests sowie künftig Memory und Semantic gelangen als getrennte, auf denselben
Run und Snapshot geprüfte `RetrievalCandidateSet`s in die Fusion. Höchstens sechs Kanäle mit je 100
Zielen halten die Vorstufe begrenzt. Dateien verwenden innerhalb des veröffentlichten Runs den
verlustfreien Pfad und Symbole ihre `SymbolId` als Deduplizierungsschlüssel. Mehrere Kanäle für
dasselbe Ziel werden zusammengeführt; widersprüchliche Revisionen, Symbolprojektionen oder
Zielsignale werden nicht still ausgewählt, sondern abgelehnt. Exact-/Lexical-Cursor und
Graphtrunkierung werden als typisierter Mengenstatus in das Fusionsergebnis fortgepflanzt.
Memory gelangt nur mit mindestens einer und höchstens 16 zuvor als fresh aufgelösten
`EvidenceRef`s in das Evidence-Band; Semantic Similarity bleibt auch bei Maximalscore nicht
beweisend.

Policy V1 normalisiert die bestehenden nativen Scores, Graphdistanzen und Confidencewerte und
wendet vor jedem Score die Bänder Exact → Evidence → Semantic an. Innerhalb eines Bands gewichtet
sie Kanal 30.000, Goal 20.000, Step 20.000, Freshness 10.000, inverse Tokenkosten 10.000 und
nicht-semantische Mehrkanalbestätigung 10.000 Punkte; Redundanz kann bis zu 20.000 Punkte abziehen.
Die Berechnung ist ganzzahlig, auf 100.000 Punkte begrenzt und verwendet die stabile Ziel-ID als
letzten Tie-Breaker. Jeder Treffer gibt Quellgründe, normalisierte Eingaben, Einzelbeiträge,
Redundanzabzug und Endscore zurück. Der Ergebniscontainer behält Policyversion, Run und Snapshot.

Der versionierte Golden-Eval-Runner führt dieselben Fixtures zweimal aus und fixiert Deduplizierung,
Graph- und Testkanal, Goal-/Step-/Freshness-/Token-/Redundanzbeiträge, Stable Tie-Breaking,
Resultlimit sowie den Vorrang eines schwach gewichteten Exact-Treffers vor einem maximal
gewichteten Semantic-Treffer. Dies ist die isolierte R4-Policy-Golden.

Die separate Gate-M4/M5-Evalbaseline indiziert das repo-eigene, gemischte
`fixtures/graph-linker` über den vollständigen Snapshot-, Compiler- und atomaren Publishpfad und
fragt danach die echten libSQL-Ports ab. Schema V1 fixiert in einer reviewbaren Golden-Datei sechs
Fälle mit sieben erwarteten Zielen: exakte Rust- und Python-Symbole, typo-tolerantes Symbol,
Manifestarchitektur sowie Rust- und TypeScript-Importbeziehungen. Jeder Treffer muss aus dem erwarteten Kanal stammen,
eine native Begründung tragen und an den aktuellen Run, Snapshot und eine aktuelle Dateirevision
gebunden sein. Zwei Abfragen derselben Publikation müssen bytegleich normalisieren. Der feste
Top-5-Grenzwert liegt derzeit bei Recall 10.000 Basispunkten und MRR 9.285 Basispunkten. Die
Baseline verwendet weder Modell noch Netzwerk oder Embeddings und läuft mit
`cargo test -p a3-repo-index --test retrieval_eval_baseline --locked`.

Die darauf aufbauende No-Embeddings-Abnahme des aktuellen M4/M5-Anwendungskerns verwendet
denselben echten Snapshot-, Compiler-, Publish- und libSQL-Pfad. Sie erzeugt ohne Semantic-Port
zweimal eine identische, vollständig budgetgedeckte Deep-Map-Planung und Task Lens, bindet beide an
die aktuelle Publikation und weist Exact- und Graphquellen bei null semantischen Quellen nach. Ein
nicht leerer
snapshotgebundener Semantic-Card-Batch wird parallel durch den konstruktiv provider- und
cachelosen deaktivierten Use Case vollständig übersprungen. Dieser Vertrag zieht weder den
allgemeinen Agent Harness noch die Desktop-Produktoberfläche aus M6 beziehungsweise M8 vor und
läuft mit `cargo test -p a3-repo-index --test no_embeddings_app_acceptance --locked`.

Vektoren werden ausschließlich für Semantic Cards und ausgewählte Symbolbeschreibungen erzeugt. Standardmäßig werden nicht beliebige überlappende Zeilenchunks eingebettet.

### R5 Semantic-, Provider- und Cachevertrag

R5 normalisiert bounded Semantic Cards deterministisch und verwendet
ihren `BodyHash` zusammen mit `SemanticCardId` und der aus allen vektorformenden Profilfeldern
abgeleiteten `ModelProfileId` als einzig zulässigen Cacheschlüssel. Modellnamen steuern keine
Capability; Dimension, Float32-Datentyp, Quantisierung und L2-Normalisierung sind explizit
validierte Profilfelder. Provideroutput wird begrenzt, dimensionsgeprüft und normalisiert, bevor
der Storage-Port ihn sehen kann.

Embeddings sind konstruktiv optional: Der deaktivierte Use Case besitzt keine Provider- oder
Cacheobjekte. Der aktivierte lokale Batchjob arbeitet snapshotgebunden, cancellable, mit
monotonem Progress und einem expliziten Requesttimeout. `VectorHit` enthält Similarity und
Cacheprovenienz, aber keine Evidence.

Knowledge-Schema V7 persistiert kanonische Cards, Snapshotzuordnungen, vollständige
vektorformende Profilmetadaten und normalisierte Float32-Vektoren. Cachehits sind nur bei exakter
Card-/Body-/Profilidentität zulässig und werden beim Lesen erneut validiert. Provider-, Modell- und
Dimensionswechsel können deshalb keine Vektoren vermischen.

Der libSQL-Adapter prüft die native DiskANN-Capability dimensionsspezifisch in einer isolierten,
kurzlebigen In-Memory-Projektion. Native und lineare Suche lesen denselben stabil sortierten,
snapshot- und profilgebundenen Korridor von höchstens 4.096 Karten. Die native Erweiterung ist nur
Kandidatengenerator; Similarity, Stable Tie-Breaking, Resultlimit und sichtbare Trunkierung bleiben
im Adapter deterministisch. Ohne Vector-Erweiterung arbeitet der lineare Fallback, während Exact,
FTS und Graph unverändert bleiben. Der gemeinsame Storage-Contract prüft Cache-Reopen,
Bodyrevisionen über Snapshots, Profilisolation, Cancellation und einen semantikexklusiven Rebuild.

Jeder Treffer erklärt seine Herkunft:

- exact identifier
- lexical
- graph relation
- test relation
- task memory
- semantic similarity

## Qualitätsmetriken

- Parse Coverage pro Sprache
- Symbol- und Edge-Anzahl
- Anteil ungeklärter Kanten
- Deep-Map-Coverage pro Modul
- Claims mit gültiger Evidenz
- stale Claims und Cards
- Retrieval Recall auf Eval-Aufgaben
- Indexlatenz cold und incremental
- DB-Größe pro 100.000 LOC
