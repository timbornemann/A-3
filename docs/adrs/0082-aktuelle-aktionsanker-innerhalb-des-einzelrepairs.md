# ADR-0082: Aktuelle Aktionsanker innerhalb des Einzelrepairs

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Ein echter Luna-Lauf liefert eine formal decodierbare Patchaktion mit falschen
aktuellen Controllerankern. Der Mutationscontroller lehnt sie korrekt ab, aber
erst nach Abschluss der Action-Decodierung; der Desktop meldet Unavailable.
Die bestehende einzige Korrekturrunde wurde für diesen ungültigen Modellvorschlag
gar nicht angeboten. Eine weitere Live-Antwort scheitert separat an InvalidValue;
deren genaue Ursache wird durch diese Entscheidung nicht als behoben behauptet.

## Entscheidung

ExecuteAgentTurn bindet seinen Primärdecoder und dieselbe einzige Repair-Capability
an die zuvor unabhängig geprüften Run-, Worktree-, Snapshot-, Current-Step- und
Verification-Spec-IDs. Patchvorschläge müssen alle fünf Anker exakt wiedergeben;
Run- und Ledgeraktionen müssen den Core-ausgewählten aktuellen Schritt nennen.
Ein Unterschied ist anchor_mismatch und läuft durch dieselbe einmalige Reparatur
wie ein Strukturfehler. Struktur- und Ankerfehler erhalten kein getrenntes Budget.
Bleibt die Korrektur ungültig, entsteht keine ausführbare Aktion oder Toolanwendung.

Der Core überschreibt keine Modell-ID und interpretiert keine ungültige Ausgabe.
Er liefert nur den inhaltsfreien Fehlercode im bestehenden begrenzten Repair-Hinweis.
Aktueller Index, Policy, Command-Katalog, alle Hash-/Pfadprüfungen, Einmalfreigaben
und erneute Ankerprüfung an der Mutationsgrenze bleiben unverändert. Änderungen
während einer Modellanfrage können deshalb weiterhin vor einer Wirkung stoppen.

Schema-only Decoder für historische Daten behalten ihre strikte Strukturprüfung;
nur der ausführende Turn ergänzt seine vertrauenswürdige aktuelle Bindung.
Replan-Localization bleibt read-only. Tool-, Provider-, Kontext-, Zeit- und
Repairbudgets sowie persistierte Wire-/DB-Schemas werden nicht erweitert.

## Compliance

Tests variieren die fünf Patchanker getrennt, ebenso Run-/Ledger-Step-IDs. Primär
falsch und einmal korrekt darf genau eine validierte Aktion liefern; zweimal
falsch bleibt terminal ohne Toolaufruf. Aktuelle Reads/Finish, Legacy-Decodierung,
Cancellation, Abrechnung und Localization-Ausschlüsse müssen erhalten bleiben.
Anschließend folgen Gesamtgates und ein echter begrenzter Modellnachtest. Die
separaten Patchvorschau-Konflikte und übrigen Modellwerte bleiben weitere Befunde.
