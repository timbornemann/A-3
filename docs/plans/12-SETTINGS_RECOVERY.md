# Plan 12: Gemischte Modellrollen laden und gezielt wiederherstellen

Status: Umgesetzt und lokal verifiziert · 2026-09-10

## Auftrag und Autorität

Behebung des gemeldeten Desktop-Ausfalls nach einem Coding-Wechsel auf Ollama
bei unverändertem OpenAI-Mapping. Maßgeblich: [ADR-0066](../adrs/0066-mehrere-provider-und-gemeinsame-modellauswahl.md),
[ADR-0023](../adrs/0023-local-settings-and-model-activation.md),
[Architekturregeln](../ARCHITECTURE_RULES.md), [Qualitätsgates](../QUALITY_GATES.md)
und [Sicherheitsgrenzen](../SECURITY_AND_EXECUTION.md).
Die enge Wiederherstellung folgt [ADR-0106](../adrs/0106-gezielte-modellprofil-wiederherstellung.md).

Ein read-only Metadatencheck bestätigt Revision 100 mit Coding Ollama/gemma4:e4b
und Mapping OpenAI/gpt-5.6-luna. Der aktuelle Loader prüft beide zunächst gegen
den Legacy-Einzelprovider OpenAI. Er verwirft damit einen gültigen V8-Snapshot.
Keine Schlüssel wurden gelesen, kein Index und kein Benutzerkatalog verändert.

## Akzeptanz und Nicht-Ziele

- [x] V8-Rollen atomar gegen ihren eigenen Provider laden; V7 bleibt streng lesbar.
      Echte gemischte Coding-/Mapping-/Embedding-Roundtrips, Wechsel und Reopen.
- [x] Ungültige Rollenprofile gezielt diagnostizieren und nach expliziter Auswahl
      ohne Verlust gültiger Rollen, Provider, Credentials oder Projekte deaktivieren.
      Aktuelle Revision, atomarer Append, erhaltene Historie, kein Historienfallback.
- [x] Erreichbare UI-Recovery mit verständlichen Fehlern und klarer Datenwirkung.
- [x] Fehlerstatus darf ein Ladeproblem nicht als fehlende Modellverifikation oder
      bewiesene Suchbegrenzung ausgeben.
- [x] Rust-/Frontend-/Storage-/Boundary-Regressionsgates und read-only Nachprüfung
      des tatsächlichen Benutzer-Snapshots; kein automatischer Modellrequest.

Kein Datenbankreset, keine Schlüsselrotation, kein Löschen von Projektwissen,
kein neuer Index, keine Providerabfrage und keine Änderung von Recherchebudgets.
Physische DB-Korruption und ungültige Provider-/Credentialanker werden nicht durch
stilles Überspringen oder Rückrollen auf ältere Credentials als geheilt dargestellt.

## Umsetzung und Regressionsnachweise

- Der reale gemeinsame Settings-Storagevertrag reproduzierte den Rollenwechsel
  zunächst rot. Der Loader rekonstruiert jetzt zuerst Legacyanker und vollständige
  Providerslots, danach jede Rolle gegen ihren eigenen Slot. Keine neue Probe oder
  erfundene Health-Beobachtung beim Laden. Der Writer prüft denselben vollständigen
  Roundtrip vor dem Append. Tests wechseln Coding zwischen Gemini, OpenAI und
  Ollama bei erhaltenem OpenAI-Mapping und Ollama-Embedding, einschließlich Reopen.
- Reale SQLite-Fixtures prüfen defekte Coding-/Mapping-/Embeddingprofile,
  Revisionskonflikte, erzwungenen Rollback nach begonnenem Append, unveränderte
  gültige Profile und historische Originalzeilen. Gesunde Einstellungen erzeugen
  keinen Recovery-Write. Providerankerfehler, unvollständige Gruppen, unlesbare
  Rollentabellen, fehlende V8-Providertabellen und neuere Schemas bleiben geschlossen.
- Diagnose und Bestätigung sind getrennte versionierte Commands. Tests belegen
  die enge Requestform, native Operationssperre und null Schlüsselzugriffe.
  Die UI bleibt im Ladefehler erreichbar, verlangt zwei Klicks, erlaubt Abbrechen,
  zeigt betroffene Rollen und verlangt nach Konflikten eine neue Diagnose.
- Recherchevorbereitung erhält den typisierten Modellfehler. Terminales Scheitern
  erzeugt keine erfundene Suchbegrenzung mehr; auch historische Limited-Fehlerevents
  führen nicht mehr zur Behauptung einer tatsächlich ausgeführten begrenzten Suche.
- Der tatsächliche Katalog wurde über `SQLITE_OPEN_READ_ONLY` erfolgreich geladen:
  Revision 100, Coding und Mapping vorhanden. SHA-256 vor/nach diesem Zugriff
  unverändert (ReadWrite-Sharing, da die Desktop-App geöffnet blieb).
  Keine Modellrequests, keine Schlüsselzugriffe, kein Recovery-Write am Benutzerkatalog.

## Prüfkommandos und Laufbedingungen

Rust mit `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` und
`CARGO_PROFILE_TEST_DEBUG=0`, ausschließlich offline/locked:

```text
cargo fmt --all -- --check
cargo test -p a3-storage-libsql --test shared_contract libsql_satisfies_shared_desktop_settings_contract --offline --locked
cargo test -p a3-storage-libsql --lib settings_repository::tests --offline --locked
cargo test --workspace --all-features --offline --locked
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
cargo test -p a3-storage-libsql --test connection_lifecycle --offline --locked
cargo test -p a3-storage-libsql --lib settings_repository::tests::existing_user_catalog_loads_read_only --offline --locked -- --ignored --exact --nocapture
pnpm ci:frontend
pnpm check:links
git diff --check
```

Der explizite read-only Test erhält ausschließlich den Katalogpfad über
`A3_SETTINGS_READ_ONLY_CATALOG`; normale Gates überspringen diesen Benutzerzugriff.
Die vollständigen Workspace-Gates laufen mit
`CARGO_TARGET_DIR=target/settings-recovery-gates`, weil Windows das Binary der
geöffneten Desktop-App in `target/debug` sperrt. Die App wurde nicht geschlossen.
Der finale Gesamtlauf nach zusätzlicher Schemaabsicherung bestand mit 1.324
Rusttests, 20 explizit ignorierten Tests und keinen Fehlern (84 Suite-Ergebnisse).
`pnpm ci:frontend` bestand: 406 Frontendtests, 14 vorhandene Skips, fünf Tooltests,
Format, Lint, Typecheck und Build. Lokal liegt Node 25.6.1 statt des gepinnten
24.14.0 vor; Engine- und bestehende BigInt-Targetwarnungen werden transparent
beibehalten. Clippy mit verweigerten Warnungen und der native Lifecycle-Test
ohne Crash-Retry bestehen. Cross-Platform-/native WebView-Smokes sowie erneute
Live-LLM-Tests sind in diesem lokalen Bugfixlauf nicht nachgewiesen.

Lokale, regenerierbare Gate-Protokolle liegen unter `target/`:
`settings-recovery-final-test.log`, `settings-recovery-clippy.log` und
`settings-recovery-frontend-final.log`. Alle endeten mit Exitcode 0. Der finale
Diff enthält ausschließlich den Settings-Fix, Recovery, zugehörige Fehleranzeige,
Tests, die beiden erforderlichen generierten IPC-Permissions und Dokumentation.
Keine Fremdänderungen, Secrets, Projekt-/Indexmutationen oder zusätzlichen
Dependencies. Der laufende App-Prozess blieb erhalten; für die Nutzung der
Korrektur ist die aktuelle gebaute App-Version erforderlich.
