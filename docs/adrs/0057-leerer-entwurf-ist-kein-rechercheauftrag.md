# ADR-0057: Leerer Entwurf ist kein Rechercheauftrag

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs.\
Ergänzt ADR-0049/0054; allgemeine Analyze-Verträge bleiben unverändert.

## Befund und Entscheidung

Der lokale Nachtest lieferte in Design Q2 `progress` mit leerem `results`.
Der Decoder akzeptierte dies und der Core startete zwölf adaptive Zugriffe,
obwohl keine Bestandsfrage mehr offen war. Der noch nicht entworfene CSV-Import
ist kein fehlender Originalbeleg.

In Design verlangt `progress` genau eine konkrete Designentscheidung für die
aktive Pflicht. Der unabhängige Phasendecoder erzwingt diese bedingte Kardinalität;
das providerkompatible Schema beschreibt sie zusätzlich. Die Ergebnisliste bleibt
nur für die explizite Entscheidung `question` leer, wenn eine folgenreiche
Nutzerentscheidung fehlt. Der Core leitet aus leerem Fortschritt keinen Read ab.
Ein ungültiges Dokument nutzt ausschließlich den vorhandenen einmaligen Repair.
Bleibt es ungültig, gibt es keinen Erfolg, keine Analysequittung und keine Leserunde.

Analyze darf weiterhin echte Beleglücken ohne Ergebnis melden. Berechtigungen,
Budgets, gespeicherte Verträge und die V5-Dokumentstruktur ändern sich nicht.
Eine formal gültige Entwurfsantwort ist keine Garantie inhaltlicher Richtigkeit.

## Verifikation

Die Decoderregression prüft leeren Designfortschritt, zulässige Nutzerfragen,
vollständigen Entwurf und unverändertes nullable Analyze. Zwei Controllerregressionen
mit realem Git, Fast Index, Safe Reader und libSQL reproduzieren vor der Korrektur
die unerwünschten Zugriffe. Sie verlangen nach der Korrektur exakt einen Repair,
null adaptive Reads sowie entweder vollständigen Plan oder ehrlichen Fehler ohne
vergiftete Paketquittung. Plan und Agent-Vorbereitung werden getrennt durchlaufen.
Reale lokale und konfigurierte Modellnachtests prüfen den Praxisweg gesondert.
