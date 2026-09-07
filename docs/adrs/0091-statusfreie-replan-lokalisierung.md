# ADR-0091: Statusfreie Replan-Lokalisierung

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

Supersedes: Ausschließlich die Beibehaltung von AgentAction V4 für neue
Replan-Leseturns in [ADR-0087](0087-agentaktionen-ohne-modellstatusnotiz.md).
Der gesonderte Research-V5-Analysevertrag aus
[ADR-0048](0048-rungebundene-replan-recherche.md) bleibt zunächst unverändert.

## Befund

Normale Agentturns verwenden V5 ohne modellgenerierte Statusnotiz. Sobald ein
fehlgeschlagener Test Replan erfordert, schalten Prompt und Decoder jedoch auf
V4 mit sechs zusätzlichen Statusfeldern zurück. Der Desktop übernimmt diese
Notiz auch hier nicht als Arbeitsstand; Fortschritt und Lesequittungen kommen
aus tatsächlichen Ereignissen. Die unnötige Darstellungspflicht kann somit
weiterhin eine ansonsten zulässige Rechercheaktion verhindern. Das ist ein
nachgewiesener Vertragsunterschied, kein rückwirkender Beweis für einen
bestimmten bislang unaufgeschlüsselten Live-Modellfehler.

## Entscheidung

Neue Replan-Lokalisierung verwendet den bestehenden AgentAction-V5-Umschlag
mit genau `schema_version` und `action`, beschränkt auf Search und Inspect.
`public_note` ist verboten, nicht optional oder still entfernt. Prompt,
Providerschema, wiederholtes Schema und unabhängiger Decoder müssen denselben
Vertrag verlangen, auch im einzigen Repair. Die Auswahl von Replan bleibt
Core-eigen; das Modell kann dadurch keine Phase oder Berechtigung wechseln.

V1–V4-Schemas und deren explizite historische Decoder bleiben unverändert.
Die separate Replan-Analyse bleibt Research V5, nicht AgentAction V5.
Keine historische Rohantwort wird neu als aktuelle Aktion interpretiert.
Run-Abrechnung, vier dauerhafte Reads, kanonische Duplikatprüfung einschließlich
mehrdeutiger Legacy-Claims, Original-/Snapshot-Freshness und echte Verifikation
bleiben unverändert. Auch ein korrektes Read-Dokument ist kein Abschlussbeleg.

Kein zusätzlicher Repair, Modellaufruf, Promptbudget, Index, Persistenzformat,
Providerprofil oder UI-Status aus Modellprosa wird eingeführt. Der separate
Befund zum Verlust eines Analyse-Belegbedarfs wird hier ausdrücklich nicht
durch das Speichern freier Statusprosa scheinbar behoben.

## Nachweis

Rot→Grün für V5-Read ohne Notiz; strikte Ablehnung von Notizen, falscher Version
und allen Nicht-Leseaktionen primär und nach Repair. Bestehende Duplikat-,
Legacy-Claim-, Budget- und Quellenregressionen müssen mit aktuellen Dokumenten
weiterhin bestehen. Der reale Context- und Providerpfad prüft das projizierte
Schema, die vollständige Zählung und unveränderte historische Schemas.
Schemaumfang wird reproduzierbar vor und nach Projektion gemessen; daraus folgt
kein Geschwindigkeitsversprechen. Vollständige lokale Qualitätsgates und echte
Coding-Nachtests bleiben erforderlich. Einzelne erfolgreiche Modellläufe
ersetzen keine zuverlässige Ask-/Plan-/Agent-Abnahme.

Referenzen: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md),
[ADR-0089](0089-replan-leseduplikate-im-einzelrepair.md),
[ADR-0090](0090-wertgebundene-claim-leseidentitaeten.md).
