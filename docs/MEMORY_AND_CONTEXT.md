# Memory System und Context Compiler

Status: verbindliche Baseline  
Stand: 2026-08-03

## Ziel

Das Memory System soll das Modell nicht imitieren. Es hält überprüfbaren Arbeitszustand außerhalb des Modells und stellt pro Turn nur die relevanten Informationen bereit.

Es gibt keine unstrukturierte, dauerhaft wachsende Chat-Historie als primäres Gedächtnis.

## Memory-Schichten

| Schicht | Inhalt | Lebensdauer |
| --- | --- | --- |
| Project Truth | Dateien, Symbole, Kanten, Manifeste, Tests | aus Snapshot regenerierbar |
| Project Interpretation | Module Cards, Flows, Hypothesen | bis Invalidierung oder Ersetzung |
| Task Memory | Goal Contract, Schritte, Entscheidungen, Akzeptanz | bis Taskabschluss und Retention |
| Run Memory | Toolereignisse, Fehler, Context Packs | pro Run |
| User Preference | ausdrücklich gespeicherte A^3-Einstellungen | projekt- oder appweit |

Chattext wird nicht ungeprüft in Project Truth übernommen.

## EvidenceRef

EvidenceRef ist eine typisierte Union:

- FileSpanEvidence: FileRevisionId, Byte- und Zeilenbereich
- SymbolEvidence: SymbolId, FileRevisionId
- GraphEvidence: EdgeId und Provider
- CommandEvidence: ToolRunId, Exitstatus und Outputdigest
- TestEvidence: VerificationRunId und Testfall
- UserEvidence: bestätigte Benutzerentscheidung
- DecisionEvidence: ADR- oder Project-Rule-Referenz

Ein Ausschnitt speichert genug Metadaten, um ihn neu aus der echten Codebasis zu laden. Große Source Blobs werden nicht als Memory dupliziert.

## Freshness

Ein Eintrag ist Fresh, wenn:

- alle direkten EvidenceRefs auf dem aktuellen oder ausdrücklich zulässigen Snapshot auflösbar sind;
- referenzierte File Revisions weiterhin denselben Content Hash besitzen;
- der erzeugende Parser, Mapper oder Policy-Algorithmus kompatibel ist;
- keine neuere Decision ihn ersetzt.

Zustände:

- Fresh
- Stale
- Superseded
- Refuted
- Unavailable

Stale Inhalte dürfen nur zur Historie oder als klar markierter Revalidierungshinweis erscheinen.

Für Module Cards materialisiert Knowledge-Schema V10 diesen Zustand getrennt von den unveränderten
historischen Card- und Claimzeilen. Ein neuer Index-Publish markiert direkte Claims vor Sichtbarkeit
des Runs als stale und schließt die gesamte `Stale`- oder `NeedsReview`-Card aus der Task Lens aus.
Weiterhin aktive Claims einer unveränderten `Published`-Card dürfen über Run-Grenzen verwendet
werden, aber nur nachdem Modul und jede Evidence im aktuellen Index erneut aufgelöst wurden.

Der interaktive Evidence Inspector verändert diese Autoritätsregel nicht. Er zeigt eine dauerhaft
gespeicherte historische File-, Symbol- oder Graph-Provenienz nur bei einer stale Card und mit
einem getrennten dominanten `Stale`-Hinweis. Eine solche Ansicht darf weder in Task Lens noch in
Context Pack als aktueller Beleg einfließen. Bei `NeedsReview` muss die ausgewählte Evidence selbst
weiterhin aktuell auflösbar sein; der offene Revalidierungsbedarf stammt von der direkten
Abhängigkeit und bleibt unabhängig sichtbar.

## Goal Contract

Pflichtfelder:

~~~text
GoalContract
  task_id
  revision
  previous_revision?
  revision_reason?
  objective
  acceptance_criteria[] { criterion_id, statement, requirement: must | should }
  constraints[]
  non_goals[]
  user_decisions[]
  success_verification
  created_at
~~~

Der Goal Contract wird in jedem Modelleingang vor der aktuellen Arbeit eingefügt. Eine Zieländerung wird als neue Revision gespeichert und macht davon abhängige Planungsschritte prüfbedürftig.

Die implementierte H1-Domäne normalisiert Zeilenenden und Randwhitespace und begrenzt Objective auf
16 KiB, Success Verification auf 8 KiB sowie jedes Kriterium, Constraint, Non-Goal, User Decision
und jede Revisionsbegründung auf 4 KiB UTF-8. Ein Contract benötigt 1 bis 64 eindeutige
Acceptance Criteria; die drei optionalen Listen enthalten jeweils höchstens 64 eindeutige Einträge.
Revision eins besitzt weder Vorgänger noch Begründung. Jede spätere Revision benennt exakt den
unmittelbaren Vorgänger, enthält eine nicht leere Begründung und ändert den Inhalt materiell.
Zeitstempel dürfen nicht zurücklaufen.

Die UI-Projektion `GoalContractV1` spiegelt den bisherigen Must-only-Vertrag ohne Datenbankdetails
und trägt immer eine
explizite Protokollversion. 32-Byte-Identitäten werden als 64-stellige lowercase Hexwerte und Unix-
Millisekunden als exakter Dezimalstring übertragen, damit die WebView keine 64-Bit-Präzision
verliert. Die Rust-Wireform lehnt unbekannte Felder ab; der TypeScript-Runtimeparser prüft die
Domainlängen, Listengrenzen, Identitäten und Revisionsbeziehungen erneut, bevor die WebView den
Wert verwendet. E6 ändert diesen versionierten V1-Wirevertrag nicht still: Soll-Kriterien werden
erst mit einem eigenen späteren Protokolltyp durch den Desktop exponiert; derzeit existiert keine
Domain-zu-V1-Abbildung für solche Contracts.

Nur ein vollständig validierter `GoalContract` kann eine `GoalContractReference` erzeugen. Diese
Referenz bindet `TaskId` und konkrete Revision und ist die spätere run-seitige Eintrittskarte; rohe
IDs können keinen scheinbar gültigen Zielanker konstruieren. Persistenz hängt Revisionen atomar an,
hält den aktuellen Zeiger getrennt und liest alte Revisionen unverändert für Audit und Resume.

## Task Ledger

Jeder Schritt enthält:

~~~text
TaskStep
  step_id
  parent_step_id?
  intended_outcome
  rationale
  dependencies[]
  expected_evidence[]
  verification_spec
  acceptance_criteria[]
  status
  attempts
  result_summary?
  evidence_ids[]
  blocking_reason?
~~~

Regeln:

- Es gibt genau einen InProgress-Schritt je Agentenlauf.
- Completed setzt eine erfolgreiche Verification voraus.
- Eine Behauptung wie „Tests bestanden“ muss auf TestEvidence zeigen.
- Ein fehlgeschlagener Versuch wird nicht überschrieben.
- Replan verändert zukünftige Schritte und erhält die Historie.

ADR-0042 schließt die bisherige Lücke zwischen sichtbarem Conversation-Plan und Task Ledger. Der
Core liest aus den verbindlichen Abschnitten `Implementation Changes` und `Test Plan` höchstens 64
geordnete Ergebnisse. Für jedes Ergebnis erzeugt er eine eigene Step- und Verification-Identität,
Begründung, erwartete Evidence und eine explizite Abhängigkeit zum Vorgänger. Ein normaler Plan ist
damit nicht länger ein einzelner großer Ledger-Schritt; Implementierung und reale Tests werden
einzeln ausgeführt und bestätigt.

Fordert der aktuelle Agent-Turn aufgrund neuer Evidence einen Replan an, wird zuerst sein aktiver
Versuch als blockiert abgeschlossen. Danach pensioniert eine atomare Ledger-Revision ausschließlich
offene betroffene Schritte, fügt ein sichtbares adaptives Todo für die konkrete Planlücke ein und
legt frische Ersatzschritte mit neuen Verification-Identitäten an. Bereits erfolgreich verifizierte
Schritte und ihre ursprünglichen Evidence-Ketten bleiben unverändert. Der Run lokalisiert danach
mit leerem flüchtigem Read-Kontext erneut und setzt denselben endlichen Controller fort. Eine
fachliche Richtungsfrage wird als `AwaitingUser` sichtbar; sie ist weder Replan noch
Policy-Freigabe.

E6 macht `verification_spec` operational: Command, Test, DiffInvariant, Diagnostic und UserConfirm
tragen statt bloßer Methodenbezeichnung ihre Command-ID, den Targeted-/Package-/Workspace-Scope
und die jeweils nötige Test-, Pfad-, Diagnose- oder Confirmation-Semantik. Jeder neue Schritt
ordnet sich konkreten Must-/Should-Kriterien derselben Goal-Revision zu. Nur Must-Zuordnungen
erzwingen Completed vor Acceptance; Should bleibt sichtbar, aber nicht blockierend.

Der implementierte Ledger-Aggregat erzwingt diese Regeln bereits ohne Infrastrukturabhängigkeit:
Statuswechsel erfolgen nur über typisierte Methoden, `Completed` benötigt eine erfolgreiche,
lauf- und spezifikationsgleiche Verifikation, und invalidierte Verifikationsevidenz setzt direkte
sowie transitive abhängige Abschlüsse auf `Stale`. Das libSQL-Knowledge-Schema V12 persistiert den
vollständigen materialisierten Zustand samt unveränderlichen Versuchen und Replan-Historie. Eine
separate monotone Store-Version verhindert verlorene Updates; jeder Reopen rekonstruiert und
validiert das Domain-Aggregat erneut.

## Context Compiler

Der vorgezogene H6-Systemvertrag bleibt mit der konservativen ModelProfile-Zählung unter 900
Tokens. Neu kompilierter Kontext verwendet das geschlossene `AgentAction`-V3-Schema für Search,
Inspect, vollständig gebundene ApplyPatch-Aktionen, ausschließlich per `DiscoveredCommandId`
adressierte Runs, sichere Ledger-Intents und eine Finish-Anforderung. Die öffentliche V3-Notiz
bleibt von dieser Aktionsautorität getrennt. Die historischen V1-/V2-Verträge
bleiben separat lesbar. Repository- und Context-Inhalte werden darin
explizit als untrusted data bezeichnet. Das Structured-Output-Schema liegt gleichzeitig im
Provider-Formatfeld; nur Profile mit `RepeatSchemaInPrompt` erhalten zusätzlich dieselbe
kanonische Schemafassung als getrennte User-Nachricht. Diese optionale Nachricht zählt der
Compiler vollständig gegen das Contextbudget und ist nicht Teil der unveränderlichen
Systemvertragsgrenze.

Der Compiler arbeitet deterministisch in fünf Phasen:

~~~text
ANCHOR → RETRIEVE → RANK → PACK → VALIDATE
~~~

Der implementierte `DeterministicAgentContextCompiler` ist das `a3-context`-Feature hinter dem
inbound `AgentContextCompiler`-Port der Application. Er ruft weder Modell noch Werkzeug auf und
persistiert nichts. Statt einen zweiten Such- oder Rankingpfad einzuführen, komponiert er
`CompileTaskLens`: Exact → Lexical → Graph/Test → Claims → optional Semantic → Fusion bleibt damit
die einzige versionierte Retrieve-/Rank-Reihenfolge. Kooperative Cancellation, ein
Gesamtdeadline-Fehler und die festen Phasen werden bis zur besitzenden Runtime weitergereicht.

### Anchor

Unkürzbarer Kern:

- System- und Sicherheitsregeln
- Goal Contract
- aktueller Task-Schritt
- Akzeptanz- und Verifikationsstatus
- aktueller Snapshot
- aktives Modellprofil

### Retrieve

Quellen werden in dieser Reihenfolge aktiviert:

1. explizite Pfade, Symbole und Fehlermeldungs-Identifier;
2. exakte und lexikalische Suche;
3. Graphnachbarn und Tests;
4. relevante fresh Claims und Decisions;
5. semantische Suche;
6. jüngste, noch relevante Toolergebnisse.

### Rank

Muss-Signale:

- Goal- und Step-Relevanz
- exakter Identifier oder Pfad
- Graphdistanz
- Test- oder Verifikationsbezug
- Evidenzfrische
- öffentlicher oder zentraler Symbolstatus
- Aktualität
- Tokenkosten
- Redundanz

Die konkrete Gewichtung ist als RetrievalPolicy versioniert und wird mit Eval-Daten kalibriert. Eine Änderung an Gewichten ist keine versteckte Codeänderung.

Die implementierte Vorstufe `FusionPolicy::v1` verwendet drei harte Provenienzbänder: Exact vor
sonstiger Evidence vor rein semantischen Kandidaten. Innerhalb eines Bands gelten die versionierten
Integergewichte Kanal 30 %, Goal 20 %, Step 20 %, Freshness 10 %, Token-Effizienz 10 % und
kanalübergreifende nicht-semantische Bestätigung 10 %; Redundanz kann bis zu 20 % abziehen. Diese
Vorstufe packt noch keinen Kontext und berechnet Goal-/Step- oder Overlapsignale nicht selbst. Sie
verlangt sie als validierte, zielgleiche Eingaben vom späteren Goal-/Ledger- und Context-Compiler
und gibt sämtliche Beiträge in `ResultExplanation` zurück.

### Pack

Standard bei einem Modellkontext von 16.384 Tokens:

| Bereich | Zielbudget |
| --- | ---: |
| System, Tools, Sicherheitsvertrag | höchstens 900 |
| Goal Contract und Task Ledger | höchstens 1.100 |
| Project Map und Decisions | höchstens 1.200 |
| Code und strukturierte Evidenz | höchstens 6.800 |
| aktuelle Toolresultate und Fehler | höchstens 1.500 |
| Sicherheitsreserve | mindestens 900 |
| Modelloutput | mindestens 3.500 |

Budgets skalieren proportional, aber Goal Contract und Outputreserve dürfen nicht auf null verdrängt werden. Mindestens 22 Prozent des Modellkontexts werden standardmäßig für Output reserviert.

`ContextBudgetPlan::V1` skaliert alle Grenzen ganzzahlig und reproduzierbar. Beim 16.384er-Profil
beträgt die durch Aufrunden tatsächlich reservierte 22-Prozent-Outputmenge 3.605 Tokens. Statischer
Prompt, optional wiederholtes Schema, vollständiger Anchor, Pack-Framing, Project Map,
Code/Evidence und Toolresultate werden lückenlos genau einer Sektion zugerechnet; zusätzlich
bleiben 900 Tokens Sicherheitsreserve frei. Eine ungekürzte Pflichtsektion, die ihre Grenze
überschreitet, bricht den Compile ab, statt still Inhalte zu verlieren.

`ContextCompilerPolicyVersion::V4` behält den vollständigen kompakten L0-Repository-Anchor aus V2
vor allen optionalen gerankten L1-/L2-Einträgen. Package- und Entrypointmengen erscheinen in L0 als
Anzahlen; konkrete IDs werden nicht dort und später erneut bezahlt, sondern bleiben in den
evidenzgebundenen Modul- und Symboleinträgen. Die relative Retrievalreihenfolge innerhalb der
Anchor- beziehungsweise Detailgruppe bleibt stabil. V3 injiziert zusätzlich Must-/Should-Status,
Criterion-Mappings und operationalen Verification-Scope samt erwarteter Semantik vollständig in
Goal/Ledger. V4 übernimmt außerdem höchstens 64 eindeutige Pfade aus einem typisierten
`ResearchHandoff` als exakte Retrieval-Seeds. Vor der Provideranfrage müssen dessen IndexRun- und
Snapshot-Anker mit der aktuellen Task Lens übereinstimmen; nach einer Indexänderung werden nur
weiterhin exakt passende `FileRevision`s an einen neuen Handoff gebunden. V4 besitzt dafür eine
eigene Digest-Domäne, die Indexanker, Pfade und Content-Hashes der Übergabe bindet.

Packregeln:

- kurze Signatur vor Implementierung;
- Module Card vor vielen Einzeldateien;
- konkrete Implementierung nur für wahrscheinliche Änderungsstellen;
- Tests neben betroffenem Produktionscode;
- überlappende Spans zusammenführen;
- lange Toolausgabe durch strukturierte Befunde und gezielte Ausschnitte ersetzen;
- keine Summary einer Summary ohne ursprüngliche Source IDs.

### Validate

Vor dem Modellaufruf:

- Tokenbudget eingehalten;
- alle IDs auflösbar;
- keine stale Claims im Faktenabschnitt;
- Goal Contract vorhanden;
- aktueller Step vorhanden;
- Toolschemas zum Modellprofil passend;
- keine Secret-Kandidaten;
- deterministischer ContextDigest erzeugt.

Der V1-Digest ist BLAKE3-domänensepariert und bindet Compilerpolicy, Modellprofil, Goal- und
Ledgerrevision, aktuellen Step, IndexRun, Snapshot, Task-Lens-Digest, den optionalen
Run-Memory-Digest, den optionalen Research-Handoff, Budgetplan, tatsächliche Sektionkosten,
Nachrichteninhalt und Structured-Output-Schema. Aktive Claims werden vor dem Packen erneut gegen
Modul und konkrete Evidence des
aktuellen Published Index geprüft; stale oder evidence-inkompatible Claims gelangen nicht in den
Faktenabschnitt. Toolresultate sind journalgeordnet, auf den aktuellen Vorher-/Nachher-Snapshot
gebunden und werden unter dem harten Bereichsbudget von neu nach alt ausgewählt. Überlappende
Source-Spans und identische Ziele werden deterministisch dedupliziert.

## Zoomstufen

- L0 Repository Card: Pakete, Sprachen, Einstiegspunkte
- L1 Module Card: Zweck, API, Abhängigkeiten, Tests, Risiken
- L2 Symbol Card: Signatur, Beziehungen, Doc, relevante Claims
- L3 Source Span: konkrete Implementierung

Der Compiler beginnt grob und zoomt nur dort ein, wo die aktuelle Entscheidung konkrete Syntax benötigt.

## Toolresultate

Toolresultate enthalten:

- strukturierte Kerndaten;
- Exit- oder Ergebnisstatus;
- begrenzte Vorschau;
- Digest des vollständigen Ergebnisses;
- Trunkierungsstatus;
- optionale Artifact-Referenz;
- Snapshot vor und nach der Aktion.

Das Modell erhält nicht automatisch vollständige Logs. Es kann über ein gezieltes Read-Tool weitere Bereiche anfordern.

Das implementierte H10-Read-Toolset verwendet für Search dieselbe geordnete Task-Lens-Retrieval-
Pipeline wie der Context Compiler. Inspect besitzt ausschließlich typisierte Ziele für File,
Symbol, Graph, Claim und Test; Claim-Inspektion liest die exakte aktive ClaimId und hängt nicht von
einer abgeschnittenen Claim-Seite ab. Source-Seiten sind vorwärtsgerichtet und auf 12 KiB begrenzt,
der normalisierte Context-Preview auf 16 KiB und die controllerseitig zugelassene Tool-Evidence auf
100 content-adressierte Quellen. Digest und Bytezahl beziehen sich weiterhin auf das vollständige
normalisierte Resultat vor Preview-Trunkierung.

E2 härtet den File-Pfad unterhalb dieses unveränderten Context-Vertrags: Vor einer Seite werden
kanonischer Worktree-Root, regulärer Dateityp, feste 4-MiB-Grenze, vollständiger Content Hash sowie
Binary- und Secret-Klassifikation erneut bestätigt. Nach dem begrenzten Lesen müssen Handlegröße
und kanonisches Ziel unverändert sein. Die `AgentSourcePage` erzeugt ihre File-/Span-Evidence aus
genau dieser bestätigten Revision und dem ausgelieferten Range; der Context-Adapter muss Evidence
nicht mehr nachträglich aus Previewtext rekonstruieren. Erkannte Kandidaten werden nicht redigiert
weitergereicht, sondern stoppen den Read content-frei vor der Context-Grenze.

Jeder Read-Lauf erzeugt nach dem Model-Event genau ein journalgeordnetes `tool_action`-Event. V16
persistiert nur Status, Digest, Trunkierungsmetadaten, Snapshot-Anker und typisierte File-/Span-
Locators; Query und Preview bleiben flüchtig. `UpdateLedger` darf ein Ergebnis nur mit aktueller,
vom Controller übernommener Tool-Evidence auf `Verifying` setzen. Ledgerprojektion und zugehörige
Runtransition werden atomar gespeichert. `Finish` fordert ausschließlich `Verify` an; `Done` bleibt
dem separaten Acceptance-Verifier nach erfolgreicher objektiver Verifikation vorbehalten.

E6 persistiert dafür immutable, schema-versionierte Verification-Evidence ohne Source- oder
Prozessoutput. Command-Artifacts enthalten Process-/Policy-IDs, Termination, Dauer, vollständige
Stream-Digests, Bytezahlen, Limits, Trunkierung/Redaction und die kanonische Menge betroffener
FileRevisionen beziehungsweise bestätigter Abwesenheiten. Test-, Diagnostic-, Diff- und
UserConfirm-Artifacts ergänzen ausschließlich ihre typisierte Semantik. Der Acceptance-Verifier
lädt genau die Must-Evidence und den aktuellen Published Index in einer begrenzten konsistenten
Operation, vergleicht Spec, Run, Snapshot, Semantik und Abhängigkeiten und verwendet denselben
regenerierten Run-Memory-Checkpoint zum konservativen Ausschluss offener taskbezogener
Hypothesen. Eine Useränderung macht nur Evidence mit betroffener Abhängigkeit stale; ein bloßer
erfolgreicher Exitcode kann strukturierte Test- oder Diagnosesemantik nicht ersetzen.

E7 gibt einen mutierenden Model-Turn nie direkt an einen Adapter. Der zentrale Controller hält
zuerst den einzigen Worktree-Mutations-Lease, persistiert Policy beziehungsweise Approval und
führt höchstens eine strukturierte Patch- oder kataloggebundene Run-Aktion aus. Nach jedem
tatsächlich sichtbaren Patch werden die exakten `PatchChangeSet.changed_paths` sofort über den
normalen vollständigen Indexpublish verarbeitet. Ein weiterer Modellturn ist nur zulässig, wenn
der neu kompilierte Context Pack, die Runprojektion und der jüngste Published Index denselben neuen
Snapshot tragen; andernfalls stoppt der Lauf content-frei. Der erste identische Fehler erhält
höchstens einen solchen frischen Retry-Kontext, die zweite Wiederholung wechselt nach `Replan`.

H11 persistiert vor dem eigentlichen Toolaufruf einen content-freien `in_flight`-Versuch.
Nur der gemeinsame Resultat-/Journal-Commit darf diesen Versuch erfolgreich abschließen; ein
Boundaryfehler wird als Failed, Cancelled oder Denied gespeichert. Bleibt ein Versuch durch einen
Appabbruch offen, markiert der nächste Recovery-Load ihn als Interrupted. Ein erneuter Aufruf
behält die logische ToolRunId, erhält aber eine monotone Versuchsnummer und kann deshalb weder den
alten Versuch überschreiben noch ein Ergebnis doppelt journalisieren.

E8 trennt den Versuchslifecycle von der dauerhaften Wirkung einer Mutation. Patch- und
Prozessaktionen beginnen fail-closed als `Unknown`; vollständige oder partielle Patchwirkungen und
terminal beobachtete Prozess-Exits werden `Applied`, nachweislich nicht geöffnete Grenzen werden
`NotApplied`, und Timeout, Cancellation nach Prozessstart oder verlorene Ergebnisbeobachtung
bleiben `Unknown`. Ein unbekannter Versuch wird weder wiederholt noch zurückgesetzt. Stattdessen
publiziert die Reconciliation unter dem einzigen Worktree-Lease einen vollständigen aktuellen
Repositorysnapshot einschließlich fremder Änderungen. Die historische Disposition bleibt
`Unknown`; bis zu einem anschließenden dauerhaften `Replan` bleibt jede weitere Mutation gesperrt.

Recovery rekonstruiert den materialisierten Run und das vollständige Task Ledger, lädt den jüngsten
atomar veröffentlichten Index und löst jede Evidence-ID abgeschlossener Verifikationen zurück auf
ihre content-adressierte FileRevision. Resume ist nur ohne stale Evidence zulässig und übernimmt
vor weiterer Agentenarbeit den aktuellen Published Snapshot. Bei einem reconcilierten `Unknown`
ist Resume zusätzlich gesperrt. Replan und Cancel invalidieren stale Evidence transitiv, öffnen
betroffene Completed-Steps neu und beenden einen aktiven Step; Replan quittiert zugleich atomar
den reconcilierten Mutationszustand, Cancel terminiert zusätzlich den Run. Die gewählte Wirkung
wird zusammen mit Ledger, Mutationsgate und Recovery-Event unter Published-Snapshot-,
Ledger-Version- und Run-Sequenz-CAS atomar gespeichert.

## Compaction

Nach jedem erfolgreichen Schritt:

1. dauerhafte Facts, Observations und Decisions mit Evidenz speichern;
2. Step-Ergebnis knapp materialisieren;
3. redundante Run-Texte aus dem aktiven Kontext entfernen;
4. offene Fragen und Fehler explizit behalten;
5. Goal Contract unverändert verankern.

Compaction löscht keine Audit-Events. Sie verändert nur den nächsten Context Pack.

Der implementierte H8-Kern erzeugt dafür einen `RunMemoryCheckpoint` ausschließlich aus dem
vollständigen `GoalContract`, der aktuellen `TaskLedgerRevision`, dem materialisierten `AgentRun`,
dem aktuellen atomar publizierten Index und originalen `TaskLensClaim`-Projektionen. Ein vorheriger
Checkpoint ist absichtlich kein zulässiger Eingang. Dadurch kann eine wiederholte Compaction keine
Summary-of-Summary erzeugen und die ursprüngliche Quellkette nicht schleichend verlieren.

Der Checkpoint enthält:

- jeden terminalen Step-Versuch mit Step-, Attempt- und Run-ID, aktuellem Step-Status, Outcome,
  begrenzter Ergebnissummary sowie der kanonisch deduplizierten Menge direkter und
  Verifikations-Evidence-IDs;
- ausschließlich aktive Claims, deren Modul und konkrete Evidence im aktuellen Published Index
  erneut auflösbar sind, weiterhin getrennt nach Fact, Observation und Hypothesis und mit ihren
  originalen Source-Run-, Snapshot-, Claim- und Evidence-IDs; ein älterer Source-Run bleibt dabei
  Provenienz und wird nicht allein durch einen unabhängigen Publish stale;
- offene fehlgeschlagene Verifikationen, Blocker, Approvals, Fehler, Abbrüche und stale Steps mit
  der letzten vorhandenen Attempt-Quelle;
- die beobachtete `RunEventSequence` nur als Audit-Watermark sowie einen domänenseparierten
  `RunMemoryDigest` über alle autoritativen Eingaben.

Der Checkpoint ist regenerierbar und erhält deshalb keine zweite Persistenzwahrheit neben Goal,
Ledger, Run Journal und Knowledge Index. Seine Kompilierung besitzt nur unveränderliche
Referenzen und kann das append-only Journal weder kürzen noch umschreiben. Nach Neustart wird
dieselbe Projektion aus den dauerhaften Quellen neu aufgebaut.

Vor jedem Ask-, Plan- oder Agent-Vorbereitungs-Modellturn erzeugt der Application-Core zusätzlich
einen `ResearchMemoryCheckpoint` aus der aktuellen Frage, den öffentlichen Befunden, offenen
Evidence-Lücken und den ursprünglichen Source-Referenzen. Er ist keine Summary-of-Summary und keine
fachliche Evidence. Frühere Beobachtungen und Schlussfolgerungen werden nur nach exakter
Revalidierung ihrer `FileRevision` gegen den neu gebundenen Index übernommen; Hypothesen dürfen nur
als Suchhinweis fortleben. Für die Agent-Materialisierung übergibt ein typisierter
`ResearchHandoff` ausschließlich Index-, Snapshot- und revalidierte Revisionsanker. Goal, Ledger,
`RunMemoryCheckpoint`, Verification und das Run Journal bleiben danach die Autorität.

Der in Ask-Research-Decision V3 enthaltene Evidence-Status ist ausschließlich ein geschlossener
Controllerhinweis und wird nicht als Befund in den `ResearchMemoryCheckpoint` übernommen. Eine als
`incomplete` ausgewiesene Antwort erzwingt weitere begrenzte Reads oder `AwaitingContinuation`.
Eindeutig im Auftrag genannte Indexdateien müssen eine aktuelle sichere Source im Working Set
besitzen. Abschnittsweise `inspectPath`-Reads erzeugen jeweils eine eigene exakte SourceRange; der
nächste sichere Seitenanfang bleibt nur im aktuellen Working Set. Quelltext und Seiten-Cursor
werden weiterhin weder im Checkpoint noch im Journal persistiert.

Eine nach ADR-0041 vorgemerkte Nachricht übernimmt keine fachlichen Task-, Run-, Evidence- oder
Plananker des vorherigen Work Items. Der Zielmodus wird erst beim FIFO-Start atomar zum
Sessionmodus. Nach Ask oder Plan werden frühere Agent-Anker nur im Verlauf dargestellt; ein neuer
Agent-Auftrag erhält erst nach aktueller Recherche und ausdrücklicher Planfreigabe einen neuen
`ResearchHandoff`, Goal Contract und Task Ledger.

Ein nach ADR-0039 validierter Slash-Aufruf wird getrennt als `CommandExecutionProfile` in die
Kontextkompilierung eingebunden. Der Core erzeugt daraus feste Constraints für Ziel, Linse,
Ergebnis und Verification; der ursprüngliche Slash-Text wird nicht zur privilegierten
Systemanweisung. Das Profil kann Quellen priorisieren, aber weder öffentliche Arbeitsnotizen noch
Hypothesen zu Evidence hochstufen. Diagrammentwürfe erhalten ausschließlich die bereits
turnlokalen Source-Referenzen des aktuellen Rechercheabschnitts und bilden keine neue
Memory-Autorität.

Das typisierte Profil bleibt über Recherchefortsetzung, Planfreigabe, Neustart und Agent-
Materialisierung erhalten. Vor jeder erneuten Verwendung werden gespeicherter Katalogstand,
Hauptauftrag, Linsen, Tiefe und die ursprüngliche User-Entry erneut durch den Domain-Parser
abgeglichen; der `ResearchHandoff` übernimmt nur das validierte Profil und gegen den aktuellen
Index revalidierte Source-Revisionen. Eine direkte Antwort auf eine Core-Rückfrage ergänzt nur das
fehlende Ziel und kann weder Command noch Linsen stillschweigend ändern.

Im nächsten Context Pack steht `[RUN_MEMORY]` direkt nach dem vollständigen Anchor. Metadaten,
offene Fehler und offene Hypothesen sind Pflichtinhalt; passen sie nicht in das harte Budget, wird
der Compile abgebrochen. Step-Ergebnisse und weitere aktuelle Claims folgen deterministisch und
dürfen am Ende des Budgets als Ganzes ausgelassen werden, wobei `truncated=true` gesetzt wird.
Ausgewählte Claims werden nicht ein zweites Mal aus der Task Lens gepackt. Run Memory wird
vollständig gegen `CodeAndEvidence` gerechnet, vor dem normalen Lens-Packing reserviert und wie
jede andere Context-Einheit auf Secret-Kandidaten geprüft. Goal- und Ledger-Anchor bleiben davon
unberührt und ungekürzt.

## Erfolgsmetriken

- Goal-Retention über lange Runs
- Anteil turnspezifisch nützlicher Tokens
- Retrieval Recall für tatsächlich geänderte Symbole
- stale Claim Leakage gleich null
- Anzahl unnötiger Wiederholungslesungen
- Toolerfolg beim ersten Versuch
- Taskabschluss mit aktueller Verifikation
