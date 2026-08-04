# Domänenmodell

Status: verbindliche Baseline  
Stand: 2026-08-04

## Ubiquitous Language

| Begriff | Bedeutung |
| --- | --- |
| Repository | Logische Git-Codebasis |
| Worktree | Konkrete lokale Arbeitskopie mit eigenem Dateizustand |
| Snapshot | Konsistente Sicht auf HEAD plus lokale Änderungen zu einem Zeitpunkt |
| File Revision | Dateiinhalt, identifiziert durch einen kryptografischen Content Hash |
| Symbol | Sprachabhängige semantische Einheit wie Funktion, Typ oder Methode |
| Edge | Typisierte Beziehung zwischen Symbolen, Dateien, Modulen, Tests oder Claims |
| Evidence | Prüffähiger Verweis auf echten Code, Konfiguration oder Werkzeugausgabe |
| Claim | Persistierte Aussage mit Typ, Evidenz, Status und Aktualität |
| Module Card | Kompakte, evidenzgebundene Beschreibung eines Projektbereichs |
| Project Map | Mehrstufige Sicht aus deterministischem Graph und geprüften Module Cards |
| Goal Contract | Unveränderter Zielanker einer Aufgabe mit Akzeptanzkriterien und Grenzen |
| Task Ledger | Dauerhafter Plan samt Schrittzuständen, Ergebnissen und Verifikation |
| Run | Ein kontrollierter Agentenlauf für eine Aufgabe |
| Context Pack | Tokenbegrenzter, reproduzierbarer Modelleingang für genau einen Turn |
| Tool Action | Typisierte, durch Policy geprüfte Interaktion mit Workspace oder Prozessen |

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

Besteht aus RepositoryId und kanonischem Worktree-Root. Die WorktreeId wird mit der versionierten
Ableitung `a3.worktree-id.v1` deterministisch aus beiden Werten gebildet. Dadurch bleibt sie bei
wiederholter Erkennung und über Appneustarts stabil, ändert sich aber bei einem Worktree-Umzug.
Jeder Worktree besitzt eine eigene Wissens- und Mutationsdomäne. Die spätere Reconciliation muss
einen geänderten Pfad gegen zuvor persistierte Identitäts- und Remote-Evidenz prüfen; sie darf eine
Übereinstimmung nicht allein aus dem neuen Pfad behaupten.

### Catalog Project Identity

`ProjectId` identifiziert einen Eintrag im globalen lokalen Projektkatalog, während `RepositoryId` und
`WorktreeId` die jeweils beobachtete Git- beziehungsweise Worktree-Identität beschreiben. Bei der ersten
Aufnahme wird `ProjectId` mit der versionierten Ableitung `a3.catalog-project-id.v1` deterministisch aus
der `RepositoryId` gebildet und anschließend persistiert. Linked Worktrees derselben `RepositoryId`
teilen einen Katalogeintrag. Eine spätere Umzugs-Reconciliation darf eine bestehende `ProjectId` nur
nach bestätigter Identitäts- und Remote-Evidenz beibehalten; die aktuelle Implementierung führt diese
Reconciliation noch nicht durch.

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
Zustand `building` besitzen. Der aktuelle S2-Port erlaubt nur die terminalen Übergänge `building` →
`failed` oder `building` → `cancelled`; `published` bleibt dem atomaren S10-Publish vorbehalten.

### SymbolId

Ein SymbolId wird deterministisch aus Sprache, normalisiertem Pfad, Symbolart, qualifiziertem Namen und Signaturfingerprint gebildet. Zeilennummern sind kein Teil der Identität. Umbenennung oder fachliche Signaturänderung kann eine neue Identität erzeugen und muss alte Evidenz invalidieren.

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

### Task

Verwaltet Goal Contract, Akzeptanzkriterien, Schritte, Entscheidungen, Runs und Abschluss.

Invarianten:

- Der Goal Contract wird nach Start nicht still verändert. Änderungen erzeugen eine neue Revision mit Begründung.
- Jeder Schritt besitzt Outcome, Status und VerificationSpec.
- Completed benötigt erfolgreiche Verification.
- Ein Task ist Done, wenn alle Muss-Akzeptanzkriterien aktuell verifiziert sind und keine blockierende offene Hypothese existiert.

### Agent Run

Verwaltet Zustandsmaschine, Turnnummer, Context Pack, Tool Action, Events, Budgets und Abbruch.

Invarianten:

- Pro Turn gibt es höchstens eine ausführbare Tool Action.
- Mutierende Tool Actions werden serialisiert.
- Ein Turn verweist auf genau einen Snapshot.
- Vor einer Mutation wird geprüft, ob der erwartete Snapshot noch aktuell ist.
- Abschluss ist ein expliziter Zustandsübergang und keine bloße Textausgabe.

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
