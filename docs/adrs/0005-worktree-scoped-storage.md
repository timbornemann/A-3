# ADR-0005: Worktree-bezogener Speicher außerhalb des Repositories

Status: Accepted  
Datum: 2026-08-03

## Kontext

Index, Embeddings und Runlogs sind groß, teilweise sensibel und überwiegend regenerierbar. Dateien im Repository würden Status verschmutzen und könnten versehentlich committed werden. Mehrere Git-Worktrees dürfen sich nicht gegenseitig überschreiben.

## Entscheidung

- Runtime-Daten liegen im plattformspezifischen lokalen App-Data-Verzeichnis.
- Es gibt eine kleine globale catalog.db und eine knowledge.db je WorktreeIdentity.
- WorktreeIdentity kombiniert RepositoryIdentity und kanonischen Worktree-Root.
- Im Repository sind nur explizite, menschenlesbare .a3/project.toml und .a3/rules.md zulässig.
- Quellcode wird nicht vollständig als duplizierter Blob gespeichert.
- Indexdaten, Taskdaten und Secrets werden logisch getrennt; Secrets liegen ausschließlich im OS-Schlüsselspeicher.

## Konsequenzen

### Positiv

- saubere Git-Worktrees
- getrennte Zustände für parallele Branch-Worktrees
- geringeres Risiko versehentlicher Veröffentlichung

### Negativ

- Projektzustand ist nicht automatisch mit dem Repository portabel.
- Pfad- und Worktree-Umzüge benötigen Reconciliation.

### Risiken und Gegenmaßnahmen

- verwaiste Daten — sichtbare Storage-Verwaltung mit sicherem Cleanup.
- Pfadalias oder Symlink — kanonische Pfade plus Git Common Directory.

## Verworfene Alternativen

- .a3/index.db im Repository — Verschmutzung und Leakrisiko.
- eine globale DB für alle großen Indizes — größere Fehlerdomäne und schwieriger Cleanup.

## Compliance

Storage-Pfadtests auf allen Plattformen; kein Runtime-DB-Pfad unterhalb des Repository-Roots.

