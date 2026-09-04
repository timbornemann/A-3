# IPC-Protokoll

Status: verbindliche Baseline

Stand: 2026-08-29

## Zweck und Grenze

Das IPC-Protokoll ist die einzige Datengrenze zwischen der unprivilegierten WebView und dem
privilegierten Rust-Kern. `a3-protocol` enthält ausschließlich primitive, versionierte Grenztypen und
besitzt keine Abhängigkeit auf Domain, Application, Tauri oder Adapter.

Domain- und Protocol-Typen werden im Desktop-Composition-Root explizit gemappt. Ein Protocol-DTO darf
niemals als Domänenobjekt verwendet werden. Die DTOs werden mit Serde als JSON gebunden; Feldnamen am
WebView-Rand verwenden `camelCase`. Eingaben lehnen unbekannte Felder ab.

## Versionierung

- Jede IPC-Nachricht trägt eine `ProtocolVersion`.
- Die erste und aktuell emittierte Version ist `1`.
- Eine inkompatible Schemaänderung benötigt eine neue Protokollversion und einen dokumentierten
  Migrations- oder Ablehnungspfad.
- Unbekannte Eingabeversionen werden am IPC-Rand abgelehnt und niemals als ausführbare Eingabe
  interpretiert.

## Gemeinsamer V1-Request

Die pfadlosen Status- und Control-Commands erhalten genau ein Argument `request`. Das gilt neben
`query_health`, `open_project`, `list_recent_projects` und `restore_last_project` auch für Projekt-,
Index- und Deep-Map-Status sowie für `pause_deep_map`, `resume_deep_map` und `cancel_deep_map`. Ihr
gemeinsamer V1-Request enthält ausschließlich:

| JSON-Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | für V1 exakt `1` |

Zusätzliche Felder, ein fehlender Request oder ein nicht numerischer Versionswert werden vor
Ausführung des jeweiligen Use Cases abgelehnt. Insbesondere akzeptiert `open_project` keinen Pfad und
`list_recent_projects` weder einen Pfad noch ein WebView-gesteuertes Limit.

## Deep Map Start V2, Status V3 und Journal-Reads V1

Die historischen V1-/V2-DTOs bleiben unverändert decodierbar. Die Desktop-Oberfläche verwendet
geschlossen `StartDeepMapRequestV2` und `DeepMapStatusResponseV3`. `start_deep_map` akzeptiert neben
der Protokollversion ausschließlich `mode: fast | standard | thorough`; daraus wählt der Core die
festen Budgets 8.000/32.000/128.000 Tokens sowie die festen Zeit- und Read-Grenzen. Freie
Budgetwerte, Profile, Provider-Parameter und Projektanker sind nicht darstellbar. Der Start-
Preflight antwortet mit `queued` oder `alreadyCurrent` und ruft im zweiten Fall weder Planner,
Provider noch Publisher auf.

`query_deep_map` V3 liefert genau `noProject`, `unavailable` oder `available`. `available` enthält
nur das verifizierte ModelProfile und eine kompakte diskriminierte Lifecycle-Projektion ohne
Ereignisfeed. Der Lifecycle unterscheidet `ready`, `current`, `queued`, `running`, `pausing`,
`paused`, `cancelling`, `succeeded`, `failed` und `cancelled`. `current` bindet die vollständige
Module-Card-Publikation an den neuesten Fast Index. Neue Fast-Index-Publikationen ohne aktuelle
Cards führen wieder zu `ready`.

Ein vorübergehend nicht verfügbarer Read der Publikationsprojektion darf keinen bekannten
Manager-Lifecycle als terminales `failed` überblenden. Deshalb bleibt auch bei `ready` der
Managerzustand maßgeblich; ein Status-Read ist kein Ausführungsfehler der Deep Map. Der nächste
Polling-Zyklus liest die Projektion erneut, und `start_deep_map` prüft sie vor einer Planung ohnehin
noch einmal autoritativ. Dadurch zeigt die kompakte Leiste keinen Scheinfehler, obwohl kein Lauf
fehlgeschlagen ist oder derselbe Lauf nachweislich weiterarbeitet.

Die content-freien Detaildaten werden ausschließlich über getrennte V1-Reads geladen:
`query_deep_map_runs` liefert neueste 20 Läufe, `query_deep_map_entries` höchstens 50 chronologische
Einträge und `query_deep_map_entry_detail` genau einen ausgewählten Eintrag. Cursor sowie Run- und
Entry-Selektionen sind opak, Core-ausgegeben und an das aktive Projekt gebunden. Fremde, erfundene,
stale oder übergroße Werte werden abgelehnt. Die Reads enthalten sichere Phasen, Aktionen,
Zeitpunkte, feste Budgetreservierungen, Modell-/Profilreferenzen, Planstopp- und
Publikationsergebnis sowie geschlossene Diagnosecodes. Prompts, Modellantworten, Chain-of-Thought,
Providerpayloads, Source-Inhalt, Credentials, rohe Fehlertexte und nicht zuverlässig gemessener
Tokenverbrauch überschreiten die IPC-Grenze nicht.

Die geschlossenen V3-Diagnosen trennen insbesondere stale Index, Provider-Ablehnung/-Timeout,
Publication-Rejection, Storage, Timeout und Progress. Ein fehlgeschlagener Status ist damit ein
sicherer Schlüssel für die Detailansicht und keine Projektion eines rohen Adapterfehlers.

ADR-0036 ergänzt die normale Detailansicht um vier projektgebundene V1-Read-Modelle:
`query_deep_map_run_dashboard` liefert die fünf Produktphasen, aktuellen verständlichen Arbeitsbezug,
Fortschritt und Freshness; `query_deep_map_run_modules` liefert höchstens 20 Module pro Seite;
`query_deep_map_module_steps` löst höchstens 50 Erkundungsziele pro Seite gegen den exakten
Laufindex auf; `query_deep_map_atlas_impact` liefert höchstens 50 exakt passende aktuelle
Atlas-Auswirkungen. Card-Inhalt wird nicht dupliziert, sondern über `query_module_card_detail` mit
der Core-ausgegebenen Run-/Modulselektion nachgeladen. Die vier Dashboard-Antworten enthalten keine
internen IDs, Provider-/Modellnamen, Budgets, Snapshots, Prompts, Modellantworten, Quelltexte oder
numerischen Vertrauenswerte. Historische beziehungsweise nicht mehr auflösbare Ziele bleiben ohne
technische Ersatzkennung sichtbar eingeschränkt.

### Historische Statusprojektion V2

`query_deep_map` liefert genau `noProject`, `unavailable` oder `available`. Nur `available` enthält
die sichere Identität eines live verifizierten ModelProfiles, Context- und Outputlimit, den festen
Minimum-/Default-/Maximum-Budgetrahmen sowie den Core-eigenen Lifecycle. Endpoints, Credentials,
Repositorypfade, Providerpayloads und Job-IDs bleiben intern. Zähler werden als kanonische
verlustfreie u64-Dezimaltexte übertragen.

`start_deep_map` akzeptiert zusätzlich zur Protokollversion ausschließlich ein geschlossenes
`budget` mit positiven Token-, Millisekunden- und Read-only-Toollimits innerhalb des
Domainrahmens. Profil und Projekt stammen aus dem Core. Pause, Resume und Cancel nehmen nur die
Protokollversion an und quittieren ausschließlich eine vom Core akzeptierte Transition. Der
geschlossene Lifecycle lautet `idle`, `queued`, `running`, `pausing`, `paused`, `cancelling`,
`succeeded`, `failed` oder `cancelled`; `paused` darf erst nach abgeschlossenem kooperativem Abbruch
mit validiertem Checkpoint sichtbar werden.

Die V2-Statusprojektion ergänzt ausschließlich die geschlossenen Phasen `planning`, `exploring`,
`claiming`, `verifying` und `publishing`, eine optionale aktuelle Modul-ID, Ziel- und sichere
Aktionsart, bestätigte Schrittposition sowie höchstens 32 monoton sequenzierte content-freie
Ereignisse. Ereignisse enthalten weder Prompt, Suchtext, Source, Modellantwort noch Begründung. Nur
ein terminal erfolgreich abgeschlossener Publishing-Lauf darf eine begrenzte
`publicationSummary` tragen. Pause/Resume behält den Ringpuffer ohne Replay; ein neuer Start oder
Projektwechsel erzeugt eine neue Sequenz ab eins.

## Project Map Scene V1 und Search V2

`query_project_map_scene` akzeptiert ausschließlich `protocolVersion` und optional eine aktuelle
`focusModuleId`; Pfade und Limits sind nicht darstellbar. Die Übersicht liefert höchstens 64
Primärmodule und 128 Relationsgruppen, der Fokus Zentrum plus höchstens 31 direkte Nachbarn. Run,
Snapshot, `scenePolicyVersion`, vollständige Gesamt-/Trunkierungs-/Unmapped-Zähler, sichere Namen,
Counts, Mappingstatus, Coverage und opaque Evidence-Hooks gehören zwingend zu derselben atomaren
Publikation.

`query_project_map_search` behält den submit-gebundenen pfadlosen Request. Search V2 ergänzt jeden
Treffer um eine optionale eindeutige Primärmodulbindung und eine streng typisierte opaque File-
oder Symbol-Evidence-Auswahl. Uneindeutige oder ungemappte Treffer führen `moduleId: null`; der
Core erfindet keine Region. Decoder lehnen fehlende oder unbekannte Felder, unkanonische IDs,
gemischte Publikationen, inkonsistente Selection-/Target-Paare und mehr als 20 Treffer ab.

## Progressive Atlas Read Models V1

`query_project_map_atlas_scene` akzeptiert neben `protocolVersion` nur eine optionale, zuvor vom
Core ausgegebene typisierte Modul-, Datei- oder Symbolauswahl. Pfade, Run-/Snapshot-IDs und Limits
sind nicht darstellbar. Die Antwort bindet genau eine aktuelle Publikation an Policy V1, Ebene,
Breadcrumb, sichere Knoten, Relationsgruppen, Evidence-Auswahlen sowie vollständige Gesamt- und
Trunkierungszähler. Die festen Obergrenzen lauten 64 Module/128 Routen, 32 Dateien, 48 Symbole,
Zentrum plus 31 Nachbarn und 16 Boundary-Stubs.

`query_project_map_entity_context` verwendet dieselbe Auswahl und liefert höchstens 32 direkte
Architekturbeziehungen sowie 16 externe oder ungelöste Ziele. `query_project_map_inventory_page`
akzeptiert zusätzlich ausschließlich `files`, `symbols` oder `members` und einen zuvor ausgegebenen,
publikations- und scopegebundenen Cursor; jede Seite umfasst fest höchstens 50 Einträge.
`query_project_map_flow_scene` akzeptiert nur `callers`, `callees`, `tests` oder `dataAccess`.
Caller/Callee sind auf zwei Hops, Tests und Datenzugriff auf direkte Kanten, alle Flows auf 31 Ziele
und 4.096 inspizierte Kanten begrenzt. Jeder Treffer enthält den vollständigen ersten kürzesten
Evidence-Pfad. Zusätzliche Felder und widersprüchliche Publikations-, Breadcrumb-, Count- oder
Trunkierungsdaten werden abgelehnt.

## Project Map Source Preview V1

`query_project_map_source_preview` akzeptiert ausschließlich eine geschlossene
`moduleCard`-Auswahl mit den sieben zuvor ausgegebenen Card-Ankern oder eine geschlossene `index`-
Auswahl für zuvor ausgegebene File-, Symbol-, aufgelöste Relations- oder ungelöste
Relationsevidence. Der Request enthält niemals Pfad, Range oder Limit. Nach erneuter Publikations-,
Membership-, Freshness-, Root-, Symlink-, Binary-, Generated-, Secret- und Hashprüfung liefert er
nur aktuelle Evidence als Plain Text: acht Kontextzeilen je Seite, insgesamt höchstens 64 Zeilen
und 16 KiB UTF-8. Sprache, sichere Pfadanzeige, Zeilennummern und Highlight sind Daten; HTML und
Source in Fehlern sind ausgeschlossen.

## Module Card Freshness V1

`query_module_card_freshness` akzeptiert ausschließlich den gemeinsamen pfadlosen V1-Request und
liefert `noProject`, `noPublishedIndex` oder `available`. `available` enthält die 64-stelligen
kleingeschriebenen Hex-IDs des aktuellen veröffentlichten Indexlaufs und Snapshots, vier
verlustfreie u64-Dezimalzähler für `publishedCount`, `staleCount`, `needsReviewCount` und
`totalCount` sowie höchstens fünf positive Ursachenzeilen.

Eine Ursachenzeile enthält `status`, `reason` und den verlustfreien `count`. `stale` ist nur mit
`evidenceChanged`, `moduleRemoved`, `parserVersionChanged` oder `mapperVersionChanged` gültig;
`needsReview` ausschließlich mit `directDependencyChanged`. Die Zeilen sind kanonisch geordnet,
ihre Summen müssen exakt den beiden invaliden Aggregatzählern entsprechen, und alle drei
Statuszähler müssen `totalCount` ergeben. Projektidentitäten, Pfade, Card-Inhalte, Claims, Evidence,
Datenbankzeilen und Remapqueue-Einträge überschreiten diese Grenze nicht.

## Module Card Detail V1

`query_module_card_detail` akzeptiert genau `protocolVersion` und die 64-stellige
kleingeschriebene `moduleId` eines bewusst ausgewählten aktuellen Primärmoduls. Projekt, Worktree,
Card-ID, Run, Snapshot, Pfade, Claims und Evidence sind keine Requestfelder. Eine ungültige ID
liefert `invalidModuleCardDetailQuery`. Die Antwort unterscheidet `noProject`,
`noPublishedIndex`, `projectionUnavailable`, `moduleUnavailable`, `cardUnavailable` und
`available`.

Eine verfügbare Antwort bindet `currentIndexRunId` und `currentSnapshotId` an die jüngste atomare
Indexpublikation. `sourceIndexRunId` und `sourceSnapshotId` benennen getrennt den historischen Run,
in dem die deterministisch jüngste Card verifiziert wurde. Dazu kommen `cardId`, `moduleId`, die
festen V1-Schema-/Mapperversionen, `confidenceBasisPoints`, ein expliziter `lifecycle` und ein bis
zwölf kanonisch geordnete Card-Felder. Die getrennte `coverage` enthält Gesamt-, Muss- und
Soll-Abdeckung mit ganzzahligen Basispunkten, exakten Feldzählern und kanonisch geordneten
fehlenden Feldern. V1 umfasst zwölf Felder, davon acht Muss- und vier Soll-Felder. Als abgedeckt
gilt ausschließlich ein tatsächlich ausgeliefertes, bereits verifiziertes und evidenzgebundenes
Feld. `current` trägt keine Invalidierungsdaten; `stale` trägt eine direkte Ursache;
`needsReview` ausschließlich `directDependencyChanged`. Ein invalidierter Run muss eine nicht
spätere veröffentlichte Publikation desselben Worktrees sein.

Jedes Feld besitzt mindestens einen begrenzten, kontrollzeichenfreien Wert und mindestens eine
kanonisch sortierte `evidenceId`. Die V1-Grenzen gelten pro Feld unverändert; die gesamte
Wertnutzlast bleibt bei höchstens 65.536 UTF-8-Bytes und die Evidence-Union bei höchstens 512 IDs.
Jeder Wert besitzt genau einen eindeutigen Claim mit unabhängigem `kind` (`fact`, `observation`
oder `hypothesis`), `confidenceBasisPoints`, höchstens 16 Evidence-IDs und einem effektiven `state`.
Claim-Evidence muss Teil der Feld-Evidence sein. Evidence-freie Claims sind nur als Hypothese
darstellbar.

Der effektive Claim-State muss für die ganze Card exakt `current`, `stale` oder `needsReview`
entsprechen. Dadurch bleibt beispielsweise die historische epistemische Klassifikation `fact`
erhalten, kann bei einer stale Card aber nie als aktuelles Faktum erscheinen. Der unabhängige
TypeScript-Decoder prüft Envelope, Schemaordnung, UTF-8-Grenzen, ID-Reihenfolge,
Evidence-Teilmengen, Claim-Eindeutigkeit, Coverage-Zähler/-Prozente/-Lücken und diese
Lifecycle-Propagation erneut. Confidence, Coverage und Freshness bleiben unabhängige Signale; ein
hoher Prozentwert kann weder fehlende Muss-Felder noch stale Evidence kompensieren. Der Command
läuft nur nach expliziter Modulauswahl, Aktualisierung oder erfolgreichem Publish, nicht im
500-ms-Statuspolling. Die gelieferten Evidence-IDs sind stabile Hooks für den nachfolgenden
Evidence-Inspector-Schnitt und verleihen für sich weder Source- noch Dateizugriff.

## Module Card Evidence Inspector V1

`query_module_card_evidence` akzeptiert genau `protocolVersion` sowie die sieben aus einer bereits
sichtbaren Module Card stammenden `currentIndexRunId`, `currentSnapshotId`, `sourceIndexRunId`,
`sourceSnapshotId`, `cardId`, `moduleId` und `evidenceId`. Alle IDs sind 64-stellige
kleingeschriebene Hexwerte. Gleicher Quell- und aktueller Run erfordern denselben Snapshot. Diese
opaken Anker sind keine allgemeine Datei-, Graph- oder Evidence-Capability.

Die Antwort unterscheidet `noProject`, `noPublishedIndex`, `projectionUnavailable`,
`moduleUnavailable`, `cardUnavailable`, `selectionChanged`, `evidenceUnavailable` und `available`.
`selectionChanged` bedeutet, dass aktuelle Publikation oder deterministisch neueste Card nicht
mehr zur sichtbaren Auswahl passen; der Command löst sie nicht still gegen den neuen Stand auf.
`evidenceUnavailable` gibt weder an, ob eine ID anderswo existiert, noch liefert es fremde
Card-Inhalte.

Eine verfügbare Antwort wiederholt alle sieben validierten Anker, den Card-`lifecycle` und die
unabhängige Evidence-`freshness` `current` oder `stale`. `stale` ist nur zusammen mit einer stale
Card zulässig. Die geschlossene Payload ist genau eine content-adressierte `file`-Revision, ein
`symbol` mit stabiler Symbol-ID und Revision oder eine `graphEdge` mit Relation, typisierten
Endpunkten, Revision, halboffener Range, Provider, Confidence und Link-Resolution. Der unabhängige
TypeScript-Decoder prüft exakte Felder, Ankerübereinstimmung, kanonische IDs und Pfade,
Graph-Evidence-ID sowie die Lifecycle-/Freshness-Kombination erneut. Source-Text, Live-Pfade,
Datenbankzeilen, Claim-Prädikate und Providerpayloads überschreiten die IPC-Grenze nicht.

## Repository Tree V1

`query_repository_tree` akzeptiert genau `protocolVersion`, `directoryPathHex`, `afterNameHex` und
`limit`. `directoryPathHex` ist `null` für das Repository-Root oder ein kleingeschriebener gerader
Hextext mit höchstens 131.072 dekodierten Bytes. Er muss eine kanonische relative
`RepositoryPath` ohne NUL, leere Segmente, `.` oder `..` ergeben. `afterNameHex` ist `null` oder ein
einzelner direkter Kindname mit höchstens 4.096 Bytes ohne NUL, Slash, `.` oder `..`. `limit` liegt
einschließlich zwischen 1 und 100. Die Tokens sind ausschließlich verlustfreie Schlüssel der
publizierten Indexprojektion und keine autoritativen Dateisystempfade oder Zugriffsrechte.

Die Antwort liefert genau `noProject`, `noPublishedIndex` oder `available`. Eine verfügbare Seite
enthält die 64-stelligen kleingeschriebenen `indexRunId` und `snapshotId`, das angeforderte
`directoryPathHex`, höchstens 100 strikt byteweise geordnete direkte `entries` und optional
`nextAfterNameHex`. Ein vorhandener Cursor entspricht exakt dem letzten gelieferten Kind und zeigt
eine weitere Seite an.

Jeder Eintrag enthält `kind`, das verlustfreie volle relative `pathHex`, einen nicht leeren,
kontrollzeichenfreien `name` mit höchstens 256 Zeichen, `nameTruncated`, den positiven
verlustfreien u64-Dezimaltext `descendantFileCount` und `contentHash`. Ein `file` hat exakt den
Zähler `1` und den 64-stelligen Hash seiner veröffentlichten `FileRevision`; ein `directory` besitzt
mindestens einen Dateinachfahren und keinen synthetischen Hash. Unbekannte Felder, indirekte Kinder,
nicht kanonische Tokens, widersprüchliche Evidenz, Reihenfolge- oder Cursorfehler werden sowohl am
Rust- als auch am TypeScript-Rand abgelehnt.

## Module Tree V1

`query_module_tree` akzeptiert genau `protocolVersion`, `parentModuleId`, `afterModuleId` und
`limit`. Beide IDs sind `null` oder 64-stellige kleingeschriebene Hexwerte; `limit` liegt
einschließlich zwischen 1 und 100. Die IDs wählen nur einen Knoten beziehungsweise exklusiven
Cursor innerhalb der aktuellen publizierten Modulprojektion. Projekt, Worktree, Repositorypfad und
Dateisystempfad sind keine Requestfelder. Eine ungültige Query liefert
`invalidModuleTreeQuery`; ein nicht mehr aktueller oder zusätzlicher Community-Elternknoten
`moduleTreeParentUnavailable`.

Die Antwort liefert genau `noProject`, `noPublishedIndex`, `projectionUnavailable` oder `available`.
`projectionUnavailable` bezeichnet eine historische Publikation ohne V8-Modulmarker und ist von
einer verfügbaren leeren Projektion getrennt. Eine verfügbare Seite enthält `indexRunId`,
`snapshotId`, das angeforderte `parentModuleId`, die verlustfreien u64-Dezimaltexte
`primaryModuleCount` und `graphCommunityCount`, höchstens 100 strikt nach `moduleId` geordnete
direkte primäre Kinder sowie optional `nextAfterModuleId`. Ein vorhandener Folgeseiten-Cursor
entspricht exakt dem letzten ausgelieferten Modul.

Ein Eintrag enthält `moduleId`, `kind` (`manifestBoundary` oder `pathBoundary`), optionales
kanonisches relatives `rootPathHex`, einen nicht leeren kontrollzeichenfreien `name` mit höchstens
256 Zeichen und `nameTruncated`. `manifestCount`, `fileCount` und `symbolCount` sind verlustfreie
u64-Dezimaltexte. `centralSymbols`, `entrypoints` und `tests` enthalten jeweils `count` und eine
explizite `truncated`-Angabe; ein trunkierter leerer Präfix ist ungültig. `childState` ist `leaf`
oder `hasChildren`.

`boundaryEvidence` enthält eine `representativeRevision`, sobald `symbolCount` positiv ist, und nur
bei einer Manifestgrenze zusätzlich eine `manifestRevision`. Jede Revision besteht aus kanonischem
relativem `pathHex` und exakt 64-stelligem `contentHash`; Source-Inhalt wird nicht übertragen. Eine
Manifestgrenze besitzt mindestens ein Manifest, eine Pfadgrenze keines. Unbekannte Felder,
Graph-Community-Knoten, widersprüchliche Counts, Evidence- oder Trunkierungsformen, nicht kanonische
IDs und Pfade sowie Reihenfolge-, Eltern- oder Cursorfehler werden am Rust- beziehungsweise
TypeScript-Rand abgelehnt.

## Module Dependency Graph V1

`query_module_dependency_graph` akzeptiert genau `protocolVersion`, `centerModuleId` und
`nodeLimit`. Die ID ist ein 64-stelliger kleingeschriebener Hexwert; das Limit umfasst das Zentrum
und liegt einschließlich zwischen 1 und 100. Projekt, Worktree, Pfade und Graphendpunkte sind keine
Requestfelder. Eine ungültige Query liefert `invalidModuleDependencyGraphQuery`.

Die Antwort liefert genau `noProject`, `noPublishedIndex`, `projectionUnavailable`,
`centerUnavailable` oder `available`. `centerUnavailable` bezeichnet eine fehlende, historische
oder zusätzliche Graph-Community-ID. Ein verfügbarer Graph enthält aktuelle 64-stellige
`indexRunId`, `snapshotId` und `centerModuleId`, ein bis 100 strikt nach `moduleId` geordnete
primäre `nodes` und höchstens 256 nach Quelle, Ziel und Relation geordnete `edges`. Das Zentrum ist
genau einmal enthalten; jede Kante ist am Zentrum inzident und referenziert zwei sichtbare Knoten.

Ein Knoten enthält `moduleId`, `kind`, optionales `rootPathHex`, sichere Anzeige `name` samt
`nameTruncated` und optionale `representativeEvidence`. Diese Evidence besteht aus stabiler
64-stelliger `evidenceId`, kanonischem relativem `pathHex` und 64-stelligem `contentHash`.

Eine Kante enthält `sourceModuleId`, `targetModuleId`, eine nicht hierarchische bekannte
`relation`, den positiven u64-Dezimaltext `observedEvidenceCount` und vollständige
`representativeEvidence`. Letztere trägt eine stabile `evidenceId`, streng typisierte File- oder
Symbolendpunkte, aktuelle Pfad-/Hashrevision, eine valide halboffene `range` mit Byteoffsets und
nullbasierten Positionen, bekannten `provider`, `confidenceBasisPoints` von 0 bis 10.000 sowie eine
bekannte `resolution`. Source-Inhalt wird nicht übertragen.

Die u64-Dezimaltexte `observedNeighborCount`, `observedEdgeGroupCount`, `inspectedEdgeCount` und
`unmappedEdgeCount` bleiben verlustfrei. `nodesTruncated`, `edgesTruncated` und
`sourceEdgesTruncated` müssen den sichtbaren Counts beziehungsweise dem festen 4.096-Edge-Präfix
exakt entsprechen. Unbekannte Felder, Communities, Hierarchierelationen, nicht kanonische IDs,
Pfade, Counts oder Evidence, Selbstkanten, falsche Inzidenz, Reihenfolge oder
Trunkierungswahrheit werden am Rust- beziehungsweise TypeScript-Rand abgelehnt.

## Module Runtime Map und Flow V1

`query_module_runtime_map` akzeptiert genau `protocolVersion`, eine aktuelle primäre `moduleId`
sowie getrennte `entrypointLimit`- und `testLimit`-Werte von jeweils 1 bis 256. Projekt, Worktree,
Pfade und Symbolrollen sind keine Requestfelder. Ungültige Werte liefern
`invalidModuleRuntimeMapQuery`. Die Antwort unterscheidet `noProject`, `noPublishedIndex`,
`projectionUnavailable`, `moduleUnavailable` und `available`.

Eine verfügbare Map bindet `indexRunId`, `snapshotId` und `moduleId` an genau eine atomare
V8-Publikation. `entrypoints` und `tests` enthalten jeweils einen ab Rang eins lückenlosen,
identitätseindeutigen Präfix aus höchstens 256 `roots`, den kanonischen Dezimaltext `storedCount`
sowie getrennte `projectionTruncated`- und `visibleTruncated`-Signale. Ein Root trägt ausschließlich
seine feste Rolle, den einbasierten Rang und eine aktuelle strukturelle Symbolprojektion mit
64-stelliger `symbolId` und `evidenceId`, bekanntem `symbolKind`, begrenztem Namen, kanonischem
relativem `pathHex`, `contentHash` und valider `selectionRange`. Source-Inhalt wird nicht
übertragen.

`query_module_runtime_flow` akzeptiert genau `protocolVersion`, die sichtbaren
`expectedIndexRunId` und `expectedSnapshotId`, `moduleId`, `rootSymbolId`, einen der festen Werte
`entrypointCalls` oder `testTargets` und ein `resultLimit` von 1 bis 100. Eine frei wählbare
Relation, Richtung oder Tiefe ist nicht darstellbar. `entrypointCalls` bedeutet höchstens zwei
ausgehende `Calls`-Kanten von einem aktuell bewiesenen Entry Point; `testTargets` bedeutet genau
eine direkte ausgehende `Tests`-Kante von einer aktuell bewiesenen Testdefinition. Ungültige Werte
liefern `invalidModuleRuntimeFlowQuery`.

Vor der Traversierung validiert der Core Publikation, Primärmodul, Rolle und Root erneut. Die
Antwort unterscheidet deshalb zusätzlich `publicationChanged` und `rootUnavailable`, statt alte
Roots mit neuer Evidence zu mischen. Ein verfügbarer Flow wiederholt alle Publikations- und
Seedanker, enthält höchstens 100 eindeutige Datei- oder Symbolziele und zu jedem Ziel den
vollständigen kürzesten Evidence-Pfad. Jeder Schritt trägt nur `calls` beziehungsweise `tests` und
eine exakte aktuelle `GraphEdge`-Evidence mit Endpoints, Revision, Range, Provider, Confidence und
Resolution. Der unabhängige TypeScript-Decoder prüft Relation, Tiefe, gerichtete
Endpoint-Kontinuität, Zyklen, Zielübereinstimmung, Duplikate, Grenzen und die Übereinstimmung mit der
sichtbaren Query erneut. Beide Commands laufen nur nach expliziter Modul-, Root-, Nachlade- oder
Aktualisierungsaktion, nicht im Statuspolling.

## Agent Sessions und Workspace Layout V1

`query_agent_sessions` listet höchstens 50 projektlokale Session-Summaries mit serverseitiger
Titelsuche und optionalem Archivfilter. `query_agent_session` liefert eine begrenzte, lückenlose
Entry-Seite. `submit_agent_message` akzeptiert entweder einen neuen geschlossenen Modus oder die
exakte Revision einer bestehenden Session. Freie Pfade, Befehle und Providerparameter sind nicht
Teil des Vertrags. `contextReferences` ist in V1 ein reserviertes, zwingend leeres Feld; ein
späterer Kontextwähler benötigt zuerst einen eigenen Core-Resolver für ausschließlich stabile,
aktuelle Indexreferenzen.

`submit_agent_message_v2` behält dieselbe enge Grenze und ergänzt ausschließlich die pro Nachricht
gewählte `researchDepth: standard | thorough`. Der kompatible V1-Command verwendet `standard`.
`continue_agent_research` akzeptiert nur Session, deren erwartete Revision und die neue Tiefe; der
Core löst damit ausschließlich den neuesten sichtbaren `awaitingContinuation`-Abschnitt auf und
bindet ihn an den dann aktuellen Index.

`control_agent_session` erlaubt ausschließlich Pause, Resume, Cancel, `Ask → Plan`, die Umsetzung
der exakten aktuellen Planrevision sowie Rename, Archive, Unarchive und Presentation Delete. Die
fachlichen Controls werden im Core gegen aktuelle Task-/Ledger-/Runanker aufgelöst. Layoutqueries
und -updates transportieren ausschließlich revisionierte Breiten und Collapse-Zustände innerhalb
fester Grenzen.

Eine angenommene neue Nachricht liefert die bereits dauerhaft angelegte und ausgewählte Session,
bevor der besessene Hintergrundjob arbeitet. Ein nicht einreihbarer Job wird als sichere
fehlgeschlagene Abschlussnachricht in genau dieser Session sichtbar und hinterlässt keinen
`Running`-Datensatz ohne Besitzer. Cancel ist für eine laufende Ask-/Plan-Vorbereitung auch dann
idempotent, wenn der Schedulerjob bereits terminal oder nicht mehr auffindbar ist: Der Core
committet den dauerhaften Sessionzustand `Cancelled` und gibt die neu geladene Projektion zurück.
Polling bleibt rein lesend und wiederholt einen transient fehlgeschlagenen Read, solange die letzte
verifizierte Projektion noch einen laufenden Zustand zeigt.

### Ask-Recherche und Quellen V1

`query_agent_ask_research_turns` akzeptiert ausschließlich `protocolVersion` und die ausgewählte
`sessionId` und liefert höchstens 32 Recherche-Turns. `query_agent_ask_research_detail` ergänzt nur
die sichtbare positive `userSequence` und liefert höchstens 64 inhaltsfreie Schritte mit
geschlossener Phase, Zustand, verständlicher Aktion, optionalem Suchtext und `complete | limited |
notApplicable`. Für Ask-Nachrichten vor Knowledge V30 lautet der eigene Zustand `notRecorded`.

`query_agent_ask_research_sources` liefert höchstens 50 Source-Metadatensätze pro Seite. Sein
optionaler opaker Cursor ist an Worktree, Session, Turn, aktuelle Trace-Revision und heutigen
Indexanker gebunden. Jeder Eintrag enthält ausschließlich eine opake `sourceRef`, sichere
Pfadanzeige, optionale Zeilen und Symbolanzeige, geschlossene Quellenart und Auswahlgrund sowie die
Kennzeichnung `usedForAnswer`. Gefundene und vom Modell tatsächlich zitierte Sources bleiben damit
unterscheidbar.

`query_agent_ask_research_source_preview` akzeptiert neben Session und Usersequenz nur diese opake
`sourceRef`. Der Core löst sie innerhalb desselben Projekts und Turns auf und verwendet die sichere
ADR-0030-Vorschau mit maximal 64 Zeilen und 16 KiB. Nach einem Indexwechsel liefert der Read
`stale`, statt heutigen Quelltext an eine historische Antwort anzuhängen. Keiner der vier Requests
akzeptiert Pfad, Range, Evidence-, Index-, Snapshot-, Worktree- oder Provider-ID. Keine Response
enthält Quelltext ohne den bewussten Preview-Klick, Prompts, Modellrohantworten, Chain-of-Thought,
Providerdaten, Budgets, Vertrauenswerte oder interne IDs.

### Gemeinsamer Arbeitsweg V1

`query_agent_work_trace_turns`, `query_agent_work_trace_detail`,
`query_agent_work_trace_sources` und `query_agent_work_trace_source_preview` sind die
projektgebundene V31-Projektion für Ask, Plan und Agent-Vorbereitung. Turnlisten enthalten höchstens
32 Einträge, Detailseiten höchstens 64 Ereignisse und Source-Seiten höchstens 50 Einträge. Ein
Schritt kann eine begrenzte öffentliche Notiz mit Ziel, epistemisch gekennzeichnetem Befund,
Evidence-Lücke, nächstem Schritt und opaken Source-Referenzen tragen. `legacy=true` kennzeichnet
V30-Ask-Daten ohne nachträglich erfundene Notizen. Requests akzeptieren weiterhin keine Pfade,
Evidence-, Snapshot-, Provider-, Task- oder Run-IDs. Cursor und Source-Capabilities bleiben an
Worktree, Session, Turn, Trace-Revision und aktuellen Index gebunden.
`query_agent_work_trace_detail_v2` behält dieselbe sichere Projektion und ist der neue
Frontend-Read für die um fünf geschlossene Analyseaktionen erweiterte Modellschema-Version.

ADR-0040 ergänzt `query_agent_work_trace_projection` V1 als bevorzugten UI-Read. Er liefert
Detail, Zähler, eine opake Projektionsreferenz und die ersten höchstens 50 Quellen gemeinsam.
`query_agent_work_trace_sources_v2` akzeptiert ausschließlich Session, Nutzersequenz, diese
Projektionsreferenz und einen opaken Cursor. Ein zwischenzeitlicher Event-, Quellen- oder
Indexwechsel liefert `updating` beziehungsweise `projectionChanged`; Pfade, Source-, Snapshot-,
Task-, Run- und Provider-IDs bleiben als Requestparameter ausgeschlossen. Quellen tragen nur das
öffentliche turnlokale Label `S1` bis `S200`; die vorhandenen V1-Reads bleiben kompatibel.

### Slash Commands und Diagrammartefakte

`query_agent_slash_commands` V1 akzeptiert nur Protokollversion und Modus und liefert den festen
Core-Katalog mit kompatiblen Hauptaufträgen, Linsen, Argumentverhalten und fester Tiefe.
`submit_agent_message_v3` ergänzt `researchDepth: standard | thorough | command`. `command` ist
nur für einen im Rust-Core vollständig validierten Slash-Aufruf erlaubt; unbekannte Commands,
doppelte oder inkompatible Linsen, mehrere Hauptaufträge und fehlende Pflichtargumente erzeugen
weder Sessionentry noch Job. V1 und V2 bleiben kompatibel.

Commands mit Core-definiertem Rückfrageverhalten dürfen ohne Ziel eingehen. Sie erzeugen genau
eine verständliche `AwaitingUser`-Antwort, aber keinen Recherche- oder Agent-Run. Antwortet der
Nutzer unmittelbar mit normalem Text, rekonstruiert der Core das fehlende Ziel aus diesem Text und
den V32-Commanddaten; die WebView übergibt dafür keine Command-, Task- oder Run-ID.

`query_agent_session_v2` verwendet die begrenzte V1-Sessionseite weiter und ergänzt pro darin
sichtbarer Usersequenz ausschließlich die persistierten Command-Chips und Diagrammzusammenfassungen.

`query_agent_diagram_artifacts` und `query_agent_diagram_artifact` V1 akzeptieren Session und
positive Usersequenz beziehungsweise eine opake Artefaktreferenz. Sie liefern höchstens drei
Core-kompilierte Diagramme mit Typ, Titel, Beschreibung und Stale-Kennzeichnung. Referenzen sind an
Worktree, Session, Turn und Artefakt gebunden. `export_agent_diagram` V1 akzeptiert zusätzlich nur
geschlossenes Format und Theme sowie begrenzte gerenderte Nutzdaten. Der native Adapter wählt den
Zielpfad; keine Response gibt ihn zurück. Requests akzeptieren insbesondere keine Pfade, argv,
Source-, Evidence-, Snapshot-, Task-, Run- oder Provider-IDs.

## Agent Diff und Verification Inspector V1

`query_agent_inspection` akzeptiert genau `protocolVersion` und die bereits ausgewählte opake
`taskId`. Projekt und Worktree stammen aus dem Composition Root. Run, Step, Verification-Spec,
Snapshot, Pfad, Command, Process, Policy und Evidence sind keine Requestfelder. Die Antwort
unterscheidet `noProject`, `taskNotFound`, `ledgerUnavailable`, `goalRevisionMismatch`,
`inspectionChanged` und `available`.

`available` verbindet zwei ausdrücklich verschiedene Quellen. Der flüchtige Teil enthält eine
optionale positive `inspectionRevision`, die exakte bereits secret-geprüfte E3-Patchprojektion und
content-freie abgeschlossene Prozesszeilen. Jede Patchdatei trägt verlustfreies `pathHex`, eine
getrennte kontrollzeichenfreie Anzeige, Operation, vollständigen Hash und Bytezahl, Encoding,
Line-Endings, Prefix-Trunkierung, verlässliche Provenienz und die vom Core berechneten gemeinsamen
Hunkzeilen für Unified und Side-by-side. Die WebView berechnet keine Diffsemantik. Prozesszeilen
tragen Test-, Build-, Diagnostic-, Lint-, Format- oder Command-Kategorie, Termination, Dauer sowie
Digest, beobachtete/retained Bytes, Limit, Redaction und `sourceTruncated`, aber keinen Logtext.

Der dauerhafte Teil enthält den konsistent gelesenen Goal-/Ledger-/Published-Snapshotanker, alle
aktiven Steps und ihre Verification-Attempts sowie alle exakt referenzierten E6-Artefakte.
Command-, strukturierte Test-, Diff-, Diagnostic- und UserConfirm-Details bleiben typisiert;
Diff-Evidence enthält höchstens 128 kanonisch geordnete exakte Pfade.
Semantik und Freshness werden im Application-Kern neu abgeleitet. Jedes Must-/Should-Kriterium
trägt genau `proven`, `pending`, `failed`, `stale` oder `missing`; nur `proven` darf exakte
beweisende Step- und Evidence-IDs enthalten. Stale oder fehlgeschlagene Evidenz kann daher nie als
Done-Beweis erscheinen.

`query_agent_inspection_log` akzeptiert zusätzlich ausschließlich die zuvor emittierte kanonische
positive `inspectionRevision`, eine 64-stellige `inspectionId`, `stdout|stderr`, einen u32-Bytecursor
und ein Limit von 4 bis 16.384 Bytes. Das Minimum garantiert auch vor einem vier Byte langen
UTF-8-Skalar einen begrenzten Seitenfortschritt. Der Core revalidiert Projekt, Task, Revision, Record, Run,
Step, Verification-Spec und Published Snapshot vor jeder Seite. Die Antwort unterscheidet
`noProject`, `unavailable`, `inspectionChanged` und `available`. Eine Seite trennt
`pageTruncated` für gezielt nachladbaren retained Text von `sourceTruncated` für dauerhaft
verworfenes Overflow. Redigierte Streams liefern leeren Text und keine Folgeseite. Beide Commands
sind rein lesend und gewähren weder Datei-, Source-, Prozesswiederholungs-, Approval-, Shell-,
SQL-, Provider- noch Mutationsbefugnis.

## Agent Approval Center V1

`query_agent_approval` akzeptiert genau `protocolVersion` und die bereits ausgewählte opake
`taskId`. Projekt und Worktree stammen aus dem Composition Root. Request-, Grant-, Run-, Snapshot-,
Step-, Verification-Spec-, Process-, Policy- und Event-IDs sind keine Requestfelder. Die Antwort
unterscheidet `noProject`, `taskNotFound`, `ledgerUnavailable`, `goalRevisionMismatch`,
`activityChanged`, `unavailable` und `available`.

`available` enthält die prozesslokale positive `approvalRevision`, Ledgerrevision/-Storeversion,
Controller- und Stepzustand, sichtbare Step-/Snapshotanker, Scope-Digest, abgeleitete Klasse,
Risiko, trusted Policy-Grund, Request-/Ablaufzeit, effektiven Lifecycle und disjunkte Controls. Die
exakte Aktion ist entweder ein Patch mit Rationale und höchstens 64 Add-/Update-/Move-/Delete-
Pfadformen oder eine ProcessSpec. Pfade tragen eine kontrollierte Anzeige und verlustfreie
Hexbytes. ProcessSpec behält getrennte argv-Tokens, CWD, höchstens 64 kanonische Env-Namen ohne
Werte, Timeout, positive Outputlimits, Execution Mode, Planbindung, Netzwerkscope und
Specification-ID. Der unabhängige TypeScript-Decoder prüft Schlüsselmenge, IDs, Dezimalwerte,
Grenzen, Pfadformen, argv-Gesamtgröße, Env-Namensyntax, Lifecycle-/Control-Paare und Planbindung.

`control_agent_approval` akzeptiert zusätzlich ausschließlich die sichtbare positive
`expectedApprovalRevision`, positive `expectedLedgerRevision`, kanonische positive
`expectedLedgerStoreVersion` und genau `allowOnce`, `deny`, `continue` oder `revoke`. Der Core
erzeugt Approval-/Event-ID und Zeit. `allowOnce` antwortet `grantStored`, startet aber keine Arbeit.
`continue` liefert `continueRequested` und genau ein `runtimeStart` von `queued`, `unavailable` oder
`failed`; die interne Grant-ID wird nie ausgegeben. `revoke` liefert `revoked`. `deny` liefert erst
nach dem atomaren Step-`Blocked`-/Run-`Failed`-Commit `denied`. Alle anderen Outcomes müssen
`runtimeStart: null` tragen. Veraltete Anker liefern `activityChanged`, nicht mehr zulässige
Lifecycle-Aktionen `unavailable`.

Die beiden expliziten Capabilities `allow-query-agent-approval` und
`allow-control-agent-approval` gewähren keinen Datei-, Source-, Shell-, SQL-, Provider-, Netzwerk-
oder allgemeinen Schedulerzugriff. Insbesondere ist weder ein breites wiederverwendbares
Scope-Allow noch eine WebView-gesteuerte Grant-ID darstellbar.

## Agent Task Recovery und Control V1

`query_agent_task_recovery` akzeptiert genau `protocolVersion` und die bereits ausgewählte opake
`taskId`. Der Core leitet den einzigen steuerbaren Run aus dem aktiven retained Ledger-Versuch ab;
Run-, Snapshot-, Step-, Evidence- oder Worktree-IDs sind keine Requestfelder. Die Abfrage markiert
nach einem Appneustart verlassene In-flight-Toolversuche dauerhaft als `Interrupted` und liefert
nur content-freie Recovery-Fakten: endlichen Controllerzustand, Run-/Published-Snapshotanker,
Snapshotwechsel, stale Evidence Count, Interrupted Count sowie die beiden Unknown-Mutationsgates.
Besitzt der aktuelle Prozess dagegen noch einen Worker, wird diese unterbrechende H11-Inspektion
nicht ausgeführt. `runtimeOwned` liefert nur Ledgerrevision/-Storeversion, nichtterminalen
Controllerzustand, `queued|running|pausing|cancelling` und das ausschließlich für `running` wahre
`canPause`. Nach tatsächlich beendetem Worker und erfolgreicher H11-Revalidierung liefert
`paused` dieselben autoritativen Recovery-Fakten wie `available`, ohne `AgentControllerState` um
einen Pausezustand zu erweitern.

`control_agent_task_run` akzeptiert zusätzlich genau eine geschlossene Aktion `pause`, `resume`,
`replan` oder `cancel` sowie die zuvor sichtbare positive `expectedLedgerRevision` und den kanonischen
positiven u64-Dezimaltext `expectedLedgerStoreVersion`. Event-ID, Zeit, Run und aktueller Published
Snapshot stammen ausschließlich aus dem Core. H11/E8 prüft Evidence und Mutation Disposition neu
und committed Published-Snapshot-, Ledger-Version- und Run-Sequenz-CAS gemeinsam. `resume` ist bei
staler Evidence oder einem ausstehenden Mutation-Replan gesperrt; `replan` und `resume` sind vor
der autoritativen Reconciliation einer unbekannten Wirkung gesperrt; `cancel` bleibt erreichbar
und liefert nach erfolgreichem Commit den terminalen Controllerzustand `cancelled`.

Für einen lebenden Worker quittiert `accepted` ausschließlich `pauseRequested` oder
`cancelRequested`; es behauptet weder `Paused` noch einen dauerhaften Cancel. Pause ist nur für
`running` zulässig. Cancel stoppt zuerst den Worker und verwendet danach die exakt übergebenen
Ledgeranker für den H11-CAS. Ein atomar angewandtes Resume oder Replan enthält zusätzlich
`runtimeStart`: `queued` für einen neu angenommenen Scheduler-Job, `unavailable` ohne verifizierte
Executor-Capability oder `failed`, wenn der Recovery-Commit zwar dauerhaft war, der begrenzte
Scheduler den neuen Versuch aber nicht annehmen konnte. Beim terminalen Cancel ist das Feld null.

Beide Antworten unterscheiden fehlendes Projekt, Task, Ledger oder Run, Goal-Mismatch,
gleichzeitige Änderung und nicht steuerbare terminale/historische Runs als geschlossene Zustände.
Unbekannte Felder, nicht kanonische IDs/Dezimalwerte, widersprüchliche Snapshot-/Resume-Flags und
unmögliche Outcome-/Controller-Paare werden am Rust- und TypeScript-Rand abgelehnt. Der Vertrag
gewährt weder Scheduler-, Provider-, Datei-, Shell-, SQL- noch Journalzugriff.

## Health Response V1

`query_health` liefert:

| Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | exakt `1` |
| `applicationVersion` | String | stammt aus validierter `ApplicationVersion` |
| `platform` | `PlatformV1` | `windows`, `linux`, `macOs` oder `unsupported` |
| `status` | `HealthStatusV1` | im Walking Skeleton `Ready` |

## Open Project Response V1

`open_project` öffnet genau einen nativen Ordnerdialog im privilegierten Prozess. Erkennt der Kern
danach genau einen evidenzbasierten Umzugskandidaten, darf er zusätzlich einen nativen
Bestätigungsdialog mit „reconciliieren“, „separat öffnen“ und „abbrechen“ anzeigen. Beide Abbruchpfade
liefern `result.kind` `cancelled`. Nach erfolgreicher normaler Registrierung oder bestätigter
Reconciliation lautet es `opened` und enthält `project`:

| Feld | Typ | Invariante |
| --- | --- | --- |
| `repositoryId` | String | 64-stellige kleingeschriebene Hex-ID |
| `worktreeId` | String | 64-stellige kleingeschriebene Hex-ID |
| `worktreeRootDisplay` | String | nicht autoritativ, kontrollzeichenfrei, höchstens 32.768 Zeichen |
| `head` | `GitHeadV1` | `born` mit Objekt-ID oder `unborn` mit vollständiger Referenz |

Der bestehende V1-Vertrag enthält absichtlich keine `ProjectId`; seine JSON-Form bleibt durch die
Katalogregistrierung und Reconciliation unverändert. Kandidat, Evidence, Katalogrevision und
Bestätigungsentscheidung sind interne Application-/Adaptertypen und keine IPC-Felder oder Commands.

## Projektkatalog V1

`list_recent_projects` bleibt als begrenzter Legacy-Read kompatibel. Die Projects-Fläche verwendet
`query_project_catalog`. Dessen Request enthält exakt `protocolVersion`, optionales `search`,
optionales `cursor` und `direction` (`initial | next | previous`). `initial` verbietet einen Cursor;
die beiden Navigationsrichtungen verlangen einen 16-stelligen kleingeschriebenen Hex-Cursor. Die
kontrollzeichenfreie Suche ist getrimmt und auf 128 Zeichen begrenzt.

Die Antwort enthält immer exakt `protocolVersion`, höchstens 25 `projects` sowie optionale
`previousCursor` und `nextCursor`. Jeder Eintrag enthält eine 64-stellige kleingeschriebene
`projectId` und unter `project` dieselbe sichere `ProjectSummaryV1`-Projektion wie `open_project`.
Autoritative gespeicherte Pfadbytes, Git Common Directory, Remote-URLs, Datenbankzeilen und
Adapterfehler werden nicht übertragen. Die Sortierung ist „zuletzt erfolgreich aktiviert zuerst“;
Suche filtert nur die sichere Root-Anzeige.

`activate_catalog_project` und `remove_catalog_project` akzeptieren neben `protocolVersion`
ausschließlich eine kanonische 64-stellige kleingeschriebene `worktreeId`. Der Core löst sie gegen
den Katalog auf; Pfad, `projectId` und `repositoryId` können nicht geliefert werden.
`restore_last_project` verwendet nur den gemeinsamen pfadlosen Request und liefert entweder
`noSavedProject` oder `activated` mit `projectId` und sicherer `project`-Projektion. Es versucht nur
den jüngsten Eintrag und enthält keine Fallback-Liste. Aktivierung und Wiederherstellung
revalidieren den gespeicherten Root und beide Git-Identitäten vor dem Runtime-Wechsel.

`remove_catalog_project` verwendet die bestehende `RemoveProjectResponseV1`-Bestätigung `removed`.
Die feste Semantik entfernt nur den Katalogeintrag und offene Reconciliation-Absichten; Repository,
Worktree, Quellcode und private `knowledge.db` bleiben außerhalb des Commands.

## Settings und Provider Model Catalog V1

`query_settings` akzeptiert nur `protocolVersion`. `configure_model_provider` akzeptiert
zusätzlich ausschließlich die erwartete dezimale Settingsrevision, die geschlossene
`providerKind`-Auswahl (`ollama | gemini | openai`) und einen optionalen credential-freien
Endpoint-Origin. Ein
fehlender Endpoint entfernt die aktive Verbindung und invalidiert Rollenprofile atomar.

`set_model_provider_credential` akzeptiert ausschließlich `protocolVersion`,
`expectedSettingsRevision` und 1 bis 4.096 uninterpretierte `apiKeyBytes`.
`delete_model_provider_credential` akzeptiert nur Version und Revision. Provider, Endpoint,
Credential-Anforderung und Generation stammen in beiden Fällen aus der exakt aktuellen
Core-Settingsrevision. Der Set-Request ist eine reine Deserialisierungsgrenze; keine Response,
Debugausgabe oder serialisierbare DTO enthält Schlüsselmaterial. `SettingsV1` liefert nur
Endpoint-Access (`local | remoteBlocked | explicitUserInitiatedRemote`) und den optionalen
Credential-Status (`missing | configured | recoveryRequired | unavailable`).

`discover_provider_models` akzeptiert neben `protocolVersion` nur `expectedSettingsRevision`.
Endpoint, Provider-ID, Modellname, Capabilitystatus und Timeout stammen nicht aus der WebView. Die
Antwort bindet die unveränderte Settingsrevision, `providerKind`, `truncated` und höchstens 256
eindeutige, streng sortierte Modell-IDs. Sie ist flüchtige Auswahlhilfe und kein `ModelProfile`.
`probe_model_role` bleibt die einzige Modellaktivierungsgrenze; Discovery und Probe lassen sich
über den gemeinsamen engen `cancel_model_probe`-Command kooperativ abbrechen.

## Command Error V1

Ein syntaktisch gültiger Requestfehler erhält einen sicheren, serialisierbaren Fehler:

| JSON-Feld | Typ | V1-Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` | exakt `1` |
| `code` | `ErrorCodeV1` | stabiler maschinenlesbarer Code |
| `message` | String | feste sichere Meldung ohne interne Fehlerdetails |

Neben den Projektinspektionsfehlern unterscheidet V1 lokale Storage-Nichtverfügbarkeit, Korruption,
eine neuere nicht unterstützte Schemaversion, ungültige persistierte Daten und einen
Projektidentitätskonflikt. Ungültige Katalogsuchen, Cursor oder IDs verwenden
`invalidProjectCatalogRequest`. Die Fehlermeldung enthält keine SQL-Texte, Enginefehler oder
Rohpfade.
Provider-Credentials verwenden zusätzlich die stabilen Codes `providerCredentialInvalid`,
`providerCredentialMissing`, `providerCredentialRecoveryRequired` und
`providerCredentialStoreUnavailable`; ihre Meldungen enthalten nie Credential-Material.

## Tauri-Capability

Die Desktop-Capability `main-capability` erlaubt dem Hauptfenster ausschließlich die dokumentierten
Health-, Project-, Settings-, Provider-Model-Catalog-, Index-, Repository-Tree-, Module-Tree-, Module-Dependency-Graph-,
Module-Runtime-, Module-Card-, Project-Map-, Task-Lens-, Agent-Goal-, Agent-Activity-,
Agent-Recovery-, Agent-Approval-, Agent-Ask-Recherche- und Deep-Map-Commands. Repository- und
Modulbaum besitzen
ausschließlich `allow-query-repository-tree` beziehungsweise `allow-query-module-tree`; der
Abhängigkeitsgraph besitzt nur `allow-query-module-dependency-graph`, die Freshness-Capability ist
`allow-query-module-card-freshness`. Runtime-Roots und feste Flows besitzen ausschließlich
`allow-query-module-runtime-map` und `allow-query-module-runtime-flow`.
`allow-query-module-card-detail` ist die einzige Capability für Card-Inhalte und akzeptiert nur
eine stabile Modul-ID. Für Deep Map sind das
`allow-query-deep-map`, `allow-query-deep-map-runs`, `allow-query-deep-map-entries`,
`allow-query-deep-map-entry-detail`, `allow-start-deep-map`, `allow-pause-deep-map`,
`allow-resume-deep-map` und `allow-cancel-deep-map`. Es gibt keine generische Datei-, Dialog-,
Shell-, Provider-, Netzwerk- oder SQL-Capability.
Für Ask-Recherche sind ausschließlich `allow-query-agent-ask-research-turns`,
`allow-query-agent-ask-research-detail`, `allow-query-agent-ask-research-sources` und
`allow-query-agent-ask-research-source-preview` freigeschaltet.
Slash Commands und Diagramme verwenden ausschließlich
`allow-query-agent-slash-commands`, `allow-query-agent-session-v2`,
`allow-submit-agent-message-v3`,
`allow-query-agent-diagram-artifacts`, `allow-query-agent-diagram-artifact` und
`allow-export-agent-diagram`. Der Exportcommand öffnet seinen Dialog ausschließlich im nativen
Rust-Adapter und stellt der WebView keine allgemeine Dialog- oder Dateicapability bereit.
Der Projektkatalog besitzt ausschließlich `allow-query-project-catalog`,
`allow-activate-catalog-project`, `allow-restore-last-project` und
`allow-remove-catalog-project`; keine dieser Capabilities akzeptiert einen Pfad.
