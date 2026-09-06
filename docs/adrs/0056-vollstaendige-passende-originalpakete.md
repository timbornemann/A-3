# ADR-0056: Vollständige passende Originalpakete

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs.

## Befund und Entscheidung

Die V5-Fokussierung bevorzugte extrahierte Funktionseinheiten auch dann, wenn alle
benannten Originaldateien gemeinsam vollständig in das vorhandene Quellenbudget
passten. Damit fehlten unnötig Import-/Initialisierungszeilen; weitere Reads konnten
zu denselben Fragmenten führen. Die geschlossene Planbestandsaufnahme durfte ohne
nachgewiesene vollständige aktuelle Auslieferung zu Recht nicht starten.

Für V5 erhält deshalb ein vollständig gelesenes, zusammen passendes Set benannter
Originaldateien Vorrang vor abgeleiteten Einheitenausschnitten. Erforderlich sind
höchstens acht eindeutige Revisionen, ein vollständiger Originalcache ab Dateianfang,
die vollständige Read-Quittung und ein exakter Fit einschließlich aller E-/S-Header.
Es gibt weder zusätzliche Bytes noch gekürzte Pflichtdateien in diesem Pfad.

Ein expliziter Fokus auf eine Stelle oder Seite bleibt vorrangig. Bei fehlenden,
größeren oder nicht zusammen passenden Originalen gelten die bisherigen progressiven
Einheiten-/Seitenregeln unverändert. Die tatsächlichen E-Fenster durchlaufen denselben
Range-/Hash-/Scope-Zulassungspfad. Es entstehen keine neuen Ergebnisse oder Beweise.

## Verifikation

Regressionen vergleichen vorhandene vollständige kurze Dateien mit konkurrierenden
Funktionseinheiten, prüfen Headerkosten, UTF-8, deterministische identische Pakete,
vollständige Originalauslieferung ohne neue Reads sowie den unveränderten expliziten
Fokus und den begrenzten Fallback bei zu kleinem Kontext. Reale Modellmatrizen bleiben
die getrennte Praxisprüfung; vollständiger Kontext allein garantiert keine richtige Antwort.
