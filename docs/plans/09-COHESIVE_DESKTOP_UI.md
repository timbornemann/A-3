# Plan 09: Zusammenhängende A^3-Arbeitsumgebung

Status: Complete · 2026-09-05

## Auftrag und Grenzen

Die gesamte vorhandene Desktopoberfläche wird anhand des
[Corporate Designs](../corporate-design-a3.md) als zusammenhängende, verständliche
Agentenumgebung überarbeitet. Grundlage sind Plan 06/U1, U9 und U13 sowie
ADR-0001, ADR-0002, ADR-0012, ADR-0014, ADR-0024, ADR-0025, ADR-0029, ADR-0033,
ADR-0036, ADR-0041 und ADR-0045. Bestehende Funktionalität, Core-Autorität,
Freigaben, Evidenzbindung, Lazy Loading und begrenzte Projektionen bleiben erhalten.

Nicht-Ziele: Backend-, Schema- oder Capabilityänderungen, neue Provider,
Installationen, Cloudzugriffe, Veröffentlichung und neue fachliche Funktionen.
Die bereits vorhandene Datei `docs/corporate-design-a3.md` bleibt unverändert.

## Verifizierbare Schritte

- [x] Gemeinsames Theme und App-Shell: semantische Crimson-/Charcoal-Palette,
  beschriftete Aktivitätsleiste, ruhige globale Statusleiste, klare Projektbibliothek.
  Nachweis: Kontrast-/DOM-Verträge, Navigation und visuelle Prüfung beider Themes.
- [x] Durchgehender Agentenbereich: flache Conversation, zusammenhängender Composer,
  abgestimmte Seitenleisten, native Dialoge und optionale technische Prüfdetails.
  Nachweis: bestehende Conversation-/Approval-/Inspection-Tests und Fokus-/Resize-Regressionen.
- [x] Karte, Abläufe, Deep Map und Einstellungen: einheitliche Controls, Trennlinien
  statt verschachtelter Karten, verständliche Beschriftung und responsive Bereichsnavigation.
  Nachweis: bestehende Komponentenverträge und interaktive Browser-/Desktopprüfung.
- [x] Gesamtabnahme: Frontend-Quality-Gate, Linkprüfung, finale Diffprüfung sowie
  kleine Fenster, Tastatur, Reduced Motion und erhaltene Chat-Scrollstabilität.
  Nachweis und verbleibende Plattformgrenzen werden nach tatsächlich erfolgten Checks ergänzt.

## Gestaltungsentscheidungen

Das Corporate Design bestimmt Flächen, Typografie und Akzent. Normale Texte bleiben
gut lesbar, statt flächendeckend Terminalsprache oder dekorative Systemdaten zu zeigen.
Schwarz auf Signalrot im Dark Theme und Weiß auf dunklerem Rot im Light Theme erhalten
den AA-Kontrast. System/Hell/Dunkel bleiben gemäß ADR-0024 verfügbar.

Hauptbereiche bleiben URL-gebunden. Die gemeinsame Aktivitätsleiste schafft Platz
für die jeweils benötigte Arbeitsfläche. Projekt-, Index-, Modell- und Laufstatus
stehen in einer schmalen dauerhaften Statusleiste. Einstellungen erhalten auf breiten
Fenstern eine vertikale Bereichsnavigation und in kleinen Fenstern eine horizontale
Leiste. Das ersetzt die frühere rein horizontale Präsentation aus Plan 06/U8, ohne
eine neue fachliche Grenze einzuführen.

Übergänge verwenden 120–150 ms. Live-Recherche wird weiterhin ausschließlich über
Opazität eingeblendet, um die Scrollgeometrie nicht zu verschieben. Reduced Motion
bleibt maßgeblich. Technische Freigabedaten sind auf Wunsch zugänglich; konkrete
Aktion, Risiko, Pfade, Argumente und die ausdrückliche Entscheidung bleiben sichtbar.

## Prüfstand

Die schrittweise Abnahme erfolgte unter Windows im echten Tauri-Entwicklungsfenster
und im Chromium-basierten Browser mit den vorhandenen Offline-Fixtures. Das native
Fenster zeigte ein bestehendes Projekt; für die Prüfung wurde kein Providerlauf gestartet.

| Fläche | Nachweis |
| --- | --- |
| App-Rahmen und Projekte | Native Navigation, aktive Projektbibliothek, lange Projekt-/Statusnamen; vollständige Browser-App bei 720 CSS-Pixeln ohne Dokumentüberbreite |
| Agent | Echter langer Chat-Titel bleibt begrenzt, Composer und Header-Aktionen bleiben sichtbar; Seitenleisten-Fokus, Escape, Tastaturgrößen und Pointer-Abbruch mit Regressionstests |
| Dialoge | Verbindungsdialog nativ mit maskierter Eingabe und Escape; Umbenennen im Browser bei 720×520, Fokus im Eingabefeld und nach Escape wieder am Auslöser; native Goal-Dialog-/Abbruch-/Fokusregression |
| Karte und Deep Map | Native Projektkarte; Offline-Atlas mit 64 Modulen; Code- und Analyseinspektor; 632×432 als verbleibende Fläche eines 720×520-Appfensters, Canvas und Arbeitsbereich mit identischer Unterkante, Drawer vollständig innerhalb des Arbeitsbereichs, keine sichtbaren Controls unter 44 Pixeln |
| Abläufe | Native Bibliothek sowie echte produktive Ablaufkomponente mit 4.096 Fixture-Schritten, begrenzten Seiten und Werteseitenleiste; bei 720×520 ohne horizontale Dokumentüberbreite |
| Einstellungen | Native Hell-/Dunkelansichten, vertikale Bereichsnavigation und Verbindungsdialog; bestehende Provider-/Modell-/Projekt-/Privacy-Verträge |
| Theme und Bewegung | AA-Kontrastverträge für beide Themes; alle semantischen Farbreferenzen definiert; echte 3-Pixel-Fokusmessung an Controls und Composer; Reduced-Motion-CSS-Vertrag |

Das U13-Browserprofil wurde mit 20 historischen Turns und zwei tatsächlich lokal
gerenderten Mermaid-SVGs erneut ausgeführt. Bei 720×520 blieben alle 22 überwachten
Knoten erhalten: 60 Stichproben, keine historischen Änderungen, keine Schrumpfung,
maximal 0,44 CSS-Pixel Abstand zum Live-Ziel. Beim manuellen Lesen war `following`
aus und die Drift über weitere 60 Stichproben exakt null. Beide Läufe hatten keine
horizontale Überbreite und keine erneuten Diagramm-Artefaktreads. Dies ist eine
Stabilitätsprüfung, keine Behauptung einer höheren Modell- oder Rendergeschwindigkeit.

Wiederholbare manuelle Layoutregressionen ergänzen die DOM-/CSS-Verträge, da jsdom
keine reale CSS-Layoutberechnung besitzt:

1. Vorhandenes U13-Fixture bei 720×520 öffnen, Profil starten, mit PageUp aus dem
   Live-Verlauf lösen und erneut prüfen. Umbenennen öffnen und mit Escape schließen;
   Auslöserfokus und sichtbare Sendeaktion kontrollieren.
2. U11-Atlas-Vorschau bei 1000×800 und bei 632×432 öffnen. Code-Inspektor,
   Aufgabenfokus und Analysefortschritt öffnen. Drawer dürfen ihren Arbeitsbereich
   nicht überschreiten; die Canvas-Unterkante darf nicht unter dem Dock liegen.
3. In der App Hell/Dunkel wechseln, durch alle fünf Bereiche navigieren und einen
   Dialog per Tastatur öffnen/schließen. Der Fokus bleibt sichtbar; Escape aus einer
   anderen Ansicht darf keinen versteckten Agent-/Kartenbereich verändern.

`pnpm ci:frontend` bestand mit 368 Component-/Contracttests und fünf Tooltests;
14 zuvor bestehende Tests bleiben übersprungen. Prettier, ESLint mit null erlaubten
Warnungen, Svelte-Check (0 Fehler, 0 Warnungen) und Vite-Produktionsbuild bestanden.
`pnpm check:links` prüfte 79 Markdown-Dateien und 300 lokale Links. Das vollständige
Frontend-Gate wurde nach dem letzten visuellen Detailpass erneut erfolgreich ausgeführt.
`git diff --check` und der unabhängige finale Diffreview bestanden; es wurden keine
blockierenden Architektur-, Sicherheits- oder Bedienfehler gefunden.

Prüfgrenzen: Verwendet wurden die vorhandene Node-24.19.0-Laufzeit und pnpm 11.9.0;
der Projekt-Pin verlangt Node 24.14.0. Der Build meldet weiterhin die bereits
vorhandenen BigInt-/Safari-13-Zielwarnungen. macOS und Linux wurden nicht nativ
getestet; plattformspezifischer Code, IPC, Schema und Berechtigungen wurden nicht
geändert. Es wurden keine Pakete installiert oder neue Abhängigkeiten hinzugefügt.
