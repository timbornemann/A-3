# ADR-0058: Kompakte Recherchephasen für kleine Kontexte

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Nutzerauftrag zu Plan 10 einschließlich zugehöriger ADRs.

## Befund

Der freigegebene lokale Qwen-Test mit 8.192 Kontext und 2.048 Output erreichte
nach der Initialisierung die Kontextgrenze. Wiederholte globale, aktionsbezogene
und phasenbezogene Anweisungen verbrauchten viel fest reservierten Platz. Zusätzlich
verlangte der Core unabhängig von der Paketgröße 1.536 Bytes Quellenreserve, obwohl
kleinere Originalfenster bereits unterstützt werden. Das größere Modell konnte diese
deterministische Platzsperre nicht lösen.

## Entscheidung

Die Systeminstruktion nennt gemeinsame Vertrauensregeln einmal und ausschließlich
den aktuellen Phasenauftrag. Schema, Decoder, Quellenzulassung, Zustandsautomat,
Budgetprüfung und Abschluss bleiben im Core; der Text ersetzt keine dieser Prüfungen.
Der vollständige Nutzerauftrag und bindende Designentscheidungen werden nicht gekürzt.

Für Repositoryfragen reserviert die Arbeitsansicht ein Drittel des tatsächlich
berechneten Evidence-Pakets, begrenzt auf 512 bis 1.536 Bytes, statt einer festen
1.536-Byte-Mindestgröße. Die Designreserve bleibt 256 Bytes. Dies ändert ausschließlich
Packing und Partitionierung innerhalb des bestehenden Budgets: kein zusätzliches
Kontexttoken, kein Read, Repair oder Modellaufruf wird bewilligt. Passt selbst die
kompakte Pflichtansicht nicht, bleibt der ehrliche ContextLimit-Stopp bestehen.
Nur tatsächlich gelieferte, erneut validierte Originalfenster können Ergebnisse tragen.

## Verifikation

Ein Offline-Controllervertrag verwendet das tatsächliche konservative 8k/2k-Profil
mit echtem Git, Fast Index, Safe Reader und libSQL für Ask, Plan und Agent-Vorbereitung.
Vor der Korrektur scheitert er an der Kontextgrenze; er darf nicht mit größeren
Testbudgets oder gekürzten Zielen repariert werden. Die vorhandenen Tiny-/UTF-8-,
Originalauslieferungs-, Provider-Packing-, Repair-, Stale- und Grenzenverträge bleiben.
Reale freigegebene Modellnachtests prüfen Inhalt und Abschluss separat; ein kleinerer
Prompt garantiert keine korrekten Modellaussagen.
