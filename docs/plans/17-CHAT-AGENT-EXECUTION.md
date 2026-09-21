# Plan 17: Tatsächliche Agentenausführung im Chat

Status: Abgeschlossen · 2026-09-22

## Ziel

Der Agent setzt Aufträge in bestehenden und leeren Projekten tatsächlich über den sicheren
Harness um. Der gesamte nachweisbare Arbeitsstand erscheint in der zentralen Unterhaltung statt in
einer separaten rechten Inspector-Tab-Leiste.

## Akzeptanzkriterien

- [x] Der reale Greenfield-Coding-Turn mit dem konfigurierten Modell kann neue Python-Quell- und
      Testdateien anlegen, reindizieren, den nachgelagerten Check exakt freigeben und bis `Done`
      verifizieren.
- [x] Ein Fehler im Modellturn, in Ankern, Zustand, Laufzeitfähigkeit oder Fortschrittskanal wird
      geschlossen klassifiziert und als konkreter sicherer Haltegrund in der Session sichtbar.
- [x] Der Agent-Chat zeigt aktuelles Ziel, Controllerphase, geordnete Schritte, aktuelle und
      verbleibende Arbeit sowie die nachweisbare Aktivität live.
- [x] Patch-/Dateiwirkung, Prozessausführung, Verifikation, Freigabe und finaler Review erscheinen
      im Chat; die rechte Tab-Leiste `Fortschritt`, `Änderungen`, `Review` existiert nicht mehr.
- [x] Bestehende Codebasen behalten Recherche-, Evidence-, Patch-, Approval- und
      Verifikationsgrenzen; Greenfield darf keine vorhandenen Dateien oder Befehle erfinden.
- [x] Rust- und Frontend-Regressionen sowie die vorgeschriebenen Qualitätsgates sind grün; der
      exakte Hello-World-Auftrag ist praktisch verifiziert.

## Constraints und Non-Goals

- Keine offenen Shell-, Datei-, Provider- oder Datenbankfähigkeiten in der WebView.
- Keine Anzeige versteckter Chain-of-Thought oder roher Providerantworten.
- Kein autonomes Installieren, Netzwerk, Push, Merge oder Publishing.
- Keine zweite fachliche Wahrheit im Chat; Goal, Ledger, Run, Journal und Evidence bleiben
  autoritativ.
- Keine opportunistische Neugestaltung anderer Workspaces.

## Vertikale Arbeitsschritte

- [x] Reales Luna-Greenfield-Fixture ergänzen und den ersten tatsächlichen Ausführungsfehler
      reproduzieren.
- [x] Executor-Fehlerursache beheben, typisierte Haltegründe erhalten und mit Regressionen sichern.
- [x] Autoritative Run-, Ledger-, Inspection- und Approval-Projektionen in eine begrenzte
      Chat-Ausführungskarte integrieren.
- [x] Inspector-Tabs entfernen und laufende, terminale sowie responsive UI-Zustände testen.
- [x] Engste Checks, vollständige Gates, finales Diff-Audit und exakten praktischen Auftrag prüfen.

## Prüfweg

1. Gezielte Rust-Contracts für Greenfield, Fehlerprojektion und Session-Synchronisierung.
2. Opt-in Live-Fixture mit `gpt-5.6-luna`, isoliertem leerem Git-Worktree, exakten Freigaben,
   Offline-`unittest` und unabhängiger physischer Nachprüfung.
3. Gezielte Svelte-/Vitest-Tests für die Chat-Timeline und das entfernte Inspector-Layout.
4. Formatierung, Lint, Typecheck, Frontendtests/-build, Rust-Workspace-Tests und Clippy mit
   verweigerten Warnungen gemäß `docs/QUALITY_GATES.md`.

## Abschlussbelege

- Der exakte deutsche Hello-World-Auftrag durchlief mit `gpt-5.6-luna` die reale Recherche in
  einem leeren Git-Worktree ohne künstliche Quellen und ohne Fortsetzungsschleife. Der erzeugte
  Plan enthielt getrennte Änderungs- und Verifikationsschritte.
- Das opt-in Live-Coding-Fixture erzeugte anschließend `server.py` und
  `tests/test_server.py`, reindizierte den neuen Bestand, band den danach entdeckten
  `python -B -m unittest discover -s tests`-Befehl an den wartenden Schritt und endete erst nach
  strukturiert belegter Verifikation in `Done`.
- Die unabhängige, nicht in den Agentenkontext eingespeiste Nachprüfung startete den erzeugten
  Prozess und bestätigte HTTP 200 mit dem exakten Body `Hello, world!\n`, 404 für unbekannte Pfade
  sowie 405 mit `Allow: GET` für nicht unterstützte Methoden.
- Gezielte Regressionen decken konkrete, in den Chat projizierte Haltegründe und das entfernte
  Inspector-Layout ab. Die vollständigen Rust-Workspace-Tests, Clippy mit verweigerten Warnungen,
  Frontend-Formatierung, ESLint, Typecheck, 424 Frontendtests, Produktions-Build und Linkprüfung
  waren grün.
