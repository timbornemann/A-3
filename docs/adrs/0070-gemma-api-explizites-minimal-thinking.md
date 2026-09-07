# ADR-0070: Explizites minimales Thinking für die gehostete Gemma-4-API

Status: Accepted\
Datum: 2026-09-07\
Freigabe: Fortgesetzter Plan-10-Auftrag einschließlich gezielter Modelltestkorrekturen.

## Kontext

Nach der getrennten Schema-Korrektur aus [ADR-0068](0068-gemini-schema-arraygrenzen.md)
liefert die reale Google-Gemma-Recherche weiterhin OutputLimit statt vollständiger
Dokumente. Der Adapter überlässt Thinking bislang dem Providerdefault. Ob dieses
Default die Abbrüche verursacht, ist noch nicht nachgewiesen; die Änderung darf
deshalb nicht ohne den kontrollierten Live-Nachtest als Abhilfe gelten.

[Google dokumentiert](https://ai.google.dev/gemma/docs/core/gemma_on_gemini_api)
für exakt `gemma-4-26b-a4b-it` und `gemma-4-31b-it` das Wire-Feld
`generationConfig.thinkingConfig.thinkingLevel`: `minimal` deaktiviert Thinking,
`high` aktiviert es. A^3 benötigt für seine kurzen, getrennten Research-Phasen
sichtbare validierbare Dokumente innerhalb der konfigurierten Ausgabegrenze.

## Entscheidung

Der Gemini-Adapter setzt für diese zwei exakten IDs, nach seiner bestehenden
Entfernung des optionalen `models/`-Präfixes, explizit `thinkingLevel: minimal`.
Dies gilt identisch für Capability-Probe und normale Generierung. Andere Modelle,
ähnliche Namen, Aliase und Embeddings ändern sich nicht. Die Abbildung verbleibt
gemäß [ADR-0018](0018-model-provider-port-ownership.md) im Adapter; sie bestätigt
keine Fähigkeit anhand eines Modellnamens. Reale Probe und unabhängige Decoder
bleiben erforderlich. Ein frei konfigurierbares Thinking-Profil ist nicht Teil
dieses Schnitts.

Keine Vergrößerung von Probe-, Kontext-, Output-, Retry-, Repair- oder Zeitbudget,
keine neue Berechtigung und kein Fallback auf unstrukturierten Text. Eventuell
trotzdem gelieferte `thought`-Teile werden unverändert nicht als Antwort, Evidenz
oder ausführbare Aktion weitergereicht. Abgeschnittene Antworten bleiben Fehler.

## Prüfung und Grenzen

Ein rot→grüner HTTP-Vertrag kontrolliert Probe, Stream, exakte Wirewerte,
unveränderte Tokenlimits und die Trennung des Thinking-Kanals. Negative Modellnamen
und unveränderte andere Gemini-Requests werden unabhängig geprüft. Der echte
Nachtest nutzt dieselbe öffentliche Fixture, dasselbe Modell und dieselben
Profilgrenzen; Laufzeit und formale wie inhaltliche Resultate werden getrennt
ausgewertet. Ein nicht belegter Effekt wird als solcher dokumentiert.

Der deaktivierte zusätzliche Denkmodus kann die Antwortqualität beeinflussen.
Deshalb ist ein formaler Stop allein keine Abnahme und die gesamte inhaltliche
Praxisabnahme von Plan 10 bleibt unabhängig offen.
