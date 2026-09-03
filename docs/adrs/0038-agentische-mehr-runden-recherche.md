# ADR-0038: Agentische Mehr-Runden-Recherche

Status: Accepted

Datum: 2026-09-03

Entscheider: Tim Bornemann

Supersedes: Die Begrenzung auf genau eine zusätzliche Ask-Rechercherunde aus ADR-0037. Die dort
festgelegten Index-, Evidence-, Source-Preview-, Persistenz- und Sicherheitsgrenzen bleiben gültig.
ADR-0010 bleibt für mutierende Agentenläufe unverändert maßgeblich.

## Kontext

ADR-0037 machte Ask nachvollziehbar und erlaubte bei fehlender Evidence genau eine zusätzliche
Read-only-Runde. Reale Repositoryfragen benötigen jedoch häufig eine Beweiskette: erst ein Symbol
lokalisieren, anschließend seine Aufrufer oder Tests finden und schließlich die relevanten
Quellbereiche lesen. Plan und Agent-Vorbereitung verwendeten diesen Rechercheweg bislang gar nicht.
Dadurch endeten schwierige Fragen zu früh oder starteten Planung und Umsetzung mit vermeidbaren
Evidence-Lücken.

Eine offene Werkzeugschleife wäre keine zulässige Lösung. Sie wäre für kleine lokale Modelle nicht
vorhersehbar, könnte Ressourcen unbeschränkt verbrauchen und würde den sicheren Agent-Controller
mit einer Conversation-Schleife vermischen. Ebenso wenig dürfen rohe Gedankengänge, Prompts oder
Providertranskripte als vermeintliche Transparenz gespeichert oder angezeigt werden.

## Entscheidung

- Ask, Plan und die Agent-Vorbereitung verwenden denselben Application-Use-Case für endliche,
  indexgebundene Read-only-Recherche. Der Ablauf ist `Vorbereiten → Lokalisieren → Entscheiden →
  Lesen → Auswerten → Antworten/Planen`; ausschließlich `Entscheiden → Lesen → Auswerten` darf
  wiederholt werden.
- `Standard` erlaubt höchstens sechs Modellentscheidungen, zwölf Read-Aktionen und fünf Minuten.
  `Gründlich` erlaubt höchstens zwölf Modellentscheidungen, 24 Read-Aktionen und fünfzehn Minuten.
  Beide Profile behalten höchstens 200 Source-Referenzen und über den gesamten Abschnitt genau
  einen Reparaturversuch für ungültige strukturierte Ausgabe.
- Eine Entscheidung darf eine fertige Antwort, einen Plan, eine Rückfrage, einen begründeten
  Fortsetzungsbedarf oder eine bis vier sequenziell auszuführende Read-only-Aktionen liefern. Die
  letzte verfügbare Modellentscheidung darf keine weitere Recherche anfordern. Ein harter Timeout
  erzeugt ausschließlich einen Core-formulierten sicheren Zwischenstand aus bereits validierten
  Metadaten.
- Das geschlossene Werkzeugset besteht aus `searchIndex`, `searchSourceText`, `inspectPath`,
  `inspectSource`, `inspectRelations` und `listDirectory`. Es besitzt feste Ergebnis-, Datei-,
  Byte-, Zeilen- und Zeitgrenzen. Identische Aktionen werden nicht erneut ausgeführt, verbrauchen
  aber einen Aktionsversuch. Zwei aufeinanderfolgende Runden ohne neue Evidence enden mit einer
  Rückfrage oder expliziten Fortsetzungsanforderung.
- Jede Modellentscheidung enthält eine öffentliche strukturierte Arbeitsnotiz mit `goal`,
  `finding`, `gap` und `nextStep`. Befunde sind als Beobachtung, Hypothese oder belegte
  Schlussfolgerung klassifiziert; Beobachtungen und Schlussfolgerungen müssen gültige turnlokale
  Source-Referenzen tragen. Diese Notiz ist reine Präsentation und kann weder Aktion, Freigabe,
  Ledgeränderung noch Verification autorisieren.
- Vor jedem Modellturn wird ein `ResearchMemoryCheckpoint` deterministisch aus Nutzerfrage,
  aktuellen Source-Referenzen, öffentlichen Befunden und offenen Evidence-Lücken erzeugt. Bei
  Follow-ups oder Fortsetzungen werden Befunde nur nach erneuter Prüfung ihrer FileRevision gegen
  den aktuellen veröffentlichten Index als Fakten übernommen. Hypothesen bleiben Suchhinweise.
  Frühere Assistentenantworten und zusammengefasste Zusammenfassungen sind keine Evidence.
- Ask liefert eine belegte Antwort oder `AwaitingContinuation`. Plan recherchiert vor einer
  Rückfrage oder einer neuen unveränderlichen Planrevision. Die Agent-Vorbereitung übergibt
  ausschließlich revalidierte Quellen als typisierten `ResearchHandoff` an die bestehende
  Materialisierung und den Context Compiler.
- Der mutierende Agentenlauf bleibt der Zustandsmaschine aus ADR-0010, RunMemoryCheckpoint, Task
  Ledger, Policy, Approval, Reindex und Verification unterstellt. AgentAction V3 ergänzt lediglich
  eine öffentliche Arbeitsnotiz neben der weiterhin exakt einen typisierten Aktion pro
  Controllerturn. Die Notiz besitzt keinerlei Ausführungsautorität.
- Knowledge-Schema V31 ergänzt generische append-only Tabellen für Rechercheabschnitte,
  öffentliche Arbeitsnotizen, Ereignisse, Source-Referenzen, Notizquellen, Antwortzitate und
  interne Task-/Run-Verknüpfungen. V30 bleibt ohne Backfill lesbar. Persistiert werden nur
  begrenzte Notizen und hashgebundene Evidence-Metadaten, niemals Quelltext, Prompts,
  Modellrohantworten, Chain-of-Thought, Providerdaten oder Credentials.
- `submit_agent_message_v2` wählt `standard | thorough` pro Nachricht; V1 bleibt kompatibel und
  verwendet `Standard`. `continue_agent_research` setzt nur den letzten fortsetzbaren Abschnitt
  einer sichtbaren Session mit neuem Indexanker fort. Projektgebundene `query_agent_work_trace_*`
  Reads liefern begrenzte Metadaten, opake Cursor und die bestehende sichere Source-Vorschau.
  WebView-Requests akzeptieren keine Pfade oder internen Evidence-, Snapshot-, Provider-, Task-
  oder Run-IDs.

## Konsequenzen

### Positiv

- Schwierige Fragen können eine überprüfbare Beweiskette über Symbole, Beziehungen, Tests und
  Quellbereiche aufbauen, ohne den Read-only-Sicherheitsrahmen zu verlassen.
- Ask, Plan und Agent-Vorbereitung erhalten dieselbe Recherchequalität und dieselbe verständliche
  Live-Projektion.
- Einfache Fragen bleiben schnell; zusätzliche Kosten entstehen nur durch festgestellte
  Evidence-Lücken und bleiben durch das gewählte Profil endlich.
- Nutzer sehen Ziel, Befund, Lücke, nächsten Schritt und Quellen, ohne internes Chain-of-Thought
  offenzulegen oder als Autorität zu behandeln.

### Negativ

- Die Conversation-Laufzeit und ihre Persistenz erhalten mehr Zustände und benötigen zusätzliche
  kleine append-only Writes.
- Gründliche Recherche kann merklich länger dauern und mehrere Modellaufrufe verursachen.
- Fortsetzungen müssen historische Befunde gegen einen möglicherweise neuen Index revalidieren.
- Der Context Compiler verwendet Policy V4: Er bindet den typisierten Research-Handoff in seinen
  Digest ein, führt höchstens 64 eindeutige aktuelle Quellpfade als exakte Retrieval-Seeds mit und
  verwirft nicht mehr passende FileRevisions vor dem nächsten Agent-Turn.

### Risiken und Gegenmaßnahmen

- Endlosschleifen werden durch feste Modell-, Aktions- und Zeitgrenzen, Aktionsdeduplizierung und
  den Stagnationsstopp ausgeschlossen.
- Erfundene Befunde werden durch geschlossene Source-Referenzen und die Pflichtbelegung für
  Beobachtungen und Schlussfolgerungen abgewiesen.
- Öffentliche Notizen könnten irrtümlich als Steuerung dienen; Decoder, Domain und Agent-Controller
  halten sie strukturell getrennt von ausführbaren Actions und Verification.
- Ein Indexwechsel könnte Beweise vermischen; jeder Abschnitt besitzt genau einen Indexanker und
  übernimmt nur erfolgreich revalidierte Befunde.

## Verworfene Alternativen

- Unbegrenzte autonome Werkzeugnutzung: nicht ressourcen- oder sicherheitsbegrenzt.
- Ein eigener Recherchecontroller je Modus: verdoppelt Regeln und lässt Verhalten auseinanderlaufen.
- Plan und Agent durch den Conversation-Loop ersetzen: verletzt ADR-0010 und die Ledgerautorität.
- Rohes Chain-of-Thought anzeigen oder speichern: weder überprüfbare Evidence noch zulässige
  Präsentations- oder Persistenzinformation.

## Compliance

- Controllerverträge prüfen Fast Path, Mehr-Runden-Kette, Profilgrenzen, Reparatur,
  Deduplizierung, Stagnation, Timeout, Cancellation und Fortsetzung.
- Persistenztests prüfen Neuinstallation, V30→V31, atomaren Abschluss, Crash-Recovery,
  Presentation Delete, Archivierung und V30-Legacyprojektion.
- IPC- und Capabilitytests prüfen Projekt-/Session-/Cursor-Isolation und den Ausschluss privater
  Laufzeit-, Provider- und Inhaltsdaten.
- Agent-Verträge prüfen, dass Arbeitsnotizen keine Aktion oder Mutation autorisieren und pro
  Controllerturn weiterhin höchstens eine ausführbare AgentAction gilt.
- Frontendtests prüfen Tiefenwahl, progressive Timeline, Notizen, Quellen, Fortsetzung,
  Phasenwechsel, Fokusstabilität, Reduced Motion und schmale Fenster.

## Referenzen

- [ADR-0009](0009-context-compiler.md)
- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0013](0013-goal-contract-ledger-and-event-journal.md)
- [ADR-0020](0020-agent-runtime-ownership-and-pause.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
- [ADR-0030](0030-bounded-evidence-source-preview.md)
- [ADR-0033](0033-chatbasierter-agent-workspace.md)
- [ADR-0037](0037-nachvollziehbare-adaptive-ask-recherche.md)
- [Daten und Persistenz](../DATA_AND_PERSISTENCE.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Memory und Context](../MEMORY_AND_CONTEXT.md)
- [Job-Laufzeit](../JOB_RUNTIME.md)
- [Plan 06](../plans/06-DESKTOP_PRODUCT.md)
