# ADR-0100: Quellenlokale Recherche im kontrollierten Vergleich

Status: Accepted

Datum: 2026-09-08

Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich zugehöriger ADRs.

## Befund

Der ADR-0099-Nachtest liefert die zusätzlichen Originale, beseitigt aber weder
erfundene Callee-Effekte noch ausgelassene konkrete Writer-Aufrufe. Globale
Bestandsanalyse und gleichzeitige Antwortformulierung bleiben modellabhängig.
Quellenmitgliedschaft oder ein zusätzlicher Satz im Prompt beweist keine Korrektur.

## Entscheidung

Ein expliziter nativer Vergleich ergänzt die bestehende gemeinsame Recherche um
`source-local`. Der Produktstandard bleibt `joint`. Vor der ersten Bestandsanalyse
prüft der Core einmal höchstens vier bereits im aktuellen Paket enthaltene
Originalfenster einzeln. Jeder Aufruf erhält den vollständigen Originalauftrag,
den aktuellen Core-Schritt und genau dieses unveränderte Originalfenster.

`SourceReview V1` enthält ausschließlich eine kurze Interpretation (höchstens
192 UTF-8-Bytes) und ein bis vier Originalzitate (je höchstens 256 Bytes).
Schema und unabhängiger Decoder prüfen Version, exakte Felder, Grenzen und
eindeutige Zitatpositionen im tatsächlich übergebenen Fenster. Der Core bindet
Revision, Source-ID und Range; das Modell kann sie nicht wählen. Die Zitatprüfung
belegt nur Herkunft, ausdrücklich nicht die Wahrheit der Interpretation.

Die vorhandenen Deciding-Ereignisse und der gemeinsame Controller rechnen jeden
Aufruf ab. Ein ungültiges Dokument erhält höchstens einen Repair; transiente
Retries verbrauchen dieselben vorhandenen globalen Retry-/Modellgrenzen. Keine
neuen Reads, Fragepflichten, Freigaben oder Budgets entstehen durch SourceReview.
Timeout, Cancellation und erneute Safe-Reader-Prüfung vor und nach der Antwort
verhindern die Verwendung veränderter Originale.

Danach läuft die unveränderte V7-Bestandsanalyse beziehungsweise Plansynthese.
Der Core fügt nur vollständig passende, an die aktuellen Originalfenster
gebundene Interpretationen als ausdrücklich nicht verifizierte Hinweise hinzu.
Pflichtauftrag, Originale und bestehende Quellenzulassung werden weder gekürzt
noch ersetzt. Reicht der Platz nicht, ist der Vergleich ehrlich nicht ausführbar;
es gibt keine stille Rückkehr zur Baseline oder Abschwächung der Evidence-Prüfung.

Die kurzlebigen Hinweise sind keine langlebigen Facts, Planentscheidungen oder
erledigten Fragen. Sie können keinen Recherche- oder Implementierungsabschluss
autorisieren und werden nicht als eigene Wissensschicht persistiert. Ein neuer
Abschnitt revalidiert Originale erneut. Gespeicherte Endergebnisse behalten ihre
normalen ursprünglichen Quellenbezüge und den epistemischen Typ Interpretation.

## Abnahme und Nichtziele

Strikte Schema-/Quellen-/UTF-8-/Revisionsgrenzen, höchstens ein Repair, unveränderte
Zeit-/Aufrufbudgets und tatsächliche Providerpakete bei 8k/2k sowie 16k/4k sind Pflicht.
Der echte Researcher wird mit Index, Safe Reader, libSQL und besessenem Scheduler
in Ask, Plan und Agent-Vorbereitung geprüft. Reale Modelle vergleichen dieselben
unveränderten Aufgaben in gegenbalancierter Reihenfolge; Ablauf, Kontextkosten,
Quellenversorgung und Inhaltsfehler werden getrennt ausgewertet.

Keine produktive Strategieumstellung ohne belastbaren Nutzenbeleg, keine allgemeine
Semantik-Verifikation, neue Persistenz, Abhängigkeit, Infrastruktur oder größere
Kontext-/Outputlimits. Der mutierende Agent und dessen Strategie bleiben unverändert.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
