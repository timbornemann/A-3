# A^3 Systemarchitektur

Status: verbindliche Baseline  
Stand: 2026-08-03

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
├── fixtures/                       # kleine, lizenzkompatible Test-Repositories
└── docs/
~~~

Die Crates sind logische Grenzen innerhalb eines ausgelieferten Produkts, keine getrennten Dienste.

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

## Hauptlaufzeiten

### Projekt öffnen

1. UI sendet einen gewählten Pfad.
2. Rust kanonisiert und validiert den Pfad.
3. RepositoryIdentity und WorktreeIdentity werden bestimmt.
4. Katalog- und Projekt-DB werden geöffnet und migriert.
5. Letzter Snapshot wird mit dem aktuellen Worktree verglichen.
6. Ein inkrementeller Fast Index wird geplant.
7. UI erhält Projektzustand und Fortschrittsereignisse.

### Agentenlauf

1. Ein Goal Contract wird erstellt oder bestätigt.
2. Der Controller lokalisiert relevante Evidenz.
3. Der Context Compiler erzeugt ein tokenbegrenztes Context Pack.
4. Das Modell liefert eine streng validierte Aktion.
5. Policy und Preconditions werden geprüft.
6. Genau ein Werkzeug wird ausgeführt.
7. Ergebnis, Evidenz und Ledger werden atomar aktualisiert.
8. Der Controller wechselt zu Verify, Replan, AwaitApproval oder Done.

### Repositoryänderung

1. File Watcher erzeugt ein gebündeltes Change Set.
2. Betroffene Dateien werden neu gehasht und geparst.
3. Symbole und Kanten werden als Delta aktualisiert.
4. Abhängige Evidenz und Claims werden invalidiert.
5. Betroffene Modul- und Task-Lenses werden neu berechnet.
6. Ein laufender Agent darf erst nach Zustandsabgleich weiter mutieren.

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
