# Plan 20: Kompakter, frei lesbarer Agentenverlauf

Status: Abgeschlossen · 2026-09-22

## Ziel und Rahmen

Der Agent-Workspace erhält einen chronologischen, kompakten Verlauf und einen gemeinsamen unteren
Bereich für Nachrichten und erforderliche Entscheidungen. Nutzer können jederzeit bis zum Ende
scrollen oder ältere Schritte lesen. Lange Pläne und technische Details sind vollständig auf
Abruf erreichbar. Die Corporate-Design-Tokens und die vorhandenen Core-Verträge bleiben erhalten.

Die Änderung betrifft vor allem das Frontend. Neue Provider, autonome Freigaben, zusätzliche
Werkzeuge, Migrationen und Änderungen an der Agenten-Zustandsmaschine sind keine Ziele.

## Akzeptanz und Arbeitsschritte

- [x] Offizielle Referenzen mehrerer Agentenoberflächen prüfen und konkrete Ableitungen dokumentieren.
- [x] Scrollfehler vor der Korrektur reproduzieren; tatsächliches Verlaufsende und manuelles Lesen
      mit Regression und Browsergeometrie absichern.
- [x] Auftrag → Recherche → Plan → Ausführung in der richtigen Reihenfolge erhalten, ohne Poll-Remounts.
- [x] Pläne als kompakte Karten mit Öffnen-Aktion und vollständigem, zugänglichem Inhalt darstellen.
- [x] Aktuelle Arbeit und Verlauf deutlich reduzieren; vollständige Schritte, Aktivitäten und
      Prüfungen auf Abruf erreichbar halten.
- [x] Nach Entscheidung zu [ADR-0116](../adrs/0116-kompakter-agentenverlauf-und-entscheidungsdock.md)
      die kompakte Freigabe außerhalb des Verlaufs am Eingabebereich umsetzen; Entwürfe erhalten.
- [x] Neutraler Freigabestart, Konflikte, Speichern/Fortsetzen/Widerrufen sowie Taskwechsel prüfen.
- [x] Hell/Dunkel, kleine Fenster, Tastatur und Fokus praktisch prüfen; Reduced Motion durch
      Komponentenregressionen und die begrenzten CSS-Media-Regeln absichern.
- [x] Frontendgate, gegebenenfalls Rustgate, Links und Diff-Audit abschließen.

## Ausgangsbefund

`scrollConversationToEnd` richtet den Viewport auf `.execution-focus` oberhalb des tatsächlichen
Endes aus. Manuelles Abwärtsscrollen aktiviert am Ende wieder das Folgen und springt damit zum
älteren Anker zurück. Recherche wird außerdem nach sämtlichen Einträgen eines Turns gerendert;
deshalb erscheint die fertige Planantwort vor ihrer Recherche. Plantexte sind vollständig inline.

## Referenzen und Ableitungen

Geprüft am 22.09.2026, ausschließlich offizielle Produktdokumentation:

| Referenz | Beobachtung | Umsetzung in A^3 |
| --- | --- | --- |
| [Codex-App](https://openai.com/index/introducing-the-codex-app/) | Unterhaltungen bilden den Arbeitskontext; Änderungen lassen sich separat prüfen. | Eine zusammenhängende Unterhaltung mit abrufbaren Prüfbelegen statt dauerhaft offener Unterbereiche. |
| [Claude Code Desktop](https://code.claude.com/docs/en/desktop#switch-view-modes) | Die normale Ansicht fasst Werkzeugaufrufe zusammen; ausführliche Ansichten dienen der gezielten Untersuchung. | Kompakte Recherche und eine aktuelle Arbeitskarte; vollständige Aktivitäten im eigenen Lesebereich. |
| [Cursor Plan Mode](https://cursor.com/docs/agent/plan-mode) | Recherche geht dem überprüfbaren Plan voraus; Umsetzung folgt bewusst auf die Prüfung. | Chronologie Auftrag → Recherche → Plan, Plan öffnen und exakte bestehende Umsetzungsaktion. |
| [Cursor Agents Window](https://cursor.com/docs/agent/agents-window) | Gemeinsame Agentenarbeitsfläche mit getrennten Datei- und Reviewansichten. | Durchgängige Oberfläche, zurückhaltende Bedienelemente und Details auf Abruf. |

Die Ableitungen sind eigene Layoutentscheidungen. A^3 übernimmt keine zusätzlichen autonomen
Freigaben oder Berechtigungsmodi. Der Nutzer hat ADR-0116 ausdrücklich angenommen.

## Implementierung und Nachweise

- `scrollConversationToEnd` folgt der tatsächlichen Inhaltsgrenze. Der neue Ausführungsregressionstest
  scheiterte vorher reproduzierbar: `scrollTop=612` statt `1100`. Nach der Korrektur bleiben `1100`
  und nach Wachstum `1200` erreichbar; manuelles Lesen bei `800` bleibt unverändert.
- Recherche bleibt an ihrem Nutzereintrag montiert. Das Eintreffen der Planantwort versetzt sie
  nicht. Der Regressionstest prüft Reihenfolge, DOM-Identität und vollständigen Planinhalt im Dialog.
- Native Detaildialoge besitzen einen eigenen Scrollbereich, einen erreichbaren Schließen-Button,
  Escape und Fokus-Rückgabe ohne Scrollsprung. Polls ersetzen weder Dialog noch Inspector.
- Das Freigabedock wechselt mit der Eingabe. Regressionen erhalten Entwurf und aktuelle Auswahl
  über mehrere Polls; ein anderer Task beginnt ohne Auswahl. Exakte Aktion und Sicherheitsdaten
  bleiben vor den Controls, seltene Metadaten in einer gemeinsamen Detailanzeige.
- Die vorhandenen Freigabe-Regressionen prüfen weiterhin neutralen Start, veraltete Reads,
  Konflikte, Taskwechsel, Speichern ohne Start, Fortsetzen und Widerrufen.
- Die Offline-Browserfixture verwendet die echten gespeicherten Rust-IPC-Vertragsprojektionen
  für Patch, Prozess und Inspection sowie einen 40-teiligen Plan und 40 Arbeitsschritte. Sie ist
  kein echter Modelllauf und führt keine Prozesse oder Dateimutationen aus.

### Abschlussprüfung

- Browser: 1280 × 720, 720 × 620 und 680 × 520; Dark/Light, keine horizontale Überschreitung.
  Bei 1280 × 720 belegt die geschlossene Plankarte 104 Pixel, auch mit 40 Planabschnitten;
  die Ausführungskarte mit 40 Schritten etwa 201 Pixel. Das sind Layoutmessungen, keine
  Laufzeit- oder RAM-Performanceclaims.
- Je 20 Stichproben mit sechs Activity-Reads: am Ende Abstand **0 px**, beim bewussten Lesen
  Abstand **60 px**; Scroll-, Inhalts-, Kartenhöhen- und Aktivitätsankerdrift jeweils **0 px**.
  Dieselben Karten/Controls, kein neu eingesetzter Planloader und keine Statusrücksetzung.
- Ein zusätzlicher Randfall ist abgedeckt: Abwärtsrollen am bereits erreichten Ende erzeugt
  keinen nativen Scroll-Event. Es erhält die Folgebindung trotzdem; ein verschachtelter
  Detailbereich bleibt beim Lesen unabhängig.
- Browserbedienung: vollständiger Plan bis zum Ende per Ctrl+End (4863/4863 px), Escape und
  Fokus auf „Plan öffnen“, Aktivitäten und reale Patch-/Prüfprojektionen im Detaildialog.
  Der Eingabeentwurf bleibt über Freigabewechsel erhalten. Bei Readkonflikt bleibt Bestätigen
  gesperrt. Einmalfreigabe speichern und widerrufen wurde ohne Start geprüft; Fortsetzen bleibt
  als separate Aktion sichtbar. Die zugrunde liegenden Core-Contracts wurden nicht geändert.
- Reduced Motion: bestehende Komponentenregressionen laufen mit; neue Dialog-/Dockanimationen
  und der Disclosure-Pfeil besitzen explizite `prefers-reduced-motion: reduce`-Regeln.
  Eine systemweite Reduced-Motion-Einstellung wurde nicht verändert.
- `pnpm ci:frontend`: Formatierung, ESLint, Svelte/TypeScript (0 Fehler, 0 Warnungen),
  **453 bestandene Tests in 73 Dateien**, 14 bestehende opt-in Tests übersprungen,
  5 Tooltests bestanden und Produktionsbuild erfolgreich.
- `pnpm check:links`: 176 Markdown-Dateien, 798 lokale Links geprüft. `git diff --check` sauber.
  Der Diff verändert weder IPC-/Storage-Schemata noch Permissions, Dependencies oder Rust-Code.
- Umgebungsgrenzen: Node 24.19.0 statt gepinntem 24.14.0 meldet die bekannte Engine-Warnung.
  Die vorhandenen Buildhinweise zu Chunkgrößen und BigInt-Zielumgebungen bleiben bestehen.
  Die Prüfung verwendet Browser und echte serialisierte Core-Verträge; kein neuer echter
  Modelllauf und kein neu gestarteter nativer Tauri-Prozess waren dafür erforderlich.

## Nachbesserung: fehlende Freigabe bei abweichendem Unterhaltungsstatus

Der Nutzer meldet einen wartenden Agentenlauf ohne Freigabekarte. Die Dockbedingung verwendete
ausschließlich `session.summary.state`; Activity und Fußleiste zeigten bereits `awaitApproval`,
während die Unterhaltung noch `running` lieferte. Unterhaltungs-/Workerstatus und dauerhafter
Controllerstatus werden unabhängig gelesen. Der neue Regressionstest reproduzierte genau diesen
Fall vor der Korrektur: Die erwartete Überschrift „Aktion freigeben“ fehlte.

Ziel ist die zuverlässige Anzeige dieser vorhandenen Entscheidung einschließlich konsistenter
Statusbeschriftung. Neue Freigaberegeln, Änderungen am Core-Automaten und weitere Layoutumbauten
sind nicht Teil dieser Nachbesserung.

- [x] Fehler mit laufender Unterhaltung und wartendem Controller reproduzieren und korrigieren.
- [x] Nur die Activity des ausgewählten Tasks für den ergänzenden Anzeigehinweis verwenden.
      Beendete und pausierte Unterhaltungen durch alte Runs nicht wieder öffnen.
- [x] Statuswechsel über mehrere Polls, erhaltene Eingabe/Auswahl, verspätete Reads nach
      Taskwechsel sowie Rückkehr zur Eingabe bei fortgesetztem Lauf prüfen.
- [x] Entscheidbarkeit weiterhin allein aus der exakten Approval-Projektion ableiten;
      fehlende Darstellung bietet keine Freigabeaktion. Mount/Polling starten keine Ausführung.
- [x] Sichtbarkeit und Bedienung mit der Offline-Browserfixture prüfen.
- [x] Vollständiges Frontendgate, Links und finalen Diff prüfen.

Die Korrektur ergänzt ausschließlich den Präsentationszustand für eine laufende Unterhaltung,
deren aktueller Task nachweislich auf Freigabe wartet. Kopfzeile, Verlaufsliste und Statusmarke
zeigen dann ebenfalls „Freigabe nötig“. Die persistierte Session, Runtime-Controls und alle
Core-Validierungen bleiben unverändert. Der taskgebundene Approval-Read entscheidet weiterhin,
welche exakte Aktion angezeigt und bestätigt werden darf (ADR-0022/ADR-0116).

Nachweise: Die gezielte Ausführung von `AgentWorkspace.test.ts` und
`AgentApprovalCenter.test.ts` besteht mit 56 Tests. `pnpm ci:frontend` besteht einschließlich
Formatierung, Lint, Typecheck (0 Fehler/0 Warnungen), 461 Tests in 73 Dateien, 5 Tooltests und
Produktionsbuild; 14 bestehende opt-in Tests bleiben übersprungen. `pnpm check:links` prüft
176 Markdown-Dateien und 798 lokale Links. `git diff --check` ist sauber.

Die Offline-Browserfixture reproduziert den abweichenden Status mit den vorhandenen echten
serialisierten Approval-Verträgen. Bei 1280 × 720 sowie auf einer 720 × 520 begrenzten Appfläche
bleibt die Karte sichtbar und bedienbar, ohne horizontalen Überlauf. Einmalfreigabe speichern
zeigt getrennt „Agent fortsetzen“ und „Freigabe widerrufen“; erst die simulierte Fortsetzung
blendet wieder den erhaltenen Eingabeentwurf ein. Hell/Dunkel und der manuelle Wechsel zwischen
Nachricht und Freigabe wurden geprüft. Dies ist ein Browsernachweis, kein neuer nativer Modelllauf.
Die oben dokumentierten bestehenden Engine-/Buildwarnungen bleiben unverändert.
