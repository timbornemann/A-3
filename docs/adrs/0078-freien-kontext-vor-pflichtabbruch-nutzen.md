# ADR-0078: Freien Kontext vor Pflichtabbruch nutzen

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

Supersedes: [ADR-0077](0077-vollstaendige-agent-anker-im-konfigurierten-kontext.md)
ausschließlich hinsichtlich des alleinigen Code/Evidence-Spenders für Pflichtanteile.

## Kontext

Der reale Luna-Nachtest mit unverändertem RepeatSchemaInPrompt enthält 823 Bytes
Systemtext und 6681 Bytes Schema-Grounding. Nur Code/Evidence als Spender zuzulassen
verwirft ein solches Pflichtpaket, obwohl im gesamten konfigurierten Kontext noch
ungenutzter Platz und optionale Project-Map-/Toolresultatanteile verfügbar sind.
Die explizit gewählte Schemawiederholung darf nicht still entfernt werden.

## Entscheidung

Die noch nicht abgenommene Context-Policy V5 behält die vollständigen Pflichtanteile
und unveränderten Reserven aus ADR-0077. Zusätzlich benötigte Pflichttokens nutzen
zuerst bisher nicht zugeteilten Platz innerhalb desselben Kontextlimits. Danach
verdrängen sie in fester Reihenfolge optionale Code/Evidence-, Project-Map- und
Toolresultatanteile. Jeder dieser Bereiche behält mindestens 256 Tokens oder seinen
kleineren skalierten Referenzanteil. Reichen diese Quellen nicht aus, scheitert die
Kompilierung vor Retrieval. Keine Referenzreservierung wird als tatsächliche
Modellausgabe ausgegeben, kein Inhalt über seine spätere Bereichsgrenze gepackt.

Die Spenderreihenfolge ist deterministisch; optionale Inhalte werden wie bisher
mit expliziter Trunkierungsinformation gepackt. Pflicht-L0 oder notwendige dauerhafte
Memory-Anteile, die danach nicht hineinpassen, dürfen weiterhin nicht gekürzt werden.
Diese Entscheidung behauptet keine unbegrenzte Nutzbarkeit großer Ziele in 8k.
Profil, Outputcap, Sicherheitsreserve, Source-/Action-Validierung und Freigaben
bleiben identisch. Es gibt keine DB- oder Tool-Schemamigration.

## Compliance

Budgettests prüfen die exakte Gesamtgleichung, feste Spenderreihenfolge, Minima,
Idempotenz, deterministische Ablehnung und unveränderte Sicherheits-/Outputreserve.
Kontexttests verwenden sowohl FormatFieldOnly als auch RepeatSchemaInPrompt und
prüfen unveränderte Profile und vollständige Pflichtanker. Der reale Live-Agent
wird nach der Korrektur erneut geprüft; ein bloßer Compile-Erfolg genügt nicht.
