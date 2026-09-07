# A^3 Live-Coding-Abnahme V2

V2 verwendet unverändert die öffentlichen Startdateien von
`agent-coding-eval-v1/small-local-bugfix` und `two-module-change`. Der native,
ignorierte Test `agent_approved_live_coding_fixture` erzeugt ein isoliertes
Repository und nutzt den echten ProductionAgentRunExecutor. Nur das Live-Modell
liefert Patchinhalte. Ein bestätigter Ein-Schritt-Plan ist der Startpunkt;
eine vollständige Conversation-Planübergabe ist hier nicht enthalten.

`A3_LIVE_AGENT_CASE` wählt ausschließlich `small-local-bugfix` (Standard) oder
`two-module-change`. `A3_LIVE_AGENT_GENERATION` wählt unverändert `baseline`,
`staged` oder `guided`. Modell-/Providerfreigaben sind weiterhin ausdrücklich
erforderlich; lokale Modelle werden strikt nacheinander getestet. V2 verändert
weder die produktive Strategie noch die nativen Einstellungen.

Die exakte Testfreigabe erlaubt Updates ausschließlich an `increment.py`
beziehungsweise `pricing.py` und `invoice.py`, einzeln oder zusammen. Add, Move,
Delete, Test-/Runner-/Manifeständerungen und weitere Pfade werden abgelehnt.
Der einzige Agent-Prozess bleibt `python -m pytest` im Fixture-Root ohne Netz.
Alle angeforderten Quellmodule müssen tatsächlich geändert sein. Geschützte
Dateien und die native Einstellungskopie werden vor Erfolg erneut verglichen.

Die unabhängige Zusatzprüfung [oracle.py](oracle.py) bleibt außerhalb des
Modell-Worktrees. Der Test-Supervisor führt sie in einem eigenen, begrenzten
Python-Prozess mit `-I -B -c` aus, ohne ihre Ergebnisse als Agent-Evidence
einzuspeisen. Sie prüft zwölf ganzzahlige Increment-Eingaben, 49 Kombinationen
für Preis und Rabatt sowie 35 Rechnungen einschließlich leerer Listen,
Rundung auf ganze Cent, 0/100 Prozent, Formatierung und tatsächlicher Delegation
an den Preishelfer. Die öffentlichen V1-Tests bleiben unverändert. Diese kleine
deterministische Eingabemenge ist kein Beweis allgemeiner Korrektheit und keine
Sandbox für feindlichen Python-Code.

Offline-Regressionen belegen, dass die sichtbaren Tests durch Konstantantworten
bestanden werden können, V2 diese aber ablehnt. Teilimplementierung und korrekte
Rechnungsarithmetik ohne den verlangten Helferaufruf werden ebenfalls abgelehnt;
korrekte Ein-/Zwei-Modul-Implementierungen bestehen. Die Scope- und
Originalquellen-Regressionen prüfen den geschlossenen Fallkatalog und die
Zuordnung jeder tatsächlichen Source-Body zu ihrem eigenen Marker.

Liveberichte trennen `Done`, dauerhafte Verifikation, physische V1-Tests und
`independent_oracle_passed`. Erst alle zusammen zählen als Erfolg. Für
Strategievergleiche werden derselbe eingefrorene Build, Fall und dasselbe Profil
gegenbalanciert wiederholt. V2-Werte sind wegen der erweiterten Aufgabenformulierung
nicht unmittelbar mit V1 gleichzusetzen. Ergebnisse und verbleibende Grenzen:
[Plan-10-Protokoll](../../docs/plans/10-RESEARCH_VALIDATION.md).
