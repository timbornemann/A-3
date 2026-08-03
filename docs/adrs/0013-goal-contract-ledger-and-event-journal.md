# ADR-0013: Goal Contract, Task Ledger und append-only Run Journal

Status: Accepted  
Datum: 2026-08-03

## Kontext

Kleine Modelle verlieren bei langen Aufgaben Ziel, erledigte Schritte und Begründungen. Nur Chat-Historie ist weder strukturiert noch zuverlässig verifizierbar. Vollständiges Event Sourcing wäre für V1 unnötig.

## Entscheidung

- Jede Aufgabe besitzt einen revisionierten Goal Contract.
- Planung wird als typisiertes Task Ledger persistiert.
- Jeder Schritt besitzt Ergebnisziel, Abhängigkeiten, erwartete Evidenz und VerificationSpec.
- Completed benötigt erfolgreiche Verification.
- Runs schreiben ein append-only Event Journal.
- Aktueller Fachzustand wird relational materialisiert; das Journal ist Audit, nicht alleinige Wahrheitsquelle.
- Replan erhält alte Versuche und begründet Änderungen.
- Evidenzinvalidierung kann Completed-Schritte auf Stale setzen.

## Konsequenzen

### Positiv

- Ziel und Fortschritt überleben Context Compaction und Appneustart.
- Aussagen über erledigte Arbeit sind prüfbar.
- Agentenläufe können analysiert und reproduziert werden.

### Negativ

- mehr persistenter Zustand und Zustandsübergänge
- Retention für Logs und Context Packs notwendig

### Risiken und Gegenmaßnahmen

- Journal wächst — sichere Retention bei Erhalt von Digests, Evidenz und Verifikationen.
- Goal Drift durch Benutzeränderung — explizite Contract Revision.

## Verworfene Alternativen

- nur Todo-Text im Prompt — nicht dauerhaft und nicht typisiert.
- vollständiges Event Sourcing — hohe Migrations- und Replaykomplexität.
- Überschreiben alter Versuche — verliert Fehlerwissen und Audit.

## Compliance

DB-Constraints für Eventsequenz und Stepstatus; Controller kann Done nur über Acceptance-Verifier erreichen.

