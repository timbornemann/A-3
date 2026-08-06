# Domänenmodell

Status: verbindliche Baseline  
Stand: 2026-08-06

## Ubiquitous Language

| Begriff | Bedeutung |
| --- | --- |
| Repository | Logische Git-Codebasis |
| Worktree | Konkrete lokale Arbeitskopie mit eigenem Dateizustand |
| Snapshot | Konsistente Sicht auf HEAD plus lokale Änderungen zu einem Zeitpunkt |
| File Revision | Dateiinhalt, identifiziert durch einen kryptografischen Content Hash |
| Symbol | Sprachabhängige semantische Einheit wie Funktion, Typ oder Methode |
| Edge | Typisierte Beziehung zwischen Symbolen, Dateien, Modulen, Tests oder Claims |
| Module | Deterministischer primärer Pfadbereich oder zusätzliche graphbelegte Community |
| Evidence | Prüffähiger Verweis auf echten Code, Konfiguration oder Werkzeugausgabe |
| Claim | Persistierte Aussage mit Typ, Evidenz, Status und Aktualität |
| Module Card | Kompakte, evidenzgebundene Beschreibung eines Projektbereichs |
| Project Map | Mehrstufige Sicht aus deterministischem Graph und geprüften Module Cards |
| Goal Contract | Unveränderter Zielanker einer Aufgabe mit Akzeptanzkriterien und Grenzen |
| Task Ledger | Dauerhafter Plan samt Schrittzuständen, Ergebnissen und Verifikation |
| Run | Ein kontrollierter Agentenlauf für eine Aufgabe |
| Context Pack | Tokenbegrenzter, reproduzierbarer Modelleingang für genau einen Turn |
| Model Provider | Austauschbare, providerneutrale lokale Textgenerierungs-Capability |
| Provider Event | Begrenztes Textfragment oder genau ein terminaler Abschluss eines Modellstreams |
| Tool Action | Typisierte, durch Policy geprüfte Interaktion mit Workspace oder Prozessen |
| Source Channel | Explizite Herkunft eines Retrievaltreffers, etwa Exact, Lexical oder Graph |

## Identitäten

### RepositoryIdentity

Besteht aus:

- kanonischem Git Common Directory;
- normalisierter Haupt-Remote, falls vorhanden;
- stabilem lokalem RepositoryId.

Ein Repository ohne Remote ist vollständig unterstützt. Pfade allein sind keine portable Identität.
Die lokale RepositoryId wird mit der versionierten Ableitung `a3.repository-id.v1` als BLAKE3-Digest
des kanonischen Git Common Directory gebildet. Eine normalisierte Haupt-Remote wird separat und ohne
Benutzername, Passwort, Query oder Fragment als Fingerprint erfasst; sie verändert die lokale
RepositoryId nicht.

### WorktreeIdentity

Besteht aus RepositoryId, kanonischem Worktree-Root und `WorktreeAnchorId`. Die WorktreeId wird mit
der versionierten Ableitung `a3.worktree-id.v1` deterministisch aus RepositoryId und Root gebildet.
Dadurch bleibt sie bei wiederholter Erkennung und über Appneustarts stabil, ändert sich aber bei einem
Worktree-Umzug. Die `WorktreeAnchorId` wird mit `a3.worktree-anchor-id.v1` aus dem relativen Pfad des
Git-Metadatenverzeichnisses innerhalb des kanonischen Git Common Directory gebildet. Sie bleibt bei
`git worktree move` sowie beim Umzug des gesamten Repositories stabil, ist aber allein kein
Identitätsbeweis.

Jeder Worktree besitzt eine eigene Wissens- und Mutationsdomäne. A^3 bietet eine Reconciliation nur
für genau einen vorherigen Katalogeintrag mit derselben `WorktreeAnchorId` und entweder derselben
`RepositoryId` oder demselben vorhandenen Remote-Fingerprint an. Pfad, HEAD oder Ähnlichkeit genügen
nie. Erst die native Bestätigung erhält die vorhandene `ProjectId` und bindet die private
Worktree-Datenbank an die neu inspizierten IDs; „separat öffnen“ erzeugt eine getrennte Identität.

### Catalog Project Identity

`ProjectId` identifiziert einen Eintrag im globalen lokalen Projektkatalog, während `RepositoryId` und
`WorktreeId` die jeweils beobachtete Git- beziehungsweise Worktree-Identität beschreiben. Bei der ersten
Aufnahme wird `ProjectId` mit der versionierten Ableitung `a3.catalog-project-id.v1` deterministisch aus
der `RepositoryId` gebildet und anschließend persistiert. Linked Worktrees derselben `RepositoryId`
teilen einen Katalogeintrag. Eine Umzugs-Reconciliation behält diese `ProjectId` nur nach eindeutiger
Evidenz, nativer Bestätigung und erneuter Prüfung der exakten Katalogrevision bei. Mehrdeutige oder
reine Pfad-/Remote-Treffer werden als separates Projekt geöffnet beziehungsweise nicht angeboten.

### SnapshotId

Ein Snapshot enthält:

- eigene SnapshotId und optional die unmittelbare Parent-SnapshotId desselben Worktrees;
- HEAD Object ID oder den Zustand Unborn;
- monotone Worktree Generation;
- geordnete Menge geänderter Pfade und ihrer Content Hashes;
- Index Schema Version;
- Sprachadapter-Versionen.

Snapshots sind unveränderlich. Der erste Snapshot besitzt Generation eins und keinen Parent. Jeder
weitere Snapshot verweist auf den unmittelbar vorherigen Snapshot und erhöht die Generation genau um
eins. Repository-Pfade sind verlustlose, relative Git-artige Rohbytes mit `/` als Separator; leere
Segmente, `.` und `..`, NUL sowie absolute oder nicht normalisierte Formen sind ungültig. Änderungen
und Adapterrevisionen werden kanonisch sortiert und dürfen innerhalb eines Snapshots nicht doppelt
vorkommen. Ein neuer beobachteter Dateizustand erzeugt eine neue Generation.

### IndexRunId

Ein IndexRun referenziert genau einen existierenden Snapshot desselben Worktrees, eine monotone
worktree-lokale Sequenz und eine Ranking-Policy-Version. Pro Worktree darf höchstens ein Lauf den
Zustand `building` besitzen. Ein Lauf kann ohne Veröffentlichung über `building` → `failed` oder
`building` → `cancelled` enden. `building` → `published` ist ausschließlich der letzte Schritt der
S10-Transaktion, die den vollständigen, exakt passenden Index gemeinsam sichtbar macht.

### SymbolId

Ein `SymbolId` ist ein domänengetrennter BLAKE3-Digest über verlustfreie Repository-Pfadbytes,
Content Hash, Sprache, Adapterversion, LanguageAdapter-Contract-Version und die dateilokale Symbol-ID.
Snapshot-, Rankingversion und Zeilennummern sind kein Teil der Identität. Identische Parse-Evidenz
behält ihre ID beim Re-Rank; Inhalts-, Pfad- oder Adapteränderungen erzeugen absichtlich neue IDs und
invalidieren alte Evidenz.

### ModuleId und ModuleProjection

Eine `ModuleId` ist ein BLAKE3-Digest mit getrennter V1-Domäne. Primärmodule leiten ihn nur aus
ihrem kanonischen Repository- oder Verzeichnisroot ab; dadurch bleibt dieselbe Grenze bei
Inhaltsänderungen stabil. Eine zusätzliche Graphcommunity leitet ihn aus der sortierten Menge ihrer
content- und adaptergebundenen `SymbolId`s ab. Die `ModulePolicyVersion` ist unabhängig von der
Rankingversion und wird mit Projektion und Repository Card gespeichert.

`ModuleProjection` gehört genau zu einem `SnapshotId` und enthält `RepositoryModule`s,
`ModuleMembership`s und eine deterministische L0-`RepositoryCard`. Ein Modul ist entweder eine
Manifestgrenze mit mindestens einer aktuellen Manifestrevision, eine Pfadgrenze oder eine
rootlose Graphcommunity. Jedes Graphsymbol besitzt genau eine primäre Manifest- oder
Pfadmembership; weitere Memberships dürfen ausschließlich Graphcommunities betreffen. Primäre
Evidence enthält die aktuelle Symbol-File-Revision und bei Manifestgrenzen zusätzlich eine
bestätigte Manifestrevision. Community-Evidence ist nicht leer und muss zu einer aktuellen,
symbolincidenten Graphkante innerhalb derselben Community auflösbar sein.

Zentrale Symbole, Entrypoints und Tests sind rankgeordnet, ID-eindeutig und begrenzt; ausgelassene
Tails bleiben durch `truncated` sichtbar. Die Repository Card enthält die kanonische Menge der
Primärmodule als Pakete, beobachtete Sprachfamilien, globale Entrypoints sowie Datei- und
Symbolzahlen. Ihr Entrypoint-Präfix darf nur bei sichtbarer Trunkierung eine echte Teilmenge der
primären Moduleinträge sein. `IndexPublication` prüft Graph-, Rank-, Manifest- und Modulprojektion
gemeinsam, bevor der Storageadapter sie atomar veröffentlichen darf.

### ModuleCardSchema und ExplorePlan

`ModuleCardSchemaVersion::V1` beschreibt zwölf begrenzte Fachfelder sowie die verpflichtende
Envelope aus `ModuleCardId`, `ModuleId`, `SnapshotId`, Evidence-IDs, Confidence,
`MapperProfileVersion` und `ModuleCardStatus`. Jedes nicht leere Fachfeld verlangt Evidenz. Das
Gesamtdokument ist vor strukturierter Validierung auf 64 KiB, die kanonische Evidenzmenge auf 512
IDs und jedes Feld zusätzlich auf eine eigene Item- und Bytezahl begrenzt. Confidence bleibt von
Proposal-, Verified-, Published-, Stale- und NeedsReview-Status getrennt.

`IndexInvalidationPlan` entscheidet den Lebenszyklus rein aus einem aktuellen `PublishedIndex`,
der Mapperprofilversion und adaptergeprüften Card-Kandidaten. Geänderte direkte Evidence sowie
inkompatible Parser- oder Mapperversionen setzen die eigene neueste Card auf `Stale`; entfernte
Module werden stale, aber nicht erneut eingeplant. Ausschließlich direkte Graphabhängige erhalten
`NeedsReview`. `RemapRequest` macht Priorität und Grund zu einer gültigen Kombination: direkte
Requests tragen Evidence-, Parser- oder Mapperänderung, abhängige Requests ausschließlich
`DirectDependencyChanged`. Sortierung, Modul-Eindeutigkeit und feste Kandidaten-/Seitengrenzen sind
Domänen- beziehungsweise Application-Invarianten.

`ModuleCoverageSnapshot` bindet bereits verifizierte Feld-Coverage an Snapshot und Schemaversion.
Der `DeepMapPlanner` akzeptiert ausschließlich einen `PublishedIndex` und überspringt Module, deren
Muss-Felder bereits vollständig belegt sind. Ein `ExplorePlan` behält IndexRun, Snapshot, Schema,
Plannerpolicy, Hartbudget, reservierte Gesamtkosten, geordnete Schritte und den abschließenden
Stopgrund. Jeder Schritt nennt Ziel, erwartetes Ergebnis als Feldmenge, exakte Evidenzanforderung,
Verifikationsmethode, Status und positive Token-, Zeit- und Toolkosten.

Seed-Ranking und erwarteter Informationsgewinn verwenden ausschließlich Manifestrevisionen,
Entrypoints, rankgeordnete zentrale Symbole, Test-Roots, Graphcommunities und fehlende Coverage aus
der veröffentlichten Modulprojektion. Höchstens 16.384 Kandidaten bleiben nach deterministischem
Top-K erhalten. Addition und Auswahl erfolgen geprüft in allen drei Budgetdimensionen; ein Plan
kann daher sein eigenes Budget konstruktiv nicht überschreiten.

### ExplorerAction, ModuleCardProposal und ExplorerCheckpoint

`ExplorerActionSchemaVersion::V1` begrenzt die Modellausgabe auf die geschlossene Union
`Inspect`, `Search` und `Propose`. `Inspect` kann ausschließlich das vom aktuellen Planschritt
vorgegebene Ziel lesen. `Search` unterscheidet typseitig Exact-/Lexical-Text von den
symbolgebundenen Graphpresets Callers, Callees, Imports, Exports und Tests; jede Leseanforderung
trägt einen begrenzten erwarteten Informationsgewinn und eine kontrollzeichenfreie Begründung.
Schreib-, Prozess-, Shell-, Git- oder generische Toolaktionen sind in dieser Union nicht
darstellbar.

Ein `ModuleCardProposal` besteht aus einer typisierten Envelope und kanonisch geordneten
`ProposedModuleCardField`s. Leere Felder werden nicht dargestellt; jedes vorhandene Feld benötigt
mindestens eine eigene `ModuleCardEvidenceId`, und Werte, Duplikate, UTF-8-Bytes sowie die
vereinigte Evidenzmenge werden gegen `ModuleCardSchema::V1` geprüft. Der Typ liefert ausschließlich
`ModuleCardStatus::Proposed`; er kann weder Verification noch Fact-, Observation- oder
Hypothesis-Status vergeben. Der R9-Verifier löst diese Evidenz gegen genau einen atomar
veröffentlichten Indexlauf auf und erzeugt erst danach einen verifizierten Zustand.

`ExplorerCheckpoint` bindet Run, Snapshot, Card-Schema und Plannerpolicy an ein lückenloses Präfix
bestätigter Vorschläge. Eine Bestätigung ist nur für den nächsten Planschritt möglich und muss
Modul, Snapshot, Schema und alle erwarteten Felder treffen. Deshalb beginnt Resume exakt beim
ersten unbestätigten Schritt, während ein Checkpoint eines anderen Plans abgelehnt wird.

### ModuleCardVerificationCandidate und VerifiedModuleCardBatch

Ein `ModuleCardVerificationCandidate` bindet genau einen typisierten Claim an jeden einzelnen
Feldwert eines `ModuleCardProposal`. Claim-, Card-, Modul- und Snapshotidentität müssen
übereinstimmen; Claim-Evidence muss eine Teilmenge der Evidence IDs des betroffenen Felds sein.
Prüfbare Prädikate sind aktuelle Repository-Pfade, strukturelle Symbol-IDs sowie exakte Import-,
Export-, Call- und Testkanten. Freie Beobachtung und Architekturabsicht sind getrennte
Prädikate, sodass Prosa nie als struktureller Graphbeweis interpretiert wird.

`ModuleClaimSchemaVersion::V1` versioniert die zugehörige strikte JSON-Grenze unabhängig vom
Card-Schema. Der Decoder übernimmt die bereits validierte Card als Capability und lehnt ein
abweichendes Card-, Modul- oder Snapshot-Envelope sowie fehlende, doppelte oder zusätzliche
Feldwertclaims ab, bevor Evidence-Auflösung möglich ist.

`ModuleCardEvidenceId` wird domänensepariert aus der exakten File Revision, Symbol-ID oder
vollständigen Graphkante abgeleitet. Der Resolver liest nur den letzten atomar publizierten
Knowledge Index und akzeptiert ausschließlich die angeforderte Kombination aus `IndexRunId` und
`SnapshotId`. Der Verifier vergleicht jedes aufgelöste Objekt nochmals mit demselben
`PublishedIndex`; fehlende, zusätzliche, stale oder erfundene Evidence wird abgelehnt.

Nur positive, exakt passende Strukturclaims werden `Fact`. Direkte aktuelle Beobachtungen werden
`Observation`; Architekturabsicht und durch bloße Abwesenheit nicht beweisbare negative Claims
bleiben `Hypothesis`. `Confidence` bleibt davon sowie vom Lebenszyklusstatus `Active` unabhängig.
Gegensätzliche strukturierte Claims erzeugen einen sichtbaren Widerspruchsbericht und keine
zusammengeführte Card. Erst der nicht öffentlich konstruierbare `VerifiedModuleCardBatch` darf die
verified-only Publish-Grenze passieren.

Der Batch behält neben den Cards die exakt verifizierten Evidence-Objekte. Damit kann ein
Storageadapter vollständige Provenienz atomar persistieren, ohne Evidence IDs nachträglich aus
Prosa oder einem möglicherweise neueren Index zu rekonstruieren. Publication ist begrenzt,
abbrechbar und fortschrittsmeldend; ein erfolgreicher Commit ist der einzige Übergang zur
dauerhaften Published-Repräsentation.

### Exact Retrieval

Eine `ExactSearchQuery` wählt entweder einen normalisierten Repository-Pfad, einen begrenzten
Identifier-/Signaturtext oder die strukturelle Rolle Manifest, Entrypoint beziehungsweise Test. Ein
Treffer enthält stets `SourceChannel::Exact`, eine maschinenlesbare `ExactSearchExplanation` und die
aktuelle `FileRevision`; Symboltreffer tragen zusätzlich die containment-abgeleitete
`QualifiedSymbolName` und den `GraphSymbol`.

Resultate gehören exakt zu einer atomisch veröffentlichten Kombination aus `IndexRunId` und
`SnapshotId`. Die Reihenfolge ist Matchklasse, rohe Repository-Pfadbytes, qualifizierter Name und
`SymbolId`. Ein `ExactSearchCursor` enthält diesen letzten Keyset-Schlüssel sowie Query, Run und
Snapshot. Er kann deshalb weder für eine andere Query noch nach einem neueren Publish weitergenutzt
werden. Exakte qualifizierte Namen stehen vor exakten einfachen Namen und Signaturen; danach folgen
deren Präfixtreffer. Pfadsuche vergleicht normalisierte Rohbytes und erzeugt keinen Textpfad.

### Lexical Retrieval

Eine `LexicalSearchQuery` enthält höchstens 4 KiB einzeiligen, untrusted Text und zwischen einem und
32 jeweils mindestens drei Zeichen lange alphanumerische oder `_`-Tokens. Sie ist niemals selbst
FTS-Syntax. Der Adapter
erzeugt daraus eine begrenzte Trigram-Abfrage, bindet sie als SQL-Parameter und bewertet höchstens
512 vorselektierte Kandidaten je Resultatklasse nochmals deterministisch. Symbolname,
qualifizierter Name, Signatur und Pfad tragen die Gewichte 10, 8, 6 und 4; der stärkste Anteil wird
als `LexicalSearchExplanation` mit ausgegeben.

Jeder `LexicalSearchHit` trägt `SourceChannel::Lexical`, einen ganzzahligen Score und eine aktuelle
`FileRevision`. Die stabile Reihenfolge ist absteigender Score, Zielklasse, rohe Pfadbytes,
qualifizierter Name und `SymbolId`. `LexicalSearchCursor` bindet diesen vollständigen Schlüssel an
Query, `IndexRunId` und `SnapshotId`; ein Replacement-Publish macht ihn dadurch ungültig.

### Graph Retrieval

Eine `TraversalQuery` bindet genau einen aktuellen Datei- oder Symbolendpunkt an Richtung,
`SyntaxRelationKind`, positive maximale Tiefe und Resultlimit. V1 erlaubt ausschließlich ein oder
zwei Hops und höchstens 100 Treffer; die Presets `callers`, `callees`, `imports`, `exports` und
`tests` setzen die jeweilige Richtung und Beziehung explizit. Testbeziehungen liefern
`SourceChannel::Test`, alle übrigen Traversals `SourceChannel::Graph`.

Ein `GraphTraversalHit` enthält das aktuelle `ExactSearchTarget` und den vollständigen geordneten
`GraphEdge`-Pfad vom Seed zum Ziel. Jede Kante muss Relation und `SnapshotId` der Query entsprechen,
an die vorige Kante anschließen und darf keinen Endpunkt erneut besuchen. Der Adapter ermittelt
Treffer levelweise; damit gewinnt bei mehreren Wegen der kürzeste Pfad und innerhalb derselben
Tiefe die kanonische Kantenreihenfolge. Ein `GraphTraversalResult` gehört genau zu einer
`IndexRunId`-/`SnapshotId`-Kombination, enthält keine doppelten Ziele und weist ein abgeschnittenes
Resultat explizit aus.

### Retrieval Fusion

`RetrievalCandidateSet` trennt Exact, Lexical, Graph, Test, Memory und Semantic bereits vor der
Fusion und bindet jede Menge an genau eine `IndexRunId`-/`SnapshotId`-Kombination. Pro Kanal sind
höchstens 100 eindeutige Ziele zulässig. `RetrievalCandidateSets` akzeptiert höchstens eine Menge je
Kanal und lehnt gemischte Publikationen ab. Dateien werden innerhalb eines Runs über ihre
verlustfreien `RepositoryPath`-Bytes, Symbole über `SymbolId` dedupliziert; abweichende Revisionen,
Projektionen oder Signale für dieselbe stabile ID sind ein typisierter Fehler. Der typisierte
Mengenstatus `Complete` oder `Truncated` übernimmt vorgelagerte Cursor und Graphlimits; kein
Endergebnis darf eine bereits abgeschnittene Quelle als vollständig darstellen.

Jeder Kandidat trägt getrennte normalisierte Goal- und Step-Relevanz, ausschließlich `Current` oder
explizit `Compatible` Freshness, eine positive begrenzte Tokenkostenschätzung und ein
Redundanzsignal. Die kanalnative Begründung bleibt erhalten: Exact-Erklärung, Lexical-Feld und
-Score, vollständiger Graph-/Testpfad, fresh Memory-Relevanz mit einer nicht leeren auf 16
begrenzten `EvidenceRef`-Menge oder ausdrücklich nicht beweisende Semantic Similarity.

`FusionPolicy::v1` ordnet zuerst das unverhandelbare Provenienzband `Exact`, dann `Evidence` und
zuletzt `Semantic`. Kein gewichteter Semantic-Score kann deshalb einen Exact-Treffer verdrängen.
Innerhalb eines Bands berechnet die Policy ausschließlich mit Integerarithmetik einen Score aus
Kanal 30 %, Goal 20 %, Step 20 %, Freshness 10 %, Token-Effizienz 10 % und unabhängiger
nicht-semantischer Bestätigung 10 %; Redundanz zieht bis zu 20 % ab. Gleichstände löst die stabile
Ziel-ID auf. `ResultExplanation` bewahrt jeden Normalwert, Beitrag, Abzug und Quellgrund;
`FusedRetrievalResult` speichert außerdem Run, Snapshot, `FusionPolicyVersion`, Trunkierung und die
finale Reihenfolge.

### Task Lens

`TaskLensSeedSet` hält den nicht leeren Goal- und Step-Anker sowie höchstens 64 kanonisch geordnete
explizite Pfad-, Symbol-, Identifier-, Diagnose-, Change-, Hypothesen- und fehlgeschlagene
Verification-Seeds. Seedtext ist auf vier KiB begrenzt, normalisiert Zeilenenden und bleibt in
Debugausgaben redigiert. `TaskLensPolicy::v1` akzeptiert ausschließlich einen `PublishedIndex` und
eine R4-Fusionsausgabe desselben Runs und Snapshots.

Die Auswahl beginnt mit der verpflichtenden L0-Repository Card und ergänzt nur Module, Symbole und
Source Spans tatsächlich fusionierter oder durch aktuelle Claims verbundener Ziele. L0 bis L3 sind
unterschiedliche Typvarianten; höchstens acht Module und 64 Einträge passen in ein konfigurierbares
Budget von 256 bis 32.768 geschätzten Tokens. Der konservative Fallback berechnet strukturellen
Overhead plus höchstens ein Token je UTF-8-Byte. Ein zu großes Detail wird sichtbar trunkiert, ohne
bereits ausgewählte gröbere Zoomstufen zu verwerfen.

Eine `TaskLensClaim`-Projektion rekonstruiert Classification, Status und Confidence getrennt und
prüft die R9-Zuweisung erneut. Jeder nicht ausschließlich auf Architekturabsicht beruhende Claim
verlangt weiterhin aktuelle aufgelöste Evidence; Fact verlangt zusätzlich ein positives
strukturelles Prädikat und mindestens eine exakt passende Evidence. Vor Aufnahme vergleicht der
Compiler Claim-Run, Snapshot, Modul und jede File-, Symbol- oder Graphkanten-Evidence nochmals mit
dem veröffentlichten Index. Stale oder inkompatible Claims werden gezählt und ausgeschlossen,
niemals als Fakten materialisiert. Ein begrenztes Claim-Read bleibt durch einen eigenen
Trunkierungszustand in Lens und Digest sichtbar.

`LensDigest` ist domänensepariert und umfasst Task-Lens- und Fusionpolicy, Budget, kanonische
Seedmenge, Run, Snapshot, geordnete Einträge, aktuelle Claims, Trunkierung und den Stale-Zähler.
Identische Eingaben ergeben denselben Digest; ein neuer veröffentlichter Indexlauf macht die alte
Lens konstruktiv unaktuell und erzeugt beim Recompile eine andere Identität.

Die Application erhält den vollständigen Index als geteilte, unveränderliche Capability statt als
tiefe Kopie. Diese Laufzeitrepräsentation ändert keine Domainidentität: Run und Snapshot werden vor
jeder Ausgabe erneut gegen die dauerhafte aktuelle Publikation geprüft.

### Context Pack

`ContextCompilerPolicyVersion::V1`, `ContextBudgetPlan`, `ContextBudgetUsage` und `ContextDigest`
sind infrastrukturfreie Domainwerte. Der Plan legt für jedes Modellprofil harte Grenzen für
System/Tools, Goal/Ledger, Project Map, Code/Evidence und aktuelle Toolresultate sowie getrennte
Sicherheits- und Outputreserven fest. Eine beobachtete Nutzung ist nur konstruierbar, wenn jede
Sektion und die Gesamtrechnung innerhalb dieser Grenzen liegen.

`AgentContextCompileInput` bindet im Application-Layer genau einen Goal Contract an seine passende
Task-Ledger-Revision, einen aktiven aktuellen Schritt, ein Modellprofil, kanonische optionale Seeds
und höchstens 64 journalgeordnete normalisierte Toolresultate. `CompiledAgentContext` gibt nur
einen providerneutralen `ModelProviderRequest` samt Policy-, Goal-, Ledger-, Step-, Index-,
Snapshot-, Task-Lens-, Budget- und Digestidentität aus. Dadurch kann der spätere Controller keinen
Context Pack ohne nachvollziehbare Ausgangsevidenz als aktuellen Turn verwenden.

## Aggregate

### Project

Verwaltet Repository- und Worktree-Identität, aktive Snapshot-Generation, Projektkonfiguration und Indexzustand.

Invarianten:

- Es gibt höchstens einen veröffentlichten Indexstand pro Snapshot und Policy-Version.
- Ein veröffentlichter Stand verweist nur auf vollständig committed Indexdaten.
- Projektzustand ist auch ohne Modellprovider lesbar.

### Knowledge Base

Verwaltet Dateien, Symbole, Kanten, Module, Cards, Claims und Evidenz.

Invarianten:

- Jeder Claim hat mindestens eine EvidenceRef oder ist ausdrücklich als Hypothesis markiert.
- Fact benötigt deterministisch verifizierte Evidenz.
- Ein ungültiger EvidenceRef macht abhängige Claims stale.
- Embeddings sind abgeleitete Beschleunigungsdaten, keine Evidenz.

### Optionale Semantik

`NormalizedSemanticCard` gehört genau einem `SnapshotId`, besitzt eine logische
`SemanticCardId`, eine `SemanticCardNormalizationVersion` und einen aus dem kanonischen Körper
domänensepariert abgeleiteten `BodyHash`. Normalisierung V1 vereinheitlicht Zeilenenden und
Whitespace, entfernt leere Ränder, begrenzt Rohinput auf 64 KiB sowie den kanonischen Körper auf
16 KiB und lehnt sonstige Steuerzeichen ab. Ein `SemanticCardBatch` enthält höchstens 512 Karten
desselben Snapshots und keine doppelte logische ID.

`EmbeddingModelProfile` ist die embedding-spezifische Capability-Projektion vor dem späteren
allgemeinen LLM-`ModelProfile`. V1 bindet Provider-ID, opaque Modell-ID, 1 bis 8.192 Dimensionen,
Float32, keine Quantisierung und L2-Einheitsnormalisierung an eine abgeleitete `ModelProfileId`.
Provider-Batchgröße und Requesttimeout sind Betriebsgrenzen und ändern diese
Vektorkompatibilitäts-ID nicht. `EmbeddingVector` kann nur mit exakt passender Dimension,
endlichen Komponenten und von null verschiedener Norm entstehen; seine Komponenten werden in
Debugausgaben nicht offengelegt.

Der regenerierbare Cache-Schlüssel ist exakt `(SemanticCardId, ModelProfileId, BodyHash)`.
`VectorHit` trägt Card-, Body- und Profilidentität sowie normalisierte Similarity, aber keine
`EvidenceRef`; sein einziger `SourceChannel` ist `Semantic`. `VectorSearchResult` bindet Treffer an
Snapshot, Profil, Capability und Resultlimit und kanonisiert Gleichstände über Card- und Body-ID.
`VectorSearchCapability::Indexed` bedeutet, dass eine dimensionskompatible libSQL-DiskANN-
Projektion alle Kandidaten des begrenzten Korridors erzeugt hat; `LinearFallback` kennzeichnet die
deterministische direkte Cosine-Berechnung. Beide Pfade erzeugen denselben nicht beweisenden
Treffertyp und markieren ausgelassene Kandidaten explizit als Trunkierung.

### Model Provider

`ModelProviderId` ist eine begrenzte, credential- und endpointfreie Provideridentität. `ModelId`
ist eine begrenzte opaque Provideridentität; ihr Name beweist keine Capability. Der allgemeine
Application-Port nimmt ausschließlich einen `ModelProviderRequest`, ein Gesamttimeout und eine
wakebare Cancellation-Grenze entgegen. Ein Request enthält 1 bis 256 geordnete System-, User- oder
Assistant-Nachrichten, höchstens 2 MiB Text sowie optional ein JSON-Objektschema bis 64 KiB. Inhalte
werden in Debugprojektionen nur als Anzahl und Byteumfang dargestellt. Jeder Request trägt das
vollständige versionierte `ModelProfile`; ein Adapter lehnt eine fremde Provider-ID ab. Ein
Structured-Output-Schema ist bereits an dieser neutralen Grenze nur mit live verifiziertem
Capabilitystatus konstruierbar.

Ein erfolgreicher Stream besteht aus geordneten `ProviderEvent::OutputText`-Fragmenten von jeweils
höchstens 64 KiB und genau einem `Completed`-Event mit normalisiertem Stopgrund und optionalen
Providerzählern. Der Ollama-kompatible Adapter begrenzt zusätzlich eine einzelne NDJSON-Zeile auf
128 KiB, den Puffer auf 256 KiB und den gesamten Textoutput auf 4 MiB. Ein Abschluss wird erst nach
sauberem Body-Ende sichtbar; fehlender Abschluss, abweichendes Modell, Tool Calls im H4-Textmodus,
ungültige Rollen oder Daten nach `done` sind keine ausführbare Ausgabe.

Fehler werden ausschließlich als `Unavailable`, `Rejected`, `InvalidResponse`, `TimedOut`,
`Cancelled` oder `EndpointDenied` über die Adaptergrenze gegeben. Provider-Fehlertexte,
HTTP-Payloads und Endpoints sind kein Teil dieser Typen.

`ModelProfile` V1 bindet Provider- und opaque Modell-ID an effektives Kontext- und Outputlimit,
konservative Tokenzählung, Parallelitätslimit, fixed-point Temperatur und Top-p, kanonische
Stopbedingungen, Schema-Grounding, expliziten Toolmodus und das Ergebnis einer echten
Structured-Output-Probe. Alle runformenden Felder, Capabilitystatus und die Quellrevision fließen
domänensepariert in `ModelProfileId` ein. Nur `Verified` aktiviert ausführbare strukturierte
Aktionen. Ein manueller Override kann Limits und Laufparameter ändern, übernimmt aber den
Capabilitystatus unverändert und kann eine fehlgeschlagene Probe nicht hochstufen. Die V1-
Fallbackzählung bewertet jedes UTF-8-Byte als ein Token und ist damit deterministisch und
tokenizerunabhängig konservativ; Stoptexte bleiben in Debugausgaben redigiert.

Der Application-Port `ModelCapabilityProbe` erhält ausschließlich opaque Modell-ID, validierte
Profileinstellungen, Gesamttimeout und Cancellation. `ProbeModelProfile` erzeugt aus seiner
providerneutralen Beobachtung das vollständige Profil und lehnt ein konfiguriertes Kontextlimit
oberhalb expliziter Providermetadaten ab. Toolfähigkeit wird nur aus einem exakten Providermerkmal
übernommen; `NativeProviderReported` allein aktiviert keine ausführbare Aktion.

Der Ollama-Adapter liest zuerst die begrenzte `/api/show`-Antwort und sucht genau ein eindeutiges
numerisches `*.context_length` sowie die exakte Capability `tools`. Anschließend sendet er unter
derselben Gesamtdauer eine reale nicht streamende `/api/chat`-Anfrage mit einem kleinen strikten
Schema und einer auf 4.096 Kontext- sowie 32 Outputtokens begrenzten Probe. Nur Modell, Assistant-
Rolle, terminaler Zustand und das exakte Objekt `{"a3_probe":"ok"}` ergeben `Verified`; abgelehnte,
formal ungültige oder schemawidrige Antworten ergeben `Unavailable`. Normale Chatrequests bilden
Kontext- und Outputlimit, fixed-point Temperatur, Top-p und kanonische Stopbedingungen aus dem
Profil auf Ollama-Optionen ab.

### AgentAction und Promptvertrag

`AgentActionSchemaVersion::V1` ist die geschlossene strukturierte Modellausgabe für die read-only
Harnessphase. Die Union enthält ausschließlich `Search`, `Inspect`, `UpdateLedger` und `Finish`.
`Search` übergibt eine begrenzte Query und ein Limit an die spätere deterministische Retrieval-
Pipeline, ohne einen Vertrauenskanal wählen zu können. `Inspect` adressiert genau eine begrenzte
Dateiseite, eine Symbol-ID, eine typisierte Graphtraversierung, eine Claim-ID oder einen
Testselektor. Datei-, Test-, Query- und Ledgertexte sind normalisiert, bytebegrenzt und in Debug-
Ausgaben redigiert.

`UpdateLedger` kann nur ein nicht autoritatives Resultat vormerken, einen Blocker melden oder einen
Replan anfordern. Kein Variant kann Verifikation oder Completion setzen. `Finish` enthält keine
Modellbehauptung und fordert lediglich die spätere deterministische Acceptance-Verifikation an.
Patch, Prozess, Shell, Git, Netzwerk, Publishing und destruktive Aktionen sind in V1 nicht
darstellbar; die eigentlichen read-only Ports sowie Zustandsautorisierung folgen in H10 und H9.

Das eingebettete `agent-action-v1`-JSON-Schema setzt auf jeder Objektebene
`additionalProperties: false`; ein separater Runtime-Decoder prüft das vollständige Dokument bis
64 KiB erneut gegen exakte Schlüssel, Version, lowercase IDs, sichere Pfade, Zahlen- und
Textgrenzen sowie Domaininvarianten. Der statische Systemvertrag kostet mit der konservativen
V1-Zählung weniger als 900 Tokens und kann nur für ein ModelProfile mit live verifiziertem
Structured Output vorbereitet werden. Profilabhängiges Schema-Grounding wiederholt bei Bedarf
dieselbe kanonische Schemafassung. Ein ungültiges Primärergebnis erzeugt genau eine nicht clonebare,
verbrauchbare Repair-Befugnis mit ausschließlich content-freiem Fehlercode. Auch deren ungültiges
Ergebnis ist terminal und erzeugt keine weitere Befugnis.

### Task

Verwaltet Goal Contract, Akzeptanzkriterien, Schritte, Entscheidungen, Runs und Abschluss.

Der implementierte H1-Schnitt verwendet `TaskId` und `AcceptanceCriterionId` als 32-Byte-Newtypes.
`GoalContractDraft` fasst normalisierte, begrenzte Fachtypen für Objective, Acceptance Criteria,
Constraints, Non-Goals, User Decisions und Success Verification zusammen. `GoalContract` ist eine
einzelne unveränderliche Revision; `GoalContractHistory` akzeptiert ausschließlich eine
lückenlose, zeitlich monotone Folge desselben Tasks. `GoalContractReference` kann nur aus einem
validen Contract entstehen und bindet jeden späteren Run an eine konkrete Revision.

Invarianten:

- Ein initialer Goal Contract ist Revision eins ohne Vorgänger und Revisionsbegründung.
- Der Goal Contract wird nach Start nicht still verändert. Eine materielle Änderung erzeugt exakt
  die nächste Revision mit Vorgänger, Begründung und nicht rückläufigem Zeitstempel.
- Mindestens ein und höchstens 64 eindeutige Acceptance Criteria sind Pflicht; Constraints,
  Non-Goals und User Decisions sind optional, eindeutig und jeweils auf 64 Einträge begrenzt.
- Leere, überlange, nicht normalisierte oder kontrollzeichenhaltige Goal-Texte sind nicht
  konstruierbar; Debug-Ausgaben legen ihren Inhalt nicht offen.
- Jeder Schritt besitzt Outcome, Status und VerificationSpec.
- Completed benötigt erfolgreiche Verification.
- Ein Task ist Done, wenn alle Muss-Akzeptanzkriterien aktuell verifiziert sind und keine blockierende offene Hypothese existiert.

### Agent Run

Verwaltet Zustandsmaschine, Turnnummer, Context Pack, Tool Action, Events, Budgets und Abbruch.
Der implementierte H3-Kern bindet jeden `AgentRun` an genau eine `GoalContractReference`, die
aktuelle `TaskLedgerRevision` und einen vorhandenen Snapshot. Seit H5 trägt jeder neue Run außerdem
die exakte `ModelProfileReference` aus Profil-ID und Schemaversion. Nur aus V13 migrierte Alt-Runs
dürfen diesen Bezug explizit gemeinsam leer lassen. `RunEventSequence` beginnt bei eins und kann
ausschließlich lückenlos wachsen. Jeder erlaubte Zustandsübergang und jeder Replan erzeugt ein
typisiertes Event und aktualisiert zugleich die in-memory Materialisierung; terminale Runs
akzeptieren keine weiteren Events.

H9 ergänzt ein am Start unveränderliches `AgentRunBudget` für Turns, Prompt-/Outputtokens,
Actionen, Laufzeit und Structured-Output-Repairs. Jeder `ModelInteraction`-Event besitzt eine
`AgentTurnCharge`; ihr optionales `AgentTurnActionClass` kann strukturell nur null oder genau eine
Actionklasse enthalten. `AgentRunUsage` wird mit dem Event angewendet und persistent mit derselben
Sequenz-CAS-Transaktion materialisiert. Erschöpfung wird in der festen Reihenfolge Zeit, Turns,
Prompttokens, Outputtokens, Actionen und Repairs ausgewertet und bleibt ab dem Grenzwert sichtbar.

Invarianten:

- Pro Turn gibt es höchstens eine ausführbare Tool Action.
- Jeder Turn erhöht genau einen Turnzähler; Token-, Action- und Repairverbrauch überleben Reopen.
- Mutierende Tool Actions werden serialisiert.
- Ein Turn verweist auf genau einen Snapshot.
- Vor einer Mutation wird geprüft, ob der erwartete Snapshot noch aktuell ist.
- Abschluss ist ein expliziter Zustandsübergang und keine bloße Textausgabe.

### Run Memory

`RunMemoryCheckpoint` ist die deterministische, regenerierbare H8-Projektion für den nächsten
Context Pack und kein zweites Run-Aggregat. Er bindet exakt eine Goal-Referenz, Ledgerrevision,
Run-ID, Event-Watermark, IndexRun-ID und Snapshot-ID. `CompactedStepResult` verweist immer auf die
ursprüngliche `StepResultSource` aus Step-ID, Attemptnummer und Run-ID und behält direkte sowie
Verifikations-Evidence-IDs. `CompactedRunClaim` kapselt den originalen frischen `TaskLensClaim`;
eine zuvor erzeugte Summary kann nicht als Claim- oder Step-Quelle eingesetzt werden.

`OpenRunIssue` hält fehlgeschlagene Verifikation, Blocked, AwaitingApproval, Failed, Cancelled und
Stale getrennt. Aktive Hypothesen bleiben ebenfalls typisiert und werden nicht durch Confidence
zu Facts hochgestuft. Der `RunMemoryDigest` bindet den vollständigen normalisierten Inhalt und die
Policyversion. Weil die Projektion nur aus unveränderlichen autoritativen Referenzen kompiliert
wird, verändert sie weder Ledger noch append-only Audit-Events.

## Claim-Typen

| Typ | Bedeutung | Darf als Wahrheit in Kontext? |
| --- | --- | --- |
| Fact | deterministisch verifizierte Aussage | ja, solange Evidenz frisch ist |
| Observation | direkt beobachtetes Tool- oder Codeergebnis | ja, mit Herkunft und Aktualität |
| Decision | bewusst getroffene Projekt- oder Task-Entscheidung | ja, bis ersetzt |
| Hypothesis | plausible, noch nicht verifizierte Annahme | nur deutlich markiert |
| Summary | abgeleitete Verdichtung anderer Einträge | nur mit erhaltenen Source IDs |

Statuswerte: Active, Stale, Superseded, Refuted.

Ein Claim kann nur durch eine deterministische Prüfung oder eine bestätigte menschliche Entscheidung zu Fact beziehungsweise Decision werden. Das LLM darf den Status lediglich vorschlagen.

## Task-Schritte

Statuswerte:

- Pending
- Ready
- InProgress
- Blocked
- AwaitingApproval
- Verifying
- Completed
- Failed
- Cancelled
- Stale

Zulässige Kernübergänge:

~~~mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> Ready
    Ready --> InProgress
    InProgress --> Verifying
    Verifying --> Completed
    Verifying --> Ready
    InProgress --> Blocked
    InProgress --> AwaitingApproval
    Completed --> Stale
    Stale --> Ready
~~~

Completed → Stale ist verpflichtend, wenn eine für die Verifikation notwendige Evidenz ungültig wird.

## Agenten-Zustandsmaschine

~~~mermaid
stateDiagram-v2
    [*] --> Intake
    Intake --> Localize
    Localize --> Plan
    Plan --> Execute
    Execute --> Verify
    Verify --> Execute
    Verify --> Replan
    Replan --> Localize
    Execute --> AwaitApproval
    AwaitApproval --> Execute
    Verify --> Done
    Intake --> Failed
    Localize --> Failed
    Plan --> Failed
    Execute --> Failed
    Verify --> Failed
~~~

Cancelled ist aus jedem aktiven Zustand möglich. Done und Failed sind terminal. Ein neuer Versuch erzeugt einen neuen Run.

## Events

RunEvent ist ein append-only Audit-Eintrag. Mindestfelder:

- EventId
- RunId
- Sequenznummer
- Zeit
- Eventtyp
- sichere strukturierte Nutzlast
- SnapshotId
- optional ToolRunId oder EvidenceId

Die Eventfolge ist kein vollständiges Event-Sourcing des Produkts. Fachzustand wird relational materialisiert; das Journal dient Audit, Debugging und reproduzierbarer Laufanalyse.

`RunEventPayload` V1 besitzt keinen Freitextkanal. Sie enthält ausschließlich einen geschlossenen
`RunEventCode`, ein optionales grobes Outcome, optionale `RunEventRedaction` aus Quellkategorie,
beobachteter Bytezahl und Trunkierungsflag sowie einen domain-separierten Digest dieser sicheren
Felder. Ein `RunEventSubject` kann nur eine typisierte `ToolRunId` oder `TaskEvidenceId` sein.
Roher User-, Repository-, Modell-, Tool- oder Fehlertext kann daher weder persistiert noch über das
stabile JSONL-Exportformat rekonstruiert werden.

`a3.run-journal.jsonl` V2 ergänzt im Header das unveränderliche Runbudget und die kumulative
Nutzung sowie je Event optionale content-freie Turn-Charges. Alte V1-Felder bleiben unverändert;
Rohtext oder Providerpayloads werden auch durch die neuen Felder nicht darstellbar.

Die V1-Retention `PreserveAuditEvents` ist nicht destruktiv: Das bereits content-freie Journal bleibt
als Audit erhalten. `agent_runs` ist eine unabhängige relationale Projektion und kann ohne Replay
des Journalinhalts rekonstruiert werden; vor einem neuen Append prüft der Adapter dagegen den
aktuellen Eventtail und führt Eventinsert plus Zustands-CAS atomar aus.
