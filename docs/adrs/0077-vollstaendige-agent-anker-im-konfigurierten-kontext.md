# ADR-0077: Vollständige Agent-Anker im konfigurierten Kontext

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (ausdrückliche Vorabannahme der mit Plan 10 verbundenen ADRs)

Supersedes: [ADR-0009](0009-context-compiler.md) ausschließlich hinsichtlich starrer
Bereichsgrenzen und der Gleichsetzung von Outputreservierung mit einer Mindestfähigkeit.

## Kontext

Der echte mutierende Live-Test mit dem unveränderten Luna-Profil (16384 Kontext,
2048 Output) scheitert vor dem ersten Modellturn mit OutputCapabilityTooSmall:
3605 reservierte Tokens werden irrtümlich als Mindestoutput verlangt. Umgekehrt
muss eine konfigurierte maximale Ausgabe oberhalb dieser Reserve vollständig in
die Budgetgleichung eingehen. Kleine Kontexte skalieren außerdem Pflichtsektionen
unter die Größe ihrer vollständigen IDs und Verträge; sie dürfen nicht gekürzt werden.

## Entscheidung

ContextCompilerPolicyVersion V5 reserviert das Maximum aus der bisherigen
22-Prozent-/Referenzreserve und dem tatsächlich konfigurierten Outputlimit.
Ein kleineres Outputlimit bleibt unverändert und ist kein Ablehnungsgrund:
ungenutzte Reservierung wird nicht mit Promptmaterial gefüllt. Überschreitet die
Summe dadurch das Kontextlimit, wird ausschließlich optionales Code/Evidence-Budget
um den Überschuss reduziert. Sicherheitsreserve und Modellprofil bleiben unverändert.

Vor Retrieval zählt der Compiler den vollständigen Systemvertrag einschließlich
explizitem Schema-Grounding und den vollständigen Goal-/Ledger-Anker. Reichen die
Referenzanteile nicht, übernimmt er nur die tatsächlich zusätzlich benötigten
Tokens aus demselben Code/Evidence-Anteil. Die fünf Sektionen bleiben disjunkt,
deterministisch begrenzt und bei der abschließenden Gesamtprüfung hart validiert.
Mindestens 256 Tokens beziehungsweise der kleinere skalierte Ausgangsanteil
bleiben für Code/Evidence übrig. Passt der unveränderte Pflichtinhalt trotzdem
nicht, scheitert die Kompilierung vor jedem Provider- oder Toolaufruf.

Es gibt weder automatische Einstellungsänderung noch Tokenizerheuristik,
Inhaltstrunkierung, zusätzliche Reparaturversuche oder neue Werkzeugautorität.
Die Policyversion fließt in den bestehenden ContextDigest ein; keine Migration.

## Konsequenzen und Risiken

Konfigurierte Profile sind unabhängig vom Verhältnis Kontext/Output verwendbar,
sofern vollständige Pflichtinhalte und feste Reserven tatsächlich hineinpassen.
Mehr Pflichtinhalt verdrängt optionale Evidenz und kann zusätzliche begrenzte
Leseschritte erfordern. Große Ziele können weiterhin ehrlich zu groß sein.
Modellqualität oder Vollständigkeit eines Ergebnisses wird dadurch nicht behauptet.

## Compliance

Domainregressionen prüfen 8k/16k, niedrige und höhere Outputlimits, exakte
Gesamtrechnung, unveränderte Sicherheitsreserve, minimale Evidenz und Überlauf.
Reale Context-/Agent-Tests prüfen unveränderte Profile und Pflichtinhalte,
danach Live-Patch, echte Verifikation und unveränderte geschützte Fixturedateien.
Workspace-Tests, Clippy, Formatierung und Dokumentationslinks bleiben Pflicht.
