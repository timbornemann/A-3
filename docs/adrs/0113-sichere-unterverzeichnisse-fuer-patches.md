# ADR-0113: Sichere Unterverzeichnisse für strukturierte Patches

Status: Accepted  
Datum: 2026-09-21  
Entscheider: Tim Bornemann

Freigabe: Der Auftraggeber hat alle für den Agentenumbau erforderlichen ADRs ausdrücklich
angenommen und die Fortsetzung bis zu einem funktionsfähigen Agenten beauftragt.

## Kontext

Der E3-Workspace-Adapter kann neue Dateien bisher nur unter bereits vorhandenen Verzeichnissen
anlegen. Ein leerer Worktree akzeptiert deshalb zwar `server.py`, lehnt aber denselben sicheren
Patch bei `tests/test_server.py` ab, weil `tests/` noch nicht existiert. Ein Greenfield-Agent kann
damit keine übliche Projekt- oder Teststruktur als eine gebundene PatchAction erzeugen.

Ein generisches Verzeichniswerkzeug oder vorab ausgeführtes `mkdir` würde eine zweite, nicht an
den Patch-Digest und sein Change-Set gebundene Mutation einführen. Die Root-, Symlink-,
No-Replace- und Partial-Change-Invarianten müssen auch beim Erzeugen von Elternverzeichnissen
erhalten bleiben.

## Entscheidung

- `Add` und `Move` dürfen fehlende Elternverzeichnisse innerhalb der kanonischen ausgewählten
  Worktree-Wurzel während `apply` erzeugen. `preview` verändert weiterhin nichts.
- RepositoryPath-Validierung und zweimaliger Preflight prüfen alle bereits vorhandenen
  Komponenten. Symlinks und Windows-Reparse-Points werden sowohl vor als auch unmittelbar vor
  der Mutation abgelehnt.
- Neue Komponenten werden einzeln von der Wurzel abwärts mit `create_dir` erzeugt. Eine
  zwischenzeitlich fremd erzeugte Komponente muss ein echtes Verzeichnis sein; Links werden
  abgelehnt und Dateien führen zu einem Konflikt.
- Neue Dateiinhalte werden in der bereits vorhandenen kanonischen Wurzel gestaged. Die endgültige
  Installation bleibt atomar `no replace`; vorhandene Ziele werden nie überschrieben.
- Scheitert die Datei- oder Move-Operation, entfernt der Adapter ausschließlich die von dieser
  Operation gerade erzeugten und noch leeren Verzeichnisse in umgekehrter Reihenfolge. Fremde
  oder inzwischen nicht leere Verzeichnisse bleiben unberührt.
- Verzeichnisse sind keine eigenständigen Modelloperationen und keine eigenständige
  Erfolgsevidence. Ein erfolgreicher Dateipfad wird durch das bestehende PatchChangeSet belegt;
  nach einer sichtbaren Teilwirkung gelten weiterhin die vorhandenen Partial-Change-Verträge.

## Konsequenzen

Greenfield-Patches können reale Quell-, Paket- und Testbäume in einer einzigen überprüfbaren
Aktion anlegen. Die bestehende Root- und No-Replace-Grenze bleibt erhalten. Leere Verzeichnisse
ohne zugehörige erfolgreiche Datei sind weiterhin kein unterstütztes Agentenergebnis.

## Compliance

- Der öffentliche Workspace-Patchvertrag prüft Vorschau und Anwendung eines mehrstufig fehlenden
  Elternpfads.
- Ein nach der Vorschau eingeschleuster Symlink beziehungsweise Junction muss die Anwendung
  schließen und darf keine Datei außerhalb der ausgewählten Wurzel erzeugen.
- Der Greenfield-Executor-Test legt Quell- und Testdateien in einem real leeren Git-Worktree an
  und erreicht erst nach Reindex, exakter Prozessfreigabe und realem `unittest` den Zustand Done.

## Referenzen

- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0112](0112-nachgelagerte-greenfield-verifikation.md)
- [Plan 16](../plans/16-AGENT-GREENFIELD-AND-EXISTING.md)
