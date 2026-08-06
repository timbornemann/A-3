# Plan 03: Retrieval, Deep Map und Task Lens

Ziel: A^3 findet relevante Codebereiche präzise und erstellt eine evidenzgebundene, inkrementell aktualisierbare Projektkarte.

Relevante ADRs: 0006, 0007, 0008, 0009

## R1 Exact Search

Abhängigkeiten: Gate M3

Status: Completed

- [x] Suche nach normalisiertem Pfad
- [x] exakter und präfixbasierter Symbolname
- [x] qualifizierter Name und Signatur
- [x] Manifest, Einstiegspunkt und Test
- [x] paginierte, stabil sortierte Resultate
- [x] SourceChannel und Ergebnisbegründung

Verifizierter Abschluss vom 2026-08-05: Der infra-freie Domänenkern begrenzt Querytext und
Seitengröße, bindet jeden Treffer an `SourceChannel::Exact`, eine typisierte Erklärung und die
aktuelle `FileRevision` und macht Cursor über Query, `IndexRunId`, `SnapshotId` und den letzten
vollständigen Sortierschlüssel stale-sicher. Der Application-Port `KnowledgeSearchStore` bleibt
read-only, cancellable und frei von SQL- oder libSQL-Typen.

Index-Schema V2 übernimmt Manifestrollen aus der bestätigten Discovery und erzeugt qualifizierte
Namen ausschließlich aus eindeutigen, azyklischen `Contains`-Kanten. Knowledge-Schema V5 speichert
Marker, erwartete Zeilenzahlen, qualifizierte Namen und Manifestrevisionen atomar mit File
Revisions, Graph und Ranking. Exakte qualifizierte Namen, Simple Names und Signaturen stehen in
dieser Reihenfolge vor ihren Präfixtreffern; rohe Pfadbytes, qualifizierter Name und `SymbolId`
bilden danach eine stabile Keyset-Reihenfolge. Entrypoint und Test verwenden belegte Symbolrollen.

Der gemeinsame Storage-Vertrag belegt Reopen, identische Wiederholung, Mehrseitenabfrage ohne
Duplikate, Path/Manifest/Entrypoint/Test/Signatur, SQL-Parameterbindung, Cancellation, aktuelle
Revisionen und die Ablehnung eines Cursors nach Replacement-Publish. Migrations-, Zyklen- und
Mehrfach-Eltern-Regressionstests sind grün. Die reproduzierbare Release-Messung mit 50.000 Symbolen
aus 100.000 strukturellen Zeilen erreichte über 30 Exact-Query-Samples P50 37,0 ms und P95 39,7 ms;
der vorher notwendige vollständige Index-Load plus Scan lag über fünf Samples bei P50 652,8 ms und
P95 656,8 ms.

Akzeptanz:

- exakter qualifizierter Name wird vor unscharfen Treffern geliefert;
- gleiche Query und Snapshot ergeben gleiche Reihenfolge;
- Ergebnisse verweisen auf aktuelle File Revisions.

## R2 FTS

Abhängigkeiten: R1

Status: Completed

- [x] FTS-Schema für Namen, Signaturen, Pfade und Cards
- [x] gewichtete Felder
- [x] transaktionale Aktualisierung mit Index Publish
- [x] Queryescaping und Limits
- [x] Delete- und Rebuildtests

Verifizierter Abschluss vom 2026-08-05: Index-Schema V3 und Knowledge-Schema V6 materialisieren
eine versionierte FTS5-Trigram-Projektion für einfachen und qualifizierten Symbolnamen, Signatur und
Pfad im selben atomaren Publish wie File Revisions, Graph, Ranking und Exact Search. Das
evidenzgebundene Card-Schema ist bereits vorhanden und über `card_count = 0` vollständig geprüft;
inhaltliche Card-Zeilen folgen erst mit R5 und werden nicht vorgetäuscht. Replacement-Publish und
Rebuild entfernen alle Symbol-, Pfad- und Card-FTS-Zeilen begrenzt und transaktional.

Der infra-freie Domain-Vertrag begrenzt untrusted Querytext auf 4 KiB und höchstens 32 mindestens
drei Zeichen lange Suchtokens und bindet Treffer und Cursor an Source Channel, Erklärung, Score,
aktuelle `FileRevision`, Query, Run und Snapshot. Der Adapter transformiert niemals Rohtext in SQL,
sondern ausschließlich normalisierte Tokens in gequotete Trigramme und übergibt auch den daraus
gebildeten FTS-Ausdruck als Parameter. FTS ist nur Kandidatengenerierung; die finale deterministische
Gewichtung lautet Name 10, qualifizierter Name 8, Signatur 6 und Pfad 4. Je Klasse werden höchstens
512 Kandidaten unter Cancellation und festem Zwei-Sekunden-Limit geprüft.

Der gemeinsame Storage-Vertrag belegt Identifier- und Signatur-Top-Treffer, Tippfehlertoleranz,
identische Wiederholung, Mehrseitenabfrage, SQL-Injection als Daten, Cancellation, stale Cursor,
aktuelle Revisionen, das Verschwinden eines gelöschten Pfads nach Replacement-Publish und einen
leeren Index nach Rebuild. Migrationsleerstand, alle Vorgängerversionen und der Rollback einer
fehlgeschlagenen V6-Migration sind getestet. Die reproduzierbare Release-Messung mit 50.000
Symbolen aus 100.000 strukturellen Zeilen erreichte über 30 Tippfehler-Queries P50 34,9 ms und
P95 35,3 ms. Die dokumentierten verworfenen breiten Kandidatenabfragen lagen bei P95 195,9 ms und
201,8 ms; der vollständige Index-Load plus Scan des finalen Laufs lag bei P95 1,189 s.

Akzeptanz:

- Identifier- und Keywordfixtures besitzen erwartete Top-Treffer;
- gelöschte Symbole erscheinen nicht;
- untrusted Query kann kein beliebiges SQL ausführen.

## R3 Graph Query

Abhängigkeiten: R1

Status: Completed

- [x] typisierte TraversalQuery
- [x] Richtung, Kantentyp und maximale Tiefe
- [x] Cycle Detection und Resultlimit
- [x] kürzeste Evidence-Pfade
- [x] Callers, Callees, Imports, Exports und Tests

Verifizierter Abschluss vom 2026-08-05: Der infra-freie Domainvertrag begrenzt Graphabfragen auf
einen typisierten Datei- oder Symbolseed, genau eine Richtung und Relation, ein oder zwei Hops und
höchstens 100 Treffer. Callers, Callees, Imports, Exports und Tests besitzen explizite Presets;
Testtreffer verwenden `SourceChannel::Test`, alle übrigen `SourceChannel::Graph`. Jeder Treffer
enthält ein aktuelles Datei- oder Symbolziel und den vollständigen, zusammenhängenden
`GraphEdge`-Evidenzpfad derselben Snapshot-Version.

Der libSQL-Adapter liest nur den jüngsten atomar veröffentlichten Run in einer konsistenten
Deferred-Transaktion. Eine levelweise Breitensuche in kanonischer Kantenreihenfolge erkennt Zyklen,
liefert pro Ziel den ersten kürzesten Pfad und prüft Seed sowie jedes Ziel gegen die aktuelle
Run-Projektion. Resultate sind auf 100 Ziele, zwei Hops, 4.096 geprüfte Kanten und zwei Sekunden
begrenzt; Cancellation, Timeout und abgeschnittene Ergebnisse bleiben typisiert sichtbar.

Der gemeinsame Storage-Vertrag belegt Indexleerstand, Cancellation, fehlende und nach Replacement
stale Seeds, deterministische Wiederholung, Cycle-Terminierung, direkten Vorrang vor einem
alternativen Zwei-Hop-Pfad, Resultlimit sowie Callers, Callees, Imports, Exports und Tests. Jede
Beziehung wird auf Relation, Snapshot und aktuelle Evidence-Revision geprüft; Rebuild entfernt auch
die abfragbare Graphprojektion.

Akzeptanz:

- Traversal terminiert bei Zyklen;
- maximal zwei Hops im interaktiven Standard;
- jeder Treffer erklärt den Beziehungspfad.

## R4 Retrieval Fusion

Abhängigkeiten: R1 bis R3

Status: Completed

- [x] getrennte Candidate Sets
- [x] Normalisierung und Stable-ID-Deduplizierung
- [x] versionierte FusionPolicy
- [x] Goal-, Step-, Freshness-, Token- und Redundanzsignale
- [x] ResultExplanation
- [x] Golden Eval Runner

Verifizierter Abschluss vom 2026-08-05: Der infra-freie Domainvertrag bindet höchstens eine
Candidate-Menge je Exact-, Lexical-, Graph-, Test-, Memory- und Semantic-Kanal an denselben
veröffentlichten Run und Snapshot. Pro Kanal gelten 100 Kandidaten. Dateien werden über
verlustfreie aktuelle Pfade, Symbole über `SymbolId` dedupliziert; widersprüchliche Revisionen,
Projektionen oder Signale derselben stabilen ID werden abgelehnt statt implizit ausgewählt.
Vorgelagerte Exact-/Lexical-Cursor und Graphlimits bleiben als `Truncated` sichtbar.
Memory benötigt eine nicht leere, auf 16 begrenzte Menge zuvor als fresh aufgelöster
`EvidenceRef`s; Semantic Similarity kann keinen Evidence-Status erzeugen.

`FusionPolicy::v1` normalisiert kanalnative Erklärungen und Scores und verwendet vor der
gewichteten Bewertung die unverhandelbaren Bänder Exact, Evidence und Semantic. Damit kann selbst
ein maximal bewerteter rein semantischer Kandidat keinen Exact-Treffer verdrängen. Innerhalb eines
Bands fließen Kanal mit 30 %, Goal und Step mit je 20 %, Freshness, inverse Tokenkosten und
nicht-semantische Mehrkanalbestätigung mit je 10 % ein; Redundanz zieht bis zu 20 % ab. Die
Berechnung ist begrenzt, ganzzahlig und über stabile Ziel-IDs vollständig determiniert.

Jede `ResultExplanation` enthält sämtliche kanalnativen Gründe, normalisierten Signale,
Einzelbeiträge, den Redundanzabzug und Endscore. Das Ergebnis speichert Run, Snapshot,
`FusionPolicyVersion` und Trunkierung. Der versionierte Golden-Eval-Runner belegt in zwei
identischen Durchläufen Deduplizierung, Graph/Test/Memory/Semantic, alle fünf geforderten Signale,
Stable Tie-Breaking, Resultlimit und Exact-vor-Semantic. Die breitere Retrieval-Evalbaseline des
Gates M4/M5 bleibt bewusst ein späteres, weiterhin offenes Arbeitspaket.

Akzeptanz:

- Exact Match wird nicht durch semantische Popularität verdrängt;
- Fusion ist für gleiche Eingaben deterministisch;
- Policyversion wird mit Ergebnis gespeichert.

## R5 Optional Embeddings

Abhängigkeiten: R4

Status: Completed

- [x] EmbeddingProvider-Port
- [x] ModelProfile und Dimensionvalidierung
- [x] Semantic-Card-Normalisierung
- [x] BodyHash-basierter Cache
- [x] lokaler Batchjob mit Cancellation
- [x] libSQL-Vector-Capability
- [x] Fallback ohne Vektorindex

Verifizierter erster Slice vom 2026-08-05: Der Domainkern normalisiert Semantic Cards mit
versionierter Whitespace-Policy, begrenzt Rohinput auf 64 KiB und den kanonischen Kartentext auf
16 KiB und leitet daraus einen domänenseparierten BLAKE3-`BodyHash` ab. Ein snapshotgebundener Job
akzeptiert höchstens 512 logisch eindeutige Karten. Weder Karten- noch Vektor-Debugausgaben geben
Inhalte oder Komponenten aus.

Das embedding-spezifische `EmbeddingModelProfile` validiert Provider, opaque Modell-ID,
Dimension, Batchgröße, Float32-Datentyp, fehlende Quantisierung und L2-Normalisierung. Seine
`ModelProfileId` wird aus allen vektorformenden Feldern abgeleitet; Betriebswerte wie Batchgröße
ändern die Vektorkompatibilität nicht. Providerantworten bleiben auf 64 Vektoren mit je höchstens
8.192 Komponenten begrenzt und werden vor Persistenz auf exakte Dimension, endliche Werte und
nicht leere L2-Norm geprüft. `VectorHit` besitzt ausschließlich `SourceChannel::Semantic` und kann
keine `EvidenceRef` tragen.

Der Application-Slice trennt `EmbeddingProvider` und `SemanticEmbeddingStore` als schmale Ports.
`GenerateSemanticEmbeddings::disabled()` besitzt konstruktiv weder Provider noch Cache;
`enabled(...)` verlangt alle Capabilities gemeinsam. Der lokale Scheduler-Job prüft Cancellation
vor und nach jeder Boundary, erzwingt einen Provider-Timeoutvertrag, meldet monotone Card-Progress
und batched nur exakte `(SemanticCardId, ModelProfileId, BodyHash)`-Misses.

Verifizierter Abschluss vom 2026-08-05: Knowledge-Schema V7 persistiert kanonische Cards,
Snapshotzuordnungen, vollständige vektorformende Profilmetadaten und normalisierte
Little-Endian-Float32-Vektoren. Lookup und Store sind transaktional; der Rebuild arbeitet in
referenziell sicheren, resumierbaren 4.096-Zeilen-Transaktionen mit determiniertem Row-Progress.
Alle Operationen sind cancellable und zeitbegrenzt und validieren Profil, Bodyrevision, Dimension
und L2-Norm erneut beim Lesen. Upgrade
aus jeder unterstützten Knowledge-Version sowie der fehlgeschlagene V6→V7-Upgradepfad sind
getestet.

Die native libSQL-Capability wird für die konkrete Profildimension in einer isolierten
In-Memory-DiskANN-Projektion geprüft. Native und lineare Suche verwenden denselben stabilen,
snapshot- und profilgebundenen Korridor von höchstens 4.096 Karten; Similarity, Stable Tie-Breaking
und sichtbare Trunkierung bleiben im Rust-Adapter deterministisch. Die gemeinsame Storage-
Contract-Suite verifiziert persistentes Reopen, Bodyrevisionen über zwei Snapshots,
Profil-/Dimensionsisolation, Cancellation und einen Rebuild, der Snapshot- und deterministische
Retrievaldaten erhält.

Abschlussgates: `cargo fmt --all -- --check`, Workspace-Clippy mit allen Targets/Features und
Warnings denied, `cargo test --workspace --all-features -- --test-threads=1`, Rustdoc mit Warnings
denied, Frontend-CI, Markdown-Linkcheck, Dependency-/Lizenzreport und Tauri-Release-Build ohne
Bundle sind grün.

Akzeptanz:

- Anbieter- oder Dimensionswechsel vermischt keine Vektoren;
- ausgeschaltete Embeddings beeinträchtigen Exact, FTS und Graph nicht;
- VectorHit ist typseitig keine Evidence.

## R6 Modulbildung

Abhängigkeiten: R4

Status: Completed

- [x] Manifest- und Pfadgrenzen als Primärsignal
- [x] Graphcommunities als Ergänzung
- [x] Modul-IDs und Membership Evidence
- [x] zentrale Symbole, Entry Points und Tests pro Modul
- [x] deterministische Repository Card

Verifizierter Abschluss vom 2026-08-05: `ModuleFormationPolicy::v1` ordnet jedes indexierte Symbol
genau einem Primärmodul zu. Der jeweils tiefste bestätigte Paketmanifest-Vorfahre gewinnt;
mehrere Deskriptoren desselben Verzeichnisses werden konsolidiert. Ohne Manifest entsteht eine
deterministische Grenze aus der ersten Pfadkomponente, während Dateien an der Wurzel einem
expliziten Repository-Modul angehören. Verschachtelte Monorepo-Pakete bleiben dadurch getrennt.

Iterativ berechnete stark zusammenhängende Komponenten aus nicht strukturellen Symbolkanten
ergänzen Primärmodule als optionale Graphcommunities, ohne sie zu überschreiben. Domänenseparierte
BLAKE3-`ModuleId`s, aktuelle Symbol-/Manifestrevisionen und konkrete Graph-`EvidenceRef`s machen
jede Membership reproduzierbar und prüfbar. Die Publikationsgrenze lehnt fehlende, veraltete oder
nur durch `Contains`/`Defines` gestützte Communitybelege ab.

Zentrale Symbole folgen dem versionierten Rank, Entry Points und Tests den bestätigten Dateirollen.
Alle Mengen sind kanonisch sortiert und sichtbar begrenzt. Die snapshotgebundene L0-
`RepositoryCard` enthält Paketmodule, Sprachen, Entry Points sowie Datei-, Symbol- und
Modulanzahlen. Bildung und Karte sind vollständig deterministisch und benötigen kein LLM.

Knowledge-Schema V8 veröffentlicht Graph, Ranking, Manifestbelege, Module, Memberships, Features
und Repository Card atomar. Der gemeinsame Storage Contract belegt Reopen, Retention und Rebuild;
Upgrade und fehlgeschlagener V7→V8-Upgradepfad sind getestet. Die reproduzierbare 50.000-Symbol-
Messung ergab Exact P95 60,6 ms und FTS P95 39,5 ms; beide bleiben unter dem 100-ms-Gate.

Abschlussgates: `cargo fmt --all -- --check`, Workspace-Clippy mit allen Targets/Features und
Warnings denied, `cargo test --workspace --all-features -- --test-threads=1`, Rustdoc mit Warnings
denied, Frontend-CI, Markdown-Linkcheck, Dependency-/Lizenzreport und Tauri-Release-Build ohne
Bundle sind grün.

Akzeptanz:

- Monorepo-Pakete bleiben unterscheidbar;
- ein Symbol besitzt eine primäre und optional weitere Memberships;
- Modulbildung funktioniert ohne LLM.

## R7 Deep-Map Schema und Planner

Abhängigkeiten: R6

Status: Completed

- [x] versioniertes ModuleCard-Schema
- [x] Coverage-Ziele
- [x] Seed Ranking
- [x] Token-, Zeit- und Toolbudgets
- [x] Informationsgewinn für nächste Expansion
- [x] Stopbedingungen

Verifizierter Abschluss vom 2026-08-05: `ModuleCardSchemaVersion::V1` definiert zwölf begrenzte
Detailfelder, sieben Pflichtmetadaten, feldgenaue Evidence-Pflicht, ein Dokumentlimit von 64 KiB
und höchstens 512 Evidence IDs. Muss- und Soll-Coverage sind explizit; Coverage bleibt an Snapshot
und Schemaversion gebunden und lehnt unbekannte oder doppelte Module ab.

`DeepMapPlanner::v1` erzeugt ausschließlich aus einem veröffentlichten Index und der aktuellen
Coverage einen kanonischen `ExplorePlan`. Manifest, Entrypoint, Zentralsymbol, Testwurzel,
Graphcommunity und offene Modulgrenze bilden die versionierten Seeds. Kandidaten werden lokal
dedupliziert, stabil bewertet und auf 16.384 Einträge begrenzt. Bereits vollständig durch
Muss-Felder abgedeckte Module erzeugen keine Schritte.

Token-, Zeit- und Toolkosten werden vor jeder Reservierung mit geprüfter Arithmetik gegen alle
drei Budgetdimensionen getestet. Jeder Schritt enthält Outcome, Evidence-Anforderung,
Verifikationsmethode und Status. Statische Planung und Laufzeit unterscheiden Coverage-, Budget-,
Cancellation-, Stagnations-, Mindestgewinn- und fehlende-Seed-Stopgründe. Modellaufrufe,
Read-only-Exploration, Proposal-Prüfung und Persistenz bleiben bewusst R8 beziehungsweise R9.

Golden-, Schema-, Snapshot-, Unknown-Module-, Coverage-, Budgetgrenzen- und Stoptests sind grün.
Abschlussgates: `cargo fmt --all -- --check`, Workspace-Clippy mit allen Targets/Features und
Warnings denied, `cargo test --workspace --all-features`, Rustdoc mit Warnings denied,
Frontend-CI, Markdown-Linkcheck, Dependency-/Lizenzreport und Tauri-Release-Build ohne Bundle.

Akzeptanz:

- Planner kann einen vollständigen deterministischen ExplorePlan ohne Modell erzeugen;
- Budgetüberschreitung ist unmöglich;
- bereits ausreichend abgedeckte Module werden übersprungen.

## R8 Read-only Explorer

Abhängigkeiten: R7, Providergrundlage aus Plan 04 darf als Stub vorgezogen werden

Status: Completed

- [x] typisierte Inspect- und Search-Aktionen
- [x] strukturierte Modelausgabe
- [x] Schema Validation
- [x] maximal eine Repair-Anfrage
- [x] ModuleCard-Proposal mit feldgenauen Evidence IDs
- [x] Cancellation und Resume

Der Explorer-Action-Vertrag V1 ist eine geschlossene Domain-Union aus Inspect, Search und Propose.
Inspect liest ausschließlich das aktuelle planbestimmte Modul-, Manifest- oder Symbolziel. Search
trennt Exact-/Lexical-Text konstruktiv von symbolgebundenen Callers-, Callees-, Imports-, Exports-
und Tests-Graphpresets; Query, Limit, Informationsgewinn und Begründung sind begrenzt. Schreib-,
Shell-, Prozess-, Git- oder generische Execute-Aktionen existieren im Capability-Port nicht.

Das eingebettete `deep-map-explorer-action-v1`-JSON-Schema verwendet
`additionalProperties: false`; der unabhängige Runtime-Decoder akzeptiert genau ein Dokument bis
64 KiB, lehnt unbekannte Felder und Text außerhalb des Schemas ab und validiert IDs, Versionen,
Querytypen sowie alle ModuleCard-Feldgrenzen. `serde_json` war bereits exakt im Workspace gepinnt
und wird als direkte Application-Abhängigkeit verwendet, weil die Rust-Standardbibliothek keinen
JSON-Parser bereitstellt; der externe Dependency-Graph erhielt keine neue Version.

`ModuleCardProposal` kann ausschließlich den Status Proposed tragen. Jedes nicht leere Feld besitzt
eigene, kanonische Evidence IDs. Der Controller bestätigt einen Vorschlag erst, wenn Modul,
Snapshot, Schemaversion, sämtliche vom Planschritt erwarteten Felder und jede Evidence ID mit dem
unmittelbar zuvor normalisierten Read-Ergebnis übereinstimmen. Deterministische Evidence-Auflösung,
Claimstatus, Persistenz und Publish bleiben R9 vorbehalten.

Der neutrale `ExplorerModelProvider`-Stub erhält das statische JSON Schema, plan- und
snapshotgebundene Metadaten, feste Timeouts und Cancellation, aber keine Ollama-Payload oder
Netzwerkfähigkeit. Er ist bewusst nur die für R8 benötigte Teilgrundlage; der allgemeine
ModelProvider, Streaming, `ModelProfile`, Capability Probe, Endpoint Policy und ein echter Adapter
bleiben H4/H5. Providerrohoutput, Toolpreview und Tool-Evidenz sind auf 64 KiB, 16 KiB und 100 IDs
begrenzt und in Debugausgaben redigiert.

Über einen kompletten Explorer-Aufruf ist höchstens eine inhaltsfreie Repair-Anfrage erlaubt. Erst
vollständig dekodierte und zustandsautorisierte Aktionen erreichen den Read-Port; ein ungültiges
Original und eine ungültige Reparatur werden nie ausgeführt. Der run-, snapshot-, schema- und
policygebundene `ExplorerCheckpoint` speichert nur ein lückenloses Präfix bestätigter Schritte.
Cancellation gibt diesen Zustand zurück, und Resume startet exakt beim ersten unbestätigten Schritt.

Contract-Tests belegen genau einen Read je Planschritt, Resume ohne Wiederholung, Cancellation vor
Provider und Tool, genau einen erfolgreichen Repair sowie zwei ungültige Modellantworten mit null
Werkzeugaufrufen. Abschlussgates: `cargo fmt --all -- --check`,
`cargo test --workspace --all-features`, Workspace-Clippy mit allen Targets/Features und Warnings
denied, Rustdoc mit Warnings denied, Frontend-CI, Markdown-Linkcheck,
Dependency-/Lizenzbericht und Tauri-Release-Build ohne Bundle.

Akzeptanz:

- ungültige oder evidencefreie Felder werden verworfen;
- Explorer kann nichts schreiben oder ausführen;
- Resume wiederholt keine bereits bestätigten Schritte.

## R9 Claim Verifier

Abhängigkeiten: R8

Status: Completed

- [x] Evidence-Auflösung
- [x] Import-, Export-, Test- und Graphclaimprüfung
- [x] Widerspruchserkennung
- [x] Fact-, Observation- und Hypothesis-Zuweisung
- [x] Confidence ist getrennt vom Status
- [x] Publish nur nach Verify

Das geschlossene `module-card-claims-v1`-Schema und sein unabhängiger Runtime-Decoder akzeptieren
genau ein streng versioniertes JSON-Dokument. Card, Modul, Snapshot und jedes Claim-Feld sind an
den Proposal-Envelope gebunden; unbekannte Felder, Text außerhalb des Dokuments, nicht kanonische
IDs und Prädikate außerhalb der typisierten Path-, Symbol-, Import-, Export-, Call-, Test-,
Observation- und Architecture-Intent-Union werden abgelehnt.

Evidence IDs sind domänenseparierte BLAKE3-Identitäten der vollständigen File Revision, des
Symbols oder der Graphkante. Der read-only Resolver lädt ausschließlich den jüngsten atomar
veröffentlichten Index und löst exakt die angefragten IDs innerhalb desselben Runs und Snapshots
auf. Der deterministische Verifier konstruiert Facts nur aus bejahenden, exakt belegten
Strukturclaims, Observations aus beobachteter Prosa und Hypotheses aus negativen Aussagen oder
nicht deterministisch beweisbarer Architekturabsicht. Classification, Active-Status und
Confidence bleiben getrennte Typen. Opponierende Claims erzeugen einen sichtbaren
Widerspruchsbericht; Cards werden weder still zusammengeführt noch per Mehrheitsentscheid
umklassifiziert.

Nur der privat konstruierbare `VerifiedModuleCardBatch` überschreitet den Publish-Port. Das
Knowledge-Schema V9 persistiert Cards, Felder, Claims und die vollständige aufgelöste Provenienz in
einer `IMMEDIATE`-Transaktion und aktualisiert `card_fts` samt Lexical-Marker atomar. Stale Runs,
fremde oder erfundene Evidence, Duplicate-Publish, Cancellation, Deadline und nicht zustellbarer
Progress werden vor Commit abgelehnt; SQL-Fehler rollen alle Teilzeilen zurück. Ein Fast-Index-
Rebuild entfernt die regenerierbare Suchprojektion, erhält aber die dauerhaften Claim- und
Evidence-Zeilen für die Invalidierung in R11.

Domain-, Application-, Migration-, konkrete libSQL- und gemeinsame adapterneutrale Storage-
Contracts belegen Fake-ID- und Stale-Run-Ablehnung, Fact/Observation/Hypothesis,
Widerspruchssichtbarkeit, verified-only Publish, höchstens 64 monotone Progressereignisse,
atomaren Fehlerrollback, persistente Classification/Confidence, Lexical Search,
Duplicate-Rejection und Rebuild-Erhalt. Abschlussgates am 2026-08-06: `cargo fmt --all -- --check`,
`cargo test --workspace --all-features`, Workspace-Clippy mit allen Targets/Features und Warnings
denied, Rustdoc mit Warnings denied, Frontend-CI mit Node 24.14.0/pnpm 11.9.0,
Markdown-Linkcheck, Dependency-/Lizenzbericht und Tauri-Release-Build ohne Bundle sind grün.

Akzeptanz:

- erfundene Symbol-IDs werden abgelehnt;
- nicht deterministisch prüfbare Architekturabsicht bleibt Hypothesis;
- widersprüchliche Cards werden sichtbar und nicht still gemerged.

## R10 Task Lens

Abhängigkeiten: R4, R6, R9

Status: Completed

- [x] Seeds aus Goal, Step, Fehlern und expliziten Pfaden
- [x] begrenzte Expansion exact → FTS → graph/test → claims → semantic
- [x] Zoomstufen L0 bis L3
- [x] Tokenkostenschätzung
- [x] LensDigest und Policyversion
- [x] Aktualisierung nach Indexdelta

Der Domain-Compiler V1 kanonisiert Goal, aktuellen Schritt, Diagnosen, explizite Pfade, Symbole und
IDs, geänderte Dateien, Hypothesen sowie fehlgeschlagene Verifikationen. Er fusioniert höchstens
32 Retrieval-Einträge in der festen Reihenfolge Exact, FTS, Graph/Test, Claims und optional
Semantic. L0 bis L3, konservative Byte- und Strukturkosten, ein Budget von 256 bis 32.768 Tokens,
sichtbare Trunkierung und ein policy-, seed-, index-, snapshot-, claim- und reihenfolgegebundener
`LensDigest` machen identische Eingaben deterministisch und Änderungen am veröffentlichten Index
sichtbar.

Der Application-Use-Case reicht eine gemeinsame Cancellation und Deadline durch alle Kanäle,
begrenzt jeden Adapteraufruf und kompiliert Semantic-Treffer ausschließlich als Kandidaten, nie als
Evidence. Die libSQL-Adapter validieren bei jeder Auslieferung den aktuell veröffentlichten Run,
rekonstruieren Claim-Evidence gegen dessen unveränderlichen `PublishedIndex` und verwerfen alte
Publikationen. Ein eintragsgroßer, bei Publish und Rebuild aktualisierter Indexcache vermeidet eine
vollständige Kopie pro Lens, ohne die dauerhafte Aktualitätsprüfung zu überspringen.

Contracts belegen kanonische Seeds, feste Kanalreihenfolge, Produktionscode plus Test in der
Bugfixture, Ausschluss eines irrelevanten Großmoduls, sichtbare Claim-Trunkierung, Cancellation,
Deadline-Weitergabe, Indexdelta und null stale Fact Leakage. Der reproduzierbare Release-Fixture
mit 50.000 Symbolen, 100.000 Strukturzeilen und einem verifizierten Fact misst Exact P95 mit
50,81 ms, FTS P95 mit 37,76 ms und Task Lens P95 mit 267,31 ms bei einem Ziel von 300 ms.
Abschlussgates am 2026-08-06: `cargo fmt --all -- --check`,
`cargo test --workspace --all-features --locked`, Workspace-Clippy mit allen Targets/Features und
Warnings denied, Rustdoc mit Warnings denied, Frontend-CI mit Node 24.14.0/pnpm 11.9.0,
Markdown-Linkcheck, Dependency-/Lizenzbericht und Tauri-Release-Build ohne Bundle sind grün.

Akzeptanz:

- Bugfixture erhält Produktionscode und zugehörige Tests;
- irrelevante große Module bleiben außerhalb;
- stale Claims erscheinen nicht als Fakten.

## R11 Invalidation und Remap

Abhängigkeiten: R9, R10

Status: Completed

- [x] direkte Claim-Invalidierung
- [x] Module Card stale und NeedsReview
- [x] priorisierte Remapqueue
- [x] Task-Lens-Rebuild
- [x] Parser- und Mapperversion als Invalidierungsgrund

Verifizierter Abschluss vom 2026-08-06: `IndexInvalidationPlan` begrenzt die Entscheidung auf die
eigene Card und genau einen Hop direkter Graphabhängiger. Knowledge-Schema V10 persistiert Card- und
Claim-Lifecycle, direkte Evidence-Invalidierungen sowie eine stabile Direkt-vor-Abhängig-
Remapqueue. Der atomische Index-Publish markiert direkte Cards vor Sichtbarkeit des neuen Runs
`Stale`, direkte Abhängige `NeedsReview` und trägt Evidence-, Parser- oder Mapperursache typisiert
ein. Entfernte Module erzeugen keine sinnlose Remaparbeit; unveränderte unabhängige Cards und ihre
aktuelle Evidence bleiben über Run-Grenzen verwendbar.

Der begrenzte `ModuleRemapQueueStore`-Port validiert Ziel-Run/-Snapshot, Priorität und Grund,
Modul-Eindeutigkeit, kanonische Reihenfolge, Cancellation, Zwei-Sekunden-Deadline und sichtbare
Trunkierung. Eine erfolgreich neu publizierte Card entfernt ihr Modul atomar aus der Queue.
Task-Lens-Reads wählen nur die neueste lifecycle-seitig `Published`-Card mit `Active`-Claims und
lösen jede Evidence erneut gegen den aktuellen Index; stale oder `NeedsReview`-Cards können daher
keinen Fact ausliefern.

Domain-, Application-, V9→V10-Migrations-, konkrete libSQL- und gemeinsame Storage-Contracts
belegen direkte Zeilenänderung, Parser-/Mappergrund, unveränderte unabhängige Claims, genau einen
abhängigen Hop, Queue-Ersetzung, Rollback und null stale Fact Leakage. Der unveränderte
30-Sample-Release-Fixture mit 100.000 LOC erreichte nach begrenzter Wiederverwendung separater
identitätsgeprüfter Mutationshandles P50 816 ms und P95 884 ms bei einem Ziel von zwei Sekunden;
Watcher-P95 lag bei 394 ms, Refresh-/Publish-P95 bei 491 ms.

Abschlussgates am 2026-08-06: `cargo fmt --all -- --check`,
`cargo test --workspace --all-features --locked`, Workspace-Clippy mit allen Targets/Features und
Warnings denied, Rustdoc mit Warnings denied, der manuelle 30-Sample-Release-Performancetest,
Frontend-CI mit Node 24.14.0/pnpm 11.9.0, Markdown-Linkcheck, Dependency-/Lizenzbericht und
Tauri-Release-Build ohne Bundle sind grün.

Akzeptanz:

- Änderung einer Evidence-Zeile macht Claim vor nächster Auslieferung stale;
- unabhängige Module werden nicht unnötig remapped;
- Invalidationstest hat null stale Fact Leakage.

## Gate M4/M5

- [ ] Retrieval-Evalbaseline versioniert
- [ ] Deep Map eines Rust-, TS- und Python-Fixtures
- [x] jede veröffentlichte Card besitzt gültige Evidence
- [x] Task Lens bleibt innerhalb des konfigurierten Budgets
- [ ] App funktioniert vollständig ohne Embeddings
- [x] Performanceziele für Search und Context-Vorstufe gemessen
