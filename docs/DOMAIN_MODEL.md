# Domänenmodell

Status: verbindliche Baseline  
Stand: 2026-08-05

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
