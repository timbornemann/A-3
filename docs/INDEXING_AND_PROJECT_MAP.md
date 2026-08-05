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

Publish und vollständiges Lesen übernehmen Cancellation und Progress vom besitzenden Job, prüfen
spätestens nach 1.024 Zeilen erneut, emittieren höchstens 64 monotone Fortschrittswerte und brechen
nach fünf Minuten kontrolliert ab. Der Rebuild verweigert einen aktiven `building`-Run und entfernt
ausschließlich run-gebundene regenerierbare Indexzeilen in Batches von 4.096 Zeilen sowie anschließend
die IndexRun-Historie. Snapshots, Worktree-Identität und nicht regenerierbare Task-, Decision- oder
User-Evidence-Tabellen bleiben erhalten. Fehler, Cancellation, Timeout oder ein Crash vor dem finalen
Statuswechsel rollen die gesamte Mutation zurück; der zuvor veröffentlichte Index bleibt sichtbar.

## Deep Map

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

### Exploration

Das LLM darf nur über typisierte Read-only-Werkzeuge explorieren. Jeder nächste Leseschritt benötigt einen erwarteten Informationsgewinn. Vollständiges rekursives Lesen ist verboten.

Eine Exploration endet, wenn zuerst eine Bedingung erfüllt ist:

- konfiguriertes Token-, Zeit- oder Toolbudget;
- alle Muss-Coverage-Ziele erreicht;
- drei aufeinanderfolgende Expansionen liefern keinen neuen hoch bewerteten Knoten;
- verbleibende Bereiche liegen unter dem Relevanzschwellwert;
- Benutzerabbruch.

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

### Verify

Der Verifier prüft:

- existieren alle referenzierten Symbole und File Revisions?
- passen behauptete Imports, Exports, Tests und Aufrufkanten zum Graph?
- sind Pfade im Snapshot gültig?
- widersprechen sich Cards?
- welche Aussagen sind nicht deterministisch prüfbar?

Prüfbare Aussagen werden Fact oder Observation. Unprüfbare Aussagen bleiben Hypothesis. Widersprüche werden nicht durch Mehrheitsentscheid des LLM aufgelöst.

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
gewichteten Semantic-Treffer. Dies ist die R4-Policy-Golden, noch nicht die breitere Retrieval-
Evalbaseline aus Gate M4/M5.

Vektoren werden ausschließlich für Semantic Cards und ausgewählte Symbolbeschreibungen erzeugt. Standardmäßig werden nicht beliebige überlappende Zeilenchunks eingebettet.

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
