# ADR-0060: Aktueller Prüfstand vor optionaler Dialogreserve

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Fortgesetzter Nutzerauftrag zu Plan 10 einschließlich Modelltests, direkter Fehlerkorrekturen und zugehöriger ADRs.

## Befund

Der erneute Qwen-8k-Livefall `eval-1788705505032.jsonl` beantwortet Bestand und
Änderungsentwurf, hält aber vor dem Testentwurf wegen ContextLimit. Die konservative
Berechnung reserviert pauschal ein Drittel des verbleibenden Fensters für historischen
Dialog. Der tatsächliche Message-Packer schützt bereits das vollständige aktuelle
Paket und den Einzelrepair vor optionaler Historie. Das vorgelagerte Teilbudget kann
deshalb passende Pflichtdaten ablehnen, obwohl dieselbe unveränderte Profilgrenze sie
zulässt. Ein größerer Kontext oder ein erneuter Abschnitt löst diese Doppelreservierung
nicht ursächlich.

## Entscheidung

Der aktuelle Recherche-Prüfstand darf die gesamte verbleibende Paketkapazität verwenden,
nach Abzug des konservativ gezählten größten Phasen-Systemvertrags einschließlich einer
gegebenenfalls wiederholten Schemadefinition, der konfigurierten Outputreserve, der
unveränderten 1.024 Sicherheitsreserve und des 768-Byte-Einzelrepairhinweises. Die
192-KiB-Paketobergrenze bleibt bestehen. Es gibt keine zusätzliche pauschale Quote für
optionalen alten Dialog. Die bestehenden allgemeinen, aktiven und unveränderlichen
Aufträge bleiben im aktuellen Paket; historischer Dialog erhält wie bisher ausschließlich
den danach tatsächlich verfügbaren Platz im abschließenden Message-Packer.

Dies präzisiert die interne Verteilung aus ADR-0058 und die aktuelle-vor-historisch-
Priorität aus ADR-0044. Es ändert weder Modellprofil noch Output-, Read-, Repair-,
Aufruf- oder Zeitgrenzen. Der vollständige Nutzerauftrag und bindende Designentscheidungen
werden nicht gekürzt. Ein tatsächlich unpassendes Pflichtpaket wird weiterhin vor dem
Provideraufruf abgelehnt. Schema-, Freshness-, Secret-, Modus- und Ausführungsgrenzen
bleiben unverändert. Historischer Freitext ersetzt keinen dauerhaften Zielvertrag.

## Verifikation

Der unabhängige Budgettest und der reale Git-/Index-/Reader-/libSQL-Controllervertrag
mit langem, Unicode-haltigem Entwurf reproduzieren die vorzeitige Ablehnung zunächst
rot. Plan und Agent müssen mit dem unveränderten 8.192/2.048-Profil alle Entwurfsbytes
an die Testphase übergeben, ohne neue Reads oder zusätzliche Repairs. Ein unabhängiger
Capturing-Provider prüft alle fünf Phasen, alle drei Modi, beide Schema-Groundings,
lange Historie, das volle aktuelle Paket und einen maximalen Einzelrepair gegen die
tatsächliche Gesamtgrenze. Echte Überläufe bleiben negativ getestet.

Reale Modellläufe und ihre Inhaltsprüfung bleiben davon getrennt. Mehr passende
Originale und vollständige Entscheidungen sind Hilfen, kein Wahrheitsbeweis; bekannte
inhaltliche Gegenbeispiele dürfen nicht durch reine Abschlusszähler ersetzt werden.
