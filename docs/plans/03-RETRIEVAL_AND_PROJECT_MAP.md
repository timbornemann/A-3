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

- [ ] FTS-Schema für Namen, Signaturen, Pfade und Cards
- [ ] gewichtete Felder
- [ ] transaktionale Aktualisierung mit Index Publish
- [ ] Queryescaping und Limits
- [ ] Delete- und Rebuildtests

Akzeptanz:

- Identifier- und Keywordfixtures besitzen erwartete Top-Treffer;
- gelöschte Symbole erscheinen nicht;
- untrusted Query kann kein beliebiges SQL ausführen.

## R3 Graph Query

Abhängigkeiten: R1

- [ ] typisierte TraversalQuery
- [ ] Richtung, Kantentyp und maximale Tiefe
- [ ] Cycle Detection und Resultlimit
- [ ] kürzeste Evidence-Pfade
- [ ] Callers, Callees, Imports, Exports und Tests

Akzeptanz:

- Traversal terminiert bei Zyklen;
- maximal zwei Hops im interaktiven Standard;
- jeder Treffer erklärt den Beziehungspfad.

## R4 Retrieval Fusion

Abhängigkeiten: R1 bis R3

- [ ] getrennte Candidate Sets
- [ ] Normalisierung und Stable-ID-Deduplizierung
- [ ] versionierte FusionPolicy
- [ ] Goal-, Step-, Freshness-, Token- und Redundanzsignale
- [ ] ResultExplanation
- [ ] Golden Eval Runner

Akzeptanz:

- Exact Match wird nicht durch semantische Popularität verdrängt;
- Fusion ist für gleiche Eingaben deterministisch;
- Policyversion wird mit Ergebnis gespeichert.

## R5 Optional Embeddings

Abhängigkeiten: R4

- [ ] EmbeddingProvider-Port
- [ ] ModelProfile und Dimensionvalidierung
- [ ] Semantic-Card-Normalisierung
- [ ] BodyHash-basierter Cache
- [ ] lokaler Batchjob mit Cancellation
- [ ] libSQL-Vector-Capability
- [ ] Fallback ohne Vektorindex

Akzeptanz:

- Anbieter- oder Dimensionswechsel vermischt keine Vektoren;
- ausgeschaltete Embeddings beeinträchtigen Exact, FTS und Graph nicht;
- VectorHit ist typseitig keine Evidence.

## R6 Modulbildung

Abhängigkeiten: R4

- [ ] Manifest- und Pfadgrenzen als Primärsignal
- [ ] Graphcommunities als Ergänzung
- [ ] Modul-IDs und Membership Evidence
- [ ] zentrale Symbole, Entry Points und Tests pro Modul
- [ ] deterministische Repository Card

Akzeptanz:

- Monorepo-Pakete bleiben unterscheidbar;
- ein Symbol besitzt eine primäre und optional weitere Memberships;
- Modulbildung funktioniert ohne LLM.

## R7 Deep-Map Schema und Planner

Abhängigkeiten: R6

- [ ] versioniertes ModuleCard-Schema
- [ ] Coverage-Ziele
- [ ] Seed Ranking
- [ ] Token-, Zeit- und Toolbudgets
- [ ] Informationsgewinn für nächste Expansion
- [ ] Stopbedingungen

Akzeptanz:

- Planner kann einen vollständigen deterministischen ExplorePlan ohne Modell erzeugen;
- Budgetüberschreitung ist unmöglich;
- bereits ausreichend abgedeckte Module werden übersprungen.

## R8 Read-only Explorer

Abhängigkeiten: R7, Providergrundlage aus Plan 04 darf als Stub vorgezogen werden

- [ ] typisierte Inspect- und Search-Aktionen
- [ ] strukturierte Modelausgabe
- [ ] Schema Validation
- [ ] maximal eine Repair-Anfrage
- [ ] ModuleCard-Proposal mit feldgenauen Evidence IDs
- [ ] Cancellation und Resume

Akzeptanz:

- ungültige oder evidencefreie Felder werden verworfen;
- Explorer kann nichts schreiben oder ausführen;
- Resume wiederholt keine bereits bestätigten Schritte.

## R9 Claim Verifier

Abhängigkeiten: R8

- [ ] Evidence-Auflösung
- [ ] Import-, Export-, Test- und Graphclaimprüfung
- [ ] Widerspruchserkennung
- [ ] Fact-, Observation- und Hypothesis-Zuweisung
- [ ] Confidence ist getrennt vom Status
- [ ] Publish nur nach Verify

Akzeptanz:

- erfundene Symbol-IDs werden abgelehnt;
- nicht deterministisch prüfbare Architekturabsicht bleibt Hypothesis;
- widersprüchliche Cards werden sichtbar und nicht still gemerged.

## R10 Task Lens

Abhängigkeiten: R4, R6, R9

- [ ] Seeds aus Goal, Step, Fehlern und expliziten Pfaden
- [ ] begrenzte Expansion exact → FTS → graph/test → claims → semantic
- [ ] Zoomstufen L0 bis L3
- [ ] Tokenkostenschätzung
- [ ] LensDigest und Policyversion
- [ ] Aktualisierung nach Indexdelta

Akzeptanz:

- Bugfixture erhält Produktionscode und zugehörige Tests;
- irrelevante große Module bleiben außerhalb;
- stale Claims erscheinen nicht als Fakten.

## R11 Invalidation und Remap

Abhängigkeiten: R9, R10

- [ ] direkte Claim-Invalidierung
- [ ] Module Card stale und NeedsReview
- [ ] priorisierte Remapqueue
- [ ] Task-Lens-Rebuild
- [ ] Parser- und Mapperversion als Invalidierungsgrund

Akzeptanz:

- Änderung einer Evidence-Zeile macht Claim vor nächster Auslieferung stale;
- unabhängige Module werden nicht unnötig remapped;
- Invalidationstest hat null stale Fact Leakage.

## Gate M4/M5

- [ ] Retrieval-Evalbaseline versioniert
- [ ] Deep Map eines Rust-, TS- und Python-Fixtures
- [ ] jede veröffentlichte Card besitzt gültige Evidence
- [ ] Task Lens bleibt innerhalb des konfigurierten Budgets
- [ ] App funktioniert vollständig ohne Embeddings
- [ ] Performanceziele für Search und Context-Vorstufe gemessen
