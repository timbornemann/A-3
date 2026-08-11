# A^3 Systemarchitektur

Status: verbindliche Baseline  
Stand: 2026-08-06

## Architekturziele

A^3 MUSS:

- auf Windows, Linux und macOS als Desktopanwendung laufen;
- ohne Cloud-Verbindung vollständig nutzbar sein;
- große Repositories inkrementell und nachvollziehbar erfassen;
- kleine lokale Coding-Modelle durch einen starken Harness unterstützen;
- Aussagen über Code an echte, auf Aktualität prüfbare Evidenz binden;
- Änderungen kontrolliert ausführen, verifizieren und rückverfolgbar machen;
- bei langen Aufgaben Ziel, Fortschritt, Entscheidungen und offene Punkte erhalten.

## Systemkontext

~~~mermaid
flowchart TD
    User["Entwickler"]
    Desktop["A^3 Desktop"]
    Repo["Lokales Repository"]
    Model["Lokaler Modellserver"]
    Tools["Build- und Testwerkzeuge"]

    User --> Desktop
    Desktop --> Repo
    Desktop --> Model
    Desktop --> Tools
~~~

A^3 besitzt in V1 keine notwendige Serverkomponente. Ollama oder ein kompatibler lokaler Modellserver ist ein austauschbarer Nachbarprozess und nicht Teil der Domäne.

## Container

~~~mermaid
flowchart TD
    UI["Svelte WebView"]
    Shell["Tauri Composition Root"]
    App["Application Core"]
    Infra["Lokale Adapter"]
    External["Repository, DB, Modell, Prozesse"]

    UI --> Shell
    Shell --> App
    App --> Infra
    Infra --> External
~~~

### Svelte WebView

Verantwortlich für Darstellung, Navigation, lokale Formzustände und Visualisierung. Sie enthält keine Indexierungs-, Agenten-, Sicherheits- oder Persistenzlogik. Sie kommuniziert ausschließlich über versionierte Commands, Queries und Events mit dem Rust-Kern.

### Tauri Composition Root

Erzeugt und verbindet Ports, Adapter, Datenbankverbindungen, Job Scheduler und Window State. Er validiert IPC-Nachrichten, übersetzt Protokolltypen in Use-Case-Aufrufe und enthält keine Fachlogik.

### Application Core

Enthält Use Cases und Orchestrierung:

- Projekt öffnen und schließen
- Indexlauf steuern
- Projektkarte erstellen
- Aufgabe und Goal Contract verwalten
- Context Pack kompilieren
- Agentenlauf steuern
- Werkzeugaktionen freigeben und ausführen
- Ergebnis verifizieren

### Lokale Adapter

- Repository- und Git-Erkennung
- Tree-sitter-Sprachadapter
- libSQL-Persistenz
- Ollama-kompatibler LLM-Provider
- Embedding-Provider
- Dateisystem und File Watcher
- Prozessausführung
- Zeit, IDs und Plattforminformationen

## Cargo-Workspace

~~~text
a3/
├── Cargo.toml
├── apps/
│   └── desktop/
│       ├── src/                    # Svelte + TypeScript
│       └── src-tauri/              # Composition Root
├── crates/
│   ├── a3-domain/                  # Entitäten, Value Objects, Invarianten
│   ├── a3-application/             # Use Cases, Ports, Controller
│   ├── a3-protocol/                # IPC DTOs und Events
│   ├── a3-storage-libsql/          # Persistenzadapter und Migrationen
│   ├── a3-repo-index/              # deterministischer Index
│   ├── a3-project-map/             # budgetierte LLM-Exploration
│   ├── a3-context/                 # Retrieval und Context Compiler
│   ├── a3-provider/                # LLM- und Embedding-Adapter
│   └── a3-workspace/               # Dateien, Patches und Prozesse
├── tests/
│   ├── agent-harness/                # endliche Agent-End-to-End-Abnahme, dev-only
│   ├── model-provider-contract/      # neutraler Provider-Stub, dev-only
│   └── storage-contract/             # adapterneutrale Store-Verträge, dev-only
├── fixtures/                       # kleine, lizenzkompatible Test-Repositories
└── docs/
~~~

Die Crates sind logische Grenzen innerhalb eines ausgelieferten Produkts, keine getrennten Dienste.
Die dev-only Workspace-Crates unter `tests/` werden nicht ausgeliefert. Storage- und
Model-Provider-Contract hängen nur von Domain und Application ab. Die Storage-Suite führt dieselben
Portverträge gegen jeden konkreten Storageadapter aus; die Provider-Suite prüft Stub und
Ollama-Adapter über dieselbe neutrale Streamprojektion. `agent-harness` komponiert ausschließlich
für die Offline-Abnahme reale Feature- und Adaptergrenzen über die drei Produkt-Fixtures.

## Abhängigkeitsrichtung

~~~mermaid
flowchart TD
    Protocol["a3-protocol"]
    Domain["a3-domain"]
    Application["a3-application"]
    Features["Index, Map, Context"]
    Adapters["Storage, Provider, Workspace"]

    Application --> Domain
    Features --> Domain
    Features --> Application
    Adapters --> Domain
    Adapters --> Application
~~~

Regeln:

- a3-domain besitzt keine Abhängigkeit zu anderen A^3-Crates.
- a3-application kennt Ports, aber keine konkreten Adapter.
- Feature-Crates dürfen Use-Case-Ports verwenden, aber keine UI kennen.
- Adapter dürfen Domain- und Porttypen verwenden; Adaptertypen verlassen den Adapter nicht.
- a3-protocol enthält nur serialisierbare Grenztypen, hängt von keinem anderen A^3-Crate ab und ist kein zweites Domänenmodell.
- Domain- und Protocol-Typen werden am Tauri-Rand explizit gemappt.
- src-tauri ist die einzige Stelle, an der alle konkreten Adapter zusammengebaut werden.
- Zyklische Crate-Abhängigkeiten sind verboten.

Gemäß ADR-0018 besitzt `a3-application` den allgemeinen `ModelProvider`-Port und alle neutralen
Request-, Event-, Timeout-, Cancellation- und Fehlertypen. `a3-provider` implementiert diesen Port,
kennt die Ollama-kompatiblen HTTP-/NDJSON-Payloads und hängt ausschließlich nach innen von
Application und Domain ab. Der erste HTTP-Adapter verwendet das gepinnte `reqwest` nur mit JSON-,
Streaming- und Rustls-Unterstützung, weil die Standardbibliothek keinen asynchronen, abbrechbaren
HTTP-Body-Stream bereitstellt. Redirects und Umgebungsproxies sind für diesen Client deaktiviert.

## Hauptlaufzeiten

### Projekt öffnen

1. UI fordert über den versionierten `open_project`-Command eine native Ordnerauswahl an und sendet keinen Pfad.
2. Der privilegierte Desktop-Adapter öffnet genau einen nativen Ordnerdialog; Abbruch beendet den Use Case ohne Inspektion.
3. Rust kanonisiert und validiert ausschließlich den vom Betriebssystemdialog zurückgegebenen Pfad.
4. RepositoryIdentity, WorktreeIdentity einschließlich `WorktreeAnchorId` und HEAD-Zustand werden mit
   isolierter repository-lokaler Git-Konfiguration bestimmt.
5. `KnowledgeStore` prüft read-only, ob das Ziel bereits bekannt ist oder genau ein früherer Worktree
   dieselbe Anchor-ID und dieselbe Repository-ID beziehungsweise denselben vorhandenen
   Remote-Fingerprint besitzt. Mehrdeutige Kandidaten werden nicht ausgewählt.
6. Nur für einen eindeutigen Kandidaten fragt der Application-Use-Case über einen schmalen Port einen
   privilegierten nativen Dialog ab. „Abbrechen“ mutiert nichts; „separat öffnen“ folgt dem normalen
   Pfad; „reconciliieren“ bindet den bestätigten Bestand um. Die WebView erhält weder Kandidaten noch
   eine Bestätigungs-Capability.
7. Der libSQL-Adapter persistiert eine Reconciliation vor dem atomaren Verzeichnisumzug, migriert und
   schreibt die Knowledge-Identität transaktional um und aktualisiert den Katalog zuletzt. Ein
   Neustart setzt einen `prepared`-Zustand ohne zweite Bestätigung fort. Exakte Revisionen und
   Quell-/Zielzustände werden erneut geprüft; bestehende Ziele werden nicht überschrieben.
8. Ohne Reconciliation leitet der Adapter aus der validierten `WorktreeId` ausschließlich unter
   App-Data `projects/<WorktreeId>/knowledge.db` ab, öffnet beziehungsweise migriert sie, prüft die
   persistierte Bindung und registriert die Beobachtung erst danach atomar im globalen Katalog.
9. Die WebView erhält nur IDs, HEAD und einen nicht-autoritativen Anzeigepfad, niemals Git Common
   Directory, gespeicherte Rohpfade, Datenbankhandles oder Dateisystemzugriff.

Der schmale `KnowledgeIndexStore`-Port stellt unveränderliche Snapshot-Ketten, den serialisierten
IndexRun-Lifecycle und das atomische Publish bereit. Diese Application-Grenze verwendet ausschließlich
Domain-Typen; SQL, libSQL-Zeilen und Datenbankhandles bleiben im Adapter. Discovery und Hashing
erzeugen die Snapshot-Deltas, Parser, Linker und Ranker den vollständigen `IndexPublication`-Input.
Der libSQL-Adapter schreibt die run-gebundenen Datei-, Symbol-, Kanten-, Kandidaten- und Rankzeilen
und den Zustand `published` in genau einer Transaktion. Leser arbeiten ausschließlich auf dem
jüngsten vollständig veröffentlichten Run. Beim erfolgreichen Ersatz werden die regenerierbaren
Zeilen älterer Runs innerhalb derselben Transaktion entfernt; Run-Metadaten und Snapshotkette bleiben
für monotone Historie erhalten. Rebuild entfernt nur regenerierbare Indexzeilen.

Der Adapter hält getrennt höchstens vier identitäts- und policygeprüfte Mutations- sowie vier
Read-Datenbankhandles. Dadurch wiederholt ein serialisierter Refresh nicht vor jedem Snapshot- oder
Run-Schritt Migration, Integritätsprüfung und Open. Die Handles verlassen den Adapter nicht;
Reconciliation leert beide begrenzten Caches vor Preflight und Verzeichnisumzug.

Der Desktop-Composition-Root aktiviert nach einem erfolgreichen Project Open genau einen
`RepositoryIndexManager`. Sein besitzender Koordinator hält Watcher, Parse-Cache und einen
nicht-besitzenden Scheduler-Submitter. Watcher- und Scheduler-Channels sind begrenzt; ein aktiver
Refresh serialisiert Snapshot-Append und Publish für den Worktree. Projektwechsel fordert den alten
Job kooperativ ab und wartet vor einem neuen Refresh auf dessen terminalen Zustand. Manager,
Watcher und Scheduler besitzen explizite Shutdown- und Join-Pfade.

### Zuletzt verwendete Projekte

1. Die UI sendet über `list_recent_projects` ausschließlich die aktuelle Protokollversion.
2. Der Application-Use-Case setzt das feste V1-Limit von zehn Einträgen; die WebView kann weder einen
   Rohpfad noch ein unbeschränktes Limit vorgeben.
3. Der libSQL-Adapter liest most-recent-first und rekonstruiert IDs sowie HEAD in typisierte Werte.
4. Ungültige persistierte Werte werden am Adapterrand als stabiler Storagefehler abgelehnt.
5. Der Composition-Root mappt nur `ProjectId`, `RepositoryId`, `WorktreeId`, HEAD und den begrenzten,
   nicht-autoritativen Anzeigepfad auf V1-DTOs.

### Aktiver Projektstatus

1. Nach einem erfolgreichen Project Open hält der Desktop-Composition-Root die bereits validierte
   `ProjectIdentity` und `ProjectId` im Core-Prozess; ein WebView-Reload verändert diesen Zustand
   nicht.
2. `query_project_status` akzeptiert ausschließlich die Protokollversion und wählt weder Pfad noch
   Projekt-ID aus der WebView.
3. Der Application-Use-Case liest über `KnowledgeIndexStore` nur den letzten Snapshot, den letzten
   Indexversuch und den weiterhin atomar veröffentlichten Run.
4. Ein separater `ProjectStorageStore` misst begrenzt nur das private
   `projects/<WorktreeId>`-Verzeichnis. IPC V1 codiert Bytezahl und Snapshotgeneration verlustfrei als
   Dezimaltext.
5. IPC V1 projiziert Worktree-/HEAD-Anzeige, Snapshot-ID, Generation, Storagegröße und den
   geschlossenen Indexstatus. Autoritative Pfade, Datenbankhandles und Indexinhalte verlassen den
   Core nicht.

### Index-Rebuild aus Projects

1. Die WebView sendet über `rebuild_project_index` ausschließlich die Protokollversion; Projekt und
   Worktree stammen aus dem Core-eigenen aktiven Zustand.
2. Der `RepositoryIndexManager` setzt den Rebuild in seine bounded Commandqueue, fordert für einen
   laufenden Refresh Cancellation an und wartet auf dessen terminalen Schedulerzustand.
3. Der Scheduler besitzt den Rebuild-Job und reicht Cancellation sowie determinierten Progress an
   `KnowledgeIndexStore::rebuild_regenerable_index` weiter. Der Storageadapter löscht ausschließlich
   regenerierbare Indexprojektionen in einer Transaktion.
4. Nach erfolgreichem Commit fordert der Manager einen expliziten vollständigen Rescan an. Ein
   fehlgeschlagener oder abgebrochener Rebuild veröffentlicht keinen partiellen Indexzustand; der
   geschlossene Rebuildstatus bleibt über `query_project_status` sichtbar.

### Agentenlauf

1. Ein Goal Contract wird erstellt oder bestätigt.
2. Der Controller lokalisiert relevante Evidenz.
3. Der Context Compiler erzeugt ein tokenbegrenztes Context Pack.
4. Der Application-Kern ruft den neutralen `ModelProvider` mit Gesamttimeout und Cancellation auf;
   der konkrete Adapter übersetzt den begrenzten Stream in `ProviderEvent`s.
5. Das Modellresultat wird erst nach einem terminalen Provider-Event als vollständige Ausgabe
   behandelt und liefert anschließend eine streng validierte Aktion.
6. Jede typisierte Action durchläuft genau einmal den zentralen `EvaluateActionPolicy`-Use-Case.
   Aktionsklasse und Risiko werden aus der Action abgeleitet; die feste Systempolicy wird nur durch
   restriktivere Workspace-Regeln überlagert. Ergebnis ist eine begründete `PolicyDecision` mit
   genau einem content-freien RunEvent und optional einem exakten `ApprovalRequest`.
7. Genau ein Werkzeug wird nur bei `Allowed` ausgeführt. `ApprovalRequired` und `Denied` dürfen die
   Toolgrenze nicht erreichen; eine Freigabe wird für genau Run, Action-Fingerprint und Scope
   einmalig atomar mit der erlaubenden Entscheidung verbraucht.
8. Ergebnis, Evidenz und Ledger werden atomar aktualisiert.
9. Der Controller wechselt zu Verify, Replan oder AwaitApproval; `Done` ist ausschließlich nach
   vollständiger snapshotgebundener Prüfung durch den `AcceptanceVerifier` erreichbar.

Gate M6 belegt diesen Pfad ohne zweite Testarchitektur: temporäre Rust-, TypeScript- und Python-
Worktrees werden real indiziert und publiziert; zwei providerneutrale Modellturns führen über
Search, durable Tool-Evidence, Ledger-Verifikation und Acceptance bis `Done`. Vorher und nachher
bleibt der Repository-Dateibaum bytegleich. Ein Negativlauf schickt ungültige Primär- und
Reparaturausgabe durch denselben Compiler und weist null Toolaufrufe, Toolversuche und Tool-Events
nach.

M7/E1 persistiert Entscheidung, optionalen Request, Runprojektion, genau ein Audit-Event und den
optionalen Approval-Verbrauch in einer kurzen `IMMEDIATE`-Transaktion. Grant und Widerruf sind
eigene explizite User-Use-Cases mit jeweils genau einem Journalereignis. Der Application-Port gibt
weder libSQL-Zeilen noch rohe Pfade, Prozessdaten oder Netzwerkziele weiter; dauerhafte Scopes und
Actionen werden ausschließlich als domain-separierte Digests gespeichert. Der gemeinsame
Storagevertrag prüft Reopen, Pfad-Mismatch ohne Verbrauch, einmaligen Verbrauch, Widerruf,
restriktive Workspace-Regeln und vollständigen Rollback bei veralteter Runsequenz.

M7/E2 ergänzt zwei schmale read-only Application-Ports. Der Source-Reader akzeptiert nur eine
aktuelle `FileRevision` und eine typisierte begrenzte File-Inspection desselben Pfads. Der
Workspace-Adapter löst den Repository-Pfad plattformspezifisch auf, prüft ihn nach
Symlinkauflösung gegen den kanonischen Worktree-Root, öffnet ausschließlich eine reguläre Datei
ohne dem letzten Reparse-/Symlink-Element zu folgen und liest in 64-KiB-Blöcken bis zur festen
4-MiB-Grenze. Handle-Metadaten, kanonisches Ziel und vollständiger BLAKE3-Hash werden nach dem
Lesen erneut geprüft. Binary- oder Secret-Kandidaten verlassen den Adapter nur als stabile,
content-freie Fehlerklasse; eine erfolgreiche 12-KiB-Seite trägt intrinsisch ihre exakte File-
oder Span-Evidence.

Der Directory-Port bindet jede vorwärts paginierte Anfrage an `WorktreeId`, `SnapshotId` und
höchstens 256 direkte Kinder. `a3-workspace` projiziert diese Kinder ausschließlich aus dem
aktuellen `PublishedIndex`, der bereits die Git-/Projekt-Ignore-Policy aus Discovery enthält, und
wendet die nicht übersteuerbaren V1-Ausschlüsse für Secrets, Binary, Vendor und Generated als
zweite Sicherheitsgrenze an. Verzeichnisse werden nur aus aktuellen Dateinachfahren abgeleitet und
behalten eine konkrete unterstützende `FileRevision`; der angeforderte Live-Subtree wird dennoch
erneut kanonisiert und als Verzeichnis innerhalb des Roots bestätigt. Weder Port wird der WebView
als generischer Dateisystemzugriff exponiert.

M7/E3 ergänzt im Domain-Kern die geschlossene `PatchActionSchemaVersion::V1` mit vollständigen
UTF-8-Dateiinhalten und getrennten Add-, Update-, Move- und Delete-Operationen. Die Action bindet
Run, Worktree, Published Snapshot, TaskStep und erwartete Verification; ihr domain-separierter
Digest umfasst Rationale, Pfade sowie alle erwarteten und neuen BLAKE3-Hashes. Eine zweite
content-freie Scope-Projektion bindet die zentrale Policy-Freigabe an genau diesen Worktree und
Pfadsatz. `AuthorizedPatchAction` ist eine nicht klonbare Application-Capability und entsteht nur
aus einer verbrauchten `ApprovalGranted`-Entscheidung mit identischem Run, Fingerprint, Scope,
Klasse und Risiko.

Der `WorkspacePatchTool` bleibt ein schmaler Port aus Preview und einmaligem Apply. Der Adapter
prüft Published Snapshot, Indexrevisionen, kanonischen Root, jeden Pfad und jeden Live-Hash sowohl
für die Vorschau als auch erneut unmittelbar vor der ersten Mutation. Add und Move verlangen in
V1 ein bereits vorhandenes kanonisches Elternverzeichnis; Verzeichnisanlage ist keine implizite
Patchwirkung. Add- und Update-Inhalte werden vollständig in demselben Zielverzeichnis gestaged und
vor Sichtbarkeit synchronisiert. Add und Move verwenden No-Replace-Semantik, Update nutzt die
atomare Ersetzungs-Umbenennung der Plattform, Delete die atomare Dateientfernung soweit vom
Dateisystem bereitgestellt. Ein adapterlokaler Worktree-Lease verhindert parallele Anwendung;
die controllerweite Serialisierung aller Mutationstypen übernimmt E7.

`PatchPreview` speichert exakte unnormalisierte UTF-8-Präfixe mit vollständigem Hash, Bytezahl,
Encoding-, Line-Ending- und Trunkierungsmetadaten. Sie ist auf 16 KiB pro Inhaltsseite und 64 KiB
gesamt begrenzt. Nach jeder sichtbaren Operation wird der tatsächliche Zustand erneut gelesen.
`PatchChangeSet` bindet vollständige oder kanonisch partielle Änderungen an Action, Approval,
TaskStep, Verification und Basissnapshot und liefert die sortierten Pfade für die von E7 vor dem
nächsten Modellturn auszulösende Evidence- und Indexinvalidierung. Fehler nach einer bereits
sichtbaren Operation tragen dieses Change-Set ausdrücklich, statt die Mutation zu verbergen.

M7/E4 ergänzt `ProcessRunner` als Application-Port mit einer einmalig verbrauchbaren
`AuthorizedProcessSpec`; der Port enthält keine Betriebssystem-, Tauri- oder Persistenztypen. Der
Workspace-Adapter baut einen direkten `std::process::Command`, prüft das CWD gegen den kanonischen
Worktree, löst das Executable als kanonische reguläre Datei auf, leert die Umgebung und übernimmt
nur explizit injizierte Allowlist-Werte.
`command-group` kapselt dabei die vorhandenen Plattformprimitiven für eine eigene Unix Process
Group beziehungsweise ein Windows Job Object. Die Abhängigkeit bleibt ausschließlich im
Infrastruktur-Adapter; Domain und Application kennen weder die Bibliothek noch Plattformtypen.

Jede Ausführung besitzt den Gruppenprozess, beide Reader-Threads und einen begrenzten Channel. Der
Adapter drainiert stdout und stderr auch nach Erreichen der Retained Limits, liefert lückenlos
sequenzierte begrenzte Events und joint alle Reader vor dem terminalen Resultat. Timeout,
Cancellation und Event-Sink-Fehler beenden die gesamte Gruppe und warten sie ein. Das Resultat
enthält vollständige Bytezähler und Digests, aber nur secret-geprüfte begrenzte Inhalte. E4 führt
weder Verification-Klassifikation aus E6 noch die controllerweite Mutationsserialisierung und
Evidence-Invalidierung aus E7 vorweg.

M7/E5 liest keine Manifestdatei erneut und vertraut keinem Repositorytext als Instruktion. Der
Application-Use-Case `DiscoverProjectCommands` konsumiert ausschließlich `manifest_files` und
`Manifest`-Relationen eines atomar publizierten `PublishedIndex`. Rust-Befehle verwenden direkte
Cargo-argv mit `--offline` und für Test, Build und Clippy zusätzlich `--locked`. Node-Befehle werden
nur für explizite test-, build-, lint- oder format-Skriptnamen und genau einen durch aktuellen
pnpm-, npm- oder Yarn-Marker belegten Package Manager erzeugt. Python-Befehle entstehen nur aus
belegten Build-, pytest-, Ruff-, Black- oder Mypy-Relationen. Package-Eltern bestimmen das
`WorkspaceDirectory`; mehrdeutige Node-Package-Manager und alle Install-/Lifecycle-Skripte liefern
kein Kommando.

`ProjectCommandCatalog` erzeugt plan-ungebundene `ProcessSpec`-Vorschauen. Die separate
`CommandAllowlistStore`-Grenze persistiert eine explizite Auswahl append-only in der privaten
Worktree-Datenbank. Der libSQL-Adapter verwendet monotone Revisionen und `IMMEDIATE`-CAS; eine
veraltete Bestätigung schreibt keine Teilzeile. Erst aktueller Katalog, passende gespeicherte
Allowlist und validierte `TaskStepId` ergeben einen automatisch policy-fähigen Spec. Ein
Package-Manager kann ein bestätigtes Repositoryskript intern über seinen eigenen Interpreter
ausführen; A^3 selbst fügt dabei keine Shell ein und bestätigt niemals einen rohen Skriptwert.

M7/E6 hält die Verifikationsentscheidung im Domain-/Application-Kern und nicht im ProcessRunner
oder Storageadapter. Der Ledger persistiert die geschlossene operationale Spec samt Must-/Should-
Criterion-Mapping. Aus Process-, Patch-, zwei geordneten Published Indexes oder Userresultaten
entstehen immutable typisierte
Artifacts; der Erfolg wird ausschließlich aus Artifact-Semantik und aktuellem Published Index
abgeleitet. `VerificationEvidenceStore` ist ein schmaler lokaler Port für zeitbegrenztes,
abbrechbares Append/Reopen und einen
begrenzten konsistenten Acceptance-Read. Der produktive `DeterministicAcceptanceVerifier` lädt
genau die Must-Evidence, prüft Freshness und bindet einen aus Goal, Ledger, Run, Published Index und
originalen Task-Lens-Claims regenerierten Run-Memory-Checkpoint. Weder libSQL-Zeilen noch
Prozessoutput oder eine vom LLM behauptete Erfolgsentscheidung verlassen ihre Adaptergrenzen.

M7/E7 erweitert den allgemeinen Model-Turn ausschließlich um die streng dekodierten
`ApplyPatch`- und `Run`-Varianten. Der Turn-Use-Case besitzt weiterhin nur den read-only
`AgentReadTools`-Port und gibt Mutationen unausgeführt an `ExecuteMutatingAgentAction` zurück.
Dieser Application-Use-Case komponiert die bereits vorhandenen schmalen Ports und hält dabei einen
vom Composition Root injizierten `WorktreeMutationCoordinator`-Lease. Damit kann pro Worktree
controllerweit nur eine Patch- oder Process-Mutation gleichzeitig laufen, ohne einen globalen
Singleton oder einen Mutex-Guard über asynchrone Adapteraufrufe zu halten.

Die Reihenfolge ist fest: Action-/Ledger-/Snapshotanker prüfen, Worktree-Lease erwerben,
Patchvorschau gegebenenfalls bilden, zentrale Policyentscheidung samt Approvalzustand
persistieren, Toolversuch vor Ausführung persistieren, genau ein Tool aufrufen und das Ergebnis
auflösen. Erfolgreiche normalisierte Mutationen schließen Versuch, `tool_action`-Event und
Runprojektion atomar ab. Jeder vollständige oder partielle Patchpfad läuft noch unter demselben
Lease durch `RefreshRepositoryIndex`; nur dessen vollständig rekonstruierter neuer
`PublishedIndex` darf Run und nächsten Context Pack fortschreiben. Ein fehlgeschlagener Refresh
stoppt den Lauf, statt mit altem Kontext weiterzuarbeiten.

`DiffInvariant` wird unmittelbar aus dem tatsächlichen `PatchChangeSet` gegen den neuen Index
bewertet. Process-Evidence verwendet vollständig gedrainte Resultate und aktuelle Manifest-
Abhängigkeiten; die konservative Standardfactory erzeugt nur Command-Semantik. Test und Diagnostic
benötigen einen strukturierten Adapter und können deshalb nicht aus Exitcode allein erfolgreich
werden. Der content-freie Fortschrittsdetektor erlaubt einen ersten identischen Fehlerretry mit
frischem Kontext, wechselt beim zweiten nach `Replan` und stoppt jede weitere identische Serie.

### Agentenlauf nach Appneustart

1. Der Application-Kern lädt die materialisierte Runprojektion und das revisionsgebundene Ledger;
   ein Journal-Replay ist nicht erforderlich.
2. Vor dem Abbruch dauerhaft laufende Toolversuche werden als `Interrupted` markiert.
3. Der jüngste Published Snapshot wird geladen und abgeschlossene Verification-Evidence gegen
   dessen content-adressierte FileRevisionen geprüft.
4. Resume ist nur mit vollständig frischer Evidence zulässig; Replan und Cancel invalidieren stale
   Evidence und öffnen abhängige Schritte über die bestehende Ledgerlogik neu.
5. Die explizite Benutzerwahl wird mit Published-Snapshot-, Ledger- und Run-CAS atomar als neues
   Ledger, neue Runprojektion und append-only Recovery-Event committed.

### Repositoryänderung

1. Der Git-gestützte Polling-Watcher erzeugt nach Debounce ein kanonisch gebündeltes Change Set.
2. Queue-Verlust, Initialisierung, Repository-Metadatenänderung oder Beobachtungsfehler erzwingen
   sichtbar einen Full Rescan.
3. Git-Discovery bestätigt den relevanten Pfadsatz; nur Hinweise und neue Pfade werden vollständig
   gehasht, beim Full Rescan alle relevanten Dateien.
4. Ein exakter Parent-Snapshot-Cache wird kopiert; nur hinzugefügte oder geänderte unterstützte
   Dateien werden neu gelesen und geparst. Ohne kohärenten Cache erfolgt ein begrenzter Vollparse.
5. Der vollständige kanonische Graph wird neu gelinkt und gerankt und über den bestehenden
   `IndexRun` atomar veröffentlicht.
6. Symbole und Kanten der alten Sicht werden erst mit dem erfolgreichen Commit unsichtbar.
7. Vor Sichtbarkeit des neuen Runs invalidiert dieselbe Storage-Transaktion direkte Evidence und
   Claims, setzt die eigene Card auf `Stale`, direkte abhängige Cards auf `NeedsReview` und schreibt
   die begrenzte Direkt-vor-Abhängig-Remapqueue.
8. Der read-only `ModuleRemapQueueStore`-Port liefert eine kleine, zielrungebundene Queue-Seite;
   nach erfolgreicher Card-Neupublikation verschwindet das Modul atomar aus der Queue. Task Lenses
   werden gegen den aktuellen Run neu kompiliert und sehen nur weiterhin aktive Published-Claims.
9. Ein laufender Agent darf erst nach Zustandsabgleich weiter mutieren.

## Parallelität

- Pro Worktree existiert genau ein Mutations-Lock.
- Read-only Suche, Indexabfragen und UI-Queries dürfen parallel laufen.
- Index-Commits erfolgen transaktional und snapshotbezogen.
- Deep Map und Embeddings laufen als abbrechbare Jobs mit niedrigerer Priorität als interaktive Agentenanfragen.
- Bei GPU-Konkurrenz pausiert der Scheduler nicht dringende Mapping- oder Embedding-Jobs.
- Alle Queues sind begrenzt; Überlast wird als Zustand sichtbar gemacht.

Die konkreten Foundation-Invarianten für Jobzustände, Backpressure, Ereignisreihenfolge, Cancellation und Shutdown beschreibt [JOB_RUNTIME.md](JOB_RUNTIME.md).

## Erweiterbarkeit

Neue Sprachen werden als LanguageAdapter ergänzt. Neue Modelle werden als ModelProvider ergänzt. Neue Speicherengines werden als KnowledgeStore ergänzt. Eine Erweiterung DARF NICHT erfordern, dass Domain- oder Use-Case-Code provider- oder plattformspezifische Fallunterscheidungen erhält.

## Primärquellen zur Technologieentscheidung

- [Tauri Architecture](https://v2.tauri.app/concept/architecture/)
- [Tauri Process Model](https://v2.tauri.app/concept/process-model/)
- [Turso Rust SDK Reference](https://docs.turso.tech/sdk/rust/reference)
- [Turso Code Indexing](https://docs.turso.tech/guides/code-indexing)
- [libSQL AI and Embeddings](https://docs.turso.tech/features/ai-and-embeddings)
