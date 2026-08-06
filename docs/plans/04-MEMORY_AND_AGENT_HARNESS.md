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

- [ ] TaskStep und StepDependency
- [ ] VerificationSpec
- [ ] Zustandsübergänge
- [ ] genau ein InProgress-Schritt je Run
- [ ] Replan mit Historie
- [ ] Stale nach Evidenceinvalidierung

Akzeptanz:

- Completed ohne erfolgreiche Verification ist unmöglich;
- zyklische StepDependencies werden abgelehnt;
- Neustart stellt exakt den letzten Ledgerzustand her.

## H3 Run Journal

Abhängigkeiten: H2

- [ ] AgentRun und RunEvent
- [ ] monotone Sequenznummer
- [ ] atomare Event- plus Zustandsaktualisierung
- [ ] sichere Payloadredaction
- [ ] Retention und Exportformat

Akzeptanz:

- paralleles Eventappend erzeugt keine doppelte Sequenz;
- Eventjournal enthält keine Secret-Fixtures;
- Journalverlust ist nicht nötig, um materialisierten Zustand zu lesen.

## H4 ModelProvider

Abhängigkeiten: Foundation Job primitives

- [ ] neutraler ModelProvider-Port
- [ ] Ollama-kompatibler Adapter
- [ ] Streaming ProviderEvents
- [ ] Timeouts und Cancellation
- [ ] sichere Endpoint Policy
- [ ] normalisierte Fehler
- [ ] Stubprovider für Tests

Akzeptanz:

- Application importiert keine Ollama-Payloadtypen;
- Streamingabbruch beendet Request;
- nicht lokaler Endpoint benötigt Policyfreigabe.

R8 zieht ausschließlich den neutralen, request-/timeout-/cancellation-fähigen
`ExplorerModelProvider` samt Stubprovider für Explorer-Vertragstests vor. Dieser schmale Port kennt
keine Providerpayloads oder Endpoints, erfüllt aber bewusst noch nicht H4: allgemeiner
`ModelProvider`, Streaming-Events, Endpoint Policy und Ollama-Adapter bleiben offen.

## H5 ModelProfile und Capability Probe

Abhängigkeiten: H4

- [ ] Context- und Outputlimit
- [ ] Structured Output
- [ ] Tool Call Mode
- [ ] Tokenizer oder konservativer Counter
- [ ] Parallelitätslimit
- [ ] Startprobe mit kleinem Schema
- [ ] manueller Profile Override

Akzeptanz:

- Fähigkeiten werden nicht nur aus Modellnamen erraten;
- fehlschlagende Structured-Output-Probe deaktiviert ausführbare Aktionen;
- Profile sind versioniert und in Runs referenziert.

## H6 Prompt und Action Schemas

Abhängigkeiten: H1, H2, H5

- [ ] kompakter statischer Systemvertrag
- [ ] versionierte AgentAction-Union
- [ ] Search, Inspect, UpdateLedger und Finish in read-only Phase
- [ ] JSON-Schema-Erzeugung
- [ ] Strict Validator
- [ ] maximal ein Repair Attempt

Akzeptanz:

- unbekannte Toolnamen und Felder werden abgelehnt;
- Text außerhalb des Schemas wird nicht ausgeführt;
- Prompt bleibt unter statischem Budget.

Das in R8 vorgezogene `deep-map-explorer-action-v1`-Schema und sein Strict Validator decken nur die
read-only Deep-Map-Union Inspect, Search und Propose ab. Die allgemeine AgentAction-Union mit
Ledger- und Finish-Aktionen sowie ihre Promptintegration bleibt Teil von H6.

## H7 Context Compiler Core

Abhängigkeiten: H1, H2, Task Lens

- [ ] Anchor
- [ ] Retrieve
- [ ] Rank
- [ ] Pack
- [ ] Validate
- [ ] ContextDigest
- [ ] Bereichsbudgets und Outputreserve
- [ ] Deduplizierung und Zoom

Akzeptanz:

- Goal Contract und aktueller Step sind immer enthalten;
- stale Fact Fixture wird blockiert;
- 16k-Profil hält die definierten Budgets;
- gleiche Eingaben ergeben gleichen Digest.

## H8 Compaction

Abhängigkeiten: H3, H7

- [ ] Step Result Materialization
- [ ] Claims mit Evidence extrahieren
- [ ] offene Fehler und Hypothesen behalten
- [ ] redundante Runtexte aus aktivem Pack entfernen
- [ ] Source IDs erhalten

Akzeptanz:

- Langlauffixture kann nach mehreren Compactions Ziel und offene Punkte korrekt nennen;
- keine Summary-of-Summary ohne Quellkette;
- Audit Events bleiben erhalten.

## H9 Controller State Machine

Abhängigkeiten: H2, H3, H6, H7

- [ ] Intake
- [ ] Localize
- [ ] Plan
- [ ] Execute read-only
- [ ] Verify
- [ ] Replan
- [ ] AwaitApproval-Grundzustand
- [ ] Done, Failed und Cancelled
- [ ] Turn-, Token-, Tool- und Zeitbudgets

Akzeptanz:

- Property-Tests decken Zustandsübergänge;
- pro Turn höchstens eine Action;
- Budgetende führt zu erklärtem Failed oder AwaitingUser, nicht Endlosschleife;
- Done nur über Acceptance-Verifier.

## H10 Read-only Toolset

Abhängigkeiten: H9

- [ ] SearchTool über Retrieval
- [ ] InspectTool für File, Symbol, Graph, Claim und Test
- [ ] UpdateLedgerTool
- [ ] FinishTool
- [ ] Outputlimits und gezieltes Paging
- [ ] Tool Evidence

Akzeptanz:

- Agent beantwortet Architekturfragen mit klickbarer Evidenz;
- kein Tool kann Datei oder Prozess mutieren;
- große Ergebnisse werden nicht ungefiltert in Kontext geladen.

## H11 Resume und Recovery

Abhängigkeiten: H3, H9

- [ ] Run nach Appneustart laden
- [ ] in-flight Toolrun als Interrupted markieren
- [ ] Snapshotabgleich
- [ ] stale Steps neu öffnen
- [ ] Benutzerwahl Resume, Replan oder Cancel

Akzeptanz:

- erzwungener Appabbruch korrumpiert weder Ledger noch Index;
- Resume mutiert nicht auf altem Snapshot;
- abgeschlossene Verification bleibt nur bei frischer Evidenz gültig.

## Gate M6

- [ ] read-only End-to-End-Agent auf drei Fixture-Sprachen
- [ ] Goal-Retention- und Compaction-Eval grün
- [ ] ungültige Modellausgaben werden nie ausgeführt
- [ ] Resume nach simuliertem Crash
- [ ] Context Compile P95 innerhalb Budget
- [ ] Provider bleibt austauschbar durch Contract-Suite
