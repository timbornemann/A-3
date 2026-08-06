# ADR-0018: ModelProvider-Port gehört zur Application-Grenze

Status: Accepted  
Datum: 2026-08-06  
Entscheider: Tim Bornemann  
Supersedes: ADR-0011

## Kontext

ADR-0011 legt die providerneutrale Abstraktion, Ollama als ersten Adapter, neutrale Streaming-
Events und eine sichere Endpoint-Policy fest. Zugleich ordnet eine Formulierung dort den
`ModelProvider`-Port dem Crate `a3-provider` zu. Das widerspricht ADR-0003: Danach besitzt
`a3-application` die Use-Case-Ports, während Adapter-Crates konkrete Modellprovider kapseln und
nach innen auf Application und Domain zeigen.

Würde `a3-application` den Port aus dem konkreten Adapter-Crate importieren, zeigte seine
Abhängigkeit nach außen. Würde `a3-provider` gleichzeitig Application-Typen implementieren,
entstünde ein Crate-Zyklus. Eine eindeutige Port-Zuständigkeit ist deshalb vor H4 erforderlich.

## Entscheidung

- `a3-application` definiert den allgemeinen `ModelProvider`-Port, providerneutrale Requests,
  `ProviderEvent`s, normalisierte Fehler und die Cancellation-/Timeout-Grenze.
- Infrastrukturfreie Provider- und Modellidentitäten liegen in `a3-domain`.
- `a3-provider` ist ein Adapter-Crate. Es implementiert den Application-Port und hält
  Ollama-Payloads, HTTP, Endpoint-Parsing und Endpoint-Policy-Integration vollständig innerhalb
  seiner Grenze.
- Provider-spezifische Payloads und Fehler verlassen den Adapter weiterhin nicht.
- Die übrigen Entscheidungen von ADR-0011 bleiben unverändert: Ollama-kompatibel zuerst, kein
  gebündelter Modellserver, neutrale Streaming-Events, Capability-basierte `ModelProfile`s und
  standardmäßig blockierte nicht lokale Endpoints.

## Konsequenzen

### Positiv

- Die Abhängigkeitsrichtung bleibt `a3-provider` → `a3-application` → `a3-domain`.
- Use Cases können jeden Adapter oder einen deterministischen Stub über denselben Port verwenden.
- Ollama- und HTTP-Typen bleiben von Domain, Application und späterem Controller isoliert.

### Negativ

- Port und erste Implementierung liegen in unterschiedlichen Crates.
- Provider-Vertragstests benötigen ein separates dev-only Test-Crate oder eine äquivalente
  wiederverwendbare Testsuite.

### Risiken und Gegenmaßnahmen

- Providerdetails wandern in neutrale Typen — Dependency- und Quelltextprüfungen verbieten
  `reqwest`- sowie Ollama-Imports in `a3-application`.
- Der neutrale Port wird zum kleinsten gemeinsamen Nenner — Fähigkeiten bleiben in versionierten
  `ModelProfile`s ausdrückbar und werden nicht aus Modellnamen abgeleitet.

## Verworfene Alternativen

- Application hängt von `a3-provider` ab — verletzt ADR-0003 und koppelt Use Cases an Adapter.
- Port und Ollama-Adapter liegen gemeinsam in `a3-provider` — erzwingt auswärts gerichtete
  Application-Abhängigkeiten oder einen Zyklus.
- Zusätzliches `a3-provider-api`-Crate — schafft für den ersten vertikalen Slice eine leere Grenze,
  obwohl ADR-0003 bereits einen eindeutigen Port-Eigentümer festlegt.

## Compliance

- Der Cargo-Graph enthält ausschließlich die Richtung `a3-provider` → `a3-application` →
  `a3-domain`.
- `a3-application` importiert weder Ollama-Payloadtypen noch HTTP-Clienttypen.
- Eine adapterneutrale Stub-Suite prüft den Port; der Ollama-Stubserver prüft das konkrete Mapping.

## Referenzen

- [ADR-0003](0003-modular-monolith-and-dependencies.md)
- [ADR-0011](0011-local-model-provider-abstraction.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Harness-Plan H4](../plans/04-MEMORY_AND_AGENT_HARNESS.md#h4-modelprovider)
