# ADR-0114: Chatgebundene Agentenausführung und konkrete Haltegründe

Status: Accepted  
Datum: 2026-09-21  
Entscheider: Tim Bornemann

Freigabe: Der Auftraggeber hat alle für den Agentenumbau erforderlichen ADRs ausdrücklich
angenommen und verlangt eine tatsächlich arbeitende Greenfield-Ausführung mit vollständiger
Arbeitsanzeige im Chat.

Ersetzt: die Inspector-Layoutentscheidung aus
[ADR-0033](0033-chatbasierter-agent-workspace.md). Dessen Capability-, Autoritäts-,
Scheduler-, Kontext- und Sicherheitsentscheidungen bleiben unverändert.

## Kontext

Der Agent kann Goal, Ledger, Run, Journal, Inspection, Approval und Evidence bereits als getrennte
Core-Projektionen führen. Die Produktoberfläche verteilt den Arbeitsablauf jedoch weiterhin auf
die rechte Tab-Leiste `Fortschritt`, `Änderungen` und `Review`. Terminale Fehler erscheinen dadurch
hauptsächlich als Status in der linken Sessionliste; die laufende Unterhaltung zeigt weder den
konkreten Haltepunkt noch den vollständigen nachweisbaren Arbeitsstand.

Zudem reduziert der Desktop-Executor unterschiedliche Fehler des ersten Coding-Turns auf
`Unavailable`. Die Session erhält dann nur einen Verweis auf den Inspector. Ein Nutzer kann nicht
erkennen, ob das Modell keine gültige Aktion lieferte, ein Anker stale war, eine Freigabe fehlt oder
eine sichere Werkzeugausführung scheiterte. Das erschwert besonders Greenfield-Aufträge, bei denen
vor der ersten Patch-Aktion noch keine Datei existiert.

## Entscheidung

- Die zentrale Unterhaltung ist die einzige normale Arbeitsprojektion des Agent-Modus. Die rechte
  Inspector-Spalte und ihre drei Tabs entfallen.
- Nach dem Nutzerauftrag zeigt der Chat eine begrenzte, Core-abgeleitete Ausführungskarte mit
  aktuellem Ziel, Controllerphase, geordneten Ledger-Schritten, aktuellem und verbleibendem Schritt,
  Journalaktivität, Patch-/Prozessbelegen, Freigaben und terminalem Ergebnis.
- Diese Karte ist eine Read-Projektion. Sessiontext, Modellantworten und UI-Zustand werden weder
  fachliche Autorität noch Erfolgsbeweis. `Done` bleibt an Step-Verifikation und Acceptance gebunden.
- Die Anzeige unterscheidet Modellentscheidung, freigegebene beziehungsweise ausgeführte Aktion,
  Dateiwirkung und Verifikation. Sie zeigt begrenzte öffentliche Rationale und Statusdaten, aber
  keine versteckte Chain-of-Thought, Prompts, Providerpayloads, Credentials, interne Pfade oder IDs.
- Eine exakte Freigabe wird innerhalb derselben Chatkarte bedient. Nach Abschluss folgt dort ein
  finaler Review mit nachgewiesenen Änderungen und Checks; bei Abbruch oder Fehler folgt dort ein
  konkreter sicherer Haltegrund und, soweit vorhanden, eine zulässige Recovery-Aktion.
- Der Executor bewahrt eine geschlossene, nutzergeeignete Fehlerklasse für den aktuellen Versuch.
  Provider- und Parserinhalte bleiben privat; die Session darf aber zwischen ungültiger
  Modellaktion, fehlender Laufzeitfähigkeit, stale Ankern, ungültigem Zustand und
  Fortschrittsausfall unterscheiden.
- Ein terminal abgelehnter Modellturn darf nicht nur den Run auf `Failed` setzen und als technisch
  erfolgreich ausgeführten Schedulerjob erscheinen. Der konkrete geschlossene Ablehnungsgrund wird
  in die Session-Synchronisierung übernommen.
- Greenfield-Fähigkeit wird zusätzlich mit dem real konfigurierten Coding-Modell in einem isolierten
  leeren Git-Worktree geprüft. Erfolg verlangt reale Add-Patches, Reindex, exakte Prozessfreigabe,
  echte lokale Tests und eine unabhängige physische Nachprüfung.
- Der geschlossene manifestfreie Python-Adapter leitet bei genau einem belegten Testverzeichnis
  den relativen Startpfad ab und verwendet `python -B -m unittest discover -s <test-root>`.
  Dadurch hängt rekursive Testentdeckung nicht von einer sachlich unnötigen `__init__.py` ab.
  Mehrere, nicht-UTF-8- oder anderweitig ungültige Testwurzeln liefern weiterhin keinen Befehl.
- Auf Windows gehört `SYSTEMROOT` zur expliziten, unveränderlichen Umgebung jedes entdeckten
  Direktprozesses. Ohne diesen Hostanker kann Winsock trotz verweigertem externem Netzwerk nicht
  einmal einen lokalen Socketprovider initialisieren. Andere Umgebungsvariablen bleiben nach
  `env_clear` ausgeschlossen; die erlaubte Variable wird weiterhin im exakten ProcessSpec und
  seiner Freigabe gebunden.

## Konsequenzen

Die Unterhaltung zeigt nicht mehr nur, dass ein Lauf existiert, sondern den nachweisbaren Weg von
Plan zu Dateien und Verifikation. Fehler werden lokalisierbar, ohne rohe Provider- oder
Repositoryinhalte in die Session zu kopieren. Die frühere breite Desktopaufteilung wird einfacher;
große Verläufe benötigen weiterhin begrenzte Listen und progressive Details.

## Compliance

- Rust-Regressionen prüfen die geschlossene Fehlerklassifikation und konkrete Sessionmeldungen.
- Ein opt-in Luna-Fixture erstellt in einem leeren Repository den kleinen Python-Hello-World-Server
  und besteht echte `unittest`- sowie unabhängige Verhaltensprüfungen.
- Svelte-Komponententests prüfen, dass keine Inspector-Tabs mehr existieren und laufende,
  wartende, fehlgeschlagene sowie abgeschlossene Arbeit mit Schritten, Aktionen, Änderungen,
  Verifikation und Freigabe innerhalb des Chatverlaufs sichtbar ist.
- IPC bleibt begrenzt und pfadlos; die UI erhält keine neue Mutationsfähigkeit.

## Referenzen

- [ADR-0010](0010-single-controller-state-machine.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0021](0021-bounded-agent-inspection.md)
- [ADR-0022](0022-task-bound-approval-center.md)
- [ADR-0033](0033-chatbasierter-agent-workspace.md)
- [ADR-0088](0088-journalgebundene-ausfuehrungsrueckmeldung.md)
- [ADR-0112](0112-nachgelagerte-greenfield-verifikation.md)
- [Plan 17](../plans/17-CHAT-AGENT-EXECUTION.md)
