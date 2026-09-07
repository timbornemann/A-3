# ADR-0085: Patch-Snapshotkonflikte im Einzelrepair

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Der echte Qwen-Lauf endet vor jeder Mutation mit PatchPreview
`Conflict(TargetAlreadyExists)`. Die Action ist strukturell gültig, widerspricht
aber bereits dem aktuellen veröffentlichten Dateibestand. Der bisherige Turn gibt
sie trotzdem als ausführbaren Vorschlag weiter; die spätere Vorschau beendet den
Lauf als Unavailable. Ein vorhandenes Ziel, eine nicht indexierte Quelle oder ein
falscher erwarteter Indexhash lassen sich ohne weiteren Dateizugriff feststellen.

## Entscheidung

Der produktive Turn erhält die aktuelle unveränderliche PublishedIndex-Referenz.
Vor der Modellanfrage müssen deren Index-Run und Snapshot zum frisch kompilierten
Kontext und zum Controller passen. Nach strikter Schema-/Ankerdecodierung werden
Patchoperationen rein gegen die geordneten FileRevisions dieser Publikation geprüft:
Add/Move verlangen ein abwesendes Ziel; Update/Move/Delete verlangen genau die
veröffentlichte Quellrevision. Der Core ersetzt weder Operationsart noch Pfad,
erwarteten Hash oder neue Inhalte.

Eine bekannte Unvereinbarkeit erhält einen geschlossenen, inhaltsfreien Fehlercode
im selben einzigen Repair wie Struktur- und Ankerfehler. Primär- und korrigiertes
Dokument werden gegen dieselbe immutable Publikation geprüft. Ein erneut falscher
Vorschlag bleibt terminal; ein gültiger Search-/Inspect-Vorschlag darf regulär
aktuelle Originale beschaffen. Kein zusätzliches Repair-, Recherche- oder Toolbudget
wird eingeführt. Schema-only-Legacy-Verträge bleiben unverändert.

Die Prüfung führt keine Datei-/Prozessaktion aus, vergibt keine Freigabe und macht
keine Quelle durch bloße Indexzugehörigkeit zur zulässigen Schreibquelle. Sie ist
auf höchstens 64 Operationen mit binärer Suche im bereits vorhandenen Snapshot
begrenzt. Live-Änderungen nach Indexpublikation können weiterhin erst an der normalen
Vorschau-/Schreibgrenze erkannt werden. Alle dortigen Root-, Secret-, Symlink-,
Snapshot-, Hash-, Policy-, Approval- und Unknown-Reconciliation-Regeln bleiben bestehen.

## Compliance

Tests prüfen existierende Ziele, fehlende Quellen und falsche Quellrevisionen,
gültige Korrekturen und nochmals falsche Repairs. Jeder Turn verbraucht höchstens
zwei Modellantworten; abgewiesene Vorschläge verursachen null Tools und keine
Mutation. Ein abweichender Publikationsanker muss vor der Modellanfrage scheitern.
Reale Patch-, Provider-, Legacy-, Storage- und Mehrmodusverträge bleiben erhalten.
Ein echter unveränderter Qwen-Nachtest entscheidet getrennt, ob die Fehlerklasse
praktisch überwindbar ist; es wird kein allgemeiner Modell-Erfolg vorausgesetzt.

Referenzen: [ADR-0082](0082-aktuelle-aktionsanker-innerhalb-des-einzelrepairs.md),
[ADR-0083](0083-bekannte-schrittidentitaeten-als-schemakonstanten.md),
[E7/E8](../plans/05-EDITING_AND_VERIFICATION.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
