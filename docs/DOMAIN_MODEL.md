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

Die Desktopprojektion `ModuleCardCoverage` rekonstruiert dieselbe Schemabedeutung aus den
tatsächlich ausgelieferten verifizierten Card-Feldern. Sie trennt die acht Muss- von den vier
Soll-Feldern, behält kanonisch geordnete Lücken und berechnet jede Quote ganzzahlig in Basispunkten.
Coverage ist weder `Confidence` noch Lebenszyklus: Vollständige Felder können stale sein, und eine
hohe Confidence kann ein fehlendes Muss-Feld nicht ersetzen.

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

Der spätere `ModuleCardEvidenceQuery` bindet genau einen dieser opaken Hooks an aktuellen Run und
Snapshot, historischen Card-Quell-Run und -Snapshot sowie Card und primäres Modul. Das validierte
Read-Model kann ausschließlich `File`, `Symbol` oder `GraphEdge` tragen und leitet seine
`ModuleCardEvidenceId` erneut aus dieser Payload ab. Der Card-Lifecycle bleibt dabei unabhängig von
`ModuleCardEvidenceFreshness`: Eine historisch persistierte Graphkante kann für eine stale Card als
stale Provenienz sichtbar bleiben, ist aber kein aktueller Graphbeweis. `Current` verlangt dagegen
eine exakte Auflösung derselben Payload im gegenwärtigen Published Index.

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

`ContextCompilerPolicyVersion`, `ContextBudgetPlan`, `ContextBudgetUsage` und `ContextDigest`
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

Policy V2 behält die V1-Budgets bei, reserviert aber den unverkürzbaren L0-Repository-Anchor vor
optionalen gerankten L1-/L2-Details. L0 trägt kompakte Datei-, Symbol-, Package- und Entrypoint-
Anzahlen; konkrete Modul- und Symbolidentitäten bleiben in L1/L2. Die Policyversion und die neue
Digest-Domäne verhindern, dass V1- und V2-Packs dieselbe Identität beanspruchen.

Policy V3 injiziert zusätzlich Must-/Should-Klassifikation, Criterion-Mapping und alle operationalen
VerificationSpec-Felder in den ungekürzten Anchor. Dafür steigt Goal/Ledger auf 1.100 Tokens und
Code/Evidence sinkt auf 6.800 Tokens; Gesamt-, Sicherheits- und Outputreserve bleiben unverändert.
Die eigene V3-Digest-Domäne verhindert Identitätsgleichheit mit Packs ohne E6-Vertrag.

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

`ProviderModelCatalog` ist eine flüchtige, providerneutrale Projektion von höchstens 256
eindeutigen, kanonisch sortierten `ModelId`s. Der Application-Port `ModelCatalogProvider` erhält
nur die erwartete `ModelProviderId`, Gesamttimeout und Cancellation; Endpoint, Transport und
Providerpayload bleiben im Adapter. Ein Katalogeintrag beweist ausschließlich, dass der
konfigurierte Provider den Namen zum Abfragezeitpunkt gemeldet hat. Er kann weder ein
`ModelProfile` erzeugen noch eine Capability aktivieren.

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

`AgentActionSchemaVersion::V1` bleibt der lesbare historische Vertrag der read-only Harnessphase
mit `Search`, `Inspect`, `UpdateLedger` und `Finish`. `AgentActionSchemaVersion::V2` ist der aktuelle
geschlossene Vertrag und ergänzt ausschließlich `ApplyPatch` und `Run`.
`Search` übergibt eine begrenzte Query und ein Limit an die spätere deterministische Retrieval-
Pipeline, ohne einen Vertrauenskanal wählen zu können. `Inspect` adressiert genau eine begrenzte
Dateiseite, eine Symbol-ID, eine typisierte Graphtraversierung, eine Claim-ID oder einen
Testselektor. Datei-, Test-, Query- und Ledgertexte sind normalisiert, bytebegrenzt und in Debug-
Ausgaben redigiert.

`UpdateLedger` kann nur ein nicht autoritatives Resultat vormerken, einen Blocker melden oder einen
Replan anfordern. Kein Variant kann Verifikation oder Completion setzen. `Finish` enthält keine
Modellbehauptung und fordert lediglich die spätere deterministische Acceptance-Verifikation an.
Patch und Prozess sind in V1 nicht darstellbar. V2 bindet `ApplyPatch` vollständig an Run,
Worktree, Published Snapshot, aktuellen TaskStep, VerificationSpec, Rationale, Pfade, erwartete
Hashes und vollständige neue Inhalte. `Run` enthält nur aktuelle `TaskStepId` und
`DiscoveredCommandId`; argv, Shell-, Git-, Netzwerk-, Install- oder Publishdaten können nicht aus
der Modellausgabe entstehen. `ExecuteAgentTurn` autorisiert selbst weiterhin nur den geschlossenen
`AgentReadTools`-Port für `Search` und `Inspect` und gibt beide Mutationstypen unausgeführt an den
E7-Controller weiter.

Die eingebetteten `agent-action-v1`- und `agent-action-v2`-JSON-Schemas setzen auf jeder
Objektebene `additionalProperties: false`; getrennte Runtime-Decoder prüfen das vollständige
Dokument bis 64 KiB erneut gegen exakte Schlüssel, Version, lowercase IDs, sichere Pfade, Zahlen-,
Text- und Patchgrößen sowie Domaininvarianten. V1 bleibt rückwärtskompatibel decodierbar, während
neu kompilierter Kontext ausschließlich V2 verlangt. Der aktuelle statische Systemvertrag kostet
mit der konservativen Zählung weniger als 900 Tokens und kann nur für ein ModelProfile mit live verifiziertem
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

E6 klassifiziert jedes Acceptance Criterion explizit als `Must` oder `Should`; der bestehende
Konstruktor bleibt als Must-Voreinstellung erhalten. Task-Schritte ordnen sich einer kanonischen
Menge konkreter Criterion-IDs derselben Goal-Revision zu. Ein leerer historischer Mappingfall gilt
weiterhin als verpflichtend, kann aber nicht als operationaler E6-Nachweis ausgeführt werden.

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
- Should-Kriterien dürfen unerfüllt bleiben; sie können `Done` nicht blockieren.
- Ein Task ist Done, wenn alle Muss-Akzeptanzkriterien aktuell verifiziert sind und keine blockierende offene Hypothese existiert.

### Policy und Approvals

`PolicyAction` ist eine geschlossene typisierte Union für Root-, Pfad-, Prozess-, Netzwerk- und
Git-Aktionen. `ActionClass` und `RiskLevel` sind daraus abgeleitet und können vom Aufrufer nicht
unabhängig behauptet werden. Die feste `SystemPolicyV1` erlaubt nur begrenzte Reads und Derivationen,
read-only Git sowie plan- und worktreegebundene bekannte argv-Prozesse ohne Netzwerk automatisch.
Alle übrigen privilegierten Aktionen benötigen eine Freigabe. `WorkspacePolicy` kann eine Klasse
ausschließlich auf `ApprovalRequired` oder `Denied` verschärfen; ein Allow oder eine Lockerung ist
in ihrem Typ nicht darstellbar.

Jede Auswertung erzeugt genau eine unveränderliche `PolicyDecision` mit Run, Action-Fingerprint,
Scope-Digest, abgeleiteter Klasse und Risiko, geschlossenem Outcome und Begründung sowie Start-,
Entscheidungs- und Dauermetadaten. `Allowed`, `ApprovalRequired` und `Denied` besitzen disjunkte
gültige Feldformen. Ein benötigter `ApprovalRequest` bindet exakt Run, Action-Fingerprint, Scope,
Klasse und Risiko und läuft spätestens nach 24 Stunden ab.

`ApprovalGrant` übernimmt diese unveränderlichen Anker aus seinem Request. Er ist nur innerhalb des
ursprünglichen Zeitfensters grantbar und kann ausschließlich von `Active` nach `Consumed` oder
`Revoked` wechseln; Ablauf ist ein aus der Beobachtungszeit abgeleiteter effektiver Zustand. Eine
Consumption benötigt denselben Run, dieselbe Action, denselben Scope und eine exakte
`PolicyDecisionId` und ist nur einmal möglich. Mismatch, Ablauf, Widerruf und Wiederverwendung
bleiben blockiert und erzeugen eine neue begründete Entscheidung statt stiller Scope-Erweiterung.

`AgentApprovalPresentation` ist die flüchtige, taskgebundene Verbindung zwischen einem bereits
dauerhaften `ApprovalRequest` und genau der validierten Aktion, über die entschieden wird. Für
Patches enthält sie Rationale sowie jede Add-/Update-/Move-/Delete-Quell- und Zielpfadform; für
Prozesse enthält sie die vollständige `ProcessSpec` mit getrennten argv-Tokens, CWD,
Environment-Allowlist-Namen, Timeout, Outputlimits, Execution Mode, Planbindung, Netzwerkscope und
Specification-ID. Werte aus der Host-Umgebung sind kein Präsentationsfeld. Die Presentation
besitzt eine monotone prozesslokale Revision und kann nur `Pending`, an eine interne Grant-ID
`Granted` oder `Denied` sein.

`AgentApprovalCenter` kombiniert diese Presentation nach erneuter Ankerprüfung mit Request,
optionalem Grant, ausgewähltem Run und Task Ledger. Sein effektiver Status ist `Pending`, `Active`,
`Consumed`, `Revoked`, `Expired` oder `Denied`; zulässige Controls sind daraus disjunkt abgeleitet.
Pending erlaubt ausschließlich AllowOnce oder Deny, Active ausschließlich Continue oder Revoke.
AllowOnce ändert den Run nicht. Deny beendet den aktiven Step-Versuch als `Blocked` und den Run als
`Failed` in einem CAS-Commit. Continue gibt die interne aktive `ApprovalId` nur an den
Composition-Root-Scheduler zurück; sie ist kein IPC-Datentyp.

### Sichere Dateiwerkzeuge

`WorkspaceDirectoryListRequest` bindet genau einen `WorktreeId`, einen veröffentlichten
`SnapshotId`, den Root oder einen normalisierten `RepositoryPath`, einen optionalen exklusiven
Vorwärtscursor und eine `DirectoryPageSize` zwischen eins und 256. Ein Cursor kann konstruktiv nur
ein direktes Kind des angeforderten Verzeichnisses sein. `WorkspaceDirectoryListing` akzeptiert
nur streng sortierte, eindeutige direkte Kinder innerhalb dieser Grenze und koppelt Trunkierung an
den letzten zurückgegebenen Pfad.

`WorkspaceDirectoryEntry` unterscheidet eine direkte Datei von einem aus dem Index abgeleiteten
Verzeichnis. Eine Datei behält ihre eigene aktuelle `FileRevision`; ein Verzeichnis ist nur mit der
`FileRevision` eines strikten Nachfahren konstruierbar. Dadurch bleibt auch ein strukturelles
Listing an konkrete Snapshot-Evidence gebunden und kann keine unbelegte Dateisystembehauptung
erzeugen.

Eine erfolgreiche `AgentSourcePage` leitet ihre `AgentToolEvidence` selbst aus der erneut
bestätigten `FileRevision` und dem tatsächlich gelieferten `SourceRange` ab. Leere EOF-Seiten
verwenden File-Evidence, nicht leere Seiten Span-Evidence. `SecretCandidateClassifierV1`
klassifiziert Private-Key-, Bearer-, GitHub-, AWS- und bekannte Secret-Assignment-Muster, gibt aber
nur eine content-freie Kategorie aus. Binary-, Secret-, Größen-, Encoding-, Stale-, Denied- und
Cancellation-Fälle bleiben getrennte stabile Fehlerzustände; Pfade oder Quelldaten sind in diesen
Fehlern nicht enthalten.

### PatchAction und Change Set

`PatchActionSchemaVersion::V1` ist eine geschlossene Volltextmutation aus höchstens 64
kanonisch geordneten, pfad-disjunkten Add-, Update-, Move- oder Delete-Operationen. Update, Move
und Delete tragen eine vollständige erwartete `FileRevision`; Add verlangt nachweisbare
Abwesenheit. Jeder bereitgestellte Dateiinhalt ist höchstens 4 MiB groß, alle neuen Inhalte
zusammen höchstens 16 MiB. `PatchFileContent` akzeptiert nur secret-geprüftes UTF-8, unterscheidet
UTF-8 mit und ohne BOM und klassifiziert LF, CRLF, CR oder Mixed, ohne ein Byte zu normalisieren.

Eine `PatchAction` bindet `AgentRunId`, `WorktreeId`, `SnapshotId`, `TaskStepId`,
`VerificationSpecId`, eine begrenzte Rationale und den vollständigen Operationssatz. Ihr
`PatchActionDigest` ändert sich bei jedem semantischen Unterschied einschließlich eines erwarteten
oder neuen Content Hashes. Der getrennte `PatchScopeDigest` enthält nur Worktree und betroffene
Pfade. Dadurch kann eine Freigabe denselben Pfadsatz nicht mit anderem Inhalt autorisieren.

`PatchPreviewEntry` besitzt je nach Operation einen Quell- und/oder Zielpfad sowie begrenzte
Vorher-/Nachherinhalte. Ein `PatchContentPreview` behält einen exakten UTF-8-Präfix, aber immer den
Hash, die Bytezahl, Encoding und Line Endings des vollständigen Inhalts. `PatchPreview` validiert
die Eins-zu-eins-Zuordnung zur Action und begrenzt alle Präfixe zusammen auf 64 KiB.

`PatchChange` unterscheidet Added, Updated, Moved und Deleted und enthält die jeweils tatsächlich
sichtbare alte und/oder neue `FileRevision`. Ein `PatchChangeSet` akzeptiert nur die vollständige
Operationsfolge oder bei einem späteren Fehler ein nicht leeres kanonisches Präfix. Es bindet
Action-Digest, Policy-Entscheidung, Run, Worktree, Basissnapshot, TaskStep und Verification und
liefert die eindeutigen geordneten Änderungspfade für nachfolgende Invalidierung. Damit kann weder
ein bloß vorgeschlagenes Ergebnis noch ein gewöhnlicher Fehler bereits ausgeführte Änderungen als
erfolgreiche Gesamtanwendung ausgeben oder verbergen.

### ProcessSpec und ProcessRunResult

`ProcessSpecSchemaVersion::V1` beschreibt genau einen direkten argv-Prozessstart. Die
Spezifikation bindet Run, Worktree, Arbeitsverzeichnis, ausführbare Datei, höchstens 256 einzelne
Argumente, eine kanonische Umgebungs-Variablen-Allowlist, Timeout, getrennte stdout-/stderr-Limits,
Execution Mode, eine optionale Bindung an genau einen validierten TaskStep und Network Scope.
Argumentgrenzen bleiben erhalten; es
gibt weder String-Splitting noch Shell-Interpretation. Ein domain-separiert abgeleiteter
`PolicyResourceId` umfasst alle ausführungsrelevanten Felder und ist zugleich der exakte
Policy-Fingerprint der Spezifikation.

Nur `ProcessExecutionMode::KnownSafe` mit `ProcessPlanBinding::Validated`, ohne Netzwerk und im
direkten Modus darf die zentrale Systempolicy um automatische Freigabe bitten. Open Commands,
Shell Mode und Netzwerk bleiben approval-pflichtig beziehungsweise werden in V1 vor Ausführung
abgelehnt. Eine
`AuthorizedProcessSpec` ist eine einmalig verbrauchte Application-Capability; sie entsteht nur aus
einer erlaubenden Entscheidung mit identischem Run, Fingerprint, Scope, Actionklasse und Risiko.

`ProcessRunResult` bindet den ausgeführten Spec-Digest und die Policy-Entscheidung an genau eine
Termination (`Exited`, `TimedOut` oder `Cancelled`), begrenzte Dauer sowie getrennte
`ProcessOutputCapture`s. Jede Capture enthält den BLAKE3-Digest und die beobachtete Bytezahl der
vollständigen Ausgabe, aber höchstens das vorab festgelegte Retained Limit. Nicht valides UTF-8,
Secret-Kandidaten oder unsichere Steuerzeichen erzeugen nur eine content-freie
`ProcessOutputRedaction`; Überlauf wird sichtbar markiert und dennoch vollständig bis EOF
verworfen. Lückenlos sequenzierte Started-, Output-, Truncated-, Redacted- und Terminated-Events
machen Fortschritt, Backpressure-Fehler und das terminale Ergebnis prüfbar.

### ProjectCommandCatalog und ProjectCommandAllowlist

`CommandDiscoverySchemaVersion::V1` erzeugt aus dem atomar publizierten Fast Index einen
deterministischen, worktreegebundenen `ProjectCommandCatalog`. Seine geschlossene
`DiscoveredCommandKind`-Union enthält ausschließlich `Test`, `Build`, `Lint` und `Format`; ein
Installationskommando ist nicht darstellbar. Jedes `DiscoveredCommand` bindet Kategorie,
package-lokales `WorkspaceDirectory`, direktes Executable, einzelne argv-Werte und höchstens 16
aktuelle `FileRevision`- oder `EvidenceRef`-Belege. `DiscoveredCommandId` und `CommandCatalogId`
werden domain-separiert aus diesen Feldern abgeleitet; eine relevante Manifest- oder
Package-Manager-Änderung erzeugt deshalb eine andere Identität.

Eine Anzeige wird bereits als `ProcessSpec` erzeugt, bleibt aber
`ProcessPlanBinding::Unbound` und ist damit nicht automatisch ausführbar. Der Benutzer kann eine
begrenzte Teilmenge genau dieses Katalogs als `ProjectCommandAllowlist` bestätigen. Nur wenn
Worktree, vollständiger Katalog-Digest und Command-ID weiterhin identisch sind, darf der
Application-Use-Case zusammen mit einer `TaskStepId` einen `KnownSafe`, netzwerkfreien,
plan-gebundenen `ProcessSpec` bilden. Diese Bestätigung ersetzt weder die zentrale PolicyDecision
noch eine einmalige privilegierte Freigabe und kann Workspace- oder Systempolicy nicht lockern.

### VerificationSpec und VerificationEvidence

E6 ersetzt frei interpretierbare Verifikationsmethoden für neue Schritte durch die geschlossene
operationale Union `Command`, `Test`, `DiffInvariant`, `Diagnostic` und `UserConfirm`. Command,
Test und Diagnostic binden eine bestätigte `DiscoveredCommandId`; Test bindet zusätzlich einen
strukturierten All-/Exact-Selector und eine positive Mindestzahl bestandener Fälle. DiffInvariant
bindet NoChanges, OnlyPaths oder ExactPaths an einen kanonischen begrenzten Pfadsatz. Scope ist
explizit Targeted, Package oder Workspace. Historische Method-plus-Text-Spezifikationen bleiben
lesbar, sind aber nicht executable.

`OrderVerificationSpecs` sortiert deterministisch von targeted Diff-/Exact-Test-Prüfungen über
Package bis Workspace und danach stabil nach Semantik und Spec-ID. Erfolg kann nicht vom Aufrufer
behauptet werden: `EvaluateStepVerification` leitet ihn aus genau einem passenden, run- und
snapshotgebundenen Artifact ab. `CommandEvidence` bewahrt content-frei Process-, Policy-, Stream-
Digest- und Abhängigkeitsdaten; `TestEvidence` ergänzt eindeutige strukturierte Testfälle,
`DiagnosticEvidence` Fehler-/Warnungszahlen, `DiffEvidence` den tatsächlichen vollständigen
Change-Set-Pfadsatz aus dem E3-Patchresultat oder dem Vergleich zweier geordneter vollständiger
Published Indexes und `UserConfirmationEvidence` den exakt bestätigten Scope. Dadurch ist auch
`NoChanges` als leerer Indexdelta beweisbar, bleibt aber exakt an dessen aktuellen Snapshot
gebunden. Exitcode 0 ohne
erwartete Test-, Diagnose- oder Diffsemantik ist kein spezialisierter Nachweis.

Freshness vergleicht jede präsente oder erwartbar abwesende Repository-Abhängigkeit mit dem
aktuellen atomar publizierten Index. UserConfirmation ohne Dateiabängigkeit verlangt denselben
Snapshot. Der produktive `DeterministicAcceptanceVerifier` lädt exakt die von abgeschlossenen
Must-Schritten referenzierten Artifacts, prüft Spec, Run, Semantik, Freshness und Published Snapshot
erneut und bindet zusätzlich einen ankergleichen regenerierten `RunMemoryCheckpoint`. Eine darin
verbliebene aktive taskbezogene Hypothesis blockiert `Done` konservativ. Soll-only-Ziele können
ohne Evidence abschließen, solange keine solche Hypothesis offen ist.

### AgentInspection

`AgentInspectionBuffer` ist eine flüchtige, begrenzte Desktopprojektion und keine fachliche
Wahrheitsquelle. `AgentInspectionContext` bindet jeden Datensatz an Task, Run, Step,
Verification-Spec und Snapshot; eine positive monotone `AgentInspectionRevision` macht veraltete
Detailselektionen erkennbar. `AgentPatchInspection` übernimmt ausschließlich die exakten bounded
E3-Präfixe und leitet deterministische gemeinsame `AgentDiffRow`s für Unified und Side-by-side ab.
`AgentProcessInspection` bewahrt ausschließlich bereits klassifizierte retained E4-Ausgabe und
liefert höchstens 16-KiB-Seiten; `pageTruncated` und `sourceTruncated` sind verschiedene Zustände.

`GetTaskVerificationInspection` liest dagegen dauerhafte Ledger-Referenzen und den jüngsten
Published Index zweimal ankergleich. Es bewertet jedes Artifact erneut und projiziert pro Kriterium
`Proven`, `Pending`, `Failed`, `Stale` oder `Missing`. Nur `Proven` enthält beweisende Step- und
Evidence-IDs; Staleness dominiert historische Completion. Volatile Patch- oder Logdaten werden im
Composition Root nur ausgeliefert, wenn Worktree, Task, Run, Step, Spec und Snapshot weiterhin zur
aktuellen dauerhaften Projektion passen.

### AgentSession und AgentWorkItem

Eine `AgentSession` ist die projektgebundene Conversation-Projektion des Agent Workspace. Sie
besitzt eine monotone Revision, einen `Ask`-, `Plan`- oder `Agent`-Modus, einen geschlossenen
Präsentationsstatus und eine begrenzte, monotone Entry-Sequenz. Ein optionaler `AgentWorkItem`
bindet genau eine Core-eigene `TaskId`; diese Bindung macht die Session nicht zur Task-Autorität.

Planrevisionen sind unveränderlich. Eine Umsetzung akzeptiert nur die aktuell sichtbare Revision.
Modi können innerhalb eines Work Items nur `Ask → Plan → Agent` fortschreiten. Fachliche
Aktivität, Mutationen und Abschluss werden ausschließlich aus Agent Run, Task Ledger und
Verification projiziert.

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

Der Application-Controller bildet ausschließlich die dokumentierten Übergänge zwischen `Intake`,
`Localize`, `Plan`, `Execute`, `Verify`, `Replan`, `AwaitApproval` und den drei terminalen Zuständen
ab. Cancellation und bereits erschöpfte Budgets haben vor einem neuen Turn Vorrang. Ein erschöpfter
Run geht aus `Execute` einmal sichtbar nach `AwaitApproval`; ohne einen neuen Run mit neuem Budget
endet die nächste Fortsetzung deterministisch in `Failed`, statt erneut Modellarbeit zu starten.

`ExecuteAgentTurn` kompiliert pro Turn frischen H7-Kontext, bindet Kontext, Provider und
optionales read-only Resultat an denselben Snapshot und akzeptiert erst nach einem terminalen Provider-Event
eine strikt dekodierte Action. Ein ungültiges Ergebnis darf genau einmal über eine content-freie
Repair-Anweisung korrigiert werden; der ungültige Originaltext wird nicht erneut in den Kontext
gegeben und nie ausgeführt. Jede akzeptierte oder endgültig verworfene Modellausgabe wird als ein
budgetierter Turn verbucht. Das Journal erhält ausschließlich Fehlerklasse und redigierte
Byte-Metadaten, nie den Rohtext.

H11 modelliert jeden Toolaufruf als `AgentToolAttempt` mit logischer ToolRunId, monotoner
Versuchsnummer, Run- und Snapshotanker sowie einem geschlossenen Lifecycle aus `InFlight`,
`Succeeded`, `Failed`, `Cancelled`, `Denied` und `Interrupted`. Der Versuch muss vor dem Aufruf
durabel sein. `Succeeded` entsteht ausschließlich gemeinsam mit dem normalisierten Toolresultat
und dessen RunEvent; ein nach Neustart noch laufender Versuch wird terminal `Interrupted`.

E7 komponiert Patch-, Process-, Policy-, Approval-, Recovery-, Index-, Context-, Ledger- und
Verification-Ports in `ExecuteMutatingAgentAction`, ohne einen zweiten offenen Agentenloop zu
erzeugen. Ein composition-root-eigener `WorktreeMutationCoordinator` vergibt genau einen nicht
klonbaren Lease je Worktree über die gesamte Mutation einschließlich Post-Patch-Refresh. Der Lease
hält während asynchroner Arbeit keinen Mutex-Guard. `MutationActionFingerprint` identifiziert nur
vollständig strukturierte Patch- oder Command-Auswahl; read-only Aktionen können keinen Lease
erhalten.

Vor jeder Toolausführung wird die zentrale `PolicyDecision` samt optionalem Request oder
Grantverbrauch persistiert. Ein erfolgreicher Mutationslauf schließt nach typisiertem Toolresultat
Toolversuch, Runprojektion und content-freies `tool_action`-Event atomar. Jede tatsächlich sichtbare vollständige oder partielle
Patchänderung erzeugt unmittelbar eine inkrementelle Repository-Änderungsmenge und muss einen
vollständigen neuen Published Index liefern. Erst danach darf ein `ContextCompiled`-Event mit exakt
diesem Snapshot entstehen. Diff-Schritte können nur über das tatsächliche `PatchChangeSet`
erfolgreich werden; Test- und Diagnostic-Semantik benötigt einen injizierten strukturierten
Evidence-Adapter und wird nie aus Exitcode allein erfunden.

Der Fortschrittsdetektor speichert pro Run und Worktree nur Action-Fingerprint, content-freie
Fehlerklasse und begrenzte Wiederholungszahl. Der erste identische Fehler darf über
`Verify → Execute` genau einen frischen Retry-Kontext erhalten, der zweite erzwingt `Replan`, jeder
weitere identische Fehler `Failed`. Eine andere Action oder ein vollständig erfolgreicher Lauf
setzt die Serie zurück.

`InspectAgentRunRecovery` rekonstruiert den nicht terminalen Run, sein revisionsgebundenes Ledger
und den aktuellen Published Snapshot und klassifiziert nicht mehr auflösbare
Verification-Evidence als stale. `RecoverAgentRun` akzeptiert ausschließlich die expliziten
Entscheidungen Resume, Replan und Cancel. Resume ist bei stale Evidence ausgeschlossen; Replan und
Cancel verwenden die bestehende Ledger-Invalidierung zum transitiven Reopen. Der Recovery-Commit
aktualisiert Ledger, Run und Event atomar und verweigert einen zwischenzeitlich gewechselten
Published Snapshot oder konkurrierende Ledger-/Run-Schreiber.

Ein normaler Zustandsübergang nach `Done` ist im Domain-Aggregat gesperrt. Nur
`VerifyAgentAcceptance` darf einen vollständigen `AcceptanceVerificationReceipt` anwenden. Dieser
bindet Run, Goal-Revision, Ledgerrevision und Snapshot und deckt jedes Must-Kriterium exakt einmal
mit mindestens einer Evidence-ID ab. Der Acceptance-Verifier ist damit der einzige
Application-Pfad nach `Done`; abgelehnte Verifikation führt abhängig vom Grund nach `Execute` oder
`Replan` zurück.

Invarianten:

- Pro Turn gibt es höchstens eine ausführbare Tool Action.
- Jeder Turn erhöht genau einen Turnzähler; Token-, Action- und Repairverbrauch überleben Reopen.
- Ungültiger oder unvollständiger Modelloutput kann keine Action auslösen und bleibt dennoch
  budgetiert und content-frei auditierbar.
- Mutierende Tool Actions werden serialisiert.
- Ein Turn verweist auf genau einen Snapshot.
- Vor einer Mutation wird geprüft, ob der erwartete Snapshot noch aktuell ist.
- Abschluss ist ein Acceptance-verifizierter expliziter Zustandsübergang und keine bloße
  Textausgabe oder Modellentscheidung.

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
