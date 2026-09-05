# U13: Langchat-Stabilität

Offline-Browserprofil der produktiven Svelte-Komponenten, ohne Provider, Tauri-Mutationen oder
Repositoryzugriff. Mermaid wird tatsächlich lokal geladen und gerendert; nur die Read-Projektionen
sind deterministische Fixtures. Die Profilseite gehört nicht zum Produktionsbundle.

## Wiederholen

1. `pnpm --filter @a3/desktop exec vite --host 127.0.0.1 --port 5175 --strictPort` starten.
2. `/performance/u13-chat-stability.html` im lokalen Browser öffnen.
3. „Live-Prüfung starten“ anklicken. Nach sechs Sekunden zeigt die Ausgabe 60 Stichproben mit
   20 historischen Turns, zwei echten Mermaid-SVGs und sechs zusätzlichen Rechercherunden.
4. Im Verlauf hochscrollen und die Prüfung erneut starten: `following: false` und
   `maxManualDrift: 0` werden erwartet. „Zum neuesten Schritt“ bindet wieder an.
5. „Abschluss“ zeigt die Antwort und klappt die Recherche einmalig nach 700 ms ein.
   „Folgefrage“ verwendet den Composerpfad und erhält den vorherigen Rechercheblock und beide SVGs.
6. Bei 720×520 und 680×760 wiederholen; Projektprofil-Server danach beenden.

## Lokale Messung vom 2026-09-05

Windows, Chromium-basierter Codex-Browser, Vite-Entwicklungsbuild, produktive Styles und Mermaid
11.12.3. Die Werte sind Fixture-Messungen, kein plattformübergreifender nativer WebView-Nachweis.

| Prüfung                                            | Historische Removals / Änderungen | Schrumpfungen | Abweichung vom Live-Ziel | DOM-Knoten |
| -------------------------------------------------- | --------------------------------- | ------------- | ------------------------ | ---------- |
| 720×520, sechs Runden                              | 0 / 0                             | 0             | 0,49 CSS-Pixel           | 1.358      |
| 680×760, sechs Runden                              | 0 / 0                             | 0             | 0,49 CSS-Pixel           | 1.358      |
| 680×760, sechs weitere Runden beim manuellen Lesen | 0 / 0                             | 0             | manuelle Drift 0 Pixel   | 1.569      |

Beide kleinen Fenster hatten keinen horizontalen Inhaltsüberlauf. Alle 22 überwachten historischen
Recherche-/SVG-Knoten blieben erhalten. Im warmen Live-Lauf wurden keine Diagrammartefakte erneut
geladen. Die P95-Verzögerung des 100-ms-Event-Loop-Timers lag bei rund 10–11 ms; das ist keine
Messung der Modellgeschwindigkeit oder vollständiger Nutzerinteraktionslatenz.

Ein isolierter Vorher-/Nachher-Vergleich während der Korrektur des Recherche-Read-Schlüssels
ergab bei identischen 70 Sessionpolls zunächst 1.470 und danach 70 Trace-Reads: historische Turns
werden nicht länger über die neue Elternobjektidentität mitgeladen. Der finale Schmalfensterlauf
zählte entsprechend 70 beziehungsweise 71 Session- und jeweils ebenso viele Trace-Reads.
Der abschließende Wiederholungslauf bei 680×760 bestätigte diese Werte ohne Browserwarnungen
oder Fehler; insbesondere treten keine Svelte-Proxy-Identitätswarnungen mehr auf.

Die Browserprüfung zeigte außerdem eine zunächst etwa 4,5 Pixel große Zielabweichung durch die
Translation der Einblendanimation. Fade-only behält die lebendige Staffelung, ohne die geometrische
Scrollzielmessung zu verfälschen. Der endgültige Rest liegt unter einem CSS-Pixel.

## Automatisierte Abnahme

- `pnpm --filter @a3/desktop exec vitest run --maxWorkers=2`: 64 Dateien, 338 bestanden,
  14 bereits bestehende übersprungene Tests.
- `pnpm test:tools`: 5 bestanden.
- `pnpm format:check`, `pnpm lint`, `pnpm typecheck`, `pnpm build`, `pnpm check:links`
  und `git diff --check`: bestanden.
- Die gezielten Component-Regressionen prüfen Instanzerhalt beim Folgeturn, genau einen
  Readbesitzer, keine historischen Folge-Reads, Staffelung trotz schneller identischer Polls,
  späte Reads nach Unmount, S-Quellen-Fokus bei Referenzrotation, Markdown-Caching, eindeutige
  Mermaid-IDs, SVG-Erhalt bei Freshness-Updates, manuellen Scrollbesitz und Observer-Cleanup.
  Vorhandene Reduced-Motion-, Fehler-, Vorschau- und 700-ms-Abschlusstests bleiben grün.

Rust, IPC, Persistenzschema, Recherchebudgets und Berechtigungen wurden nicht geändert.
Die vorhandene lokale Node-Version 25.6.1 weicht vom Pin 24.14.0 ab. Der Build meldet weiterhin
die bereits bestehenden BigInt-/Safari-13-Zielwarnungen; Svelte-Check und ESLint sind warnungsfrei.
