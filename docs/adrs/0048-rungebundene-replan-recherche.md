# ADR-0048: Rungebundene Replan-Recherche

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zur vollständigen Umsetzung von Plan 10 einschließlich der zugehörigen ADRs.

## Entscheidung

Die in [ADR-0047](0047-verbindlicher-recherchearbeitsstand.md) verlangte Replan-Lokalisierung
verwendet `ResearchWorkState` innerhalb des bestehenden Agent-Runs. Der Core erzeugt aus
der tatsächlichen Replan-Ursache und dem vorgesehenen Schrittergebnis eine feste erforderliche
Repository-Frage. Search/Inspect navigieren; nur tatsächliche Safe-Reader-Originalseiten
eröffnen eine V5-Analyse. Derselbe Zulassungsmechanismus wie in Ask/Plan bindet Interpretationen
an die exakt ausgelieferten Originalfenster. Das ist keine Implementierungsverifikation.

Modellturns einschließlich des einzelnen Repairs werden durch die bestehende Run-Abrechnung
erfasst. Es gibt keinen neuen Controller, keine neue Berechtigung und kein erneuertes Budget.
Die begrenzte Lokalisierung behält höchstens vier Reads; identische Zielwerte und identische
Analysepakete werden nicht als neue Untersuchung gewertet. Fehlende Belege schließen nicht ab.

Knowledge V37 speichert metadatenhaltige Checkpoints unter Run, Schritt, Snapshot und dem
besitzenden Journal-Event. Analyse und Event beziehungsweise Read-Quittung und Tool-Event
werden atomar geschrieben. Ein unzulässiger Quellenanker rollt die gesamte Transaktion
zurück. Originalseiten und Rohantworten werden nicht gespeichert. V36 und historische ADRs
bleiben unverändert. Vor erneuter Nutzung werden Originalhashes und Snapshot geprüft.

## Konsequenzen

Persistenz und Run-Journal erhalten enge typisierte Methoden; ältere Testadapter ohne
diese Fähigkeit scheitern geschlossen. Neue Tests müssen denselben Storage-Vertrag erfüllen.
Ein Neustart erneuert weder Read-Zähler noch die unveränderten Analysequittungen. Replan-Ergebnisse
bleiben Interpretationen; natürliche Sprache ist kein deterministisch bewiesener Ursachennachweis.

Die ausdrücklich zusätzlich angeforderte Prüfung des aktuell eingestellten Modells liest
nur einen Settings-Snapshot des bestehenden Katalogs, ohne Migration oder Settings-Schreibzugriff.
Credentials verbleiben beim vorhandenen nativen Credential-Adapter. Provideraufrufe erfolgen
nur im explizit aktivierten Test mit freigegebenen synthetischen Dateien. Dieser Test ersetzt
nicht die lokale Vorher-/Nachher-Matrix aus [Plan 10](../plans/10-RESEARCH_WORK_STATE.md).
