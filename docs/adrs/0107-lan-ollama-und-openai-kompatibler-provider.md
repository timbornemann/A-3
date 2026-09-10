# ADR-0107: LAN-Ollama und frei konfigurierbarer OpenAI-kompatibler Provider

Status: Accepted

Datum: 2026-09-10

Entscheider: Tim Bornemann

Supersedes: die Ollama-Loopback-only-Regel aus ADR-0018, ADR-0023 und ADR-0026 sowie die exakt
drei Provider-Slots und den Ausschluss frei konfigurierbarer kompatibler Ziele aus ADR-0032 und
ADR-0066. Die historischen Entscheidungen bleiben unverändert.

## Kontext

A^3 soll auf einem Notebook laufen und einen Ollama-Server auf einem anderen Rechner im selben
WLAN über dessen private IP-Adresse verwenden können. Die bisherige Policy akzeptiert produktiv
nur Loopback; selbst eine bewusst konfigurierte LAN-Adresse bleibt deshalb vor Discovery,
Capability-Probe und Agentenlauf gesperrt.

Zusätzlich sollen Dienste mit der verbreiteten OpenAI-kompatiblen Chat-Completions-Schnittstelle,
beispielsweise OpenRouter oder Groq, über eine frei konfigurierbare Basis-URL nutzbar sein. Deren
Basis-URLs enthalten notwendige Pfadpräfixe. Der native OpenAI-Slot bleibt davon getrennt, weil er
weiterhin die Responses API und deren engeren kanonischen Vertrag verwendet.

Beide Erweiterungen vergrößern die Netzwerk- und Credential-Trust-Boundary. Eine freie URL aus der
WebView darf deshalb weiterhin niemals unmittelbar einen Request oder die Weitergabe eines Secrets
autorisieren.

## Entscheidung

- Ollama bleibt ein credential-freier Provider. HTTP ist nur für Loopback sowie literale private
  oder link-lokale IPv4-/IPv6-Adressen zulässig; öffentliche IPs und DNS-Namen benötigen HTTPS.
  Userinfo, Pfad, Query und Fragment bleiben verboten.
- Jede nicht-loopback Ollama-Adresse wird vor dem Speichern in einem nativen Dialog mit der exakt
  kanonisierten Adresse bestätigt. Settings-Reads und das Speichern selbst erzeugen keinen
  Netzwerkzugriff. Der Adapter prüft vor jedem Discovery-, Probe-, Embedding- und Modellrequest
  erneut eine exakt an die gespeicherte Adresse gebundene Policy. Redirects und Umgebungsproxys
  bleiben deaktiviert.
- Settings V2 erhält einen vierten kanonischen Slot `openaiCompatible` mit der stabilen internen
  Provider-ID `openai-compatible`. Der bestehende Slot `openai` bleibt unverändert und verwendet
  weiterhin die native OpenAI Responses API.
- Der kompatible Slot akzeptiert eine HTTPS-Basis-URL mit optionalem, segmentweise sicherem
  Pfadpräfix, aber ohne Userinfo, Query oder Fragment. Die Basis-URL wird kanonisiert und nativ
  exakt bestätigt. Produktionsrequests sind ausschließlich gegen diese gespeicherte Basis-URL
  erlaubt; ihre festen relativen Ziele lauten `models`, `chat/completions` und `embeddings`.
- Der kompatible Provider benötigt genau einen API-Key. Er nutzt den bestehenden one-way
  Credential-Command, den nativen OS-Keyring und die Bindung an den Fingerprint der vollständigen
  Basis-URL. Es gibt keine Environment-, Datei- oder Datenbankkopie und keine frei konfigurierbaren
  Header oder alternativen Authentifizierungsarten.
- Textgenerierung verwendet den OpenAI-kompatiblen Chat-Completions-Vertrag mit Bearer-Auth,
  chronologisch geordneten Textnachrichten, fest deaktivierten Provider-Tools, begrenztem Streaming
  und optionalem strikt übersetztem `json_schema`-Response-Format. Nur eine erfolgreiche reale
  Probe des ausgewählten Modells aktiviert Coding oder Mapping.
- Modellkatalog und Embeddings verwenden die kompatiblen `models`- und `embeddings`-Formen. Alle
  syntaktisch gültigen Modell-IDs sind Kandidaten; Namen beweisen keine Fähigkeit. Embeddings
  werden wie bei den nativen Providern erst nach einer endlichen, nicht leeren und
  dimensionsgebundenen Probe aktiviert.
- Ein Schema-V9-Katalogmigration erweitert die append-only Providerzeilen ohne bestehende
  Settingsrevisionen, Rollenprofile oder Credentialgenerationen zu verlieren. Alte V8-Snapshots
  erhalten einen initialen deaktivierten vierten Slot.

## Folgen

### Positiv

- Ein portables A^3 kann Ollama auf einem ausdrücklich gewählten Rechner im lokalen Netz nutzen.
- OpenRouter, Groq und andere hinreichend kompatible HTTPS-Dienste können ohne eigenen
  providerspezifischen Adapter konfiguriert und durch dieselben Capability-Gates geprüft werden.
- Der native OpenAI-Vertrag, die providerneutrale Application-Grenze und die one-way
  Credential-Speicherung bleiben erhalten.

### Negativ

- LAN-Ollama über unverschlüsseltes HTTP schützt Inhalte nicht gegen andere Teilnehmer im lokalen
  Netz; der native Bestätigungsdialog und die UI müssen diese Grenze deutlich machen.
- „OpenAI-kompatibel“ garantiert keine vollständige Featuregleichheit. Modelle ohne unterstütztes
  Strict JSON Schema oder Embeddings bleiben für die jeweilige Rolle nicht ausführbar.
- Benutzerdefinierte Provider können Kosten verursachen und Repositoryinhalt an das bestätigte
  Ziel übertragen.

## Verworfene Alternativen

- Alle HTTP-Hosts für Ollama zulassen — würde unverschlüsselte Übertragung an öffentliche oder per
  DNS umleitbare Ziele erlauben.
- Den bestehenden OpenAI-Slot auf beliebige URLs umstellen — vermischt Responses API und
  Chat-Completions-Verträge und könnte vorhandene verifizierte Profile still anders interpretieren.
- API-Key, URL oder Header pro Modellrequest aus der WebView übernehmen — erzeugt eine generische
  Netzwerk- und Secret-Exfiltrationsfähigkeit.
- Structured Output aus einem Modellnamen ableiten oder bei Fehlern auf Prompt-only JSON
  hochstufen — liefert keine belastbare Ausführungsevidence.

## Compliance

- Endpointtests prüfen private und link-lokale HTTP-Adressen, öffentliche HTTP-Ablehnung,
  HTTPS-Ziele, Pfad-/Credential-Ablehnung und exakte Policybindung vor Netzwerkzugriff.
- Offline-Providerverträge prüfen Models, Chat-Completions-Streaming, Strict-JSON-Probe,
  Embeddings, Auth-Header, Modellbindung, Größenlimits, Timeout, Cancellation, Redirect- und
  Fehlerklassifikation ohne echte Providerverbindung.
- Application-, Storage-, IPC- und TypeScript-Contracts prüfen vier kanonische Slots,
  V8→V9-Erhalt, Credentialgeneration, Rollenbindung, unbekannte Providerarten und geschlossene
  Requests.
- Component-Tests zeigen LAN-/Cloud-Grenzen, den vierten Provider, editierbare Basis-URL,
  one-way Key-Cleanup und explizite Discovery. Mount, Read und Save starten keinen Request.

## Referenzen

- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0018](0018-model-provider-port-ownership.md)
- [ADR-0023](0023-local-settings-and-model-activation.md)
- [ADR-0026](0026-explicit-local-provider-model-discovery.md)
- [ADR-0032](0032-openai-model-provider.md)
- [ADR-0066](0066-mehrere-provider-und-gemeinsame-modellauswahl.md)
- [Architekturregeln](../ARCHITECTURE_RULES.md)
- [Qualitätsgates](../QUALITY_GATES.md)

