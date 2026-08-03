# ADR-0008: Epistemisches Memory mit Evidenzinvalidierung

Status: Accepted  
Datum: 2026-08-03

## Kontext

Persistierte LLM-Zusammenfassungen können falsch oder veraltet sein. Ein Coding-Agent muss Fakten, Beobachtungen, Entscheidungen, Hypothesen und Zusammenfassungen unterscheiden und Änderungen der Codebasis berücksichtigen.

## Entscheidung

- Durable Claims besitzen einen der Typen Fact, Observation, Decision, Hypothesis oder Summary.
- Jeder Claim speichert Herkunft, Snapshot, EvidenceRefs, Confidence und Status.
- Fact erfordert deterministische Verifikation.
- Summary behält die Source IDs ihrer Grundlage.
- Datei-, Symbol- oder Tooländerungen invalidieren abhängige EvidenceRefs und Claims.
- Stale Claims werden aus Faktenbereichen des Context Pack ausgeschlossen.
- Abgeschlossene Task-Schritte werden Stale, wenn ihre Verification Evidence ungültig wird.

## Konsequenzen

### Positiv

- Modellannahmen werden nicht still zu Wahrheit.
- Memory bleibt an echter Codebasis verankert.
- lange Aufgaben reagieren korrekt auf zwischenzeitliche Änderungen.

### Negativ

- Provenienz und Invalidierung erhöhen Schema- und Rechenaufwand.
- einige Aussagen bleiben bewusst Hypothesen.

### Risiken und Gegenmaßnahmen

- zu breite Invalidierung — direkte und transitive Radien unterscheiden.
- zu enge Invalidierung — konservative NeedsReview-Markierung abhängiger Module.

## Verworfene Alternativen

- Chatverlauf als Gedächtnis — nicht strukturiert und nicht frisch prüfbar.
- periodische freie Zusammenfassung — verliert Provenienz.
- Confidence allein — numerische Sicherheit ersetzt keinen Beleg.

## Compliance

Context-Validator blockiert stale Facts. Invalidierungsfixtures decken Änderung, Löschung, Umbenennung und Parserupgrade ab.

