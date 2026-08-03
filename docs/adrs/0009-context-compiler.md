# ADR-0009: Deterministischer Context Compiler

Status: Accepted  
Datum: 2026-08-03

## Kontext

Kleine lokale Modelle verlieren bei langen, redundanten Prompts Ziel und relevante Details. Ein vollständiger Chatverlauf oder ein großer Repositorydump nutzt das knappe Kontextfenster schlecht.

## Entscheidung

- Jeder Modellturn erhält ein neu kompiliertes Context Pack.
- Der unkürzbare Anchor enthält Sicherheitsregeln, Goal Contract, aktuellen Schritt, Verifikationsstatus und Snapshot.
- Retrieval folgt exact → lexical → graph/test → memory → semantic.
- Inhalte werden nach Relevanz, Evidenzfrische, Graphdistanz, Tokenkosten und Redundanz gepackt.
- Mindestens 22 Prozent des Modellkontexts bleiben standardmäßig für Output reserviert.
- Bei 16.384 Tokens gelten die Budgets aus MEMORY_AND_CONTEXT.md.
- Identischer Zustand, Snapshot und RetrievalPolicy erzeugen denselben normalisierten ContextDigest.
- Nach jedem Schritt wird aktiver Kontext verdichtet, ohne Audit oder Source IDs zu löschen.

## Konsequenzen

### Positiv

- Ziel bleibt über lange Runs präsent.
- hohe Informationsdichte für kleine Modelle
- Retrieval- und Promptänderungen werden messbar.

### Negativ

- Compiler und Tokenzählung benötigen eigene Implementierung.
- ein schlechter Ranker kann trotz gutem Index falsche Inhalte auswählen.

### Risiken und Gegenmaßnahmen

- relevante Implementierung fehlt — Zoomstufen und explizite Nachladeaktion.
- Tooloutput verdrängt Code — harte Bereichsbudgets.
- Modelltokenizer differiert — Tokenizer je ModelProfile oder konservativer Fallback.

## Verworfene Alternativen

- kompletter Chatverlauf — wachsend und redundant.
- Modell entscheidet frei, was es erinnert — nicht reproduzierbar.
- nur feste Top-k-Vektortreffer — ignoriert Ziel, Graph und Tokenkosten.

## Compliance

Golden Tests prüfen Packreihenfolge, Budget, Deduplizierung, stale Ausschluss und ContextDigest.

