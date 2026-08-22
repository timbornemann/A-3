# ADR-0029: Core-eigener Projektkatalog und sichere Startwiederherstellung

Status: Accepted

Datum: 2026-08-22

Entscheider: Tim Bornemann

Ergänzt ADR-0005, ADR-0016 und ADR-0025

## Kontext

Der bisherige Desktopvertrag projiziert höchstens zehn zuletzt geöffnete Worktrees, erlaubt aber
weder einen sicheren Wechsel noch eine Wiederherstellung nach einem Prozessneustart. Ein erneutes
Öffnen über den nativen Ordnerdialog ist unnötig, solange A^3 den Worktree bereits kennt. Eine
WebView-gesteuerte Pfadauswahl würde jedoch die bestehende Vertrauensgrenze umgehen und könnte
gespeicherte Roots ohne erneute Git-Identitätsprüfung aktivieren.

## Entscheidung

- Der Application-Core besitzt einen unbegrenzten dauerhaften Projektkatalog. Genau ein Worktree
  kann aktiv sein; inaktive Einträge erhalten keine Watcher, Indexjobs oder Maintenance-Reads.
- Die WebView liest ausschließlich sichere `ProjectSummaryV1`-Projektionen in festen Seiten zu 25
  Einträgen. Die Sortierung ist „zuletzt erfolgreich aktiviert zuerst“. Suche betrifft nur die
  sichere Root-Anzeige. Vor- und Rücknavigation verwenden opake Cursor.
- Aktivieren und Entfernen akzeptieren ausschließlich eine 64-stellige kleingeschriebene
  `worktreeId`, die zuvor aus dem Katalog gelesen wurde. Pfade, Repository-IDs und Projekt-IDs sind
  keine WebView-Autorität.
- Vor jeder Aktivierung rekonstruiert der Storage-Adapter den gespeicherten nativen Root intern.
  Der Core inspiziert ihn erneut und verlangt exakt dieselbe Repository- und Worktree-Identität.
  Erst nach erfolgreichem Wechsel aller Runtime-Besitzer wird die Aktivierungsreihenfolge
  dauerhaft aktualisiert. Ein Fehler erhält das vorher aktive Projekt.
- Beim Prozessstart wird ohne WebView-Parameter ausschließlich der jüngste Katalogeintrag erneut
  validiert. Ein fehlender Root oder Identitätskonflikt lässt A^3 ohne aktives Projekt; ein anderer
  Eintrag wird niemals als Fallback geöffnet.
- Der native Ordnerdialog bleibt der einzige Weg, einen neuen oder verschobenen Worktree zu
  autorisieren. Es gibt keine Hintergrundprüfung aller gespeicherten Roots.
- Entfernen verlangt eine UI-Bestätigung und löscht nur den exakten Katalogeintrag sowie offene
  Reconciliation-Absichten. Repository, Worktree, Quellcode, stabile Projektzeile und private
  `projects/<WorktreeId>/knowledge.db` bleiben erhalten. Beim aktiven Eintrag werden Runtime-
  Besitzer zuerst geordnet deaktiviert; danach bleibt kein Projekt aktiv.
- Katalogschema V6 ergänzt den Aktivierungsindex und eine durch Trigger synchronisierte
  FTS5-Projektion der sicheren Root-Anzeige. Vorhandene Einträge werden vorwärts migriert.

## Konsequenzen

### Positiv

- Projects ist eine echte Verwaltung für beliebig viele lokale Worktrees und überlebt Neustarts.
- Gespeicherte Pfade verlassen den privilegierten Core nicht als Autorität.
- Fehlende und verschobene Worktrees führen zu sichtbarer Recovery statt zu einem überraschenden
  Projektwechsel.
- Linked Worktrees bleiben getrennte Katalog- und Knowledge-Einheiten.

### Negativ

- Die Aktivierung umfasst eine erneute lokale Git-Inspektion und einen koordinierten Runtime-
  Wechsel, bevor sie als erfolgreich gilt.
- FTS5 und Cursor-Paging erweitern das globale Katalogschema und benötigen eine Vorwärtsmigration.
- Verschobene Worktrees müssen bewusst über den nativen Dialog erneut ausgewählt werden.

## Verworfene Alternativen

- Gespeicherte Pfade aus der WebView zurücksenden — erweitert die unprivilegierte Oberfläche um
  Dateisystemautorität.
- Beim Start den nächsten erreichbaren Eintrag probieren — aktiviert ohne aktuelle Nutzerabsicht
  ein anderes Projekt und kaschiert einen Recovery-Fall.
- Alle Roots im Hintergrund prüfen — erzeugt ungebundene I/O-Arbeit und skaliert mit dem gesamten
  Katalog.
- Katalogeintrag und private Knowledge-Daten gemeinsam löschen — macht eine reine Listenaktion
  destruktiv und widerspricht der bestehenden Retention.

## Compliance

- Storage-Contracts prüfen mehr als 25 Einträge, FTS-Suche, beide Cursorrichtungen, Migration,
  Linked-Worktree-Trennung und exaktes Entfernen bei erhaltener `knowledge.db`.
- Application- und IPC-Tests prüfen fehlenden Root, Identitätskonflikt, fehlenden Fallback,
  ungültige IDs/Cursor/Felder und die Abwesenheit von Pfadautorität.
- Lifecycle-Tests prüfen, dass fehlgeschlagene Wechsel den bisherigen Runtime- und
  Aktivierungszustand erhalten.
- Frontend-Tests prüfen Startreihenfolge, Suche, Seitennavigation, Wechsel, Hinzufügen,
  Bestätigungsdialog und redigierte Recovery.

## Referenzen

- [ADR-0005](0005-worktree-scoped-storage.md)
- [ADR-0016](0016-evidence-based-worktree-reconciliation.md)
- [ADR-0025](0025-bounded-desktop-rendering-and-lifecycle.md)
