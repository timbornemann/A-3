# ADR-0121 Revisionsgebundener Quellbeleg für Python-Ganzzahlfunktionen

Status: Accepted  
Datum: 2026-11-09  
Entscheider: Tim Bornemann

Annahme durch Tim Bornemann am 2026-11-09.

## Kontext

[Plan 22/P5](../plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md#p5-erster-vollständiger-agent-und-sachliche-fehlerkorrektur)
verlangt, dass A^3 den eingefrorenen Auftrag A01 über den vorhandenen Testpfad
abschließt. Der Auftrag fordert eine feste Verschiebung für jede ganze Zahl
und schließt Konstantantworten aus. Der einzige erlaubte Prüfprozess führt
den vorhandenen `unittest` aus. Im isolierten Entwicklungsfall prüft dieser
genau `adjust(41) == 42`. Mit den unveränderten Testdateien scheiterte der
Originalcode, während `return 42` denselben Test bestand.

Der Prozessbeleg bestätigt den ausgeführten Fall. Für die weitergehende
Originalpflicht fehlt ein passender Produktbeleg. Das externe Eval-Orakel
bleibt eine unabhängige Kontrolle des Ergebnisses gemäß [ADR-0119](0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md).

## Entscheidung

- Ein Kriterium mit einem passenden vorhandenen Test darf zusätzlich eine
  typisierte Pflicht für eine reine einstellige Python-Ganzzahlfunktion
  tragen. Die Pflicht bindet den Originalbereich, den erlaubten Pfad, die
  bezeichnete Deklaration und die erwartete vorzeichenbehaftete Verschiebung.
  Der aktive Prüfschritt übernimmt diese Pflicht unverändert. Das Modell darf
  den Nachweis weder selbst ausstellen noch beim Replan abschwächen.
- Der vorhandene sichere Quellpfad liefert die vollständigen aktuellen Bytes
  des begrenzten Moduls. Der bestehende Python-Parser prüft einen geschlossenen
  Ausschnitt mit genau einer eindeutigen undekorierten Funktion, einem Parameter
  und einem direkten Return. Zulässige Ausdrücke bestehen aus dem Parameter,
  Ganzzahlliteralen, Klammern und `+` oder `-`. Der Adapter normalisiert den
  Ausdruck mit geprüfter Arithmetik zu `a*x+b`. Er liefert einen Nachweis nur
  für `a=1` und die erwartete Verschiebung `b`. Weitere Deklarationen mit
  möglichen Seiteneffekten, mehrdeutige Bindung, Parserfehler und überschrittene
  Ressourcenlimits halten die Pflicht offen.
- Der bestehende Prüfschritt verlangt weiterhin den tatsächlichen erfolgreichen
  Test unter der bestehenden Prozesspolicy. Bei dieser typisierten Pflicht bindet
  sein `TestEvidence` zusätzlich den Quellbeleg und die vollständige aktuelle
  Dateirevision. Nach Reopen prüft die vorhandene Source-Acceptance beide Belege
  und ihre Gültigkeit für dieselbe Prüfung. Ein neuer Dateiinhalt entwertet den
  abhängigen Beleg vor weiterer Modellarbeit. Der Store hält nur typisierte
  Ergebnisse, Bereichs- und Hashbezüge.
- Der Nachweis verwendet dieselben Grenzen für Projektzugriff, Cancellation,
  Zeitbudget und Recovery. Es gibt genau den ursprünglich erlaubten Testprozess.
  [ADR-0117](0117-auftragsbezogene-tests-und-verifikation.md) regelt weiterhin,
  wann Tests in einem Nutzerprojekt erstellt werden dürfen. Der Kandidat wird
  bis P11 ausschließlich in isolierten Tests komponiert.

## Konsequenzen

Der Core kann die definierte Funktionseigenschaft auch bei einem kleinen
vorhandenen Testbestand belegen. Das Rezept akzeptiert bewusst einen begrenzten
syntaktischen Ausschnitt. Gleichwertige Implementierungen außerhalb dieses
Ausschnitts behalten einen offenen Nachweis. Andere Programmiersprachen oder
Eigenschaften benötigen eine eigene passende Produktprüfung.

## Verworfene Alternativen

- Ein grüner unittest allein lässt die belegte Konstantantwort passieren.
- Ein aus dem Fast Index abgeleiteter Parameterread genügt bei `value-value`
  und anderen wirkungslosen Datenabhängigkeiten nicht. Das bestehende
  [Flow-Modell](0045-fast-index-function-flow-analysis.md) bleibt Navigation.
- Zusätzliche Tests oder ein weiterer Prüfprozess würden die eingefrorenen
  Rechte von A01 erweitern. Ein externes Orakel soll die Produktabnahme prüfen.

## Compliance

- Der exakte vorhandene `unittest` muss die Konstantantwort tatsächlich grün
  melden, während die Produktprüfung denselben Fall ohne `Done` hält.
- Die gebundene Verschiebung muss im aktuellen Quellbereich nachweisbar sein.
  Positive Kontrollen prüfen beide Vorzeichen und der unabhängige A01-Lauf
  prüft zusätzlich negative und sehr große Eingaben.
- Eine falsche Verschiebung, eine scheinbare Parameterabhängigkeit, ein fremder
  Pfad, eine veränderte Revision, unvollständige Eingaben und Reopen ohne
  vollständigen Beleg dürfen keinen Nachweis erzeugen.
