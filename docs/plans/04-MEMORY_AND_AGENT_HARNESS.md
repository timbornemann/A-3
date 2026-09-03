# Plan 04: Memory, Context Compiler und read-only Agent Harness

Ziel: Ein lokales Modell kann eine Aufgabe zieltreu analysieren, planen und mit Read-only-Werkzeugen bearbeiten; Zustand und Evidenz überleben Neustart und Context Compaction.

Relevante ADRs: 0008, 0009, 0010, 0011, 0013

## H1 Goal Contract

Abhängigkeiten: Gate M5

- [x] GoalContract-Domänentyp
- [x] AcceptanceCriterion und Constraint
- [x] Non-Goals und User Decisions
- [x] Revision statt stiller Mutation
- [x] UI- und Persistenz-DTO
- [x] Validierungsregeln

Akzeptanz:

- kein Agentenrun ohne validen Goal Contract;
- Zieländerung erzeugt neue Revision;
- alte Revision bleibt auditierbar.

Verifiziert am 2026-08-06: Die infra-freie Domäne erzeugt die run-sichere Referenz ausschließlich
aus einem validen Contract. Application-Port und libSQL-Schema V11 erstellen Revision eins atomar,
hängen nur den unmittelbaren Nachfolger an und rekonstruieren aktuelle wie historische Revisionen
nach Reopen. Der gemeinsame Storage-Contract belegt Isolation, Konfliktablehnung und unveränderte
Auditstände. Rust- und TypeScript-V1-DTOs besitzen stabile JSON-Formen und strikte Grenztests.

## H2 Task Ledger

Abhängigkeiten: H1

- [x] TaskStep und StepDependency
- [x] VerificationSpec
- [x] Zustandsübergänge
- [x] genau ein InProgress-Schritt je Run
- [x] Replan mit Historie
- [x] Stale nach Evidenceinvalidierung

Akzeptanz:

- Completed ohne erfolgreiche Verification ist unmöglich;
- zyklische StepDependencies werden abgelehnt;
- Neustart stellt exakt den letzten Ledgerzustand her.

Verifiziert am 2026-08-06: Die infra-freie Domain modelliert typisierte Schrittdefinitionen,
Abhängigkeiten, bounded Evidence, unveränderliche Versuche, strikte Statusübergänge und
evidence-gebundene Verifikationen. Nur eine passende erfolgreiche Verifikation kann `Completed`
erzeugen; Zyklen und ein zweiter aktiver Versuch werden abgelehnt. Replans behalten ersetzte
Schritte und ihre Historie, während Evidenceinvalidierung direkte und transitive Abschlüsse auf
`Stale` setzt. Application-Port und libSQL-Knowledge-Schema V12 speichern das vollständige Ledger
atomar mit Compare-and-Swap-Version. Der gemeinsame Storage-Contract belegt Konfliktablehnung,
unveränderliche Historie, Linked-Worktree-Isolation und exakte Rekonstruktion nach Reopen.

## H3 Run Journal

Abhängigkeiten: H2

- [x] AgentRun und RunEvent
- [x] monotone Sequenznummer
- [x] atomare Event- plus Zustandsaktualisierung
- [x] sichere Payloadredaction
- [x] Retention und Exportformat

Akzeptanz:

- paralleles Eventappend erzeugt keine doppelte Sequenz;
- Eventjournal enthält keine Secret-Fixtures;
- Journalverlust ist nicht nötig, um materialisierten Zustand zu lesen.

Verifiziert am 2026-08-06: Die infra-freie Domain bindet jeden `AgentRun` an Goal Contract,
Task-Ledger-Revision und Snapshot, erzwingt die dokumentierte endliche Zustandsmaschine und erzeugt
nur lückenlose, typisierte `RunEvent`s. Der Application-Port paginiert höchstens 256 Events und
exportiert mit Cancellation, monotonem Progress sowie festen Event-/Bytegrenzen das deterministische
`a3.run-journal.jsonl` V1. Seine Payload kann ausschließlich geschlossene Codes, Outcomes und
content-freie Redaktionsmetadaten enthalten. Knowledge-Schema V13 erstellt Run plus StartEvent und
CAS-appendet jedes weitere Event samt Materialisierung atomar. Der gemeinsame Storage-Contract
belegt genau einen Gewinner konkurrierender Appends, Worktree-Isolation, Reopen, Paging,
deterministischen Export und das Fehlen der Secret-Fixture; ein Adaptertest belegt zusätzlich, dass
die relationale Run-Materialisierung selbst nach simuliertem Journalverlust ohne Replay lesbar ist.
Die V1-Retention erhält alle sicheren Audit-Events unverändert.

## H4 ModelProvider

Abhängigkeiten: Foundation Job primitives

- [x] neutraler ModelProvider-Port
- [x] Ollama-kompatibler Adapter
- [x] Streaming ProviderEvents
- [x] Timeouts und Cancellation
- [x] sichere Endpoint Policy
- [x] normalisierte Fehler
- [x] Stubprovider für Tests

Akzeptanz:

- Application importiert keine Ollama-Payloadtypen;
- Streamingabbruch beendet Request;
- nicht lokaler Endpoint benötigt Policyfreigabe.

R8 zieht ausschließlich den neutralen, request-/timeout-/cancellation-fähigen
`ExplorerModelProvider` samt Stubprovider für Explorer-Vertragstests vor. Dieser schmale Port kennt
keine Providerpayloads oder Endpoints, erfüllt aber bewusst noch nicht H4: allgemeiner
`ModelProvider`, Streaming-Events, Endpoint Policy und Ollama-Adapter bleiben offen.

Verifiziert am 2026-08-06: ADR-0018 ordnet den allgemeinen Port eindeutig der Application-Grenze
und den konkreten HTTP-/NDJSON-Adapter `a3-provider` zu. Die neutralen, begrenzten und in Debug
redigierten Requests und `ProviderEvent`s kennen weder Ollama- noch HTTP-Typen. Der
Ollama-kompatible Adapter serialisiert das statische Chat-Mapping, akzeptiert ausschließlich
strikte begrenzte NDJSON-Streams, wartet mit einem terminalen Event bis zum sauberen Body-Ende und
normalisiert alle Fehler content-frei. Ein redirect- und proxyfreier Client prüft die dynamische
Endpoint-Policy vor jedem Request; Local-only ist Standard, Remote erfordert HTTPS plus exakte
Freigabe. Connect und Body-Reads sind wakebar abbrechbar, und ein Gesamttimeout umfasst den
vollständigen Stream. Acht Offline-Adaptertests belegen Mapping, Fragmentierung, Cancellation samt
Verbindungsabbau, Timeout vor Headern und im Body, Policyablehnung vor Netzwerk sowie Parser-
Negativfälle. Drei neutrale Stubtests belegen Event-, Fehler- und Cancellation-Verhalten ohne
Promptpersistenz. Workspace-Test, Clippy mit `-D warnings`, Rustdoc und der Dependency-/Lizenzbericht
waren vollständig grün; `a3-application` besitzt weder `reqwest`- noch `a3-provider`-Abhängigkeit.

## H5 ModelProfile und Capability Probe

Abhängigkeiten: H4

- [x] Context- und Outputlimit
- [x] Structured Output
- [x] Tool Call Mode
- [x] Tokenizer oder konservativer Counter
- [x] Parallelitätslimit
- [x] Startprobe mit kleinem Schema
- [x] manueller Profile Override

Akzeptanz:

- Fähigkeiten werden nicht nur aus Modellnamen erraten;
- fehlschlagende Structured-Output-Probe deaktiviert ausführbare Aktionen;
- Profile sind versioniert und in Runs referenziert.

Verifiziert am 2026-08-06: `ModelProfile` V1 bindet Provider und opaque Modell-ID an validierte
Context-/Outputgrenzen, konservative UTF-8-Bytezählung, Parallelität, fixed-point Sampling,
kanonische Stopbedingungen, Schema-Grounding und explizite Capability-Evidenz. Manuelle Overrides
ändern ausschließlich Laufparameter und übernehmen einen fehlgeschlagenen Capabilitystatus. Jeder
neue Run referenziert Profil-ID plus Schemaversion; Knowledge-Schema V14 erhält diesen Bezug nach
Reopen und erlaubt nur migrierten Legacy-Runs das vollständige Nullpaar.

Der Application-Port `ModelCapabilityProbe` und der Use Case `ProbeModelProfile` erzeugen Profile
ohne Modellnamensheuristik und lehnen konfigurierte Contextlimits oberhalb eindeutiger
Providermetadaten ab. Der Ollama-Adapter kombiniert begrenztes `/api/show` mit einer realen kleinen
Strict-Schema-Anfrage an `/api/chat`, autorisiert beide Netzwerkzugriffe separat und teilt ein
Gesamttimeout. Nur das exakte Probeobjekt aktiviert Structured Output; Toolmetadaten allein bleiben
nicht ausführbar. Normale Requests tragen das vollständige Profil, blockieren ungeprüfte Schemas
vor HTTP und bilden Context, Output, Sampling sowie Stops auf Ollama-Optionen ab.

Vier Domain-Profiltests, sechs neutrale Port-/Stubtests, fünf Ollama-Unittests und neun vollständige
Offline-HTTP-Adaptertests decken Grenzen, deterministische Identität, Override-Sicherheit,
Metadatenambiguität, exakte Requests und Antworten, Policy, Cancellation und die gemeinsame
Deadline ab. `cargo fmt --check`, Workspace-Test mit allen Features, Workspace-Clippy mit
`-D warnings`, Rustdoc mit `-D warnings`, Markdown-Linkcheck, Tooltests und der deterministische
Dependency-/Lizenzbericht sind vollständig grün; der Bericht enthält keine unbekannten Lizenzen.

## H6 Prompt und Action Schemas

Abhängigkeiten: H1, H2, H5

- [x] kompakter statischer Systemvertrag
- [x] versionierte AgentAction-Union
- [x] Search, Inspect, UpdateLedger und Finish in read-only Phase
- [x] JSON-Schema-Erzeugung
- [x] Strict Validator
- [x] maximal ein Repair Attempt

Akzeptanz:

- unbekannte Toolnamen und Felder werden abgelehnt;
- Text außerhalb des Schemas wird nicht ausgeführt;
- Prompt bleibt unter statischem Budget.

Das in R8 vorgezogene `deep-map-explorer-action-v1`-Schema und sein Strict Validator decken nur die
read-only Deep-Map-Union Inspect, Search und Propose ab. Die allgemeine AgentAction-Union mit
Ledger- und Finish-Aktionen sowie ihre Promptintegration bleibt Teil von H6.

Verifiziert am 2026-08-06: `AgentActionSchemaVersion::V1` schließt Search, fünf begrenzte Inspect-
Ziele, nicht verifizierende Ledger-Intents und eine inhaltslose Finish-Anforderung zu einer Domain-
Union ohne Workspace-Mutation. Patch-, Prozess-, Shell-, Git-, Netzwerk-, Publish- und destruktive
Aktionen sind nicht darstellbar. Das eingebettete `agent-action-v1`-JSON-Schema wird an der
providerneutralen Formatgrenze bereitgestellt; der unabhängige Decoder begrenzt Dokumente auf
64 KiB und lehnt unbekannte Actions, Felder, Versionen, Trailing Text, unsichere Pfade, nicht
kanonische IDs und verletzte Domaininvarianten ab.

Der statische Systemvertrag bleibt mit dem konservativen ModelProfile-Counter unter 900 Tokens,
behandelt Repositoryinhalt als untrusted data und kann nur mit live verifiziertem Structured
Output vorbereitet werden. Profilabhängiges Schema-Grounding verwendet exakt dieselbe kanonische
Schemafassung. Eine ungültige Primärausgabe erzeugt genau eine nicht clonebare Repair-Befugnis;
ihre content-freie Anweisung verbraucht sie, und eine zweite ungültige Ausgabe ist terminal.
Zwei Domain-, vier Codec- und vier Prompt-/Repair-Tests belegen diese Grenzen. `cargo fmt --all --
--check`, fokussierte Domain-/Application-Tests, Workspace-Test mit allen Features, Workspace-
Clippy mit `-D warnings`, Rustdoc mit `-D warnings`, Markdown-Linkcheck, Tooltests und der
deterministische Dependency-/Lizenzbericht sind vollständig grün; der Bericht enthält keine
unbekannten Lizenzen.

## H7 Context Compiler Core

Abhängigkeiten: H1, H2, Task Lens

- [x] Anchor
- [x] Retrieve
- [x] Rank
- [x] Pack
- [x] Validate
- [x] ContextDigest
- [x] Bereichsbudgets und Outputreserve
- [x] Deduplizierung und Zoom

Akzeptanz:

- Goal Contract und aktueller Step sind immer enthalten;
- stale Fact Fixture wird blockiert;
- 16k-Profil hält die definierten Budgets;
- gleiche Eingaben ergeben gleichen Digest.

Verifiziert am 2026-08-06: Der infra-freie Domainkern modelliert versionierte Bereichsbudgets,
tatsächliche Nutzung und `ContextDigest`; der Application-Port bindet Goal Contract, Ledgerrevision,
aktuellen Schritt, Modellprofil, optionale Seeds und begrenzte Toolresultate. Das neue
`a3-context`-Feature komponiert die vorhandene geordnete Task Lens für Retrieve/Rank und packt L0
bis L3, aktuelle Claims sowie snapshotgleiche Toolresultate deterministisch. Der End-to-End-
Contract belegt den vollständigen Goal-/Step-Anchor, Ausschluss eines stale Fact, lückenlose
Budgetrechnung des 16.384er-Profils mit 3.605 Outputtokens, bytegleichen Request und Digest bei
gleichen Eingaben sowie Cancellation vor dem ersten Read. Das reproduzierbare 100.000-
Strukturzeilen-/50.000-Symbole-Release-Fixture maß über 30 Samples Task Lens P95 141,473 ms und
vollständigen Context Compile P95 215,220 ms. `cargo fmt --all -- --check`, Workspace-Clippy mit
allen Targets/Features und `-D warnings`, Workspace-Tests mit allen Features und Rustdoc mit
`-D warnings` sind vollständig grün. Die unter dem gepinnten Node 24.14.0 direkt ausgeführten
Frontend-/Tool-Schritte bestätigen Prettier, ESLint, Svelte-Typecheck, 20 Frontendtests, Build und
vier Tooltests; der lokale pnpm-Wrapper war 11.16.0 statt des geforderten 11.9.0 und lehnte deshalb
nur den aggregierenden Skriptstart ab. Markdown-Linkcheck und der deterministische Bericht sind
grün; 608 Rust- und 239 JavaScript-Pakete enthalten keine unbekannte Lizenz.

## H8 Compaction

Abhängigkeiten: H3, H7

- [x] Step Result Materialization
- [x] Claims mit Evidence extrahieren
- [x] offene Fehler und Hypothesen behalten
- [x] redundante Runtexte aus aktivem Pack entfernen
- [x] Source IDs erhalten

Akzeptanz:

- Langlauffixture kann nach mehreren Compactions Ziel und offene Punkte korrekt nennen;
- keine Summary-of-Summary ohne Quellkette;
- Audit Events bleiben erhalten.

Verifiziert am 2026-08-06: `RunMemoryCheckpoint` wird deterministisch nur aus Goal Contract,
Task Ledger, materialisiertem Agent Run, aktuellem Published Index und originalen Claims
kompiliert. Terminale Versuche behalten Step-, Attempt-, Run- und direkte plus Verifikations-
Evidence-IDs; offene Fehler und aktive Hypothesen werden im nächsten Context Pack als Pflichtinhalt
reinjiziert. Ein vorheriger Checkpoint ist kein Eingang, und der Compiler besitzt keinen
Journal-Mutationsport. Der Langlauf-Contract baut dieselbe Projektion 64-mal aus den autoritativen
Quellen neu auf und behält Ziel, offene Punkte und Source-Kette; der bestehende Storage-Contract
weist weiterhin alle append-only Audit-Events nach Reopen nach. Evidence-basierte Freshness lässt
Claims aus einem älteren Source-Run nach einem unabhängigen Publish als Provenienz bestehen, wenn
ihre konkrete Evidence weiterhin aktuell auflösbar ist, und schließt stale oder inkompatible
Claims aus.

Der Context-Contract belegt die einmalige Reinjection von Step Result, offenem Fehler und
Hypothese, Secret-Ablehnung, Snapshot-/Run-Bindung, deterministischen Digest sowie die lückenlose
Budgetrechnung; Run Memory wird vor der Task Lens gegen `CodeAndEvidence` reserviert. `cargo fmt
--all -- --check`, Workspace-Clippy für alle Targets/Features mit `-D warnings`, alle Workspace-
Tests und Rustdoc mit `-D warnings` sind grün. Unter dem gepinnten Node 24.14.0 bestehen Prettier,
ESLint, Svelte-Typecheck, 20 Frontendtests, Build und vier Tooltests. Der lokale pnpm-Wrapper bleibt
mit 11.16.0 von der geforderten Version 11.9.0 verschieden, daher wurden die installierten
Lockfile-Werkzeuge direkt ausgeführt. Der Linkcheck prüfte 45 Markdown-Dateien und 66 lokale Links;
608 Rust- und 239 JavaScript-Pakete enthalten keine unbekannte Lizenz.

## H9 Controller State Machine

Abhängigkeiten: H2, H3, H6, H7

- [x] Intake
- [x] Localize
- [x] Plan
- [x] Execute read-only
- [x] Verify
- [x] Replan
- [x] AwaitApproval-Grundzustand
- [x] Done, Failed und Cancelled
- [x] Turn-, Token-, Tool- und Zeitbudgets

Akzeptanz:

- Property-Tests decken Zustandsübergänge;
- pro Turn höchstens eine Action;
- Budgetende führt zu erklärtem Failed oder AwaitingUser, nicht Endlosschleife;
- Done nur über Acceptance-Verifier.

Verifikation: Der Domain-Kern bindet jeden Turn an höchstens eine Actionklasse und persistiert
Budget sowie kumulativen Turn-, Prompttoken-, Outputtoken-, Action- und Repairverbrauch atomar mit
dem Run-Journal. Der Application-Controller prüft die vollständige Matrix aus zehn Zuständen und
zwölf Signalen; kein normaler Signalpfad kann `Done` erreichen. Erschöpfte Runs gehen aus
`Execute` einmal nach `AwaitApproval` und enden bei weiterer autonomer Fortsetzung nachvollziehbar
in `Failed`. `ExecuteReadOnlyAgentTurn` kompiliert frischen H7-Kontext, erlaubt ausschließlich den
geschlossenen Search-/Inspect-Port, führt höchstens eine Action aus und verwirft unvollständige
oder nach genau einer content-freien Repair-Anweisung weiterhin ungültige Ausgabe ohne
Toolausführung. Jede vollständige oder verworfene Modellausgabe wird budgetiert und nur mit
redigierten Byte-Metadaten journalisiert. Ein direkter Domain-Übergang nach `Done` ist gesperrt;
nur `VerifyAgentAcceptance` kann mit einem vollständig Goal-, Ledger-, Run- und Snapshot-
gebundenen `AcceptanceVerificationReceipt` abschließen. `cargo fmt --all -- --check`, Workspace-
Clippy für alle Targets/Features mit `-D warnings`, alle Workspace-Tests und Rustdoc mit
`-D warnings` sind grün. Zwei isolierte Windows-libSQL-Worker wurden nach der laut Quality Gate
einzig wiederholbaren `STATUS_ACCESS_VIOLATION` jeweils einmal frisch wiederholt und bestanden.
Unter Node 24.14.0 bestehen Prettier, ESLint, Svelte-Typecheck, 20 Frontendtests, Build und vier
Tooltests. Der Linkcheck prüfte 45 Markdown-Dateien und 66 lokale Links; 608 Rust- und 239
JavaScript-Pakete enthalten keine unbekannte Lizenz.

## H10 Read-only Toolset

Abhängigkeiten: H9

- [x] SearchTool über Retrieval
- [x] InspectTool für File, Symbol, Graph, Claim und Test
- [x] UpdateLedgerTool
- [x] FinishTool
- [x] Outputlimits und gezieltes Paging
- [x] Tool Evidence

Akzeptanz:

- Agent beantwortet Architekturfragen mit klickbarer Evidenz;
- kein Tool kann Datei oder Prozess mutieren;
- große Ergebnisse werden nicht ungefiltert in Kontext geladen.

Verifikation: `DeterministicAgentReadTools` verwendet Search ausschließlich über die geordnete
Task-Lens-Pipeline und bietet typisierte File-, Symbol-, Graph-, Claim- und Test-Inspektion. Der
exakte Claim-Port ist unabhängig von einer abgeschnittenen führenden Claim-Seite. Der
Workspace-Reader validiert kanonische Pfade nach Symlinkauflösung, vollständigen Contenthash und
reguläre Dateien, bevor er höchstens 12 KiB vollständige Zeilen mit einem Vorwärtscursor liefert.
Normalisierte Resultate behalten Digest und beobachtete Bytezahl des vollständigen Ergebnisses,
geben aber höchstens 16 KiB Preview und 100 content-adressierte Evidence-Locators an den Context.

Knowledge-Schema V16 persistiert Tool-Event, Runprojektion, Status/Digest/Snapshot-Anker und
File-/Span-Evidence atomar, jedoch weder Query noch Source-/Preview-Text. Ledger-mutierende
`UpdateLedger`-Ergebnisse ersetzen Ledger und Runtransition mit getrennten CAS-Ankern in derselben
Transaktion; stale Evidence ändert keinen Aggregatzustand. Blocked, Replan und Cancellation sind
getestet. `Finish` kann nur `Verify` anfordern, während `Done` weiterhin ausschließlich dem
Acceptance-Verifier gehört. `cargo fmt --all -- --check`, Workspace-Clippy für alle
Targets/Features mit `-D warnings`, alle Workspace-Tests und Rustdoc mit `-D warnings` sind grün.
Beim letzten Wiederholungslauf wurden ausschließlich zufällige native Windows-libSQL-
`STATUS_ACCESS_VIOLATION`s gemäß Quality Gate mit frischen Workern wiederholt; der zweite und letzte
zulässige, äußerlich serialisierte Retry bestand alle 15 betroffenen Contracts. Mit pnpm 11.9.0
aus dem lokalen Cache bestehen Prettier, ESLint, Svelte-Typecheck, 20 Frontendtests, Build und vier
Tooltests; die lokal verfügbare Node-Version 25.6.1 meldet gegenüber der gepinnten 24.14.0 nur einen
Engine-Hinweis. Der Linkcheck prüfte 45 Markdown-Dateien und 66 lokale Links; der Lizenzbericht
enthält keine unbekannte Lizenz.

## H11 Resume und Recovery

Abhängigkeiten: H3, H9

- [x] Run nach Appneustart laden
- [x] in-flight Toolrun als Interrupted markieren
- [x] Snapshotabgleich
- [x] stale Steps neu öffnen
- [x] Benutzerwahl Resume, Replan oder Cancel

Akzeptanz:

- erzwungener Appabbruch korrumpiert weder Ledger noch Index;
- Resume mutiert nicht auf altem Snapshot;
- abgeschlossene Verification bleibt nur bei frischer Evidenz gültig.

Verifikation: `AgentToolAttempt` hält logische ToolRunId, monotone Versuchsnummer, Run-/Snapshot-
Anker und den geschlossenen InFlight-/Terminal-Lifecycle. `ExecuteReadOnlyAgentTurn` persistiert den
Versuch vor der Werkzeuggrenze; nur der atomare Toolresultat-/Journal-Commit darf `Succeeded`
setzen. Knowledge-Schema V17 übernimmt bestehende V16-Toolläufe als ersten Versuch und besitzt
einen getesteten V16→V17-Rollback. Der gemeinsame Recovery-Adaptervertrag schließt den Store mit
einem laufenden Versuch, öffnet ihn neu, prüft Interrupted und Retry 2, Fresh-/Stale-Evidence,
Resume-Ablehnung, transitives Step-Reopen, Replan, Cancel sowie vollständigen Snapshot- und
Run-CAS-Rollback ohne Ledger- oder Indexmutation.

`cargo fmt --all -- --check`, Workspace-Clippy für alle Targets/Features mit `-D warnings`, alle
Workspace- und Doc-Tests sowie Rustdoc mit `-D warnings` sind grün. Drei voneinander unabhängige
Windows-libSQL-Kindprozesse endeten einmalig mit dem dokumentierten `STATUS_ACCESS_VIOLATION` und
bestanden jeweils beim ersten zulässigen frischen Retry. Ein abschließender vollständiger
Windows-Workspace-Lauf bestand danach ohne Retry; der vollständige Linux-Quality-Job bestand mit
`act` im mittleren `catthehacker/ubuntu:act-latest`-Image. pnpm 11.9.0 bestand
Prettier, ESLint, Svelte-Typecheck, 20 Frontendtests, Build und vier Tooltests; der Linkcheck prüfte
45 Markdown-Dateien und 66 lokale Links, der Lizenzbericht enthält keine unbekannte Lizenz.

## Gate M6

ADR-0038-Erweiterung: Vor Ask, Plan und Agent-Materialisierung läuft derselbe endliche
Read-only-Recherchecontroller. Sein `ResearchMemoryCheckpoint` wird vor jeder Modellentscheidung
deterministisch aus aktueller Frage, öffentlichen Befunden, Evidence-Lücken und den ursprünglichen
Source-Ketten aufgebaut. Ein `ResearchHandoff` revalidiert die aktuellen Revisionsanker vor der
Agent-Materialisierung; der eigentliche Run bleibt vollständig unter Goal, Ledger,
`RunMemoryCheckpoint`, ADR-0010-Controller und Verification. Öffentliche Arbeitsnotizen können
keine Aktion oder Zustandsänderung autorisieren.

- [x] read-only End-to-End-Agent auf drei Fixture-Sprachen
- [x] Goal-Retention- und Compaction-Eval grün
- [x] ungültige Modellausgaben werden nie ausgeführt
- [x] Resume nach simuliertem Crash
- [x] Context Compile P95 innerhalb Budget
- [x] Provider bleibt austauschbar durch Contract-Suite

Verifiziert am 2026-08-06: Das dev-only `agent-harness`-Crate baut die drei Produkt-Fixtures in
temporären Git-Worktrees auf, führt den realen Snapshot-, Parser-, Linker-, Ranking-, Modul- und
atomaren libSQL-Publishpfad aus und komponiert darauf den V2-Context-Compiler, den neutralen
Stubprovider, das echte Read-only Toolset, Tool-Attempts, Journal, Ledger und Acceptance-Gate. Je
Sprache liefern zwei strikt decodierte Modellturns eine Search mit content-adressierter Evidence
und einen nicht-verifizierenden Ledger-Result-Intent; erst die fixtureseitige deterministische
Evidenceprüfung vervollständigt Schritt und Acceptance Receipt. Der erneut geladene durable
Zustand ist `Done`, das Journal lückenlos und jeder Repository-Dateibaum vor und nach dem Lauf
bytegleich.

Ein Negativlauf verwendet denselben realen Rust-Stack. Ungültige Primär- und Reparaturausgabe
erzeugen genau ein redigiertes `invalid_model_output`-Ereignis, aber null Toolaufrufe, null durable
Toolversuche und null Toolereignisse. Die gemeinsame providerneutrale Streaming-Suite wird sowohl
vom In-Memory-Stub als auch vom Ollama-HTTP-Adapter ausgeführt und erzwingt Identität, feste
Ereignisgrenze, genau eine terminale Completion am Streamende und die erwartete neutrale
`ProviderEvent`-Projektion. M6 deckte zusätzlich einen realen TypeScript-Monorepo-Budgetfehler auf:
Context-Policy V2 reserviert nun den kompakten L0-Anchor vor optionalen Details und besitzt eine
eigene Digest-Domäne.
