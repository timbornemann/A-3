# ADR-0097: Großzügigere optionale Codeversorgung

Status: Accepted

Datum: 2026-09-08

Entscheider: Tim Bornemann (ausdrückliche Vorgabe großzügigerer Budgets im laufenden Vergleich)

Supersedes: [ADR-0096](0096-originalbudget-vor-optionalen-metadaten.md) ausschließlich
für den numerischen Zielwert der optionalen CodeAndEvidence-Priorisierung.

## Entscheidung

Die neue V8-Budgetpriorisierung verwendet 4096 optionale Referenztokens bei 16k,
linear skaliert also 2048 bei 8k, zusätzlich zum tatsächlich gezählten Pflichtbedarf
dieser Sektion. Das ist ein möglichst erreichbares Ziel, keine neue Obergrenze für
bereits größere Codebereiche. Ungenutzter Platz muss nicht künstlich gefüllt werden.

Die begrenzte Übertragung aus optionalen Metadaten, Mindestanteile der Donoren,
vollständige Pflichtanker, Output-/Sicherheitsreserve, exakte Tokenzählung,
Replan-Ausnahme und alle Grenzen der Originalmaterialisierung bleiben unverändert.
Das tatsächliche Gesamtbudget darf nie überschritten werden. Ein unzureichender
Donorpool bleibt als teilweise unerreichter Zielwert zulässig, nicht als Vorwand
für neue Modellaufrufe oder eine gelockerte Sicherheitsgrenze.

## Nachweis

Die Änderung wird zusammen mit V8 vor dessen erstem Live-Nachlauf geprüft.
8k/16k-Domainregressionen, tatsächliche Context Compiles, negative Budget- und
Freshnessfälle sowie die unabhängigen V2-Liveorakel bleiben Pflicht. Die bekannten
Modellprofile bleiben unverändert; die Laufberichte nennen tatsächliche Lieferung
und Erfolg getrennt. Eine pauschale Erhöhung von Turn-, Zeit- oder Repairbudgets
ist damit nicht beschlossen.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
