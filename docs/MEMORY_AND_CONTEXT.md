# Memory System und Context Compiler

## Ergänzung: Core-Planpflichten und rungebundene Recherche

[ADR-0048](adrs/0048-rungebundene-replan-recherche.md) bindet die Replan-Recherche an
Run, Schritt, Snapshot und Journal. Originale Safe-Reader-Seiten gelangen flüchtig und
mit gezählten Kontextkosten in dieselbe V5-Zulassung wie Ask/Plan. Ein Read allein
beendet die Recherche nicht. Ergebnisse bleiben Interpretationen, keine Schrittverifikation.
Read-Zähler und Analysequittungen bleiben beim Wiederanlauf erhalten; Modellturns werden
auch bei einem fehlgeschlagenen anschließenden Tool-Read abgerechnet.
Bei ausgefallenen Providerstreams bleibt der gezählte Prompt plus das konservativ reservierte
Outputlimit im Run-Verbrauch; dies ist keine behauptete Providerabrechnung. Ein einzelner
fehlgeschlagener Repair bleibt Teil desselben Turns und autorisiert keine Aktion.
Metadatenmarkierungen tatsächlich gelesener Originalseiten erlauben begrenzte Cachehydration
nach Neustart. Hash und exakter Bereich müssen erneut passen; Hydration erneuert keinen
Read-Zähler und keine Analysequittung. Suchspans besitzen diese Befugnis nicht.

[ADR-0049](adrs/0049-core-planpflichten-und-statusnotizen.md) initialisiert neue Plan-/Agent-
Vorbereitung deterministisch mit Bestand, Änderungsentwurf und Testentwurf. Vorhandene neue
Features oder Testkonventionen werden nicht als zusätzliche Pflichtvoraussetzung erfunden.
Der aktive Kontext nennt den Fragetyp ausdrücklich. Nicht belegte V5-Statusnotizen werden
Hypothesen und bilden keine Abschlussbelege. Bestehende eingefrorene Verträge bleiben erhalten.

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
Tokens. Neu kompilierter Kontext verwendet das geschlossene `AgentAction`-V4-Schema für Search,
Inspect, vollständig gebundene ApplyPatch-Aktionen, ausschließlich per `DiscoveredCommandId`
adressierte Runs, sichere Ledger-Intents und eine Finish-Anforderung. Die öffentliche V3-Notiz
bleibt von dieser Aktionsautorität getrennt. V4 ergänzt ausschließlich die begrenzten
[Fast-Index-Ablaufreads](FAST_INDEX_FLOWS.md). Die historischen V1-/V2-/V3-Verträge
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

Bei einer validierten unvollständigen Antwort leitet der Core aus ihrer konkreten
öffentlichen Lücke und dem nächsten Schritt selbst begrenzte Suchkandidaten ab:
eindeutig aufgelöste aktuelle Dateien, sichere nächste Seiten und exakt im aktuellen
Graphen vorkommende Funktions- oder Typnamen. Er führt höchstens vier neue vorhandene Read-only-
Aktionen direkt aus, ohne einen zusätzlichen Modellentscheid allein zur Werkzeugwahl
oder eine Nutzerbestätigung zu verlangen. Vollständig gelesene aktuelle Dateien
werden nicht erneut als offener Dateianfang behandelt. Die Kandidaten sind weder
Beweise noch ausführbare Repositoryanweisungen; Aktionsdeduplizierung, Stagnation,
Final-only- und Sicherheitsgrenzen bleiben bestehen; die endlichen Ressourcenprofile folgen
ADR-0046. Fehlt ein eindeutiger
Ansatz, wählt das Modell weiterhin selbst die nächste erlaubte Rechercheaktion.

Ein notwendiger Fortsetzungszustand benennt seinen Core-Grund: Zeit-/Entscheidungs-/
Aktionsende, Stagnation, verbleibende Beleglücke oder ungültige Ausgabe beziehungsweise
Zitatzuteilung. Ein Formatfehler wird nicht als begrenzte Suche ausgegeben. Versionierte,
content-freie Diagnosen unterscheiden JSON, Form, Felder, Version, Werte, Quellen,
Objekt-/Array-/String-Typfehler sowie einen ungültigen oder unvollständigen Response-Stream,
Markerabweichung, geschlossene Reads und abgeschnittene Ausgabe. Ein frisch gepackter Repair
behält Frage und Evidence ungekürzt gegenüber dem Primärpaket; sein kurzer Hinweis darf
historische Conversation verdrängen, nicht die aktuelle Evidence. Auch die zweite ungültige
Ausgabe desselben Dokuments führt zu keiner Werkzeugausführung. Ein neuer Abschnitt verlangt eine explizite
Fortsetzung und ist keine Freigabe für Dateiänderungen, Prozesse oder Netzwerk.

Der flüchtige Recherchekontext folgt ADR-0044: aktuelle explizite Repositoryziele stehen vor
höchstens zwölf aktuellen Task-Lens-Zielen; höchstens acht revalidierte Quellen des vorherigen
Turns folgen nur nachrangig und werden bei einem neuen Auftrag mit expliziten Dateizielen nicht
pauschal übernommen. Ein Teil des Evidence-Fensters bleibt für adaptive Reads späterer Runden frei.
Identische öffentliche Lücken werden im Modellcheckpoint dedupliziert und ältere Hinweise
begrenzt, während die persistierte Timeline vollständig bleibt (Anzeige: neueste 64 Ereignisse).
Ein weitergerücktes vollständiges Anzeigefenster wird anhand seiner gemeinsamen Ereignisse
übernommen, ohne deren DOM-Zeilen neu anzulegen. Ein vollständig verpasster Abschnitt verlangt
durchgängig spätere Zeitstempel; ältere oder verkürzte Polls ersetzen keinen vorhandenen Stand.
Diese Präsentationsprüfung verleiht keine Evidence- oder Ausführungsautorität. Die
Core-Zielprojektion aus angefragtem Namen, aktuellem Pfad und turnlokalem `S`-Label ist
Navigationsmetadatum und keine fachliche Evidence.

Die Modellnachrichten werden pro Entscheidung aus dem begrenzten ursprünglichen
Conversation-Ausschnitt, genau dem aktuellen Aktionsfeedback und einem frisch kompilierten
Evidence-Paket aufgebaut. Frühere kompilierte Pakete werden nicht erneut als Conversation
angehängt. Der flüchtige Source-Cache hält höchstens 200 Ausschnitte zu je 32 KiB; das kleinere
modellprofilgebundene Evidence-Fenster enthält höchstens acht fokussierte Ausschnitte.
Höchstens 32 aus aktuellen Fast-Index-Funktionsbereichen und ihren umschließenden
Klassendeklarationszeilen ausgewählte Intervalle bleiben
revisionsgebunden im flüchtigen Working Set erhalten. Mehrere disjunkte Methodenkörper
derselben Revision dürfen gemeinsam sichtbar sein; überlappende Bytes werden vereinigt.
Neue validierte Lücken ergänzen die Auswahl, statt bereits benötigte Aufrufer, Konstruktoren
und Callbacks zu verdrängen. Ausgeliefert wird ausschließlich sicher gelesener Originaltext,
kein Indexersatztext und keine Modellzusammenfassung. Passt die Auswahl mit den aktiven
Lesezielen ins Fenster, werden ihre vollständigen Kosten vor Hintergrundtreffern reserviert.
Bei echtem Überlauf erhält die aktive Stelle Vorrang vor älteren Bereichen, damit
Kopfzeilen und Kürzungsmarker nicht sämtliche Textanteile aufbrauchen. Höchstens zwei
tatsächlich gecachte Leerzeilen können benachbarte Bereiche verbinden; Code und ungelesene
Lücken werden niemals übersprungen. Teilweise gelieferte lange Funktionen behalten ihren
Fortsetzungscursor auch bei erneuter symbolbasierter Verfeinerung.
Explizite neue Stellen und die einzige Recovery bleiben auch innerhalb derselben Datei
erreichbar. Kurze
relevante Dateien werden nach Möglichkeit vollständig gepackt, statt jedem Treffer pauschal
denselben kleinen Textanteil zu geben. Ein Aktionsbatch hält mehrere Dateifokusse gleichzeitig;
Cacheprüfung und erneute Priorisierung dürfen nicht wieder nur sein letztes Ziel auswählen.
Die Verteilung berücksichtigt die vollständigen Kosten einschließlich Source-Header und
Kürzungsmarker: explizite Fokusse und kurze Quellen erhalten das feste Gewicht vier, übrige
Quellen eins. Vollständige kurze Quellen geben unbenutzte Anteile an die übrigen Fenster zurück.
Ein Index-Symbolhinweis darf einen groben Read innerhalb seiner ausgewählten Datei präzisieren,
aber keine andere angeforderte Datei verdrängen. Vollständig enthaltene Ausschnitte derselben aktuellen
Revision werden nur einmal übertragen; große Dateien behalten ihren angeforderten späteren
Bereich. Quellenordinalzahlen und persistierte Quellenhistorie bleiben dabei unverändert.
Der vollständige Auftrag ist Pflichtinhalt. Passt er mit 256 Bytes Mindestkontext nicht in das
verfügbare Fenster, endet der Abschnitt vor dem ersten Modellaufruf mit einem expliziten
Kontextgrenzenhinweis, statt nachgestellte Anforderungen still abzuschneiden.
Frage, Zielauflösung, öffentliche Notizen und Seitenhinweise zählen zum selben Gesamtbudget,
berechnet gegen den tatsächlich verwendeten Modus, das Command-Profil und das Modell-Schema.
Eindeutig aufgelöste Dateinennungen werden zusätzlich als bestehende `ExplicitPath`-Seeds an die
Task Lens übergeben; das Ranking wird nicht geändert.
Ein erneutes identisches Lesen ersetzt beziehungsweise fokussiert den vorhandenen
Ausschnitt, zählt aber nicht als Evidence-Zuwachs. Später angeforderte Zeilenbereiche bleiben
vorrangig; eine Kontextkürzung nennt einen konkreten Fortsetzungsbereich. Im Modellcheckpoint
bleibt nur die zuletzt gemeldete offene Lücke aktiv, während ältere Notizen im Journal erhalten
bleiben. Gelesene und ausgelieferte Intervalle werden revisionsgebunden mit Zeilen und
UTF-8-Bytespalten getrennt geführt. Überlappende Reads und neue Ordinale erzeugen keinen
Scheinfortschritt; eine neue zusammenhängende Kontextstelle ist dagegen echter Zugangsfortschritt.
Der adaptive Fokus kann aktuelle Index-Symbolbereiche aus der Frage oder einer validierten
Lücke auswählen. Kürzung innerhalb einer langen Zeile setzt an einer gültigen Byteposition fort.
Ein wiederholter Seitenanfang kann auch über mehrere bereits ausgelieferte Zeilen hinweg zur
gecachten Reststelle springen. Explizite innere Zeilen und `inspectSource` bleiben dagegen zur
erneuten Betrachtung bereits gelieferter Schnittstellen nutzbar. Der kompakte Notizblock packt
bis zu drei jüngste öffentliche Befunde samt vollständigen Source-Referenzen als einzelne
Einheiten; Text darf begrenzt werden, Referenzen nicht. Die letzte offene Frage bleibt im
Checkpoint und aktuellen Feedback, steht aber nicht mehr als verdrängender Präfix vor Befunden.
Notizen bleiben ausdrücklich nicht autoritativ und müssen an aktuellen Quellen geprüft werden.
Vor jedem Modellversuch prüft der bestehende Safe Reader die aktuelle Revision der gelieferten
Quellen; diese Validierung lädt keine neue Evidence und verändert keinen Lesecursor. Zitierte
Antwort- und Diagrammquellen werden nach der Modellausgabe erneut geprüft.

ADR-0046 begrenzt Standard/Gründlich auf 12/24 Rechercheaufrufe inklusive Repair/Retry,
24/48 neue adaptive Read-Aktionen, 3/6 unabhängige Dokumentrepairs und 2/4 transiente
Modellwiederholungen. Je Dokument bleibt genau ein Repair erlaubt, auch bei Zitatkorrekturen.
Diagramme nutzen höchstens zwei zusätzliche, ausschließlich formatierende Aufrufe.
Gesamtdeadline (5/15 Minuten), 200 Quellen und maximal vier sequenzielle Reads pro Runde bleiben.
Nach zwei echten Nullrunden oder erfolglosem Einzelrepair darf genau eine Core-Recovery
bekannte Cachefrontier neu packen und gegebenenfalls bis zu vier neue sichere Reads aus dem
Restbudget wählen. Ungültige Rohantworten werden dabei nie ausgewertet. Kein Budget wird
zurückgesetzt. Ohne neuen Zugang bleibt ein Zwischenstand; eine inzwischen vollständig belegte
Antwort darf auch nach Nullrunden regulär abschließen. Geschlossene Leserunden erhalten ein
Answer-only-Schema, Diagramme ihr eigenes Schema und keine widersprüchlichen Recherche-Repairs.

Die explizite Fortsetzung ist kein neuer fachlicher Auftrag: Der Core bindet intern die exakte
vorherige User-Sequenz, verwendet die ursprüngliche Frage ohne verschachtelte Fortsetzungstexte
und priorisiert bei der Revalidierung die Quellen der letzten drei öffentlichen Befunde, danach
die zuletzt gefundenen Quellen. Die bisherigen Grenzen von höchstens acht zusätzlich
revalidierten Quellen und 200 Quellen insgesamt gelten weiter. Dateiseiten müssen den alten
Belegbereich abdecken; nur vollständig revalidierte Beobachtungen oder Schlussfolgerungen
erhalten neue turnlokale Source-Verweise. Übernommene öffentliche Notizen werden mit diesen
Verweisen im neuen Abschnitt aufgezeichnet. Wenn mehrere alte Bereiche denselben aktuellen Beleg
erhalten, wird dessen Notizverknüpfung in ursprünglicher Reihenfolge genau einmal gespeichert.
Die vollständige alte Source-Kette wird vor dieser Deduplizierung auf Freshness geprüft; kein
fehlender Beleg darf dadurch verschwinden. So kann eine weitere Fortsetzung nach Reopen den
Arbeitsstand erneut rekonstruieren kann. Frühere Suchversuche und offene Ziele sind begrenzte
Navigationshinweise, keine Evidence oder ausführbare Anweisungen. Sind aktuelle Quellen vorhanden,
entfällt die erneute breite Task-Lens-Basisrecherche; gezielte weitere Werkzeuge bleiben möglich.
Eine andere neue Frage übernimmt dagegen keine frühere offene Lücke. Neue Budgets benötigen
weiterhin eine ausdrückliche Nutzeraktion; es gibt keine selbststartende Fortsetzungsschleife.

Ein fehlender Antwortverweis auf eine bereits sicher gelesene benannte Datei ist ein
Zuordnungsfehler der Ausgabe, keine neue Leselücke. Er nutzt den Einzelrepair dieses Dokuments
aus dem Gesamtbudget und eine reguläre abschließende Modellentscheidung ohne weitere Reads.
Fehlende Datei-Evidence oder ein `incomplete`-Ergebnis erfordern weiterhin echte Vertiefung
innerhalb des Budgets beziehungsweise einen ehrlich markierten Zwischenstand. Auch ein
transienter Retry darf eine bereits auf Abschluss begrenzte Entscheidung nicht wieder öffnen.

Plan- und Agent-Vorbereitung zielen auf einen entscheidungsreifen Plan, nicht auf einen schon
fertigen Patch. Vorhandene Einstiegspunkte, APIs und Integrationsbedingungen benötigen Evidence;
neue CSV-Spalten, Schnittstellen und Tests dürfen als explizite Entwurfsannahmen vorgeschlagen
werden. Ihr Fehlen ist bei einer gewünschten neuen Funktion keine zusätzliche Belegpflicht.
Ein `sufficient`-Plan muss bereits an der Decision-Grenze die fünf Planabschnitte, Quellen und
die Kompilierung durch `AgentWorkPlan` bestehen. Ein Formfehler nutzt denselben Einzelrepair
wie JSON-/Quellenfehler, statt als erfundene Nutzerfrage veröffentlicht zu werden. Auch eine
danach noch fehlende Zielattribution erhält keinen zweiten Repair desselben Dokuments.
Echte explizite `QUESTION:`-Antworten bleiben ohne erzwungene Quellen gültig. Ein gültiger Plan
wird als neue Revision in `AwaitingPlanReview` gespeichert; daraus entsteht keine Ausführung.
Auch unvollständige Antworten ohne nächste mögliche Aktion zählen zu den zwei Nullrunden.
Es gibt weder ein Budgetreset noch ein automatisches Hochstufen von `incomplete` zu `sufficient`.

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

## Verbindlicher Rechercheprüfstand (ADR-0047)

Für neue Ask-, Plan- und Agent-Vorbereitungsläufe verwendet die Modellgrenze Recherche-V5.
Die vorstehenden Freitext-/Nullrundenregeln beschreiben den erhaltenen V3/V4-Legacypfad.
V5 hält stattdessen einen `ResearchWorkState`: unveränderlicher Auftrag, Core-IDs Q1–Q32,
Pflicht-/unterstützende/optionale Fragen, frühere Abhängigkeiten, Ergebnisart und Prüfstatus.
Eine Teilfrage wird nur mit einem zugelassenen Ergebnis beantwortet. Eine Interpretation
braucht Referenzen auf tatsächlich ausgelieferte Originalfenster. Der Core vergibt pro Paket
höchstens acht `E1`–`E8`-Anker und bindet sie an Source-ID, Dateihash, Originalbytes und Positionen.
Das Modell wählt `anchor_ref`, statt Zitate oder Koordinaten nachzutippen. Ein `S`-Label oder
eine reine Quellenüberschrift reicht nicht. Die strukturierte gespeicherte Provenienz sind
Originalreferenzen, nicht paketlokale E-IDs. Ältere V5-Test-/Replay-Dokumente mit exakten Zitaten werden weiterhin gegen
eindeutige Originalstellen geprüft. Die inhaltliche Interpretation bleibt modellabhängig und
wird nicht zu einem deterministisch bestätigten Fact.

Der Core wählt getrennte Ausgabeverträge: `Initialize` definiert Teilfragen ohne Ergebnisse
und bindet sie selbst an den vollständigen unveränderlichen Nutzerauftrag. `Analyze(Qn)` erlaubt
höchstens ein Ergebnis für genau die aktive ID, keine neue Fragenliste und keine freien Tools.
`Design(Qn)` erlaubt dagegen nur zukünftige Designentscheidungen und keine neuen Beleganker.
Vollständige vorausgesetzte Entwurfsentscheidungen bleiben im aktiven Kontext erhalten.
Die feste neue Planvorbereitung benötigt weder Initialize noch einen freien Finalize-Aufruf;
der Core stellt zugelassene Änderungs- und Testentscheidungen verlustfrei zusammen.
`Finalize` erlaubt nur noch Planfelder: `summary`, `changes`, `interfaces`, `tests`, `assumptions`
und die kurze öffentliche Notiz. Der Core rendert Marker, Überschriften, Nummerierung und
Recherchequellen selbst. Höchstens 32 Änderungen und 32 Tests mit jeweils einzeiligen Ergebnissen
werden akzeptiert; der bestehende Plan-/Task-Ledger-Vertrag wird anschließend erneut geprüft.
Das Modell kann durch `kind: plan` weder Rechercheabschluss noch Ausführung autorisieren.
Ein vollständig validierter Initialvorschlag ohne erforderliche Repository-Pflicht trotz
ausdrücklich benannter Originaldateien erhält vor dem Einfrieren den vollständigen Originalauftrag
als Core-Prüfpflicht. Ungültige Dokumente und bestehende Verträge werden dadurch nicht gerettet
oder umgeschrieben. Siehe [ADR-0050](adrs/0050-verlustfreie-entwurfsuebergabe.md) und
[ADR-0051](adrs/0051-core-fallback-fuer-ungeeignete-zerlegung.md). Kurze Originalaufträge
werden dabei gemäß [ADR-0052](adrs/0052-literale-teilpflichten-im-core-fallback.md)
höchstens in sechs wörtlich erhaltene Teilpflichten segmentiert. Auch der Einzelrepair
verwendet exakt die jeweilige Phasenregel: Design verlangt keine neuen Quellenanker;
Analyze interpretiert bestehendes Verhalten und fordert keinen vorgezogenen Entwurf an.

Voraussetzungsergebnisse behalten im aktiven Paket ihre epistemische Art. Nur
`DesignDecision` bindet zukünftige Fehlerpolitik; eine Repository-Interpretation
ist dafür keine Designentscheidung. Der Agent-Kontext reserviert aktuelle
Replan-Pflichten und Originalfenster vor optionalen historischen Zusammenfassungen;
unverzichtbare offene Arbeitsdaten dürfen weiterhin nicht still abgeschnitten werden.

[ADR-0053](adrs/0053-core-pflichten-fuer-benannte-codefragen.md) verwendet diese
wörtlichen Core-Pflichten auch für rein repositorybezogene Initialzerlegungen über
benannte Dateien. Modellgenerierte zusätzliche Zusammenfassungsaufträge werden dabei
nicht zu Pflichtrecherche; gemischte und bereits gespeicherte Verträge bleiben erhalten.

Die nächste offene Pflichtfrage und deren Voraussetzungen werden stabil ausgewählt. Ein
identisches Frage-/Originalrevision-/Bereichspaket darf keine weitere Analyse starten; geänderte
Quellnummern oder neue Formulierungen von Arbeitsnotizen erzeugen keinen Fortschritt. Der Core
versucht vorhandenen Cache, exakte Pfad-/Symbolanker, bestehende Fast-Index-Flows und Relationen,
gezielte Literalsuche und begrenzte Verzeichnisnavigation. Erledigte Antworten bleiben erhalten;
optionale Themen eröffnen nach Abschluss der Pflichtfragen keine weitere Recherche. Unabhängige
Pflichtfragen dürfen weiterlaufen, wenn eine andere Frage blockiert ist. Die bisherigen äußeren
12/24-Modell-, 24/48-Read- und 5/15-Minuten-Limits sowie ein Repair pro Dokument bleiben bestehen.
Byte-Stagnation darf V5 nicht zusätzlich nach zwei Runden stoppen, auch nicht im Retrypfad.
Der Paketbeleg für eine bearbeitete Analyse wird erst mit einem gültig zugelassenen Dokument
atomar übernommen. Der bereits journalisierte Beginn eines Modellaufrufs ist noch kein solcher
Beleg. Abbruch, fehlgeschlagener Einzelrepair oder verbrauchtes Aufrufbudget dürfen dadurch
einen später ausdrücklich fortgesetzten Versuch desselben Pakets nicht irrtümlich sperren.
Ein gültiger, aber ergebnisloser Analyseschritt bleibt dagegen als bearbeitet dedupliziert.

Adaptive Zugriffe haben einen dauerhaften, auf 256 Identitäten begrenzten Versuchskatalog.
Jeder tatsächliche Aktionsstart und sein getrenntes Ergebnis werden über denselben atomaren
Checkpointpfad gespeichert. Pfad-/Source-Zugriffe derselben 200-Zeilen-Seite verwenden dieselbe
kanonische Revisionsidentität; Literal-OR-Reihenfolge und Source-Anzeigenummern sind keine neue
Untersuchung. Flow-Pfade und Views bleiben getrennt. Nur ein vollständig beobachtetes leeres
Such-/Verzeichnisergebnis oder eine nicht eindeutige Indexauflösung unterdrückt denselben Zugriff
für dieselbe Frage und Publikation. Fehler, Begrenzung und unterbrochene Versuche bleiben davon
getrennt. Eine Suche mit übersprungenen Dateien erhält auch bei Adapter-`Complete` keine negative
Quittung; die Zahl tatsächlich geprüfter Dateien muss der gebundenen Dateimenge entsprechen.
Erfolgreiche Reads dürfen den flüchtigen Quellcache nach Wiederöffnen erneut befüllen.
Ein bloß zurückgekehrter Leseaufruf beweist weder neue Evidence noch Aufgabenabschluss.
Eine einzelne Quittung erlaubt keinen `boundedUnknown`-Abschluss. Erst nach einer gültigen
Analyse und Ausschöpfung der endlichen Core-Frontier darf der Core eine begrenzte Erkenntnis
formulieren: mindestens eine vollständig ausgeführte Literalsuche ohne Treffer plus eine
nicht auflösbare Pfadprüfung oder ein leeres Indexverzeichnis, alle im selben Publikationsscope.
Offene, fehlgeschlagene und begrenzte Zugriffe sperren diesen Abschluss. Benannte vorhandene
Dateien müssen weiterhin Originalbezüge besitzen. Entwurfsfragen, verbrauchte Budgets und
Cancellation sind keine negativen Erkenntnisse. Der Text erklärt ausdrücklich, dass weder
allgemeine Nichtexistenz noch Laufzeitverhalten bewiesen sind. Eine reine Ask-Grenzerklärung
braucht keine erfundenen Quellenzitate; ihr typisierter Untersuchungsweg bleibt im Prüfstand.
Der abschließende Work-Trace trägt `Limited`, nicht einen vollständigen fachlichen Beweis.
Explizit vom Nutzer genannte fehlende relative Dateipfade werden gezielt im Index aufgelöst
und ihre Dateinamen literal gesucht. Erfundene Pfade aus Modellnotizen eröffnen diesen Weg nicht.
Negative Grenzen sind an die gesamte Publikation gebunden und werden bei Scopewechsel stale,
auch wenn keine einzelne Datei als Ergebnisquelle vorliegt. Reopen löscht stale Ausschlüsse.

Passt die ausführliche Prüfliste nicht zusammen mit Auftrag und 1.536 Bytes Originalreserve,
kompiliert der Core eine Teilansicht: alle IDs, Prioritäten und Zustände, die vollständige aktive
Frage und budgetierte Ergebnisvorschauen ihrer Voraussetzungen. Inaktive Definitionen verbleiben
unverändert im dauerhaften Vertrag; die vollständige Antwort wird aus dessen Ergebnissen gebaut.
Die Darstellung ist deterministisch und setzt keine Zähler zurück. Ein zu großer Originalauftrag
oder eine nicht passende aktive Frage bleibt ein ehrlicher `ContextLimit`; beliebig viele
Teilfragen können die äußere Zahl der Modellentscheidungen nicht überschreiten.

Ask-Antworten werden aus allen zugelassenen Pflichtantworten samt Originalverweisen aufgebaut.
Pläne behalten diese Antworten als Recherchegrundlage. Neue Schnittstellen sind explizite
Entwurfsentscheidungen, keine angeblich fehlenden Bestandsdateien. Eine typisierte `question` darf
in Plan/Agent eine aktive Entwurfsentscheidung klären, wenn die erforderliche Bestandsrecherche
beantwortet ist; dadurch wird die offene Entwurfsfrage nicht als erledigt gespeichert.

Der Prüfstand wird atomar mit dem Audit gespeichert, bei ausdrücklicher Fortsetzung neu an
aktuelle Sources gebunden und vor Verwendung auf Aktualität geprüft. Geänderte Originale und
abhängige Antworten werden `Stale`; nicht verfügbare alte Antworten bleiben nur historische
Ergebnisse. Die UI zeigt eine stabile read-only Prüfliste getrennt vom animierten Verlauf.
Sie darf weder den Status setzen noch aus Ergebnissen verifizierte Fakten machen. Ihre
Quellenknöpfe verwenden ausschließlich die bestehenden geschützten Preview-Capabilities.

Der Agent-Handoff überträgt die Teilfragen und zugelassenen Ergebnisse zusätzlich zu den
Revisionen. Alle Pflichtüberschriften werden vor optionalen Ergebnistexten im Kontextbudget
reserviert. Erst die freigegebene Planrevision materialisiert je konkretem Arbeitsergebnis ein
Umsetzungskriterium. Rechercheabschluss erfüllt keine Änderungs- oder Testverifikation.
Bei automatischem Replan wird kein künstliches Recherche-Todo mit kopierter Änderungsverifikation
mehr angelegt. Vor der nächsten Mutation erlaubt die bestehende Execute-Phase höchstens vier
reine Search-/Inspect-Turns zur Lokalisierung. Prompt, Schema, Decoder und der einzelne Repair
bleiben dabei read-only; echte Reads und Modellaufrufe werden normal journalisiert/abgerechnet.
Erst eine zugelassene V5-Interpretation der Replan-Ursache mit aktuellem Originalanker beendet
diese eingeschränkte Phase. Ein Originalread allein, Such-, Symbol- und Graphmetadaten reichen
nicht. Der gemeinsame ResearchWorkState-Unterauftrag ist in Knowledge V37 dauerhaft an Run,
Schritt und Snapshot gebunden. Ein sicheres Originalfenster muss nicht die gesamte Datei enthalten. Derselbe
Arbeitsschritt mit unveränderten Kriterien und demselben Snapshot darf innerhalb des Runs keine
erneute automatische Replan-Kette eröffnen; der Core prüft dafür die dauerhaften Ledger-Events
begrenzt und cancellation-fähig. Es entsteht kein zweiter mutierender Controller.
Abnahmeumfang und noch offene Schnitte stehen in [Plan 10](plans/10-RESEARCH_WORK_STATE.md).

Die feste Q1-Planbestandsaufnahme verwendet nach
[ADR-0054](adrs/0054-vollstaendig-gelieferte-planbestandsaufnahme.md) ausschließlich
bei nachweislich vollständig gelesenen **und aktuell vollständig als E-Fenster
gelieferten** benannten Revisionen `SummarizeOriginals`: genau eine Interpretation
der sichtbaren APIs und Integrationsgrenzen. Nicht gelieferte externe Implementierung
bleibt eine benannte Grenze, kein erfundener Befund. Leere Ergebnisse/Fragen nutzen
nur den bestehenden Einzelrepair; normale Analyze-Fragen bleiben nullable.
Unabhängig von der Auslieferungsgröße muss Q1 alle ausdrücklich benannten
Originalrevisionen belegen, bevor der Core zu den belegfreien Designphasen wechselt.
Eine unvollständige Bestandsaufnahme wird dort repariert, nicht erst beim Planabschluss.
Auch vollständige relative Pfade wie `taskflow/manager.py` werden unmittelbar als
benannte Quelle der aktiven Repositoryfrage erkannt, nicht nur als Suffix eines
längeren Pfades. Fehlende Belege dürfen nicht bis zur letzten, fachfremden Teilfrage
aufgeschoben werden; Dateinamenpräfixe bleiben ausgeschlossen.
Bei einer Quellenabdeckungs-Reparatur benennt der Core die tatsächlich gelieferten
E-Ankergruppen der benötigten Dateien innerhalb desselben 768-Byte-Reparaturbudgets.
Mehrere Fenster derselben Revision bilden eine Gruppe; fremde und nicht gelieferte
Originale erhalten keine erfundenen Anker. Fehlt ein notwendiges Originalfenster,
bleibt der allgemeine phasengebundene Hinweis zuständig. Dies ist eine konkrete
Korrekturhilfe, keine automatische Quellenzuordnung oder semantische Zulassung.
Auch die unabhängige Domain-Zulassung und das Wiederherstellen gespeicherter Zustände
erzwingen beide Richtungen der Ergebnisart: Repositoryfragen erlauben Interpretation
oder begrenzte Unbekanntheit, Entwurfsfragen ausschließlich Designentscheidungen.
Eine negative Suche oder Bestandsbeschreibung kann keinen fehlenden Entwurf ersetzen.
Der feste Core-Plan zeigt in seiner abgeleiteten Zieltabelle nur aufgelöste Originale.
Nicht aufgelöste Namen neuer Funktionen oder externer APIs bleiben im unveränderten
Nutzerauftrag, werden aber nicht zusätzlich als Verzeichnis-Suchauftrag injiziert.
Die allgemeine Ask-Recherche behält ihre ungelösten Navigationsziele.

Nach [ADR-0056](adrs/0056-vollstaendige-passende-originalpakete.md) erhält ein Set aus
höchstens acht vollständig gelesenen benannten Originalen Vorrang vor Funktionsfragmenten,
wenn alle Originaltexte samt Header gemeinsam ins bestehende Budget passen. Cache und
Read-Quittung müssen bis zum identischen Ende reichen; ein vollständiger Read-Marker
allein genügt nicht. Expliziter Zeilenfokus bleibt vorrangig. Bei Überlauf gelten weiterhin
die progressiven Einheiten-/Seitenregeln. Ebenso bleibt ein neu fokussiertes Original
außerhalb des benannten Sets erreichbar; Pflichtdateien sperren keine Abhängigkeitsrecherche.
Nur tatsächlich gerenderte Fenster gelten als
ausgeliefert; es entsteht kein zusätzlicher Read, Beweis oder Kontextplatz.

Nach [ADR-0057](adrs/0057-leerer-entwurf-ist-kein-rechercheauftrag.md) benötigt Design
bei `progress` genau ein Ergebnis. Ein leerer Entwurf ist ungültiger Modelloutput,
kein Anlass für weitere Originalreads. Nur die explizite Entscheidung `question`
für eine folgenreiche Nutzerwahl darf ohne Ergebnis bleiben. Der unabhängige Decoder
führt ungültigen Output durch den vorhandenen Einzelrepair; normale Analyze-Beleglücken
bleiben nullable. Dies ersetzt keine inhaltliche Verifikation des Entwurfs.

Nach [ADR-0058](adrs/0058-kompakte-recherchephasen-fuer-kleine-kontexte.md) stehen
gemeinsame Vertrauensregeln nur einmal in der Systeminstruktion, gefolgt vom aktuellen
Phasenauftrag. Im bestehenden V5-Vertrag erhält Originalcode Vorrang vor der abgeleiteten
Zielübersicht und Cursorhinweisen; diese folgen nur im verbleibenden Paketplatz.
Die Initialisierung und Legacy-Navigation bleiben unverändert. Repository-Arbeitsansichten
werden oberhalb eines Drittels des Evidence-Pakets partitioniert; aktive Definition,
stabile IDs, Prioritäten und Status bleiben vollständig, inaktive Definitionen dauerhaft
im Core. Die Quellenreserve skaliert zwischen 512 und 1.536 Bytes. Bindende
Designentscheidungen bleiben vollständig; für Design gelten weiterhin 256 Bytes
Framingreserve. Kein Modell-/Outputlimit und kein Sicherheits- oder Repairbudget wächst.

Der reale Offline-Mehrdateivertrag mit konservativem 8.192/2.048-Profil erhält damit
2.213 Evidence-Bytes in Ask beziehungsweise 2.212 in Plan/Agent. Alle fünf benötigten
Methodenkörper liegen in der Bestandsanalyse gemeinsam vor; die drei Modi schließen
mit jeweils drei Modellstubaufrufen ab. Das beweist Packing und Controllerverhalten,
nicht die Qualität eines beliebigen echten lokalen Modells.

Nach [ADR-0059](adrs/0059-idempotente-originalanker-in-rechercheergebnissen.md) werden
identische, einzeln validierte E-Anker in einem Rechercheergebnis als Menge übernommen.
Mehrere erklärte Methoden im selben Originalfenster benötigen keine mehrfachen
Quellenidentitäten. Das 32-Elemente-Eingabelimit gilt vor der Kanonisierung; E0/E9/E01,
fremde Felder und ungelieferte oder mehrdeutige Fenster bleiben abgewiesen. Der Core
fügt keine fehlenden Quellen hinzu und verbraucht keinen Repair für exakte Wiederholungen.

## Erfolgsmetriken des Prüfstands

- Goal-Retention über lange Runs
- Anteil turnspezifisch nützlicher Tokens
- Retrieval Recall für tatsächlich geänderte Symbole
- stale Claim Leakage gleich null
- Anzahl unnötiger Wiederholungslesungen
- Toolerfolg beim ersten Versuch
- Taskabschluss mit aktueller Verifikation
