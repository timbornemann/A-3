# ADR-0096: Originalbudget vor optionalen Metadaten

Status: Accepted

Datum: 2026-09-08

Entscheider: Tim Bornemann (Plan-10-Vorabannahme und fortgesetzter Experimentauftrag)

Supersedes: [ADR-0077](0077-vollstaendige-agent-anker-im-konfigurierten-kontext.md)
ausschließlich hinsichtlich der verbleibenden optionalen Budgetverteilung im
normalen Agententurn nach vollständigem Einpassen aller Pflichtanker.

## Befund

Die breitere öffentliche Zwei-Modul-Liveaufgabe lässt mit Lunas unverändertem
16k-/2048-Profil und wiederholtem Schema nur 256 CodeAndEvidence-Tokens übrig.
Die Hälfte nach Pflichtframing genügt nicht einmal für die kleine aktuelle
Preisfunktion samt Provenienz. Gleichzeitig bleiben optionale Projektmetadaten
und Tool-Ergebnisbereiche reserviert. Vier Vorprüfungen desselben eingefrorenen
Builds scheitern vor dem ersten Coding-Modellaufruf. Dies ist kein Modellversagen.

## Entscheidung

ContextCompilerPolicy V8 passt zunächst wie bisher alle gezählten Pflichtanker
und die unveränderten Output-/Sicherheitsreserven ein. Nach Retrieval und
gezähltem L0-Framing priorisiert sie im normalen Agententurn den bestehenden
CodeAndEvidence-Bereich. Ziel ist dessen Pflichtbedarf plus 2048 bei 16k linear
skalierte Tokens für optionale Evidence. Nur fehlender Platz wird übertragen,
zuerst aus optionaler ProjectMap, dann aus optionalen ToolResults. Jeder Donor
behält mindestens 256 beziehungsweise seinen kleineren skalierten Referenzanteil;
die tatsächlichen verpflichtenden Repositoryanker bleiben vollständig geschützt.

Die Übertragung ist verlustfrei in der Budgetgleichung und höchstens so groß wie
das vorhandene optionale Donorbudget. Ein nicht vollständig erreichter Zielwert
ist kein neuer Fehler: Quellen bleiben gegebenenfalls ausdrücklich ungeliefert.
Es werden keine Reserven benutzt, Pflichtanker gekürzt oder Modellprofile geändert.
Große verpflichtende Inhalte dürfen weiterhin vor Inferenz fehlschlagen.

[ADR-0093](0093-originalquellen-im-agentenkontext.md) bleibt unverändert: Originale
dürfen höchstens die Hälfte des danach verbleibenden CodeAndEvidence-Budgets
nutzen; höchstens zwei echte Safe-Reader-Seiten, strikte Frist, Freshness und
vollständige Seiten einschließlich Provenienz. Es entstehen weder weitere Reads
noch neue Recherchebudgets. Replan-Lokalisierung und Replan-Analyse erhalten
keine Umverteilung und keine automatischen Reads. Die Compilerpolicy ist im
Digest versioniert; keine DB-Migration oder Prompt-/Toolschemaänderung.

## Nachweis und Grenzen

Eine Regression reproduziert fehlenden aktuellen Code bei längerem Ziel,
wiederholtem Schema und offener Fehlerevidence. V8 muss den Originalkörper
liefern, alle Pflichtanker und Reserven erhalten und deterministisch bleiben.
Domainregressionen prüfen 8k/16k, niedrige/hohe Ausgabe, exakte Kostenerhaltung,
begrenzte Donoren, Idempotenz und nicht erreichbare Zielbudgets. Bestehende
Stale-/Secret-/Replanverträge bleiben Pflicht. Live-Nachläufe verwenden die
unabhängigen Zusatzorakel der V2-Abnahme und unveränderte Profile.

Mehr gelieferter Code beweist weder Modellzuverlässigkeit noch Taskabschluss.
Die gestuften Strategien bleiben experimentell; ihre Produktübernahme und die
Gesamtabnahme von Ask, Plan und Agent sind hier nicht beschlossen.

Referenzen: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md),
[Live-Coding V2](../../fixtures/agent-live-coding-v2/README.md).
