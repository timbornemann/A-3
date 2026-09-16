# ADR-0110: Leeres lokales Git-Projekt über nativen Dialog anlegen

Status: Accepted

Datum: 2026-09-16

Entscheider: Tim Bornemann

Freigabe: ausdrücklicher Auftrag zur Umsetzung des Plans „Leeres Projekt von vorn
anlegen“ einschließlich der zugehörigen Architekturentscheidung.

Ergänzt: [ADR-0029](0029-core-owned-project-catalog-and-restoration.md). Der native
Ordnerdialog bleibt der einzige Weg, einen neuen Worktree zu autorisieren. Dieses
ADR fügt das Anlegen eines leeren Git-Worktrees hinzu; `open_project` bleibt der
Pfad für bestehende Repositories.

## Kontext

A^3 konnte bisher nur bestehende lokale Git-Worktrees öffnen. Eine Idee von null
an umzusetzen erforderte, außerhalb der App ein Repository anzulegen. `OpenProject`
behandelt Nicht-Git als Fehler. Init in denselben Command zu mischen würde
bestehende Ordner überraschend initialisieren.

Die WebView darf weiterhin keinen Pfad liefern. Git-Init ist eine privilegierte
Dateisystemmutation und muss fail-closed, isoliert und ohne Nutzer-Git-Config
ablaufen. Unborn HEAD ohne Commit ist bereits ein unterstützter Repositoryzustand.

## Entscheidung

- Ein eigener pfadloser Command `create_project` öffnet genau einen nativen
  Ordnerdialog. Die WebView sendet nur `protocolVersion`.
- A^3 erzeugt den Projektordner nicht aus einem getippten Namen. Der Ordner
  entsteht im Betriebssystemdialog oder existiert bereits.
- Init läuft nur, wenn der kanonisierte Ordner existiert, wirklich leer ist und
  kein Git-Worktree ist. Unterordner eines anderen Worktrees, Bare-Repos,
  vorhandenes `.git` und nicht leere Verzeichnisse werden abgelehnt.
- Init verwendet in-process `gix` mit `Kind::WithWorktree`,
  `destination_must_be_empty: Some(true)` und `open::Options::isolated()`. Die
  Shell-`git`-Binary und `gix::init()` ohne Isolation sind verboten.
- HEAD ist unborn `refs/heads/main`. Es gibt keinen Initial-Commit, kein Remote,
  kein README und keine Scaffold-Dateien.
- Nach erfolgreichem Init folgen Inspektion, Katalog und Aktivierung dem
  bestehenden `OpenProject`-Weg.
- `open_project` bleibt unverändert: Nicht-Git bleibt `notGitRepository`.
- Dialogabbruch ist `cancelled` und ändert das Dateisystem nicht.

## Konsequenzen

### Positiv

- Nutzer können in A^3 bei null beginnen, ohne die Trust-Boundary zu erweitern.
- Open und Create bleiben getrennte, testbare Semantiken.
- Unborn `main` passt zu FR-002 und vorhandenen Inspect-Verträgen.

### Negativ

- Ein Ordner mit beliebigen Dateien, einschließlich OS-Metadaten, wird nicht
  initialisiert. Der Nutzer muss einen leeren Ordner wählen.

### Risiken und Gegenmaßnahmen

- Stilles Init in einem vollen Codeordner — eigener Command und Leer-Check.
- Nested Git in einem bestehenden Worktree — Inspect- und Ancestor-`.git`-Check.
- User-`init.defaultBranch` — isolierte gix-Öffnung, HEAD bleibt `main`.
- `gix::create::into` legt fehlende Verzeichnisse an — Init nur nach
  `PathPolicy::from_selected_root` auf einem existierenden Ordner.

## Verworfene Alternativen

- `open_project` initialisiert leere Nicht-Git-Ordner — überraschende Mutation.
- Projektname und Pfad aus der WebView — widerspricht ADR-0029.
- Shell-`git init` — argv-Prozess ohne Notwendigkeit; gix ist vorhanden.
- Template, README oder Initial-Commit — nicht Bestandteil des leeren Starts.
- Git-Init in einem nicht leeren Verzeichnis — zu leicht destruktiv.

## Compliance

- Application-Tests prüfen Abbruch, Already-Git, Nicht-leer, Nested-Worktree und
  den Happy Path mit genau einem Init plus Katalog.
- Workspace-Tests prüfen Unborn `main` ohne Remote, Ablehnung nicht leerer und
  bestehender Git-Ordner sowie Isolation gegen `GIT_CONFIG_GLOBAL`.
- IPC- und Frontendtests prüfen den pfadlosen Command, beide Buttons und stabile
  Recovery-Codes ohne Adapterdetails.

## Referenzen

- [ADR-0029](0029-core-owned-project-catalog-and-restoration.md)
- [Produktanforderungen](../PRODUCT_REQUIREMENTS.md)
- [IPC-Protokoll](../IPC_PROTOCOL.md)
- [Sicherheit](../SECURITY_AND_EXECUTION.md)
- [Plan 15](../plans/15-CREATE_EMPTY_PROJECT.md)
