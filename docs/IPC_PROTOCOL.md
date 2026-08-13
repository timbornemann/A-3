# IPC-Protokoll

Status: verbindliche Baseline

Stand: 2026-08-04

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
`query_health`, `open_project` und `list_recent_projects` auch für Projekt-, Index- und
Deep-Map-Status sowie für `pause_deep_map`, `resume_deep_map` und `cancel_deep_map`. Ihr gemeinsamer
V1-Request enthält ausschließlich:

| JSON-Feld | Typ | Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` über `ProtocolVersion` | für V1 exakt `1` |

Zusätzliche Felder, ein fehlender Request oder ein nicht numerischer Versionswert werden vor
Ausführung des jeweiligen Use Cases abgelehnt. Insbesondere akzeptiert `open_project` keinen Pfad und
`list_recent_projects` weder einen Pfad noch ein WebView-gesteuertes Limit.

## Deep Map V1

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

## Recent Projects Response V1

`list_recent_projects` liefert most-recent-first höchstens zehn Einträge. Jeder Eintrag enthält eine
64-stellige kleingeschriebene `projectId` und unter `project` dieselbe sichere
`ProjectSummaryV1`-Projektion wie `open_project`. Autoritative gespeicherte Pfadbytes, Git Common
Directory, Remote-URLs, Datenbankzeilen und Adapterfehler werden nicht übertragen.

Die UI lädt diese Projektion beim Start und nach einem erfolgreichen Open erneut. Auswahl, erneutes
Öffnen oder Entfernen eines Katalogeintrags sind nicht Teil dieses V1-Teilschnitts.

## Command Error V1

Ein syntaktisch gültiger Requestfehler erhält einen sicheren, serialisierbaren Fehler:

| JSON-Feld | Typ | V1-Invariante |
| --- | --- | --- |
| `protocolVersion` | `u16` | exakt `1` |
| `code` | `ErrorCodeV1` | stabiler maschinenlesbarer Code |
| `message` | String | feste sichere Meldung ohne interne Fehlerdetails |

Neben den Projektinspektionsfehlern unterscheidet V1 lokale Storage-Nichtverfügbarkeit, Korruption,
eine neuere nicht unterstützte Schemaversion, ungültige persistierte Daten und einen
Projektidentitätskonflikt. Die Fehlermeldung enthält keine SQL-Texte, Enginefehler oder Rohpfade.

## Tauri-Capability

Die Desktop-Capability `main-capability` erlaubt dem Hauptfenster ausschließlich die dokumentierten
Health-, Project-, Index-, Repository-Tree-, Module-Tree-, Module-Dependency-Graph-,
Module-Runtime-, Module-Card-, Project-Map-, Task-Lens-, Agent-Goal-, Agent-Activity-,
Agent-Recovery-, Agent-Approval- und Deep-Map-Commands. Repository- und
Modulbaum besitzen
ausschließlich `allow-query-repository-tree` beziehungsweise `allow-query-module-tree`; der
Abhängigkeitsgraph besitzt nur `allow-query-module-dependency-graph`, die Freshness-Capability ist
`allow-query-module-card-freshness`. Runtime-Roots und feste Flows besitzen ausschließlich
`allow-query-module-runtime-map` und `allow-query-module-runtime-flow`.
`allow-query-module-card-detail` ist die einzige Capability für Card-Inhalte und akzeptiert nur
eine stabile Modul-ID. Für Deep Map sind das
`allow-query-deep-map`, `allow-start-deep-map`, `allow-pause-deep-map`,
`allow-resume-deep-map` und `allow-cancel-deep-map`. Es gibt keine generische Datei-, Dialog-,
Shell-, Provider-, Netzwerk- oder SQL-Capability.
