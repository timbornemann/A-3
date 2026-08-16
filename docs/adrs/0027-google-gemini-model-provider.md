# ADR-0027: Google Gemini Model-Provider Adapter und Rollenaktivierung

Status: Accepted

Datum: 2026-08-16

Entscheider: Tim Bornemann

Supersedes: Ergänzt ADR-0011, ADR-0018, ADR-0023, ADR-0026

## Kontext

A^3 unterstützte bisher primär lokale Modelle über den Ollama-Adapter (`ADR-0011`, `ADR-0018`, `ADR-0026`). Für Nutzer, die rechenstarke Cloud-Modelle (wie Google Gemini 2.5 Flash / Pro) oder Gemini-Embeddings (wie `text-embedding-004`) nutzen möchten, soll Google Gemini als zusätzlicher nativer Provider bereitgestellt werden.

Dabei müssen alle Architektur- und Sicherheitsinvariants aus `AGENTS.md`, `ADR-0002`, `ADR-0018` und `ADR-0023` strikt eingehalten werden:
1. Der WebView-Bereich bleibt unprivilegiert und erhält niemals API-Keys, Credentials oder unbeschränkten Netzwerkzugriff.
2. Settings-Speicher (`libsql`) speichert ausschließlich credential-freie Endpoints (z. B. `https://generativelanguage.googleapis.com` oder ein Loopback-Proxy `http://127.0.0.1:port`).
3. Authentifizierungs-Secrets (`GEMINI_API_KEY` / `GOOGLE_API_KEY`) werden ausschließlich im Rust-Desktop-Prozess aus der Host-Umgebung geladen und über den Standard-Header `x-goog-api-key` an Google-APIs gesendet.
4. Alle Capability- und Model-Operationen (Streaming über SSE, Modellerkennung via `/v1beta/models`, Structured Output Verification via JSON Schema Probe und Embeddings via `embedContent`/`batchEmbedContents`) laufen über dieselben strikten Ports (`ModelProvider`, `ModelCatalogProvider`, `ModelCapabilityProbe`, `EmbeddingCapabilityProbe`, `EmbeddingProvider`).

## Entscheidung

- **Neuer Adapter `GeminiModelProvider` in `a3-provider`**:
  - Implementiert die neutralen Ports `ModelProvider`, `ModelCatalogProvider`, `ModelCapabilityProbe`, `EmbeddingCapabilityProbe` und `EmbeddingProvider`.
  - Unterstützt Server-Sent Events (SSE) für Streaming-Antworten über `POST /v1beta/models/{model}:streamGenerateContent?alt=sse`.
  - Führt eine echte Structured-Output-Probe mit `responseMimeType: "application/json"` und `responseSchema` durch, um `ModelStructuredOutputCapability::Verified` zu verifizieren.
  - Ermöglicht Modellkatalog-Erkennung über `GET /v1beta/models` und Embeddings über `embedContent` / `batchEmbedContents`.
  - Authentifizierung erfolgt isoliert im Rust-Backend über den Header `x-goog-api-key` unter Verwendung von `GEMINI_API_KEY` oder `GOOGLE_API_KEY`.
- **Endpoint-Validierung & Policy**:
  - `GeminiEndpoint` normalisiert `localhost` auf `127.0.0.1` und erzwingt HTTPS für Remote-Origins.
  - `StandardGeminiEndpointPolicy` autorisiert sowohl lokale Loopback-Proxies als auch den kanonischen Google-HTTPS-Origin.
  - `LocalOnlyGeminiEndpointPolicy` ermöglicht 100% offline Mock- und Contract-Tests.
- **Protocol & Desktop Composition Root**:
  - `ModelProviderKindV1` in `a3-protocol` wird um die Variante `Gemini` erweitert.
  - `ModelSettingsManager` in `a3-desktop` verwendet `GeminiSettingsEndpointValidator` und instantiiert `GeminiModelProvider` für Gemini-Endpunkte.
- **Frontend**:
  - `SettingsPanel.svelte` ermöglicht die Auswahl von Google Gemini in der Provider-Verwaltung mit Standard-Endpoint `https://generativelanguage.googleapis.com` und Information zur Authentifizierung über Umgebungsvariablen.

## Konsequenzen

### Positiv

- Nutzer können Google Gemini nahtlos für Coding-, Deep-Map- und Embedding-Rollen auswählen und live verifizieren.
- Volle Einhaltung der deterministischen Harness-, Token- und Timeout-Schranken.
- Vollständige Geheimnis-Isolation: Weder die Datenbank noch die WebView berühren den API-Key.
- Offline-Testbarkeit über deterministische lokale Mock-Server ohne Internetzugriff in der CI.

### Negativ

- Für Remote-Gemini-Nutzung ist eine Internetverbindung und das Setzen von `GEMINI_API_KEY` oder `GOOGLE_API_KEY` erforderlich.
- Weiterhin ist jeweils genau ein Provider zur selben Zeit aktiv (Ollama oder Gemini).

## Compliance & Tests

- Vollständige Offline-Contract-Suite `crates/a3-provider/tests/gemini_contract.rs` testet Discovery, SSE-Streaming, Capability-Probe, Embeddings, Cancellation, Timeouts und Header-Validierung.
- Protocol-, Composition-Root- und UI-Tests verifizieren die End-to-End-Integration.
