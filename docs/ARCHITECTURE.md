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
│   ├── model-provider-contract/      # neutraler Provider-Stub, dev-only
│   └── storage-contract/             # adapterneutrale Store-Verträge, dev-only
├── fixtures/                       # kleine, lizenzkompatible Test-Repositories
└── docs/
~~~

Die Crates sind logische Grenzen innerhalb eines ausgelieferten Produkts, keine getrennten Dienste.
Die dev-only Workspace-Crates unter `tests/storage-contract` und `tests/model-provider-contract`
werden nicht ausgeliefert. Sie hängen nur von Domain und Application ab. Die Storage-Suite führt
dieselben Portverträge gegen jeden konkreten Storageadapter aus; der Provider-Stub ermöglicht
deterministische Consumer-Tests ohne Netzwerk oder Providerpayload.

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

### Agentenlauf

1. Ein Goal Contract wird erstellt oder bestätigt.
2. Der Controller lokalisiert relevante Evidenz.
3. Der Context Compiler erzeugt ein tokenbegrenztes Context Pack.
4. Der Application-Kern ruft den neutralen `ModelProvider` mit Gesamttimeout und Cancellation auf;
   der konkrete Adapter übersetzt den begrenzten Stream in `ProviderEvent`s.
5. Das Modellresultat wird erst nach einem terminalen Provider-Event als vollständige Ausgabe
   behandelt und liefert anschließend eine streng validierte Aktion.
6. Policy und Preconditions werden geprüft.
7. Genau ein Werkzeug wird ausgeführt.
8. Ergebnis, Evidenz und Ledger werden atomar aktualisiert.
9. Der Controller wechselt zu Verify, Replan oder AwaitApproval; `Done` ist ausschließlich nach
   vollständiger snapshotgebundener Prüfung durch den `AcceptanceVerifier` erreichbar.

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
