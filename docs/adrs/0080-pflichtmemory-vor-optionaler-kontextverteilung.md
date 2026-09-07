# ADR-0080: Pflichtmemory vor optionaler Kontextverteilung

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Der reale Agent-Nachtest erreicht eine fehlgeschlagene Verifikation und Replan.
Die vollständigen offenen Fehlereinträge werden erst nach der Verteilung von
System- und Goal-Tokens gepackt. Obwohl Project Map und Toolresultate noch Platz
enthalten, kann Code/Evidence bereits auf sein Mindestbudget geschrumpft sein.
Der folgende AnchorTooLarge-Abbruch verhindert die vorhandene sichere Recovery.

## Entscheidung

Die noch nicht abgenommene Context-Policy V5 ermittelt vor Retrieval zusätzlich
den tatsächlich gezählten Pflichtanteil aus Run-Memory-Identität, offenen
Fehlereinträgen, offenen Hypothesen und aktuellem Research-/Replan-Vertrag.
Die Darstellung wird einmal erzeugt und unverändert weiterverwendet. Für diese
Bytes samt Bereichsheader bleibt Code/Evidence mindestens ausreichend groß.

Die bestehende feste Spenderreihenfolge aus
[ADR-0078](0078-freien-kontext-vor-pflichtabbruch-nutzen.md) gilt weiter; nur wirklich
optionaler Platz darf abgegeben werden. Reichen Kontextlimit und unveränderte
Safety-/Outputreserven nicht für sämtliche Pflichtanteile und Bereichsminima,
scheitert der Compile vor Retrieval. Snapshot-/Freshnessprüfung erfolgt weiterhin
gegen die tatsächlich geladene Publikation vor irgendeiner Provideranfrage.
Optionale frühere Ergebnisse und Claims werden danach wie bisher begrenzt gepackt.

Kein offener Fehler wird versteckt, keine Hypothese aufgewertet, kein Profil oder
Schema-Grounding geändert. Es gibt keine neue Persistenz, keinen neuen Kontextweg
und keine zusätzliche Modellreparatur. Diese Präzisierung ändert nicht die Grenzen
des externen Recherchebudgets oder die Pflicht zur objektiven Verifikation.

## Compliance

Domain-Tests prüfen Evidenzmindestbudget, Gesamtgleichung, alle Reserven, Idempotenz
und Ablehnung echter Übergröße. Ein kleiner RepeatSchemaInPrompt-Kontext muss seine
offenen Fehlereinträge deterministisch und vollständig behalten; passende bestehende
Replan-, Secret- und Freshnessregressionen bleiben grün. Der Live-Agent wird mit
demselben Modellprofil erneut geprüft. Compile-Erfolg allein ist keine Abnahme.
