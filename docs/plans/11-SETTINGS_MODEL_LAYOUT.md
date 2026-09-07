# Plan 11: Einstellungen und Modellauswahl ordnen

Status: Complete · 2026-09-07

## Auftrag und Grenzen

Die aktualisierte Mehrprovider-Oberfläche erhält das gemeinsame
[Corporate Design](../corporate-design-a3.md) aus Plan 09. Eingaben und zugehörige
Aktionen werden zusammen angeordnet. Eine durchsuchbare, begrenzte Modellauswahl
darf weder die Einstellungsseite verlängern noch deren Bedienelemente verschieben.

Maßgeblich sind [ADR-0066](../adrs/0066-mehrere-provider-und-gemeinsame-modellauswahl.md),
[ADR-0024](../adrs/0024-semantic-design-tokens-and-accessible-themes.md) und
[ADR-0025](../adrs/0025-bounded-desktop-rendering-and-lifecycle.md).
Die drei unabhängigen Providerkarten, explizite Katalogabfragen, revisionsgebundene
Rollenprüfungen und native Sicherheitsentscheidungen bleiben erhalten.

Nicht-Ziele: Rust-, IPC-, Schema- oder Berechtigungsänderungen, zusätzliche
Provider, Installationen, automatische Modellabfragen und Änderungen an anderen
Arbeitsbereichen.

## Verifikation

- [x] Providerkarten mit zugehörigen Formularaktionen, sichtbarem Status und
  bedienbaren Fehler-/Abbruchzuständen; Component-Regressionsverträge.
- [x] Rollen als kompakte Zeilen, native Modellauswahl mit Suche, Providerfilter,
  begrenztem Listenausschnitt und ausdrücklicher Prüfung; Auswahl- und Fokusverträge.
- [x] Offline-Browserfixture mit großen Katalogen: Ausgangslayout und Ergebnis,
  Hell/Dunkel, schmale Fenster, innere Scrollgrenzen und Tastaturbedienung.
- [x] Vollständiges Frontend-Gate, Markdown-Linkprüfung und finaler Diffreview.

## Ergebnis und Nachweise

`ModelProviderCard.svelte` ordnet Status, Aktivierung und explizite Katalogabfrage
in einem kompakten Kopf an. Verbindungsdetails enthalten jeweils Adresse plus
Speicheraktion und Passwortfeld plus Speicheraktion. Eingaben bleiben bis zur
zugehörigen Aktion lokal; Credentialbytes werden unmittelbar nach Übergabe
geleert. Die drei Karten bleiben dauerhaft sichtbar. Die Einstellungsnavigation
bleibt stehen, während nur der Inhaltsbereich scrollt.

`ModelSelectionDialog.svelte` zeigt einen nativen Dialog mit Suchfeld,
Anbieterfilter, einer separat scrollbaren Liste und höchstens 40 nativen
Radiozeilen pro Seite. Suche und Seitenwechsel setzen den Listenscroll zurück,
erhalten aber die Auswahl. Erst „Auswählen und prüfen“ startet die bestehende
rollenbezogene Probe. Gespeicherte Ressourcenlimits werden übernommen und sind
unter „Erweiterte Limits“ bearbeitbar. Das Core-Ergebnis bestimmt weiterhin den
angezeigten Capabilitystatus.

Die zwölf Regressionstests in `SettingsPanelV2.test.ts` prüfen Initialzustand ohne
Providerzugriff, begrenzten DOM, gleiche IDs verschiedener Anbieter, Suche,
Seitenwechsel, explizite Probe, Escape/Fokus, Providerdeaktivierung, gespeicherte
Profile/Limits, ungültige Limits, One-way-Key-Cleanup, Fehlermeldungen und Retry,
Cancellation sowie die V2-Read-Wiederholung. Fehlgeschlagene Probes können im Core
Health und Revision persistieren; die UI liest vor einem Retry diese lokalen
Metadaten neu ein. Kataloge anderer unveränderter Provider bleiben dabei erhalten.

### Reproduzierbare visuelle Prüfung

Die lokale Vite-Fixture `apps/desktop/fixtures/settings-layout.html` mountet die
produktiven Komponenten mit injizierten synthetischen Ports. Sie lädt je Provider
240 Modellnamen ausschließlich nach einem Klick und führt keine Provider- oder
Tauri-Anfrage aus. Die letzte ID ist absichtlich sehr lang.

1. Bei 1280×800 unter „KI & Modelle“ Kataloge laden und eine Rollenwahl öffnen.
2. Suchen, Anbieter filtern, die letzte sichtbare Zeile auswählen und zur nächsten
   Seite wechseln. Auswahl bleibt erhalten, Listenscroll beginnt wieder bei null.
3. Hell/Dunkel unter „Allgemein“ wechseln; bei 720×520, 632×432 und 360×640
   Dialog, lange Namen, Felder, Aktionen und Tastaturfokus prüfen.
4. Mit Escape schließen, Auslöserfokus prüfen, Verbindung bearbeiten und
   Einstellungsbereich wechseln. Die Bereichsnavigation bleibt erreichbar.

Gemessen im Chromium-basierten Browser unter Windows:

| Prüfung | Ergebnis |
| --- | --- |
| Ausgangsversion, ein Katalog mit 240 Modellen | 720 Modellbuttons im DOM; eine aufgeklappte Rollenliste war 5.340 CSS-Pixel hoch; Provideraktionen überlappten Eingaben |
| Neue Auswahl, bis zu 720 Modelle aus drei Katalogen | Höchstens 40 Radiozeilen; Listenhöhe 224 CSS-Pixel bei 1280×800 |
| Einstellungen vor/nach Dialogöffnung bei 1280×800 | Shell jeweils 800 Pixel, Inhalt jeweils 879 Pixel Scrollhöhe, Rollenübersicht jeweils 207,5 Pixel; kein Wachstum durch die Liste |
| Seitenwechsel nach Scrollen bis zur letzten Zeile | Scrollposition von 1.538 auf exakt 0 Pixel; vorherige Auswahl bleibt im Dialogfuß sichtbar |
| Kleiner Arbeitsbereich 632×432 | Dialog vollständig innerhalb des Fensters, primärer Button 44 Pixel hoch und sichtbar; keine horizontale Dokumentüberbreite |
| Reflow 360×640 | Keine horizontale Dokumentüberbreite; sichtbare Controls/Radiozeilen mindestens 44 Pixel; gemessener Fokusrahmen 3 Pixel |
| Themes und Fokus | Hell/Dunkel visuell geprüft; Escape und erfolgreiche Auswahl stellen den Fokus auf den Auslöser zurück; keine Browser-Konsolenfehler |

Diese Messungen belegen Layout- und DOM-Grenzen, keine allgemeine
Geschwindigkeitsverbesserung.

### Ausgeführte Checks und Grenzen

- `pnpm --filter @a3/desktop exec vitest run src/lib/SettingsPanelV2.test.ts`: 12 bestanden.
- `pnpm --filter @a3/desktop exec vitest run src/lib/SettingsPanel.test.ts src/design-tokens.test.ts`: 16 bestanden.
- `pnpm ci:frontend`: nach der letzten Codekorrektur erfolgreich; Prettier,
  ESLint ohne Warnungen, Svelte-Check mit 0 Fehlern/0 Warnungen, 385 Frontendtests
  und fünf Tooltests, anschließend Produktionsbuild. 14 vorhandene Tests bleiben
  übersprungen. Die semantischen Theme- und Reduced-Motion-Verträge bestehen.
- `pnpm check:links` und `git diff --check`: erfolgreich.
- Finaler Diffreview: ausschließlich Settings-Präsentation, zugehörige
  Regressionen, Offline-Fixture und Dokumentation; keine Rust-, IPC-, Schema-,
  Capability- oder Abhängigkeitsänderung.

Verwendet wurden die vorhandenen Laufzeiten Node 24.19.0 und pnpm 11.9.0;
der Repository-Pin verlangt Node 24.14.0. Der Build meldet weiterhin BigInt-
Zieltransformationen und einen großen bestehenden Diagramm-Chunk. Native
Windows-/macOS-/Linux-WebViews und echte Providerverbindungen wurden in dieser
Layoutänderung nicht erneut ausgeführt. Die Browser-Fixture ersetzt keine
Live-Capabilityprüfung; deren bestehende Rust-Grenze bleibt unverändert.
