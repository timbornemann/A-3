# ADR-0083: Bekannte Schrittidentitäten als Schemakonstanten

Status: Accepted

Datum: 2026-09-07

Entscheider: Tim Bornemann (Vorabannahme der mit Plan 10 verbundenen ADRs)

## Kontext

Die echte Agent-Abnahme zeigt nach einem erfolgreichen Luna-Done weiterhin einen
fehlgeschlagenen Wiederholungslauf mit ungültigem Aktionswert. Frühere Läufe
enthielten falsche aktuelle Controlleranker. Der Core kennt bereits Worktree,
Schritt, Verification-Spec, den laufenden Step-Versuch und dessen geplanten
Verifikationsbefehl. Diese Identitäten im Modellschema trotzdem als frei zu
reproduzierende Hexwerte anzubieten, überlässt dem Modell unnötige Verwaltungsarbeit.
Die genaue Ursache jedes bisherigen InvalidValue ist damit nicht behauptet.

## Entscheidung

Das aktuelle AgentAction-V4-Schema wird für einen konkreten normalen Agent-Turn
vor Retrieval auf die bereits bekannten Core-Werte eingeschränkt: Worktree-,
Step- und Verification-Spec-ID des Patches, seine Run-ID bei vorhandenem Versuch,
sowie die Step-ID von Run und UpdateLedger sind Literalkonstanten. Hat der aktuelle
InProgress-Schritt einen operationalen Verifikationsbefehl, wird auch genau dessen
Command-ID im Run-Arm konstant. Andere Verifikationsbefehle verlangen einen
entsprechend bestätigten Plan; freies argv entsteht weiterhin nicht.

Schema-Grounding und Formatfeld enthalten denselben eingeschränkten Vertrag.
Alle tatsächlich zusätzlichen Bytes werden vor Retrieval vollständig gezählt und
im unveränderten Kontext-/Outputbudget berücksichtigt. Unbekannte Snapshot- oder
Dateihashwerte werden nicht geraten oder durch Platzhalter vorgetäuscht; deren
bisherige Struktur-, Freshness- und Schreibgrenzen gelten weiter.

Der unabhängige Runtime-Decoder aus ADR-0082 prüft dieselben aktuellen Anker und
gegebenenfalls die konkrete geplante Command-ID im gemeinsamen Einzelrepair.
Weder Provider-Schematreue noch eine Konstante ersetzen Policy, Katalogbestätigung,
Freigabe oder erneute Mutationsprüfung. Historische Schema-only-Verträge bleiben
unverändert; Replan-Recherche und read-only Localization werden nicht erweitert.

## Compliance

Tests prüfen exakte Konstanten, unveränderte übrige Felder, FormatFieldOnly und
bytegleiches RepeatSchemaInPrompt, deterministische Compiles sowie harte
Gesamtbudgets. Ein anderer Command im Primär- und Repair-Dokument bleibt
nicht ausführbar. Provider-, Kontext-, Legacy- und Mutationsverträge bleiben grün.
Die Änderung benötigt einen echten Modellnachtest; geringere Fehlerhäufigkeit
wird nicht allein aus der engeren Schemaform abgeleitet. Keine DB-Migration,
neue Modellrechte, Providerwechsel oder automatische Einstellungsänderung.
