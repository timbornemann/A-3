# ADR-0079: Abschlussanforderung verifiziert zuerst den Schritt

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Im echten Luna-Lauf gelingen Patch und Indexaktualisierung. Danach fordert das Modell
Finish an, obwohl der aktuelle Schritt InProgress und ohne Verification ist.
Der bisherige Desktopcontroller wechselt zu Verify und versucht sofort die
Gesamtabnahme; deren korrekte IncompleteLedger-Ablehnung wird als AnchorsChanged
gemeldet. Der exakt geplante Test wird nie ausgeführt.

## Entscheidung

Ein streng decodiertes Finish bleibt eine Anforderung an den Core, niemals ein
Erfolgsbeweis. Für einen aktuellen InProgress-Schritt mit operationalem Command-,
Test- oder Diagnostic-Ziel leitet der Application-Core daraus genau dessen vorhandene
Command-ID und Step-ID ab. Der Desktop führt diese einzelne Run-Aktion durch den
unveränderten Mutationscontroller aus. Aktueller manifestbelegter Katalog,
Bestätigung, zentrale Policy, Einmalfreigabe, Cancellation und Toolbudgets gelten
genauso wie bei einem ausdrücklich ausgewählten Run. Keine freien argv, kein neues
Kommando, keine Installation und keine allgemeine Prozessfreigabe entstehen.

Die Modellanforderung und tatsächliche Prozess-/Verification-Evidence bleiben im
bestehenden Journal getrennt nachvollziehbar. Es gibt keinen zusätzlichen Modellturn
und keine volatile Pending-Verifikation; die Auswahl stammt aus dem dauerhaften
aktuellen Ledger. Nach echter erfolgreicher Step-Verifikation gelten die vorhandene
sequenzielle Planfortsetzung und vollständige Acceptance-Prüfung. Fehler durchlaufen
unverändert die begrenzte Retry-/Replan-Logik. Test- oder Diagnostic-Semantik wird
nicht auf bloßen Exitcode reduziert. Andere Verifikationsarten erhalten keine
automatische Prozessaktion und bleiben an ihre bestehenden Evidenzgrenzen gebunden.

## Compliance

Selektortests prüfen exakte IDs, drei operationale Kommandoarten und Ablehnung anderer
Arten sowie nicht ausführbarer Schrittzustände. Der reale Live-Test verlangt einen
wirklichen Patch, unveränderte gesperrte Tests, tatsächliche Command-Evidence und Done
sowie eine unabhängige physische Nachprüfung. Ohne diese Beweise kein Erfolg.

Diese Präzisierung folgt [ADR-0010](0010-single-controller-state-machine.md) und
[E6/E7](../plans/05-EDITING_AND_VERIFICATION.md); Zustands-, Tool- und DB-Schemas bleiben unverändert.
