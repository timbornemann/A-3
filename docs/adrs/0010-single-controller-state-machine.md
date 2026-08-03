# ADR-0010: Einzelner Agentencontroller mit endlicher Zustandsmaschine

Status: Accepted  
Datum: 2026-08-03

## Kontext

Offene ReAct-Schleifen können Ziele verlieren, Werkzeuge wiederholen oder unkontrolliert mutieren. Mehrere Agenten erhöhen bei kleinen Modellen Prompt- und Koordinationskosten.

## Entscheidung

- V1 verwendet genau einen Controller pro Run.
- Zustände: Intake, Localize, Plan, Execute, Verify, Replan, AwaitApproval, Done, Failed und Cancelled.
- Pro Turn ist höchstens eine Tool Action ausführbar.
- Pro Worktree ist höchstens eine Mutation gleichzeitig erlaubt.
- Jeder Zustandsübergang wird validiert und als RunEvent gespeichert.
- Budgets begrenzen Turns, Toolcalls, Zeit, Modelltoken und Reparaturversuche.
- Abschluss ist nur nach aktueller Verifikation der Muss-Kriterien erlaubt.

Kernwerkzeuge:

- search
- inspect
- apply_patch
- run
- update_ledger
- finish

Weitere interne Queries werden nicht automatisch zu Modellwerkzeugen.

## Konsequenzen

### Positiv

- vorhersehbarer, testbarer Ablauf
- geringer Prompt- und Koordinationsaufwand
- klare Stelle für Policy, Budget und Abbruch

### Negativ

- weniger flexible freie Planung
- komplexe Aufgaben benötigen explizites Replan.

### Risiken und Gegenmaßnahmen

- Zustandsmaschine wird zu starr — Zustände fachlich grob halten, Aktionen typisieren.
- Endlosschleifen über Replan — harte Budgets und Fortschrittsdetektor.

## Verworfene Alternativen

- Multi-Agent-Orchestrierung — V1-Komplexität ohne gesicherten Nutzen.
- unbeschränkte ReAct-Schleife — schwer zu sichern und zu testen.
- Modell erzeugt komplettes Skript — zu große Ausführungs- und Fehlerdomäne.

## Compliance

Property-Tests prüfen zulässige Übergänge und terminale Zustände. Policy blockiert zweite parallele Mutation.

