# ADR-0117: Auftragsbezogene Testerstellung und Verifikation

Status: Accepted  
Datum: 2026-09-30  
Entscheider: Tim Bornemann

Annahme: Tim hat am 2026-09-30 ADR-0117 und ADR-0118 ausdrücklich angenommen
und die Fortsetzung der Umsetzung beauftragt.

## Entscheidung

Diese Entscheidung ersetzt die verpflichtenden Testschritte in ADR-0042 und die
automatische Greenfield-Testsuite. Die evidenzgebundene Abschlussverifikation aus
ADR-0112 bleibt bestehen; ein fehlender Prüfcommand erzwingt keine Testerstellung.

- Die Recherche entscheidet auftragsbezogen über Umsetzung und angemessene Prüfung.
  Ein Plan darf ausschließlich Umsetzungsschritte besitzen.
- Vorhandene Tests, Lint und kurze Funktionsprüfungen sind erlaubt. Neue Testdateien,
  Testframeworks und Testscaffolding entstehen nur auf ausdrücklichen Auftrag oder mit
  konkreter Begründung für komplexe Logik, Fehlerkorrektur oder Sicherheitsverhalten.
  Ein ausdrücklicher Testverzicht hat Vorrang.
- Testerstellung und Prüfausführung sind verschiedene Arbeitsabsichten. Ein Prüfziel
  wird nicht automatisch in einen Auftrag zur Erstellung einer Testsuite umgewandelt.
- Diff-Evidence beweist eine Änderung, kein Laufzeitverhalten. Laufzeitbehauptungen
  benötigen passende operationale Evidence. Fehlende Prüfbarkeit bleibt sichtbar.
- Bestehende historische Pläne und Verifikationsergebnisse bleiben unverändert lesbar.
  Neue Pläne und Replans werden nach dieser Regel materialisiert.
- Die Qualitätsgates für die Entwicklung von A^3 selbst bleiben verbindlich.

## Nachweise

Domain-, Recherche-, Materialisierungs- und Executorregressionen prüfen einen kleinen
Server ohne Testsuite, ausdrückliche Testaufträge, begründete Regressionen und Testverzicht.
Ein unabhängiges HTTP-Orakel prüft den tatsächlich erzeugten Server.

## Referenzen

- [ADR-0042](0042-adaptiver-agent-arbeitsplan.md)
- [ADR-0112](0112-nachgelagerte-greenfield-verifikation.md)
- [Plan 21](../plans/21-TASK-SCOPE-AND-PERMISSIONS.md)
