# ADR-0016: Evidenzbasierte Worktree-Umzugs-Reconciliation

Status: Accepted
Datum: 2026-08-04

## Kontext

ADR-0005 bindet lokale Laufzeitdaten an eine `WorktreeIdentity`, deren `WorktreeId` den kanonischen
Worktree-Root enthält. Ein Umzug ändert deshalb absichtlich die `WorktreeId`, obwohl die lokale
Git-Arbeitskopie und ihre A^3-Daten fachlich fortbestehen können. Eine Zuordnung nur anhand eines
neuen Pfads, eines gleichen HEADs oder eines gleichen Remotes wäre jedoch nicht beweiskräftig:
Linked Worktrees und mehrere Clones dürfen sich nicht gegenseitig übernehmen.

Die WebView ist untrusted. Eine dort gerenderte Bestätigung allein darf daher keine lokale
Storage-Identität umschreiben. Katalog und Worktree-Datenbank liegen außerdem in getrennten Dateien;
ein Prozessabbruch zwischen Dateisystemumzug und Katalogcommit muss sicher wiederaufnehmbar bleiben.

## Entscheidung

- Die privilegierte Repository-Inspektion leitet zusätzlich zur pfadbezogenen `WorktreeId` eine
  versionierte `WorktreeAnchorId` ab. Sie ist der BLAKE3-Digest des kanonischen, zum Git Common
  Directory relativen Worktree-Metadatenpfads. Der Anchor beeinflusst die `WorktreeId` nicht und wird
  weder als Autoritätspfad noch an die WebView ausgegeben.
- Ein Reconciliation-Kandidat benötigt denselben Worktree-Anchor und genau eine der folgenden
  Evidenzklassen:
  - dieselbe lokale `RepositoryId`; oder
  - bei geändertem Git Common Directory denselben vorhandenen, credential-freien
    `RemoteIdentity`-Fingerprint.
- Nur ein eindeutiger Kandidat wird angeboten. Gleicher Pfad, gleicher HEAD oder Vektor-/LLM-Ähnlichkeit
  sind keine Identitätsevidenz. Ohne eindeutigen Kandidaten wird der Worktree regulär separat geöffnet.
- Vor jeder Übernahme zeigt der privilegierte Rust-Adapter einen nativen Systemdialog mit altem und
  neuem sicheren Anzeigepfad. Die Auswahl lautet: lokale Daten übernehmen, separat öffnen oder
  abbrechen. Es wird kein Bestätigungs-Command und kein Token an die WebView vergeben; IPC V1 bleibt
  unverändert.
- Eine Übernahme schreibt zuerst einen exakten, wiederaufnehmbaren Intent in `catalog.db`. Danach wird
  das vollständige private Verzeichnis atomar innerhalb von `projects/` von der alten zur neuen
  `WorktreeId` umbenannt, die Repository-/Worktree-Bindung in `knowledge.db` transaktional umgestellt
  und zuletzt der Katalog atomar abgeschlossen. Ein bestätigter, aber unterbrochener Intent darf bei
  derselben Zielidentität ohne erneute Bestätigung fortgesetzt werden.
- Quelle und Ziel werden vor jedem Schritt erneut gegen ProjectId, RepositoryId, WorktreeId, Anchor,
  Remote-Evidenz und Katalogrevision geprüft. Existieren Quell- und Zielverzeichnis gleichzeitig oder
  widersprechen Datenbank und Intent einander, wird ohne Überschreiben abgebrochen.
- `ProjectId` bleibt bei bestätigter Reconciliation erhalten. Mehrere lokale Repository-Beobachtungen
  derselben ProjectId werden normalisiert persistiert, damit ein verschobenes Repository seine alte
  Evidenz nicht rückwirkend umschreibt.
- Knowledge-Fremdschlüssel, die von RepositoryId oder WorktreeId abhängen, verwenden für bestätigte
  Identitätswechsel kontrollierte `ON UPDATE CASCADE`-Beziehungen. Die Legacy-Bindung wird in derselben
  Transaktion aktualisiert.

## Konsequenzen

### Positiv

- Linked Worktrees werden über Git-Metadatenevidenz statt Pfad- oder HEAD-Heuristiken unterschieden.
- Nutzer können einen Remote-gleichen zweiten Clone ausdrücklich separat öffnen.
- Ein Absturz kann weder einen halben Katalogcommit als erfolgreiches Open ausgeben noch einen
  bestätigten Umzug still vergessen.
- Die WebView erhält keine neue privilegierte Fähigkeit und keine autoritativen Altpfade.

### Negativ

- Katalog und Knowledge-Schema benötigen zusätzliche Vorwärtsmigrationen.
- Ein Repository-Umzug ohne Remote kann nach V1-Evidenz nicht sicher einem früheren Repository
  zugeordnet werden; er wird separat geöffnet.
- Ein bestätigter Umzug kann kurzzeitig als vorbereiteter Intent sichtbar sein und benötigt einen
  erneuten Open-Versuch, falls ein Betriebssystemfehler den Fortgang verhindert.

### Risiken und Gegenmaßnahmen

- Anchor-Wiederverwendung nach Entfernung eines Worktrees — vor jeder Mutation exakte
  Katalogevidenz prüfen und immer nativ bestätigen.
- Mehrdeutige Remote-Clones — keinen Kandidaten wählen; separat öffnen.
- Crash zwischen Storage- und Katalogschritt — dauerhafter Intent und idempotente Zustandsprüfung.
- Ziel enthält bereits Daten — niemals überschreiben oder zusammenführen; stabiler
  Identitätskonflikt.

## Verworfene Alternativen

- Pfad- oder HEAD-Matching — verwechselt Clones und parallele Worktrees.
- Remote-Matching ohne Bestätigung — ein Remote ist keine eindeutige lokale Arbeitskopie.
- Bestätigung in der WebView — verletzt die Trust Boundary.
- Neue leere Knowledge-DB bei jedem Umzug — verliert Kontinuität dauerhafter Projekt- und Taskdaten.
- Kopieren statt atomarem Umbenennen — verdoppelt große Indexdaten und benötigt einen langen,
  abbrechbaren Copy-Job.

## Compliance

- Domain und Application enthalten keine Tauri-, SQL- oder libSQL-Typen.
- Contract-Tests prüfen Erkennung, separate Öffnung, bestätigte Übernahme, Neustart und Resume.
- Adaptertests prüfen jede Katalog-/Knowledge-Vorgängerversion, widersprüchliche Intents,
  bestehende Ziele und unveränderte Quelldaten bei Ablehnung.
- Die Capability-Liste der WebView wird nicht erweitert.

## Beziehungen

Ergänzt ADR-0005 um den verbindlichen Reconciliation-Ablauf, ohne dessen worktree-bezogene
Storage-Grenze oder externe App-Data-Ablage zu ersetzen.
