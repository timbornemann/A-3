# ADR-0084: Ergebnisnotizen fordern operationale Verifikation an

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Der echte Luna-Wiederholungslauf erreicht nach einem physisch korrekten Patch
Run=Verify und Step=Verifying ohne Prozessnachweis. Die Gesamtabnahme lehnt das
unvollständige Ledger korrekt ab. Der Code erlaubt zuvor `record_result`, mit
beliebiger aktueller Read-Evidence die operationale Schrittprüfung vorzubereiten.
Ein gelesener Quelltext ist jedoch kein Command-/Test-/Diagnostic-Ergebnis.
Die Finish-Korrektur aus ADR-0079 allein deckt diesen zweiten Abschlusswunsch nicht ab.

## Entscheidung

Ein streng decodiertes `record_result` für den aktuellen laufenden operationalen
Command-/Test-/Diagnostic-Schritt ist wie Finish eine Prüfanforderung an den Core.
Der Application-Selektor leitet daraus ausschließlich den im aktuellen Ledger
geplanten Command ab; fremde Step-IDs, andere Ledgeraktionen und andere Zielarten
erzeugen keinen Prozess. Der Desktop benutzt denselben vorhandenen Run-/Policy-/
Approval-/Evidence-Pfad wie bei Finish. Modellnotiz und wirklicher Prozessnachweis
bleiben getrennt. Kein zusätzlicher Modellturn, freies argv oder neuer Befehl entsteht.

Die unabhängige ApplyAgentLedgerUpdate-Grenze lehnt RecordResult für alle operationalen
Specs vor jeder Zustandsänderung ab. Ihre Verifikation muss aus dem jeweiligen echten
Artifactpfad stammen. Historische Legacy-Read-Schritte behalten ihren bisherigen
Vertrag. Diff-/UserConfirm-Schritte erhalten keine automatische Prozessfreigabe.
Nur der bestehende deterministische Verifier darf Completed und Done erzeugen.

Diese Ergänzung ändert weder V4-Wireschema, Datenbankschema, Zustandsunion noch
Retry-/Repairbudgets. Sie ersetzt keine historische Verifikation und erklärt keinen
bereits gespeicherten Verifying-Schritt ohne Artifact nachträglich für abgeschlossen.
ADR-0079, zentrale Sicherheitsprüfung und Unknown-Reconciliation bleiben unverändert.

## Compliance

Tests prüfen exakte Auswahl für beide Prüfanforderungen und alle drei operationalen
Commandarten, Ablehnung fremder Schritte/Blocker/Replan sowie unveränderte andere
Zielarten und Zustände. Ein RecordResult mit echter aktueller Read-Evidence darf eine
operationale Spec nicht in Verifying versetzen; Run und Ledger bleiben dabei gleich.
Legacy-Read-Verträge bleiben grün. Der reale Agent-Nachtest muss nach Patch einen
wirklichen Test ausführen, geschützte Dateien erhalten und unabhängig überprüfbar
Done erreichen. Ein einzelner erfolgreicher Lauf ist kein Stabilitätsnachweis.

Referenzen: [ADR-0079](0079-abschlussanforderung-verifiziert-zuerst-den-schritt.md),
[E6/E7](../plans/05-EDITING_AND_VERIFICATION.md),
[Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
