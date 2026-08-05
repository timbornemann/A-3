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

## Goal Contract

Pflichtfelder:

~~~text
GoalContract
  task_id
  revision
  objective
  acceptance_criteria[]
  constraints[]
  non_goals[]
  user_decisions[]
  success_verification
  created_at
~~~

Der Goal Contract wird in jedem Modelleingang vor der aktuellen Arbeit eingefügt. Eine Zieländerung wird als neue Revision gespeichert und macht davon abhängige Planungsschritte prüfbedürftig.

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

## Context Compiler

Der Compiler arbeitet deterministisch in fünf Phasen:

~~~text
ANCHOR → RETRIEVE → RANK → PACK → VALIDATE
~~~

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
| Goal Contract und Task Ledger | höchstens 900 |
| Project Map und Decisions | höchstens 1.200 |
| Code und strukturierte Evidenz | höchstens 7.000 |
| aktuelle Toolresultate und Fehler | höchstens 1.500 |
| Sicherheitsreserve | mindestens 900 |
| Modelloutput | mindestens 3.500 |

Budgets skalieren proportional, aber Goal Contract und Outputreserve dürfen nicht auf null verdrängt werden. Mindestens 22 Prozent des Modellkontexts werden standardmäßig für Output reserviert.

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

## Compaction

Nach jedem erfolgreichen Schritt:

1. dauerhafte Facts, Observations und Decisions mit Evidenz speichern;
2. Step-Ergebnis knapp materialisieren;
3. redundante Run-Texte aus dem aktiven Kontext entfernen;
4. offene Fragen und Fehler explizit behalten;
5. Goal Contract unverändert verankern.

Compaction löscht keine Audit-Events. Sie verändert nur den nächsten Context Pack.

## Erfolgsmetriken

- Goal-Retention über lange Runs
- Anteil turnspezifisch nützlicher Tokens
- Retrieval Recall für tatsächlich geänderte Symbole
- stale Claim Leakage gleich null
- Anzahl unnötiger Wiederholungslesungen
- Toolerfolg beim ersten Versuch
- Taskabschluss mit aktueller Verifikation
