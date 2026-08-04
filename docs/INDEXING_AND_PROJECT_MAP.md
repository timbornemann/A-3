# Indexierung und Projektkarte

Status: verbindliche Baseline  
Stand: 2026-08-04

## Grundsatz

Die Projektkarte ist kein frei formulierter Repository-Überblick des LLM. Sie ist eine versionierte Projektion aus:

1. deterministisch extrahierten Fakten;
2. graphbasierten Beziehungen;
3. LLM-erzeugten, evidenzgebundenen Module Cards;
4. expliziten Unsicherheiten und Aktualitätsstatus.

Die Karte muss ohne Vektorindex und ohne verfügbares LLM weiterhin nutzbar sein.

## Drei Indexmodi

| Modus | Auslöser | LLM | Ergebnis |
| --- | --- | --- | --- |
| Fast Index | automatisch beim Öffnen und bei Änderungen | nein | Dateien, Symbole, Kanten, FTS, Einstiegspunkte, Tests |
| Deep Map | bewusst gestartet oder nach Zustimmung | ja, budgetiert | Module Cards, Abläufe, Invarianten, Hypothesen |
| Task Lens | bei jeder Aufgabe und nach relevanten Änderungen | nur Context-Planung | kleiner zielbezogener Teilgraph |

## Fast Index

Phasen:

~~~text
DISCOVER → HASH → PARSE → LINK → RANK → PUBLISH
~~~

### Discover

- Git Common Directory, Worktree Root, HEAD und Status erkennen
- Ignore-Regeln aus Git, A^3-Konfiguration und sicheren Defaults zusammenführen
- Binärdateien, generierte Verzeichnisse, Vendor-Code und Größenlimits erkennen
- Manifest-, Build-, Test- und CI-Dateien klassifizieren
- Sprache je Datei bestimmen

Git-tracked Dateien werden standardmäßig berücksichtigt. Nicht getrackte Dateien können einbezogen werden, wenn sie nicht ignoriert sind. Secrets, bekannte Credential-Dateien und explizit ausgeschlossene Pfade werden nie indexiert.

Discovery V1 ist durch ADR-0017 konkretisiert: Repository-lokale Git-Ignores, ausschließende Muster
aus `.a3/project.toml` und nicht übersteuerbare sichere A^3-Defaults werden in einem begrenzten lokalen
Adapter zusammengeführt. Benutzerweite Git-Excludes außerhalb des freigegebenen Worktrees sind aus
Reproduzierbarkeits- und Pfadautoritätsgründen nicht implizit aktiv. Dateien oberhalb von 4 MiB werden
nicht geöffnet; von kleineren Kandidaten werden höchstens 16 KiB für Binary- und Secret-Erkennung
gelesen. Manifest-, Build-, Test- und CI-Rollen können überlappen. Die Sprachzuordnung folgt mit dem
versionierten `LanguageAdapter` in S5 und ist nicht Teil der S3-Pfadklassifikation.

### Hash

- BLAKE3 Content Hash pro relevanter Datei
- Metadaten nur als Änderungs-Hinweis, nie als Wahrheitsquelle
- unveränderte Content Hashes überspringen
- Löschungen und Umbenennungskandidaten erfassen

### Parse

V1-Sprachen mit strukturellem Support:

- Rust
- TypeScript und JavaScript
- Python

Andere Textsprachen erhalten zunächst Datei-, FTS- und Manifest-Support. Ein LanguageAdapter liefert:

- Symbole und Signaturen
- Dokumentationsbereiche
- Imports und Exports
- syntaktisch erkennbare Aufrufe
- Testdefinitionen
- Parsefehler und Abdeckungsgrad

Tree-sitter ist die Baseline. LSP oder SCIP kann später als zusätzlicher Edge Provider eingesetzt werden; es ersetzt den lokalen Parser nicht.

### Link

Kantentypen in V1:

- Contains
- Defines
- Imports
- Exports
- Calls
- Implements
- Extends
- Reads
- Writes
- Configures
- Tests
- Builds
- Documents

Jede Kante trägt Provider, Confidence, Snapshot und EvidenceRef. Syntaktisch unsichere Calls bleiben als Kandidaten gekennzeichnet.

### Rank

Deterministische Signale:

- Einstiegspunkt
- öffentlicher Export
- In-Degree und Out-Degree
- Pfad- und Manifestnähe
- Testbezug
- Änderungsfrequenz, falls lokal verfügbar
- Modulzentralität

Die Rankingversion wird gespeichert. Änderungen am Algorithmus erzwingen keine Neu-Parse, aber eine neue Rank-Projektion.

### Publish

Ein Indexlauf wird erst sichtbar, wenn:

- alle Deltas erfolgreich in einer Transaktion committed wurden;
- Löschungen verarbeitet sind;
- abgeleitete Claims invalidiert sind;
- FTS und Graph dieselbe Snapshot-Version referenzieren.

Die S2-Persistenz reserviert dafür bereits worktree-lokale, monotone IndexRun-Sequenzen und erzwingt
höchstens einen `building`-Lauf pro Worktree. Vor S10 kann der Application-Port einen Lauf nur als
`failed` oder `cancelled` abschließen. Es gibt absichtlich keinen separaten Publish-Aufruf, der den
Status ohne die zugehörigen Indexdaten sichtbar machen könnte.

## Deep Map

Phasen:

~~~text
SEED → EXPLORE → PROPOSE → VERIFY → PUBLISH
~~~

### Seeds

Der deterministische Planner wählt:

- Paket- und Anwendungseinstiegspunkte
- zentrale öffentliche Symbole
- Manifest- und Konfigurationsknoten
- Test-Roots
- stark gekoppelte oder ungewöhnliche Bereiche
- noch nicht beschriebene Module

### Exploration

Das LLM darf nur über typisierte Read-only-Werkzeuge explorieren. Jeder nächste Leseschritt benötigt einen erwarteten Informationsgewinn. Vollständiges rekursives Lesen ist verboten.

Eine Exploration endet, wenn zuerst eine Bedingung erfüllt ist:

- konfiguriertes Token-, Zeit- oder Toolbudget;
- alle Muss-Coverage-Ziele erreicht;
- drei aufeinanderfolgende Expansionen liefern keinen neuen hoch bewerteten Knoten;
- verbleibende Bereiche liegen unter dem Relevanzschwellwert;
- Benutzerabbruch.

### Module Card

~~~text
ModuleCard
  id
  snapshot_id
  title
  paths
  purpose
  responsibilities
  public_surface
  entrypoints
  dependencies
  data_flows
  invariants
  tests
  risks
  open_questions
  evidence_ids
  confidence
  mapper_profile_version
  status
~~~

Jedes nicht leere fachliche Feld muss seine Evidence IDs behalten. Ein Summary-Feld ohne Quellen wird verworfen.

### Verify

Der Verifier prüft:

- existieren alle referenzierten Symbole und File Revisions?
- passen behauptete Imports, Exports, Tests und Aufrufkanten zum Graph?
- sind Pfade im Snapshot gültig?
- widersprechen sich Cards?
- welche Aussagen sind nicht deterministisch prüfbar?

Prüfbare Aussagen werden Fact oder Observation. Unprüfbare Aussagen bleiben Hypothesis. Widersprüche werden nicht durch Mehrheitsentscheid des LLM aufgelöst.

## Task Lens

Seeds:

- Goal Contract und aktueller Schritt
- explizite Pfade, Symbole und Identifier der Benutzeranfrage
- Compiler-, Test- und Laufzeitfehler
- zuletzt geänderte Dateien
- offene Hypothesen und fehlgeschlagene Verifikationen

Expansion:

1. exakte Pfad- und Symboltreffer;
2. lexikalische FTS-Treffer;
3. direkte Graphnachbarn;
4. zugehörige Tests, Konfiguration und öffentliche Schnittstellen;
5. evidenzgebundene Claims;
6. semantische Kandidaten als Ergänzung;
7. höchstens ein weiterer Graph-Hop für hoch bewertete Knoten.

Ergebnis ist ein kleiner, priorisierter Subgraph mit Module Cards und Quellenausschnitten. Die Task Lens ist temporär, aber ihre Policy-Version und Seed-Menge werden für Reproduzierbarkeit gespeichert.

## Inkrementelle Aktualisierung

File-Watcher-Ereignisse werden entprellt und anschließend durch einen Git-Status-Scan bestätigt. Ereignisse allein sind nicht zuverlässig genug.

~~~mermaid
flowchart TD
    Change["Bestätigtes Change Set"]
    Delta["Parse- und Graph-Delta"]
    Invalidate["Evidenz invalidieren"]
    Map["Betroffene Cards neu planen"]
    Lens["Task Lens aktualisieren"]

    Change --> Delta
    Delta --> Invalidate
    Invalidate --> Map
    Map --> Lens
~~~

Invalidierungsradius:

- geänderte File Revision;
- darin definierte oder entfernte Symbole;
- ein- und ausgehende Kanten;
- Claims mit direkter Evidenz;
- Cards des eigenen Moduls;
- Cards direkter abhängiger Module als NeedsReview;
- abgeschlossene Task-Schritte mit betroffener Verification Evidence.

## Hybride Suche

Suche erzeugt getrennte Kandidatenlisten für exakt, FTS, Graph, Tests, Memory und Vektor. Die Listen werden normalisiert, über stabile IDs dedupliziert und mit einer versionierten Fusion zusammengeführt.

Vektoren werden ausschließlich für Semantic Cards und ausgewählte Symbolbeschreibungen erzeugt. Standardmäßig werden nicht beliebige überlappende Zeilenchunks eingebettet.

Jeder Treffer erklärt seine Herkunft:

- exact identifier
- lexical
- graph relation
- test relation
- task memory
- semantic similarity

## Qualitätsmetriken

- Parse Coverage pro Sprache
- Symbol- und Edge-Anzahl
- Anteil ungeklärter Kanten
- Deep-Map-Coverage pro Modul
- Claims mit gültiger Evidenz
- stale Claims und Cards
- Retrieval Recall auf Eval-Aufgaben
- Indexlatenz cold und incremental
- DB-Größe pro 100.000 LOC
