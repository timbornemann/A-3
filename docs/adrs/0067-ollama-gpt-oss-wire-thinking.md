# ADR-0067: Ollama GPT-OSS mit minimalem unterstütztem Wire-Thinking

Status: Accepted (Plan-10-Freigabe des Nutzers)

Datum: 2026-09-07

## Kontext

Der explizit beauftragte Live-Test von `gpt-oss:20b` endet vor der Recherche:
Die Structured-Output-Probe mit `think: false` liefert HTTP 200, Stop und leeren
Text. Ein kontrollierter Vergleich mit identischem JSON-Schema, Prompt, 4.096
Kontext und 32 Output-Tokens liefert bei `think: "low"` das erwartete Objekt.
Auch 256 Output-Tokens beheben den leeren Text bei `false` nicht.
[Ollamas Wire-Vertrag](https://docs.ollama.com/capabilities/thinking) beschreibt
für GPT-OSS die Stufen low/medium/high, nicht ausgeschaltetes Thinking.

## Entscheidung

Der Ollama-Adapter verwendet für die exakte offizielle Modellfamilie `gpt-oss`
(ohne Tag oder mit nichtleerem `:tag`) `think: "low"` in Probe und Generierung.
Andere Modellnamen behalten `false`. Keine unscharfe Teilstring-Erkennung und
kein automatisch erfundener Alias. Die Übersetzung bleibt im Adapter gemäß
[ADR-0018](0018-model-provider-port-ownership.md); es ist keine Ableitung von
Structured-Output-Fähigkeit, Kontextgröße oder Toolrechten aus Modellnamen.
Diese Fähigkeiten müssen weiterhin tatsächlich geprüft werden.

Probe-, Kontext-, Ausgabe-, Reparatur- und Zeitbudgets bleiben unverändert.
Der Thinking-Kanal wird weder als Antwort noch als Evidenz, Toolinput oder
persistierte Erinnerung weitergereicht. Eine Probe ohne exakt validiertes
sichtbares JSON bleibt nicht ausführbar. Keine neue Abhängigkeit, Settings-
Mutation oder zusätzliche Netzwerkfreigabe.

## Konsequenzen und Prüfung

GPT-OSS kann den unterstützten Wire-Vertrag nutzen, ohne Thinking-Inhalt an den
Harness auszugeben. Eigene Modellaliase sind damit nicht automatisch unterstützt;
ein späterer allgemeiner Thinking-Profilvertrag benötigt eigene Evidenz und ADR.
Offline-HTTP-Vertrag prüft Probe und Stream, unveränderte Grenzen und die Trennung
von Thinking/Text. Einheiten prüfen exakte Namen und ähnliche Nichttreffer.
Live-Probe und die echte Recherchematrix bleiben getrennte Abnahmeschritte.
