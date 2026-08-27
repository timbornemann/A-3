# ADR-0032: OpenAI Model-Provider und expliziter Remotezugriff

Status: Accepted

Datum: 2026-08-27

Entscheider: Tim Bornemann

Supersedes: die geschlossene Providerauswahl aus ADR-0026 und die auf Gemini begrenzte
Produktions-Remoteallowlist aus ADR-0028; ergänzt ADR-0012, ADR-0018 und ADR-0023

## Kontext

A^3 unterstützt lokale Ollama-Modelle und Google Gemini über providerneutrale Application-Ports.
OpenAI-GPT-Modelle sollen als dritter nativer Provider für Coding, Mapping und strukturierte
Ausgaben nutzbar werden. OpenAI-Embeddings sollen denselben bereits bestehenden Embedding-Port
bedienen, damit eine OpenAI-Verbindung alle drei Modellrollen abdecken kann.

Die bestehende Sicherheitsentscheidung erlaubt produktiven Remotezugriff und im OS-Keyring
gespeicherte Credentials ausschließlich für den kanonischen Gemini-Origin. Eine OpenAI-Anbindung
darf diese Grenze nicht durch einen frei wählbaren kompatiblen Endpoint, einen WebView-seitigen
HTTP-Client oder eine zweite Credential-Autorität umgehen.

OpenAI empfiehlt die Responses API für neue Textgenerierungsintegrationen. API-Keys werden als
Bearer-Credentials übertragen und dürfen nicht in clientseitigem Code offengelegt werden. Die
Responses API kann Streaming und strikte JSON-Schema-Ausgaben kombinieren; mit `store: false`
wird die spätere Speicherung des Response-Objekts über die API deaktiviert.

## Entscheidung

- `a3-provider` erhält einen nativen `OpenAiModelProvider`. Er implementiert unverändert die
  Application-Ports `ModelProvider`, `ModelCatalogProvider`, `ModelCapabilityProbe`,
  `EmbeddingCapabilityProbe` und `EmbeddingProvider`. Es entsteht kein neuer Domain- oder
  providerabhängiger Application-Port.
- Die stabile Provider-ID und das Keyring-Konto lauten `openai`. Der geschlossene
  `ModelProviderKindV1` erhält den Wire-Wert `openai`.
- Produktion erlaubt ausschließlich den credential-freien Origin `https://api.openai.com`.
  Redirects und Umgebungsproxies bleiben deaktiviert. Der Adapter fügt
  `Authorization: Bearer ...` erst nach der dynamischen Prüfung dieses exakten Origins hinzu.
  Loopback ist ausschließlich über eine injizierte Testpolicy für Offline-Contracts zulässig;
  benutzerdefinierte OpenAI-kompatible Gateways sind nicht Teil dieser Entscheidung.
- Der API-Key verwendet den bestehenden one-way Settings-Command, die bestehende Begrenzung auf
  1 bis 4.096 sichere ASCII-Bytes, CAS, Credential-Generation und den nativen OS-Keyring. Settings,
  IPC-Responses, Logs und Debugausgaben enthalten nur Credential-Anforderung und Lifecyclestatus.
  Providerwechsel oder Entfernung löschen den aktiven Remote-Key vor der neuen Konfiguration.
- Modellkataloge werden ausschließlich nach einem ausdrücklichen Nutzerklick über
  `GET /v1/models` geladen. Der Adapter übernimmt aus der begrenzten Antwort nur syntaktisch
  gültige IDs mit den Rollenpräfixen `gpt-` oder `text-embedding-`, sortiert und dedupliziert sie
  und projiziert höchstens 256 Werte. Der Name beweist keine Capability.
- Textgenerierung verwendet `POST /v1/responses` mit `stream: true`, `store: false`, einem
  expliziten leeren Toolset und dem im `ModelProfile` gebundenen Outputlimit sowie Samplingprofil.
  Providerneutrale System-, User- und Assistant-Nachrichten werden in Reihenfolge als
  Responses-Input übertragen.
- Strukturierte Ausgabe verwendet `text.format.type: "json_schema"`, einen festen sicheren
  Schemanamen und `strict: true`. Nur eine echte kleine Probe, deren einziges Assistant-
  `output_text` exakt gegen das erwartete Probeobjekt validiert wird, setzt
  `ModelStructuredOutputCapability::Verified`. OpenAI liefert in der Modellliste kein für diese
  Grenze ausreichend belastbares Kontextlimit; die Observation meldet deshalb kein
  providerseitiges Kontextmaximum. Native Tool-Calls bleiben deaktiviert.
- Der Streamingparser akzeptiert nur begrenzte SSE-Zeilen und Responses-Text-/Lifecycle-Events.
  Er gibt ausschließlich `response.output_text.delta` als Text aus und erzeugt genau einen
  neutralen Abschluss erst nach einem sauberen Body-Ende. `completed` wird `Stop`, ein
  `incomplete` ausschließlich mit Grund `max_output_tokens` wird `OutputLimit`. Refusal-, Tool-,
  Audio-, Bild-, unbekannte Inhalts- und widersprüchliche Terminalereignisse werden fail-closed
  abgelehnt. Prompt- und Outputtoken stammen nur aus dem terminalen Usage-Objekt.
- Embeddings verwenden `POST /v1/embeddings` mit `encoding_format: "float"`. Probe und
  Produktion prüfen Objektart, Modellbindung, vollständige eindeutige Indexfolge, Kardinalität,
  Dimension sowie endliche und nicht ausschließlich nullwertige Vektoren, bevor Daten die
  Adaptergrenze verlassen.
- HTTP-Status 408, 429 und retry-fähige 5xx werden wie bei den vorhandenen Adaptern als transient
  normalisiert. Fehlerbodies, OpenAI-Request-IDs und Providerfehlermeldungen werden weder gelesen
  noch weitergegeben. Ein Gesamttimeout und wakebare Cancellation umfassen Connect und das
  vollständige Streaming.
- Settings-Reads, Appstart und Credential-Schreiben erzeugen keinen OpenAI-Zugriff. Discovery und
  Capability-Probe bleiben explizite Nutzeraktionen. Ein späterer Agentenlauf benötigt unabhängig
  davon seine eigene laufgebundene Netzwerkfreigabe. Cloud, Telemetrie und Synchronisierung
  bleiben standardmäßig aus.
- Die Implementierung verwendet den bestehenden `reqwest`-Stack; ein OpenAI-SDK oder eine neue
  Abhängigkeit wird nicht aufgenommen.

## Konsequenzen

### Positiv

- Nutzer können verfügbare GPT- und OpenAI-Embeddingmodelle über dieselben evidenzgebundenen
  Rollenprofile wie Ollama und Gemini aktivieren.
- API-Key, Zielorigin, Netzwerkzeitpunkt und Response-Speicherung bleiben explizit und
  fail-closed kontrolliert.
- Offline-HTTP-Contracts können den vollständigen Wire-Vertrag ohne echtes OpenAI-Konto prüfen.
- Die Domain und die Application-Orchestrierung bleiben providerneutral.

### Negativ

- OpenAI-Nutzung überträgt ausgewählte Prompts und gegebenenfalls Quelltext an einen externen
  Cloudanbieter und kann nutzungsabhängige Kosten verursachen.
- Frei gehostete OpenAI-kompatible Endpoints werden bewusst nicht unterstützt.
- Ohne separat gepflegte vertrauenswürdige Metadaten kann A^3 kein providerseitiges Kontextmaximum
  anzeigen; das vom Nutzer gesetzte A^3-Limit bleibt konservative Laufgrenze.
- Neue oder geänderte Responses-Streamingereignisse können bis zur Adapteranpassung fail-closed
  als ungültige Antwort enden.

## Verworfene Alternativen

- Chat Completions statt Responses API verwenden — wäre für eine neue Integration nicht der von
  OpenAI empfohlene Primärpfad und würde die strukturierte Ausgabe doppelt abbilden.
- Das offizielle SDK hinzufügen — bringt für die fünf engen vorhandenen Ports zusätzliche
  Abhängigkeit und verbirgt Teile der für A^3 notwendigen Body- und Ereignisgrenzen.
- `OPENAI_API_KEY` als Environment-Fallback lesen — schafft neben Settingsrevision und Keyring
  eine zweite, unsichtbare Credential-Autorität.
- Beliebige kompatible HTTPS-Origins erlauben — könnte das Secret an einen WebView-beeinflussten
  Host senden und erweitert die Trust Boundary ohne eigenen Use Case.
- Responses standardmäßig speichern oder über `previous_response_id` verketten — verlagert
  Gesprächszustand aus dem deterministischen A^3-Harness zum Provider.

## Compliance

- Provider-Unit- und Offline-Contracttests decken Endpointnormalisierung, Originpolicy,
  Auth-Header, Modellfilter, Requests, strukturierte Probe, SSE-Fragmentierung, Terminalzustände,
  Usage, Embeddings, Größenlimits, Statusklassifikation, Timeout und Cancellation ab.
- Der gemeinsame ModelProvider-Contract beweist Reihenfolge und genau einen Abschluss.
- Protocol-, Desktop-Manager- und Component-Tests decken den geschlossenen `openai`-Wire-Wert,
  one-way Credentialfluss, Providerwechsel, Recoverystatus, explizite Discovery und sichtbare
  Remote-/Kostenwarnung ab.
- Ein echter OpenAI-Smoke bleibt ignoriert und opt-in. Er darf weder Key noch Prompt oder rohe
  Providerantwort in Ausgabe oder Artefakte schreiben.

## Referenzen

- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0018](0018-model-provider-port-ownership.md)
- [ADR-0023](0023-local-settings-and-model-activation.md)
- [ADR-0026](0026-explicit-local-provider-model-discovery.md)
- [ADR-0028](0028-provider-credentials-and-explicit-gemini-remote-access.md)
- [OpenAI API Authentication](https://developers.openai.com/api/reference/overview)
- [OpenAI Responses API](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [OpenAI Responses migration guide](https://developers.openai.com/api/docs/guides/migrate-to-responses)
- [OpenAI Models API](https://developers.openai.com/api/reference/resources/models/methods/list)
- [OpenAI Embeddings API](https://developers.openai.com/api/reference/resources/embeddings/methods/create)
