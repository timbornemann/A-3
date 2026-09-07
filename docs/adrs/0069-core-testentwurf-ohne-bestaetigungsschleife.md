# ADR-0069: Core-Testentwurf ohne Bestätigungsschleife

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich direkter Modelltestkorrekturen.

## Befund

GPT-OSS CSV 3:2 in `eval-1788770459162.jsonl` beantwortet Bestandsaufnahme und
Implementierungsentwurf, fragt dann aber nach Bestätigung der Testszenarien,
bevor es die ausdrücklich beauftragten Tests entwirft. Alle genannten Beispiele
(Erfolg, fehlende Datei, Header, Zeilenvalidierung, unbekanntes Projekt, UTF-8)
sind eigene Entwurfsarbeit, keine fehlende Nutzerfreigabe. Der bisherige generische
Design-Vertrag lässt diese Rückfrage ungeprüft bis zum Nutzer passieren.

## Entscheidung

Nur die dritte Pflicht des unverändert erkannten Core-Planvertrags aus
[ADR-0049](0049-core-planpflichten-und-statusnotizen.md) verwendet nach erfüllten
Voraussetzungen die typisierte Phase `DesignTests(Q)`. Sie verlangt genau ein
konkretes `designDecision` mit Testeingaben, erwarteten Ergebnissen und Prüfmethode,
ohne neue Fragen oder Originalbelege. Das Ergebnis bleibt ein Vorschlag, keine
Ausführungs- oder Verifikationsevidenz. Originalauftrag und vollständiger bereits
zugelassener Änderungsentwurf bleiben erhalten.

Eine `question`-Antwort ist in diesem eng abgegrenzten Schritt ein ungültiges
Dokument. Sie erhält höchstens den bestehenden Einzelrepair mit phasengenauem
Hinweis; wiederholte Ungültigkeit endet ehrlich, ohne Nutzerbestätigungsfrage,
neuen Read, abgeschlossenen Schritt oder vergiftete Analysequittung.

Dies präzisiert ausschließlich die Rückfrageausnahme aus
[ADR-0057](0057-leerer-entwurf-ist-kein-rechercheauftrag.md) für den Core-Testentwurf.
Allgemeines Design, eigentliche Änderungsentscheidungen in Q2, Ask und historische
nicht passende Verträge behalten ihre bisherigen Grenzen und echten Rückfragen.
Keine Datenlöschung, Ausführung, Netzwerkaktion oder sonstige Freigabe wird durch
Testplanung autorisiert. Stale Voraussetzungen öffnen den Prüfstand unverändert.
V5-Dokumentstruktur, persistierter Fragevertrag und sämtliche Budgets bleiben gleich.

## Prüfung

Echter Mehrmodusvertrag mit Git, Fast Index, Safe Reader und libSQL reproduziert
einmalige/wiederholte Testbestätigungsfragen. Geprüft werden genau ein Repair,
null adaptive Reads, unveränderte Dateien, ehrlicher Fehler bzw. fertiger Plan
und sichere Wiederaufnahme. Unabhängiger Phasendecoder und Providerschemas prüfen
die neue enge Form, während echte Fragen in allgemeinem Design zulässig bleiben.
Der konkrete Live-Gegenbeweis wird mit demselben Modell erneut geprüft.
