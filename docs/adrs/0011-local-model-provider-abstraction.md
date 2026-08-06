# ADR-0011: Lokale Modellprovider-Abstraktion mit Ollama zuerst

Status: Superseded by ADR-0018  
Datum: 2026-08-03

## Kontext

Der Nutzer kann A^3 bereits über die Ollama API verbinden. Modelle unterscheiden sich jedoch bei Kontextgröße, Structured Output, Tool Calling, Tokenizer und Leistung. Providerdetails dürfen den Harness nicht prägen.

## Entscheidung

- a3-provider definiert ModelProvider und EmbeddingProvider als Ports.
- Der erste Adapter spricht eine Ollama-kompatible lokale API.
- A^3 bündelt in V1 keinen Modellserver und verwaltet keine Modellgewichte.
- Jeder Provider meldet über ein validiertes ModelProfile seine Fähigkeiten.
- Strukturierte Aktionen verwenden JSON Schema; nativer Tool-Call-Support ist optional.
- Streaming wird in neutrale ProviderEvents übersetzt.
- Provider-Payloads und Modellnamenlogik verlassen den Adapter nicht.
- Nicht lokale Endpunkte sind standardmäßig blockiert beziehungsweise deutlich freigabepflichtig.

ModelProfile enthält mindestens:

- Provider und Modell-ID
- Context Limit und Output Limit
- Tokenizer oder konservative Zählstrategie
- Structured-Output- und Tool-Fähigkeit
- Parallelitätslimit
- Temperatur- und Samplingprofil
- Stopbedingungen
- bekannte Promptbesonderheiten

## Konsequenzen

### Positiv

- schneller Start mit bestehendem Ollama-Setup
- spätere Adapter für llama.cpp oder andere lokale Runtimes
- Harness bleibt modellneutral

### Negativ

- kleinster gemeinsamer Nenner darf Fähigkeiten nicht künstlich begrenzen.
- Capability Detection und Fehlernormalisierung benötigen Tests.

### Risiken und Gegenmaßnahmen

- Anbieter behauptet nicht funktionierende Fähigkeit — Start-Self-Test und Profile Override.
- inkorrektes JSON — strikte Validierung, höchstens eine Reparatur, keine Ausführung bei Fehler.

## Verworfene Alternativen

- Ollama-Typen überall — starke Kopplung.
- eingebettete Inferenz in V1 — Packaging-, Treiber- und GPU-Komplexität verzögert den Harness.
- nur OpenAI-kompatible API — deckt lokale provider-spezifische Fähigkeiten nicht immer sauber ab.

## Compliance

Provider-Contract-Suite mit Stubserver; Application-Code darf keine provider-spezifischen JSON-Strukturen importieren.
