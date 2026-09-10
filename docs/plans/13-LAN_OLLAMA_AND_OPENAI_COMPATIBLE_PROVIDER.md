# Plan 13: LAN-Ollama und OpenAI-kompatible Providerverbindung

Status: Abgeschlossen · 2026-09-10

## Auftrag und Autorität

A^3 soll von einem Windows-Notebook einen bewusst konfigurierten Ollama-Server im WLAN verwenden
und zusätzlich einen frei konfigurierbaren OpenAI-kompatiblen HTTPS-Anbieter für Coding, Mapping
und Embeddings anbinden können. Maßgeblich sind
[ADR-0107](../adrs/0107-lan-ollama-und-openai-kompatibler-provider.md),
[ADR-0012](../adrs/0012-safe-tools-and-approval-policy.md),
[ADR-0018](../adrs/0018-model-provider-port-ownership.md),
[Architekturregeln](../ARCHITECTURE_RULES.md) und [Qualitätsgates](../QUALITY_GATES.md).

## Akzeptanzkriterien

- [x] Ollama akzeptiert `http://` für Loopback und literale private/link-lokale LAN-Adressen,
      lehnt öffentliches HTTP und URL-Zusätze ab und bindet jeden Request an die exakt bestätigte
      gespeicherte Adresse.
- [x] Ein vierter kanonischer Settings-Slot `openaiCompatible` speichert eine bestätigte
      HTTPS-Basis-URL mit sicherem Pfadpräfix sowie einen daran gebundenen OS-Keyring-Schlüssel.
- [x] Der neue Adapter unterstützt explizite Modell-Discovery, echte Strict-Structured-Output-
      Probe, begrenztes Chat-Completions-Streaming und Embedding-Probe/-Ausführung über die
      bestehenden providerneutralen Ports.
- [x] Ask, Plan, Agent und Deep Map können ein verifiziertes Profil beider neuen Netzwerkpfade
      auflösen; nicht verifizierte, deaktivierte, stale oder credentiallose Konfigurationen bleiben
      geschlossen.
- [x] Schema V9 erhält vorhandene V7/V8-Settings, Provider, Rollen und Credentialgenerationen und
      ergänzt ausschließlich den initialen vierten Slot.
- [x] Die Einstellungen erklären LAN-/Cloud-Datenfluss, zeigen vier unabhängige Karten und lassen
      URL, Key, Discovery, Aktivierung und Rollenauswahl zugänglich und fehlerfest bedienen.
- [x] Offline-Regressionen, vollständige Rust-/Frontend-Gates, Linkprüfung und finaler Diffreview
      bestehen ohne echten Providerzugriff oder Secretmaterial.

## Vertikale Umsetzung

1. Endpoint- und Policygrenzen für LAN-Ollama und kompatible HTTPS-Basis-URLs implementieren und
   mit Negativtests absichern.
2. Provideradapter samt Offline-Wire-Contracts für Katalog, Probe, Streaming und Embeddings
   ergänzen.
3. Application-, Persistenz- und IPC-Verträge auf vier kanonische Slots migrieren und V8→V9
   verlustfrei testen.
4. Composition Root, Conversation/Agent/Deep Map und native Zielbestätigung vollständig anbinden.
5. TypeScript-Decoder und Settings-UI erweitern; Component- und Layoutfixtures aktualisieren.
6. Dokumentation, enge Tests, Gesamtgates und Diffreview abschließen; erst danach Checkboxes und
   Status auf abgeschlossen setzen.

## Abschlussnachweis

- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo fmt --all --check`
- `cargo test -p a3-storage-libsql --test connection_lifecycle --offline --locked -- --nocapture`
- `pnpm ci:frontend`
- `pnpm check:links`
- `git diff --check`

Alle Provider-Wiretests liefen ausschließlich gegen begrenzte Loopback-Stubs; es wurde kein echter
Provider kontaktiert und kein echter Schlüssel verwendet.

## Nicht-Ziele

Keine automatische Providererkennung, keine stillen Netzwerkrequests, keine frei wählbaren
Header oder Authverfahren, keine mehreren Instanzen derselben Providerart, kein API-Key in libSQL,
keine Aufweichung der Capability-Probe und keine echten OpenRouter-/Groq-/Ollama-Aufrufe in den
normalen Tests.
