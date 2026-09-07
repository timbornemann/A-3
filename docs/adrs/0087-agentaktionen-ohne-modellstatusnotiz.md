# ADR-0087: Agentaktionen ohne Modellstatusnotiz

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

Supersedes: Die Pflicht zur modellgenerierten öffentlichen Notiz aus
[ADR-0038](0038-agentische-mehr-runden-recherche.md) ausschließlich für neue normale
AgentAction-V5-Turns. Historische V3/V4 und gesonderte Replan-Phasen bleiben erhalten.

## Befund

Nach Behebung der lokalen Gemini-Schemaablehnung liefert Google Gemma eine Antwort,
die vor Prüfung ihrer Aktion an InvalidPublicNote scheitert. Ziel, Befundart, Befund,
Quellen, Lücke und nächste Aktion werden vom Modell als zusätzliche sechs Felder
verlangt. Der ausführende Desktop konsumiert diese AgentTurnExecution-Notiz nicht;
sein wirklicher Fortschritt stammt bereits aus Run-Journal und Task Ledger. Die
Notiz verleiht keinerlei Autorität, kann aber den gesamten Turn verhindern.
Die eigentliche Aktion dieses Live-Laufs ist nicht als gültig nachgewiesen.

## Entscheidung

Neue normale Agentturns verwenden das streng versionierte AgentAction-V5-Schema
mit genau `schema_version` und `action`. Das ausführbare Aktionsset und sämtliche
Felder und Grenzen innerhalb einer Aktion bleiben gegenüber V4 unverändert.
`public_note` ist in V5 verboten, nicht optional oder still ignoriert. Es werden
keine Status- oder Faktenbehauptungen aus Modelltext als Ersatz erzeugt.

Die vorhandene Core-Projektion von aktuellem Schritt, Runzustand, tatsächlichen
Read-/Mutations-/Prüfereignissen und Freigabestatus bleibt die Fortschrittsquelle.
Ein vorgeschlagener oder zugelassener Aktionswert ist kein Ausführungsnachweis;
nur tatsächliche Toolresultate und Verifikation dürfen den Fortschritt bestätigen.
Die Legacy-Notiz-API bleibt für V3/V4 erhalten, neue V5-Turns liefern keine Notiz.

V1–V4 behalten eigenständige strikte Decoder und Schemas. Replan-Localization bleibt
im bisherigen read-only V4-Vertrag, Replan-Analyse bei ihrem gesonderten V5-
Recherchevertrag; Ask/Plan/Vorbereitung behalten Research V7. Gleiche numerische
Versionen verschiedener Vertragstypen sind keine gegenseitige Zulassung.

Aktuelle ID-Konstanten, unabhängige Anker-/Snapshot-/Patch-Prüfungen, Einzelrepair,
Policy, Approval, Runbudgets, Freshness und echte Verifikation bleiben unverändert.
Der Compiler zählt das vollständige tatsächliche V5-Schema und den kürzeren
Systemvertrag. Kein größeres Profil, kein zusätzlicher Modellaufruf, kein neuer
Index, keine Migration, keine UI- oder Laufzeitarchitekturänderung entsteht.

## Nachweis

Rot→Grün-Tests verlangen V5 ohne Notiz, verbotene Zusatzfelder, unverändert strenge
V3/V4-Notizen und das identische ausführbare Aktionsschema. Primär-/Repair-,
Snapshot-/Anker-, echte Read-/Mutations- und Replan-Verträge müssen bestehen.
Providerverträge umfassen explizit historische und aktuelle Schemas. Das öffentliche
Live-Fixture wird unverändert mindestens mit Google Gemma und Luna nachgetestet;
ein kleinerer Vertrag allein beweist keinen Modell- oder Aufgabenerfolg.

Referenzen: [ADR-0071](0071-core-eigene-recherchestatusangaben.md),
[ADR-0083](0083-bekannte-schrittidentitaeten-als-schemakonstanten.md),
[ADR-0085](0085-patch-snapshotkonflikte-im-einzelrepair.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
