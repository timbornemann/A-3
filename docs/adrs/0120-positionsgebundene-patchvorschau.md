# ADR-0120 Positionsgebundene Patchvorschau für Editfenster

Status: Accepted  
Datum: 2026-11-09  
Entscheider: Tim Bornemann

Annahme durch Tim Bornemann am 2026-11-09.

Supersedes den ausschließlich auf Dateipräfixe begrenzten Vorschauinhalt
aus [ADR-0021](0021-bounded-agent-inspection.md) für Bereichsedits nach
[ADR-0119](0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md).

## Kontext

[Plan 22/P3](../plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md#p3-exakter-bereichsedit-durch-die-bestehende-werkzeuggrenze)
verlangt gezielte Änderungen im mittleren Abschnitt einer Datei über 200 KB. Der Core
erzeugt den vollständigen `PatchUpdate` aus aktuellen Ausgangsbytes und einem kleinen
Ersatztext. Die bisherige E3-Vorschau zeigt maximal 16 KiB je Inhaltsseite ab
Dateianfang. Bei einer Änderung hinter diesem Präfix können beide dargestellten
Seiten bytegleich sein und der Inspector zeigt keinen Änderungshunk zur Freigabe an.

## Entscheidung

- E3 erzeugt für den gebundenen Bereichsedit eine Vorschau um die tatsächliche Änderung.
  Sie nutzt dieselben sicher geprüften vollständigen Vorher- und Nachherbytes wie die
  übrige Patchvorschau. Der Core prüft, dass die ausgelassenen Außenbereiche bytegleich
  sind. Beide Ausschnitte tragen ihre absoluten Startzeilen. Die Darstellung markiert
  ausgelassenen Kontext und behält vollständige Hashes, Bytezahlen und Encoding-Metadaten.
- Es gelten weiter 16 KiB je Inhaltsseite und 64 KiB insgesamt. Zusätzlicher Kontext
  weicht zuerst. Lässt sich die Änderung innerhalb der Grenzen nicht exakt darstellen,
  hält der Core vor `AwaitApproval` mit einem typisierten Größenfehler an. Der nächste
  Versuch benötigt einen kleiner darstellbaren Edit.
- Der bestehende Inspector projiziert die E3-Vorschau aus derselben Action und demselben
  Basissnapshot. Exakter Fingerprint, Berechtigungsrevision, Policy, Lease, Liveprüfung,
  Publish und Recovery behalten ihre bisherigen Verträge. Die WebView erhält die
  berechneten Hunks über den vorhandenen taskgebundenen Lesepfad.
- Vorschau und ausstehende vollständige Patchaktion bleiben flüchtig gemäß
  [ADR-0022](0022-task-bound-approval-center.md). Nach einem Neustart kann ein verlorener
  Vorschauinhalt nicht Grundlage für die Ausführung sein. Bereits erfolgte und unbekannte
  Wirkungen behalten die bestehende dauerhafte Recovery.
- Bis P11 ist die Auswahl des Änderungsausschnitts ausschließlich in der isolierten
  Kandidaten-Testkomposition erreichbar. Die eingefrorene Baseline nutzt ihre bisherige
  Präfixvorschau.

## Konsequenzen

Nutzer können die konkret zur Freigabe stehende Änderung auch tief in einer großen Datei
prüfen. Einzelne sehr lange Zeilen können eine kleinere, darstellbare Zerlegung oder einen
Kapazitätshalt erfordern. Die Ansicht bleibt an dieselbe flüchtige, bereits klassifizierte
E3-Quelle gebunden.

## Verworfene Alternativen

- Größere Präfixe halten die Position beliebiger später Änderungen weiterhin offen und
  erhöhen die Quelltextmenge der Ansicht.
- Ein zusätzlicher Quellkanal am Inspector würde die exakte Freigabe von der E3-Vorschau
  entkoppeln. Nachträgliches Lesen könnte fremde Live-Änderungen in diese Ansicht mischen.
- Persistierte Patch- oder Hunktexte erweitern die Retention. Für die Anzeige vor der
  Freigabe reicht die bereits vorhandene flüchtige Quelle.

## Compliance

- Ein nativer P3-Test prüft den exakten Hunk mit absoluten Zeilen vor der tatsächlichen
  Freigabe einer mittigen Änderung in einer Datei über 200 KB. Er enthält Unicode, CRLF,
  doppelte Textvorkommen und große unveränderte Wächterbereiche.
- Negativtests prüfen unerlaubte Änderungen außerhalb des darstellbaren Bereichs,
  Überschreitung der Vorschaugrenzen, Secrets, eine abweichende Action und eine nach der
  Vorschau veränderte Datei. Größenfehler erzeugen keine freigabefähige Aktion.
- Der bisherige Präfixpfad und die vorhandenen Policy- und Recovery-Regressionsprüfungen
  bleiben Teil der Abnahme. Kandidaten-Reopen prüft die dauerhaften Wirkungen bereits
  ausgeführter Änderungen über den bestehenden Store.
