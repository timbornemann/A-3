# ADR-0116: Kompakter Agentenverlauf und Entscheidungsbereich

Status: Accepted  
Datum: 2026-09-22  
Entscheider: Tim Bornemann

## Anlass

Der Nutzer verlangt einen frei lesbaren, chronologischen Agentenverlauf, kompakte Plankarten
und eine vom Verlauf gelöste Freigabekarte, die am unteren Rand mit dem Eingabefeld wechselt.
ADR-0114 schreibt bisher die Freigabe innerhalb derselben Chatkarte vor.

## Entscheidung

- Diese Entscheidung ersetzt ausschließlich die Platzierung der Freigabe in ADR-0114.
  Der Agent-Workspace bleibt die gemeinsame Arbeitsfläche; keine neue globale Oberfläche.
- Der Verlauf zeigt Auftrag, Recherche, Plan und Ausführung in dieser zeitlichen Reihenfolge.
  Unveränderte historische Inhalte behalten ihre Instanzen und bleiben unabhängig von Polls.
- Pläne und lange Ausführungsdetails erscheinen zunächst als kompakte Karten. Ihr vollständiger
  Inhalt ist mit einem beschrifteten Öffnen-Button in einem zugänglichen Dialog erreichbar.
- Ein schwebender, an die Chatbreite gebundener Entscheidungsbereich liegt außerhalb des
  scrollenden Verlaufs und ersetzt bei einer erforderlichen Freigabe das normale Eingabefeld.
  Ein Wechsel zur Nachrichteneingabe bleibt möglich; ein sichtbarer Hinweis führt zur offenen
  Freigabe zurück. Entwürfe gehen dabei nicht verloren.
- Aktion, Risiko, konkrete Pfade beziehungsweise vollständiger Befehl und Netzwerkumfang bleiben
  vor einer Entscheidung sichtbar. Weitere bereits vorhandene Metadaten sind kompakt aufklappbar.
  Die vollständige exakte Darstellung bleibt erhalten; es gibt keine neue Mutationsbefugnis.
- ADR-0022 gilt weiter: keine vorausgewählte Entscheidung; ausdrückliches Bestätigen;
  eine gespeicherte Einmalfreigabe startet nichts. Fortsetzen und Widerrufen bleiben getrennt.
  Veraltete oder nicht bestätigte Anker sperren Entscheidungen weiterhin.
- Automatisches Folgen endet am tatsächlichen Ende des Verlaufs. Manuelles Scrollen, Lesen,
  Aufklappen und Auswählen dürfen nicht zu einer älteren Arbeitsschrittposition zurückspringen.
  Resize-Beobachtung schreibt weiterhin erst im folgenden Animation Frame.
- Beide Themes, 44-Pixel-Ziele, Tastaturbedienung, Fokus-Rückgabe, Reduced Motion und kleine
  Desktopfenster bleiben Teil der Abnahme.

## Nachweise

Regressionen prüfen das Scrollen bis zum Ende, manuelles Lesen während Hintergrundaktualisierungen,
die Reihenfolge Recherche vor Plan, kompakte Planansicht und vollständigen Dialoginhalt,
Freigabedock außerhalb des Verlaufs, erhaltene Entwürfe sowie unveränderte Sicherheitszustände.
Eine Browserprüfung ergänzt Komponenten- und Protokolltests mit tatsächlicher Geometrie.

## Referenzen

- [ADR-0114](0114-chatgebundene-agentenausfuehrung.md)
- [ADR-0022](0022-task-bound-approval-center.md)
- [Corporate Design](../corporate-design-a3.md)
