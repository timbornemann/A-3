# ADR-0105: Originalgebundene Entwurfsbasis im kontrollierten Vergleich

Status: Accepted\
Datum: 2026-09-08\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich automatischer ADR-Annahme.\
Supersedes: ausschließlich im expliziten nativen Vergleich die Priorisierung von
Interpretationstext vor optionalen Originalen aus [ADR-0064](0064-budgetierte-bestandsuebergabe-an-entwuerfe.md).

## Befund und Hypothese

Im [Befehlsvergleich](../plans/10-REQUEST_COMMAND_VALIDATION.md) enthält Granites
Bestandsinterpretation bereits den künftigen falschen Namen `import`; weitere
Modelle unterstellen Persistenz aufgrund eines Methodennamens. Diese Interpretationen
werden nachfolgenden Entwurfsphasen erneut vorgelegt. Mehr Hinweise hatten keinen
allgemeinen Nutzen. Geprüft wird deshalb die Informationsübergabe zwischen Phasen,
nicht eine neue Selbstauskunft des Modells über die Richtigkeit seiner Aussagen.

## Entscheidung

Ein geschlossener nativer Vergleich wählt `interpretations` (Produktstandard) oder
`originals`. Nur im festen Core-Planvertrag und seinen Design-/DesignTests-Phasen
ersetzt `originals` den Text vorausgesetzter Bestandsinterpretationen durch eine
explizite Kennzeichnung. Originalauftrag und vollständige vorausgesetzte
DesignDecision-Ergebnisse bleiben unverändert. Der dauerhafte Prüfstand wird
weder umgeschrieben noch zum Fakt befördert; Ask und die erste Bestandsanalyse
bleiben unverändert.

Vor dem Modellaufruf muss jeder ersetzte Belegbereich in einem tatsächlich
gelieferten Originalfenster derselben Revision vollständig enthalten sein:
Bytebereich und Zeilen-/Spaltenbereich müssen passen. Cache-Zugehörigkeit,
Dateiname oder ein Quellenverweis genügen nicht. Der bestehende Compiler bleibt
deterministisch und begrenzt; kann seine Auswahl die Voraussetzung nicht erfüllen,
endet der Vergleich mit ContextLimit, ohne stillen Rückfall auf Interpretationstext,
zusätzliche Reads, Repairs, Aufrufe oder angehobene Budgets. Geteilte Fenster,
die erst zusammen einen erforderlichen Bereich decken, gelten konservativ als
nicht ausreichend. Bestehende Revisionsprüfungen vor und nach Reasoning bleiben.

Die Auswahl kommt ausschließlich aus dem nativen Testmodell. Die App liest keine
neue Umgebungsvariable und erhält keine neue Einstellung oder WebView-Fähigkeit.
Keine Schema-, Speicher-, Berechtigungs- oder Controlleränderung. Der Vergleich
beweist allenfalls einen Nutzen für die geprüften Fälle, keine semantische Wahrheit.

## Abnahme

Regressionen verlangen verlustfreie Original-/Designübergabe, unveränderten
dauerhaften Stand, deterministische Pakete, Ablehnung fehlender/gekürzter/fremder
Revisionen sowie echte 8k-Mehrmodus-Reader-/Storage-Verträge. Modellvergleiche
halten Profil, Originalauftrag, Rubrik und Rechte konstant; lokale Modelle laufen
nacheinander. Produktübernahme erfordert einen gegenbalancierten inhaltlichen
Nutzenbeleg, nicht allein weniger Bytes oder bestandene Wortprüfungen.
