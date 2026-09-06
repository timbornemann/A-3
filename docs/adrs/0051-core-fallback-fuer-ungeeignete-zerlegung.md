# ADR-0051: Originalauftrag bei ungeeigneter Initialzerlegung erhalten

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs und direkter Behebung der Modelltestbefunde.\
Präzisiert ADR-0047/0049 und ersetzt ausschließlich die Initialisierungs-Reparaturregel aus ADR-0050.

## Befund

Das explizit freigegebene lokale `qwen3.5:4b` erzeugte bei einer Storage-Frage sowohl
primär als auch nach dem Einzelrepair eine formal gültige reine Entwurfszerlegung.
Damit konnte der Vertrag seine Pflicht zu aktuellen Originalbelegen nicht erfüllen.
Ein weiterer Modellrepair oder größere Budgets wären keine verlässliche Steuerung.

## Entscheidung

Erst nachdem Schema, Domain-Definitionen und Abhängigkeiten eines Initialvorschlags
vollständig validiert wurden, prüft der Core seine Mindesttauglichkeit. Sind bestehende
Dateien ausdrücklich benannt, aber keine erforderliche Repository-Frage vorhanden,
bindet der Core den unveränderten vollständigen Originalauftrag als eine erforderliche
Repository-Untersuchung. Sie verlangt alle angefragten Teile, Ziele, Bedingungen und
Reihenfolgen und trennt bestehendes Verhalten von zukünftigen Vorschlägen.

Der Fallback liefert keine Ergebnisse, Quellen, Tatsachen oder Erledigtmarkierungen.
Die Originalabdeckung und spätere Zulassung bleiben unverändert. Er benötigt keinen
zusätzlichen Modellaufruf und erneuert keine Zähler. Andere zulässige Zerlegungen und
bereits gespeicherte Verträge werden nicht umgeschrieben. Ungültiges JSON, ungültige
Abhängigkeiten oder vorgezogene Ergebnisse sind ausdrücklich keine Fallback-Eingaben.

## Konsequenzen

Die Prüfliste zeigt in diesem begrenzten Fall einen Core-Auftrag statt der ungeeigneten
Modellzerlegung. Das reduziert die Granularität, erhält aber die tatsächliche Aufgabe.
Der Core behauptet keine vollständige semantische Validierung natürlicher Sprache;
Praxisabnahme und Fehlfalltests bleiben erforderlich.
