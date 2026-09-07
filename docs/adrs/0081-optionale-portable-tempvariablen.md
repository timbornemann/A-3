# ADR-0081: Optionale portable Tempvariablen

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Entdeckte Commands tragen plattformneutral PATH, TEMP, TMP und TMPDIR als
Umgebungs-Allowlist. Der Prozessadapter verlangt bisher jeden dieser Werte.
Auf dem geprüften Windows-Host ist TMPDIR nicht gesetzt. Dadurch wird ein
gültiger bestätigter Python-Test vor Start abgewiesen und unnötig neu geplant.

## Entscheidung

Ausschließlich TEMP, TMP und TMPDIR sind beim Kopieren der erlaubten Umgebung
optional: Ein im expliziten Host-Snapshot fehlender Wert bleibt im Kindprozess
ebenfalls abwesend. Vorhandene Werte werden nur bei Eintrag in der ProcessSpec
übernommen. Es gibt weder Ersatzwerte noch Nachlesen der gesamten Hostumgebung.
Alle anderen ausdrücklich verlangten fehlenden Variablen bleiben Denied.
Insbesondere bleibt PATH für die Auflösung eines relativen Executable erforderlich.

env_clear, exakt erlaubte Namen, kanonische Executables und Arbeitsverzeichnisse,
argv, Policyentscheidung, Planbindung, Freigaben und Netzwerkscope bleiben identisch.
Die Ausnahme gewährt keinen neuen Wert oder Zugriff und verändert keine Credentials.
Domain und ProcessSpec-Schema bleiben plattformunabhängig und unverändert.

## Compliance

Der echte öffentliche ProcessRunner-Vertrag prüft alle drei optionalen Namen bei
fehlenden Werten, die unverändert geleerte Kindumgebung und weiterhin abgewiesene
andere fehlende Namen. Bestehende argv-, Root-, Timeout-, Cancellation- und
Redaction-Verträge müssen bestehen. Danach folgt derselbe echte Luna-Agent-Test
mit gesperrten Testdateien und unabhängiger Nachprüfung.
