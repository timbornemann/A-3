# ADR-0086: Gemini-Projektion von Vielfachheitsregeln

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

Ergänzt: [ADR-0027](0027-google-gemini-model-provider.md),
[ADR-0068](0068-gemini-schema-arraygrenzen.md).

## Kontext

Google Gemma besteht die Probe, aber der aktuelle Agentturn endet mit Rejected.
AgentAction V4 enthält für Flow-Seitenoffsets `multipleOf: 50`. Der Gemini-Adapter
weist dieses Schlüsselwort bereits bei der lokalen Wire-Übersetzung ab, bevor
eine HTTP-Anfrage entsteht. Der bisherige Schema-Vertrag testet nur Agent V1–V3.
Dies erklärt einen deterministischen lokalen Ablehnungspfad; daraus wird kein
HTTP- oder Safetygrund der früheren Research-Läufe abgeleitet.

## Entscheidung

Die vorhandene explizite Gemini-Schemaprojektion lässt zusätzlich das bekannte
Validierungskeyword `multipleOf` in Schema-Knoten weg, wie schon `pattern` und
Längengrenzen. Typ, Minimum, Maximum, erforderliche Felder, Varianten und andere
verbleibende Regeln bleiben erhalten. Const-/Enum-Literaldaten werden nicht verändert.
Unbekannte Keywords bleiben abgewiesen; kein allgemeines Ignorieren entsteht.

Das originale AgentAction-V4-Schema, der unabhängige Decoder und die typisierte
Flow-Auswahl behalten die exakte 50er-Teilbarkeit sowie alle Bereichsgrenzen.
Die Projektion erteilt keine Read-/Schreibfreigabe, führt keine Modellantwort aus
und verändert weder Profile, Kontext-/Outputbudgets, Retry noch Einzelrepair.
Andere Provider und persistierte Schemas bleiben unverändert.

## Nachweis und Grenzen

Ein Regressionstest muss zuerst mit dem vollständigen aktuellen V4-Schema lokal
scheitern. Danach prüft derselbe Vertrag die Übersetzung und unabhängig gültige
sowie ungültige Flow-Offsets. Der echte HTTP-Vertrag muss das vollständige aktuelle
Schema durch den Adapter schicken. Bestehende V1–V3-, Research-V5–V7-, Deep-Map-,
Timeout-, Cancellation- und Secret-Verträge bleiben erhalten. Anschließend folgt
ein echter Google-Gemma-Agentnachtest mit unverändertem Profil und öffentlichen
Fixture-Dateien; lokale Übersetzbarkeit allein ist kein Modell-Erfolgsnachweis.

Referenz: [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
