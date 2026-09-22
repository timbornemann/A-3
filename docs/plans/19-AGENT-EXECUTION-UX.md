# Plan 19: Ruhige Agentenausführung und verständliche Entscheidungen

Status: Abgeschlossen einschließlich Arbeitsplan-Nachprüfung · 2026-09-22

## Ziel und Rahmen

Die Unterhaltung zeigt während der Ausführung zuerst den aktuellen Arbeitsschritt. Der
vollständige vom Core gelieferte Arbeitsplan, der begrenzte Aktivitätsverlauf und die
Prüfergebnisse bleiben auf Wunsch erreichbar. Erforderliche Freigaben stehen direkt im Chat.
Hintergrundabfragen dürfen Inhalte, Scrollposition und bekannten Laufstatus nicht zurücksetzen.

Die bestehenden Sicherheits- und Architekturgrenzen bleiben erhalten: keine automatische
Freigabe oder Fortsetzung, keine zusätzliche fachliche Wahrheit und keine Anzeige interner
Modellgedanken. Andere Workspaces und Providerkonfigurationen gehören nicht zu diesem Auftrag.

## Arbeitsschritte und Akzeptanz

- [x] Projektionen nach Task und Lebenszyklus binden; bekannte Inhalte bei Hintergrundabfragen
      erhalten; veraltete Antworten verwerfen. Regressionen für verzögerte und fehlgeschlagene Reads.
- [x] Aktuellen Schritt hervorheben, Verlauf und Prüfungen progressiv öffnen, Freigaben davor
      sichtbar und bedienbar halten. Tastatur, Theme und kleine Fenster praktisch prüfen.
- [x] Ursachen der Approval-/Inspection-Ladefehler untersuchen und nachgewiesene Fehler mit
      Regressionen beheben; konkurrierende Änderungen weiterhin geschlossen behandeln.
- [x] Scroll-Folgen und manuelles Lesen bei realen Layoutänderungen im Browser prüfen; Status
      und stabile DOM-Identitäten mit Regressionen absichern.
- [x] Relevante Tests, vollständiges Frontendgate, gegebenenfalls Rustgate, Dokumentation und
      finales Diff-Audit durchführen. Erst dann den Gesamtauftrag abschließen.

## Verifikation

Gezielte Komponenten- und Decoderregressionen prüfen Zustandswechsel und verspätete Antworten.
Ein reproduzierbares Browserfixture prüft laufende Arbeit, Freigaben, manuelles Scrollen und
unveränderte Geometrie bei Hintergrundabfragen. Die Belege und Einschränkungen stehen unten.

## Nachgewiesene Fehlerursachen

- Die Ausführungskarte verwendete den Run-Zeitstempel als Svelte-Key der Inspektion. Jede neue
  Aktivität entfernte dadurch den geladenen Diff-/Prüfbereich und baute ihn neu auf.
- Statusabfragen ersetzten den bekannten Lauf durch `loading`; bei temporären Fehlern wurde der
  Lauf verworfen. Die neue Projektion erhält denselben Task-Stand und kennzeichnet fehlende
  Aktualisierung, ohne ihn als neue Bestätigung auszugeben.
- Mehrere Rust-Enums benannten nur die Varianten, nicht deren Felder in `camelCase` um.
  Betroffen waren Diffzeilen, Evidence-Details, Stale-Ursachen, Prozess-Planbindung,
  Netzwerk-Scope sowie Ledgerereignisse und Goal-Revisionsabweichungen. Die strikten
  Frontenddecoder bleiben unverändert; die Rust-Serialisierung entspricht jetzt dem
  bestehenden öffentlichen V1-Vertrag. Es gibt keine Lockerung von Freigabe oder Validierung.

## Reproduzierbare Prüfungen

`apps/desktop/performance/u19-agent-execution.html` zeigt 40 Arbeitsschritte und verzögerte
Reads. Die Messaktion nimmt 20 Geometrieproben auf; Freigaben, Prozessfreigaben, Lesekonflikte,
Wiederherstellung und Themes sind über die Fixture-Steuerung auswählbar. Die tatsächlichen
Approval-/Inspection-Daten stammen aus dem isolierten Rust-Ausführungsfixture.

Die alte Workspace-Komponente aus HEAD wurde vorübergehend mit demselben Ablauf verglichen:
6 Reads, 6 Statusresets und rund 1.254 px Schwankung der Inhaltshöhe. Die neue kompakte
Ansicht zeigte bei 6 Reads keine Statusresets und 0 px Schwankung von Scrollposition,
Inhalts- und Kartenhöhe. Die Vergleichskopien wurden danach entfernt.

Die JSON-Projektionen werden mit dem bestehenden Offline-Greenfield-Executor erzeugt. Er
verwendet ein vorgegebenes Testmodell, echte libSQL-Persistenz, echte Dateiänderungen,
Command Discovery und echte Python-Unittests. Weder Cloudzugriff noch Benutzerdaten werden
benötigt. Ein neuer Export ist ausdrücklich opt-in:

```powershell
$env:A3_AGENT_UI_CONTRACT_OUTPUT = Join-Path (Get-Location) 'apps/desktop/src/lib/agent-execution-contract.fixture.json'
cargo test -p a3-desktop --offline --locked empty_project_creates_files_binds_tests_and_reaches_done_with_real_unittest -- --nocapture
Remove-Item Env:A3_AGENT_UI_CONTRACT_OUTPUT
pnpm --filter @a3/desktop exec prettier --write src/lib/agent-execution-contract.fixture.json
pnpm --filter @a3/desktop exec vitest run src/lib/agent-execution-contract.test.ts
```

Vor der Protokollkorrektur scheiterten alle drei echten Inspection-Antworten und die
Prozessfreigabe am Frontenddecoder. Die gespeicherten Fixture-Antworten enthalten ausschließlich
den festen Testcode und dessen öffentliche, begrenzte IPC-Projektionen.

## Abschlussbelege der ersten Abnahme und Grenzen

- Die acht aus dem echten Offline-Ausführungsweg erzeugten Antworten (drei Aktivitäten, drei
  Inspektionen, zwei Freigaben) bestehen die unveränderten strikten Frontenddecoder. Zusätzliche
  Rust-Serialisierungstests prüfen die betroffenen Enumfelder einschließlich Stale-Ursachen,
  Ledgerrevisionen und Netzwerk-Scope.
- Regressionen sichern erhaltene Inspektions-DOM-Knoten, manuelle Diffdarstellung, verworfene
  Antworten früherer Tasks, gesperrte veraltete Freigaben, aktuellen Sidebarstatus und
  unveränderten Laufstatus während verzögerter/fehlgeschlagener Reads.
- Browserprüfung bei 1280 × 720 und 720 × 520, in Hell und Dunkel: keine horizontale Überbreite;
  Freigabeauswahl und Bestätigung erreichbar; separate Fortsetzung bleibt erforderlich.
  Enter öffnet Details und beendet das automatische Scroll-Folgen. Die Liste mit 40 Schritten
  besitzt 384 px sichtbare Höhe bei 2.562 px Inhaltshöhe und scrollt separat.
- Auch bei geöffneten Prüfungen und einer Prozessfreigabe: 20 Messproben, 6 Reads, 0 px
  Schwankung von Scrollposition/Inhalts-/Kartenhöhe, identische Karte und 0 Statusresets.
- `pnpm ci:frontend`: Formatierung, ESLint ohne Warnungen, Typecheck, **441 bestandene
  Frontendtests**, 14 bestehende opt-in Skips, 5 Toolingtests und Produktionsbuild erfolgreich.
- `cargo fmt --all --check`, gezielte Protokoll-/Greenfieldtests,
  `cargo test --workspace --all-features --offline --locked`,
  `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
  und `cargo build -p a3-desktop --offline --locked` erfolgreich. Die Rust-Suites melden
  insgesamt 1.409 bestandene Tests; 22 bestehende opt-in Tests bleiben ignoriert.
- `pnpm check:links` und `git diff --check` erfolgreich. Keine neuen Abhängigkeiten,
  Persistenzmigrationen oder erweiterten Berechtigungen.

Die Prüfung verwendete Node 24.19.0 und pnpm 11.9.0; die exakte Repository-Pinversion von Node
ist 24.14.0. Bestehende Buildhinweise zum BigInt-Ziel und zur Chunkgröße bleiben unverändert.
Die Browserabnahme ersetzt keinen plattformübergreifenden nativen WebView-Test. Der reale
Executor wurde unter Windows mit einem festen Offline-Testmodell geprüft; kein zusätzlicher
kostenpflichtiger Cloud-Modelllauf wurde gestartet.

## Nachprüfung: Flackernder Aktivitätsverlauf bei fehlendem Arbeitsplan

Der Nutzer meldete im aktuellen `pnpm tauri dev` weiterhin einen regelmäßigen Wechsel zur
Meldung „Arbeitsplan wird geladen“. Die erste Abnahme prüfte verzögerte Reads nach einem
erfolgreich geladenen Plan; die tatsächliche Arbeitsplan-IPC-Antwort fehlte im Vertragsfixture.

- [x] Fehler vor der Korrektur reproduziert: Ein wiederholt abgelehnter erster Planread lässt
      `workPlan` leer. Jeder Poll setzt `workPlanLoading` erneut und fügt den Ladeabsatz oberhalb
      des Aktivitätsverlaufs ein. Beide Regressionen (Verlauf offen/geschlossen) scheiterten
      genau an diesem erneut sichtbaren Absatz.
- [x] Rust-Serialisierung korrigiert: `TaskLensTaskResultV1` gab `ledger_revision` und
      `ledger_store_version` statt `ledgerRevision` und `ledgerStoreVersion` aus. Auch der
      Goal-Revisionskonflikt erhält die bereits vereinbarten camelCase-Felder.
- [x] Zweite Ablehnungsursache korrigiert: Der Core liefert Schritte in Abhängigkeitsreihenfolge;
      der Decoder verlangte fälschlich alphabetisch sortierte opake IDs. Er erhält jetzt die
      Core-Reihenfolge und prüft weiterhin eindeutige IDs, Taskbindung, Version, Feldschema,
      Statuswerte und Größenlimits. Die Sortierung der separaten Aufgabenliste bleibt bestehen.
- [x] Nach einem gescheiterten ersten Read bleibt der ruhige Aktualisierungshinweis während
      weiterer Versuche sichtbar; der initiale Ladeabsatz wird nicht erneut eingeblendet.
      Bereits geladene Pläne und geöffnete Verlaufsdetails bleiben erhalten.
- [x] Reales Greenfield-Vertragsfixture erweitert: drei zusätzliche Arbeitsplanantworten aus
      libSQL und derselben produktiven Projektionsfunktion. Die IDs widersprechen absichtlich
      der alphabetischen Reihenfolge, während der zweite Schritt vom ersten abhängt.
      Alle elf IPC-Antworten bestehen die Frontenddecoder.
- [x] Browserfixture über den lokalen Vite-Entwicklungsserver geprüft, mit
      `?plan-read-failure`: jeweils 20 Messproben, sechs beziehungsweise sieben Hintergrundreads,
      0 erneut sichtbare Ladeabsätze und 0 px Schwankung von Verlaufposition, Scrollposition,
      Karten- und Inhaltshöhe. Derselbe Verlaufsknoten bleibt offen beziehungsweise geschlossen.
      „Verbindung wiederherstellen“ lädt anschließend den vollständigen Plan.
- [x] `pnpm ci:frontend`: Formatierung, ESLint, Typecheck, 449 bestandene Frontendtests,
      14 bestehende opt-in Skips, fünf Toolingtests und Produktionsbuild erfolgreich.
- [x] `cargo fmt --all --check`, `cargo test -p a3-protocol --offline --locked task_lens`,
      das reale Greenfield-Fixture, `cargo test --workspace --all-features --offline --locked`,
      `cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`
      und `cargo build -p a3-desktop --offline --locked` erfolgreich. Auch
      `pnpm check:links` und `git diff --check` sind grün. Die abschließende Anpassung des
      Rusttests auf propagierte Fehler statt `unwrap` wurde zusätzlich gezielt geprüft.

Die Änderung betrifft den Read-Vertrag und seine Darstellung. Sie ändert keine Freigaben,
Ausführungsregeln, persistenten Daten oder Abhängigkeiten. Der Fehlerfall ist offline prüfbar;
ein neuer Cloud-Modellauftrag ist für seine Reproduktion nicht erforderlich.
