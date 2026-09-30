# Plan 21: Auftragsbezogene Umsetzung und Berechtigungsmodi

Status: Complete · 2026-09-30 · ADR-0117/0118 ausdrücklich angenommen

## Ziel und Autorität

Einfache Aufgaben werden ohne verpflichtende Testsuite umgesetzt. Vorhandene Prüfungen
bleiben erlaubt; neue Tests benötigen einen Auftrag oder ein konkret begründetes Risiko.
Ein expliziter Testverzicht hat Vorrang. Ein appweit gespeicherter Schalter am Chat bietet
Ask permissions und Full machine. Die Auswahl wirkt vor der nächsten Werkzeugaktion.

Maßgeblich sind [ADR-0117](../adrs/0117-auftragsbezogene-tests-und-verifikation.md),
[ADR-0118](../adrs/0118-appweite-agent-berechtigungsmodi.md),
[Architekturregeln](../ARCHITECTURE_RULES.md),
[Security](../SECURITY_AND_EXECUTION.md) und [Qualitätsgates](../QUALITY_GATES.md).
Den aktuellen Vertrags- und Implementierungsstand beschreibt
[Auftragsumfang und Berechtigungen](../AGENT_TASK_SCOPE_AND_PERMISSIONS.md).

## Arbeitsschritte und Akzeptanz

- [x] Auftragsbezogene Verifikationsentscheidung, optionale Testschritte und Greenfield
      ohne automatisch erzeugte Testsuite durch Recherche, Materialisierung und Replan schließen.
- [x] Typisierte revisionierte globale Berechtigungseinstellungen dauerhaft speichern;
      Neuinstallation, Vorwärtsmigration, CAS und Reopen prüfen.
- [x] Policy und echte Patch-/Prozessautorisierung an den aktuellen Modus binden;
      Destruktion, Veröffentlichung, unklare Wirkungen und strengere Workspace-Regeln erhalten.
- [x] Moduswechsel und wartende automatisch erlaubte Aktionen revalidieren und fortsetzen.
- [x] Versionierte Maschinen-, zusätzliche Prozess- und Netzwerkwerkzeuge vollständig über
      Policy, Evidence, Ressourcenlimits und Recovery anbinden.
- [x] Dauerhaft sichtbaren Schalter, Revisionen, Fehler und appweite Synchronisierung umsetzen;
      Entwurf, Fokus, Themes und kleine Fenster prüfen.
- [x] Echten kleinen Python-Server ohne Testartefakte bis zum belegten Abschluss prüfen;
      unabhängiges HTTP-Orakel und ausdrückliche/risikobegründete Tests absichern.
- [x] Dokumentation, relevante Regressionen, Rust-/Frontend-/Storagegates und Diff-Audit abschließen.

## Constraints und Nicht-Ziele

Keine generische privilegierte WebView-API, keine Shell als Standard, keine OS-
Privilegienerhöhung, keine automatische Destruktion oder Publikation. Keine Änderung
historischer ADR-Entscheidungen, abgeschlossener Schritte oder fremder Dateien.
Ask und Plan bleiben read-only; A^3-Entwicklungsregressionen bleiben erforderlich.

## Prüfweg

Gezielte Domain-, Application-, Storage-, IPC-, Adapter-, Executor- und Componenttests;
echte lokale Dateien, Prozesse und HTTP-Ergebnisse statt alleiniger Modellbewertung.
Anschließend `cargo fmt --all -- --check`, Workspace-Tests mit allen Features offline/locked,
Clippy mit `-D warnings`, `connection_lifecycle`, `pnpm ci:frontend`, `pnpm check:links`
und `git diff --check`. Plattformabhängige Änderungen benötigen Zielplattformnachweise.

## Präzisierung des Prüfauftrags

Der Nutzer hat am 2026-09-30 während der Umsetzung ausdrücklich festgelegt:
Linux und macOS müssen aktuell nicht geprüft werden; der Hauptfokus liegt auf Windows.
Windows-Nachweise sind damit die lokale Abnahmeplattform dieses Auftrags.
Linux-/macOS-Unterstützung und die bestehende CI-Matrix bleiben erhalten.

## Abhängigkeiten

Der begrenzte Netzwerkadapter verwendet die im Workspace bereits fest versionierten
reqwest-/rustls- und Tokio-Abhängigkeiten. Die Standardbibliothek bietet keinen
HTTPS-Client mit Zertifikatsprüfung; ein zweiter TLS-/HTTP-Stack wird nicht eingeführt.

## Lokale Nachweise vom 2026-09-30

| Gegenstand | Objektiver Nachweis |
| --- | --- |
| Auftragsumfang | Domain-/Recherche-/Materialisierungsverträge lassen reine Änderungspläne und leere Prüflisten zu; Testerstellung und Prüfausführung bleiben verschiedene Absichten. Der aktuelle V6-Vertrag enthält Risikobegründung, Testverzicht und die Grenze zwischen Diff und Laufzeit. |
| Kleiner Server | Native Greenfield-Fixtures erzeugen in beiden Modi echte Python-Dateien ohne Testartefakte oder Testabhängigkeiten. Ein unabhängiger HTTP-Prüfer startet den erzeugten Server und liest Hello World. Der ausdrückliche Testauftrag erzeugt eine begrenzte Suite und führt echtes unittest aus. |
| Einstellungen und Migration | Echte libSQL-Verträge prüfen Ask als Standard, Catalog V10, CAS-Konflikt, konkurrierende Schreiber, unveränderliche History und Reopen. Knowledge V41/V42 erhalten Policyrevisionen und alte Mutationszustände; ein Fehler nach dem Kopieren der V22-Zeilen rollt atomar zurück. |
| Werkzeugwirkungen | Der echte Controller führt externe Dateien, Python `--version` und lokales HTTP-GET mit exakter Policy und dauerhaftem Scope aus. Ask verhindert vor Freigabe jede Wirkung; Full nutzt echte automatische Entscheidungen. Adapterverträge prüfen Hashschutz, Abwesenheit, Limits, Secret-Schutz, Cancellation und Projekt-PATH-Schatten. |
| Wechsel und Recovery | Echte Patch-/Prüfprozess-Fixtures prüfen Full-Wakeup und Rückwechsel zu Ask. Unbestätigte Projektskripte und Destruktion/Publikation behalten die Abfrage. Native Recovery liest die genaue externe Datei in beiden Modi ohne Wiederholung des Schreibens; Neustarts an fünf dauerhaften Reconciliation-/Replan-Grenzen setzen denselben Replan fort. |
| UI und Verträge | Rust-/TypeScript-Negativverträge sichern Settings, Approval V2 und Recovery V1. Activity V1 bleibt geschlossen. Componenttests prüfen stale Revisionen, konkurrierende Antworten, Freigaben, Entwurf, Fokus und Scroll. Die echte Browserfixture prüft Tastatur, beide Themes und 360 × 640 Pixel ohne horizontale Überbreite; Controls sind mindestens 44 Pixel hoch. |
| Kontextbudget | Die reproduzierbare Repeat-Schema-Fixture sinkt von 9160 auf 8437 System-/Schemabytes und behält alle Pflichtanker. Die getrennte Originalfixture weist die verbleibende 16K-Grenze ehrlich aus; bei 32K werden aktuelle Originale geliefert. Siehe Detaildokument. |

Ausgeführte Abschlussprüfungen:

- `cargo fmt --all -- --check`: bestanden.
- `cargo test --workspace --all-features --offline --locked --no-fail-fast`: bestanden.
- `cargo test -p a3-provider --all-features --offline --locked`: bestanden.
- `cargo test -p a3-storage-libsql --all-features --offline --locked newer_schemas_and_missing_current_provider_tables_are_not_legacy_recovery`: bestanden.
- `cargo test -p a3-desktop --lib --all-features --offline --locked`: nach der letzten Promptkorrektur 315 bestanden, 8 bewusst ignoriert.
- `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`: bestanden.
- `cargo test -p a3-storage-libsql --test connection_lifecycle --offline --locked`: bestanden, ohne Crash-Retry.
- `pnpm ci:frontend`: Format, Lint, Typecheck und Build bestanden; 77 Testdateien mit 472 bestandenen und 14 bewusst übersprungenen Tests, zusätzlich 5 Tooltests bestanden.
- `pnpm check:links`: 180 Markdown-Dateien und 829 lokale Links geprüft.
- `pnpm --filter @a3/desktop tauri build --no-bundle`: aktualisiertes natives Releasebinary erfolgreich gebaut.
- `node scripts/run-desktop-ux-smoke.mjs`: Windows-WebView-Smoke bestanden; unverändertes Releasebinary mit Fenster 1296 × 839 Pixel, 34 verschiedene Stichprobenfarben und 56.935-Byte-Screenshot. Bericht und Bild liegen unter `target/platform-smoke/win32.*`. Das prüft den nativen Appstart, nicht sämtliche neuen IPC-Interaktionen per UI.
- `git diff --check`: bestanden.
- Finaler Diff-Audit: historische ADR-Entscheidungen unverändert, keine Produktions-`unwrap`/`expect`/`panic`- oder neuen Unsafe-Pfade, keine Secrets oder fremden/generierten Arbeitsdateien. Die fünf generierten Tauri-Permissionmanifeste sind Teil der schmalen neuen IPC-Commands; Cargo.lock ergänzt nur drei vorhandene Dependency-Kanten.

Die detaillierten lokalen Logs und Browserbilder liegen unter `target/plan21-*` und
gehören nicht zum versionierten Produkt. Live-Providertests wurden nicht aktiviert.

## Bewusst ausgewiesene Grenzen

Linux und macOS sind gemäß Nutzerentscheidung aktuell ungeprüft. Der lokale Frontendlauf
verwendet Node 25.6.1 statt der gepinnten 24.14.0 und pnpm 11.9.0; er besteht trotz der
Engine-Hinweise und der vorhandenen Vite-BigInt-Transformhinweise. Es wurde keine Runtime
installiert. Browser-Komponentenprüfungen ersetzen keine Interaktionsprüfung aller neuen
IPC-Wege im nativen WebView-Fenster.

Die Regel zur Testerstellung bleibt eine semantische Auftragsregel; der Core erkennt nicht
jeden denkbaren Testinhalt in einem Patch. Zusätzliche automatische Prozesse sind auf die
dokumentierten geschlossenen Rezepte beschränkt; es gibt kein automatisches Installationsrezept.
Synchrone lokale Dateisystemaufrufe besitzen kein hartes OS-I/O-Zeitlimit. Maschinenbeobachtungen
ersetzen keine Repository-Verifikation und werden nach Neustart nicht als dauerhaft bestätigte
Originalausgaben rekonstruiert. Die 16K-Repeat-Schema-Fixture kann optionale Originale weiterhin
ehrlich auslassen. Diese Grenzen sind im Detaildokument mit dem tatsächlichen Vertrag beschrieben.
