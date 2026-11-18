# Plan 22 Kleiner Agentenkern für zuverlässige lokale Modelle

Status: In Progress, P0 bis P3 verifiziert; aktuelles Arbeitspaket P4; ADR-0119 angenommen am 2026-11-06  
Stand: 2026-11-07  
Auftrag: A^3 als nutzbaren lokalen Assistenten mit Chat, projektbezogenem Ask und kleinschrittig ausführendem Agenten umbauen

A^3 behält seinen sicheren Unterbau und ersetzt den modellnahen Arbeitszyklus. Das Ziel ist nachgewiesene richtige Aufgabenbearbeitung mit lokalen Modellen bei begrenztem Kontext. Weniger Code, kleinere Prompts oder grüne Infrastrukturtests sind allein keine Abnahme.

Der erste Umsetzungsschritt ist eine eingefrorene reale Ausgangsmessung. Danach folgen auftragsbezogene Kriterien und kleine Pläne, exakte Bereichsedits, passende Quellenversorgung und ein enger vollständiger Codingpfad. Erst dessen Modellnachweis rechtfertigt den Ausbau der übrigen Aufgabenklassen und den einmaligen Produktionswechsel.

## 1 Autorität und Grenzen dieses Auftrags

Maßgeblich bleiben [AGENTS.md](../../AGENTS.md), [Architekturregeln](../ARCHITECTURE_RULES.md), [Qualitätsgates](../QUALITY_GATES.md), [Security](../SECURITY_AND_EXECUTION.md) und akzeptierte ADRs. Das am 2026-11-06 angenommene [ADR-0119](../adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md) entscheidet die gezielten Ablösungen. [Kleiner Agentenkern](../SMALL_LOCAL_MODEL_AGENT_CORE.md) beschreibt die Zielverträge.

Der Auftrag umfasst jetzt die vollständige schrittweise Umsetzung dieses Plans. P0 hat die bestehende Baseline mit sechs erhaltenen Versuchen und einem unveränderlichen Messmanifest untersucht. P1 hat nach der ausdrücklichen Annahme von ADR-0119 die aktiven Arbeitsverträge ausgerichtet und die präzisen Ablösungen getrennt festgehalten. ADR-0119 wurde am 2026-11-07 mit ausdrücklicher Maintainerfreigabe um den Entwicklungsneustart ohne Altdatenmigration ergänzt; andere historische Entscheidungen bleiben unverändert.

### Umfangsänderung: keine Altdatenmigration

Tim hat am 2026-11-07 ausdrücklich festgelegt, dass A^3 ausschließlich in Entwicklung
ist und bestehende Daten nicht übernommen werden müssen. Die ausdrücklich direkt
ergänzte [ADR-0119](../adrs/0119-kleinschrittiger-agentenkern-fuer-lokale-modelle.md)
hebt dafür ihre bisherigen Altübernahmepflichten auf. Der neue Kern startet mit frisch initialisierten,
versionierten A^3-Daten. Alte Chats, Tasks, Queues, Freigaben und Einstellungen müssen
weder konvertiert noch im neuen Produkt lesbar bleiben. Keine neue Legacy-Schicht
und keine Upgrade-Matrix alter Entwicklungsstände. Interne Schemaaufbauschritte für
eine leere Datenbank dürfen wiederverwendet werden. Diese Planänderung löscht keine Daten.

P9 konzentriert sich auf Bedienung, IPC und sichere Neueinrichtung; P11 auf terminalen
Shutdown und einmalige Umschaltung. Wiederanlauf, CAS, Crash-/Unknown-Recovery und
Budgeterhaltung neuer Runs bleiben Pflicht. P0-Baseline, Modellgates und Projektdateien
bleiben erhalten; kein Implementierungspunkt wird dadurch als erledigt abgehakt.

### Abnahmeziel

- Einfaches Chatten funktioniert ohne geöffnetes Projekt und ohne Coding-Schemahürde.
- Ask beantwortet Fragen ausschließlich zum gebundenen geöffneten Projekt, anhand aktueller Quellen und mit konkreten Erkenntnisgrenzen.
- Agent plant selbst innerhalb des Nutzerziels und bearbeitet bestehende sowie leere Projekte in kleinen Schritten.
- Gezielte Änderungen in großen Dateien erhalten alle nicht betroffenen Bytes und fremde Änderungen.
- Fehler führen zu einer begrenzten sachlichen Korrektur oder echten Verkleinerung, ohne Zielverlust, Endlosschleife oder falschgrünen Abschluss.
- Der ganze Weg vom Nutzerauftrag über Planung bis Dateien, Prüfungen und Done wird mit lokalen Modellen gemessen, einschließlich 4k-Kontext.
- Sicherheit, Freigaben, Cancellation, Neustart und Unknown-Recovery bleiben mindestens auf heutigem Vertragsniveau.

### Explizite Nichtziele

Kein kompletter Rewrite von Desktop, Index, Storage und Werkzeugadaptern. Keine neue Inferenzengine, kein neuer Dienst, keine autonome Git-Veröffentlichung und kein Multi-Agent-Produkt. Kein Cloudmodell als Ersatz für eine gescheiterte lokale Abnahme. Keine neue allgemeine Plugin-/Workflowplattform, keine vorausschauende Abstraktionssammlung und keine optische Gesamtneugestaltung.

Neue Tests in Nutzerprojekten folgen unverändert ADR-0117. Die Entwicklung von A^3 selbst benötigt weiterhin Regressionstests und ihre Qualitätsgates. Die vorhandenen Berechtigungsmodi aus ADR-0118 werden nicht gelockert.

## 2 Ausgangsbefund und verbindliche Reparaturziele

Die folgenden Punkte sind aus dem untersuchten Bestand abgeleitet. Dateiverweise benennen Verantwortlichkeiten, keine unveränderlichen Zeilennummern für spätere Implementierung.

| ID  | Aktueller Befund und Quelle                                                                                                                                                                                                                | Veränderung im neuen Kern                                                                       | Erforderlicher Gegenbeleg                                                                        |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| F1  | Vollständiger neuer Dateiinhalt im [Argumentvertrag](../../crates/a3-application/src/agent_turn/staged_contract.rs), automatische Quellen aus höchstens zwei begrenzten Seiten in [originals.rs](../../crates/a3-context/src/originals.rs) | Kleine exakte Editfenster, Dateierhaltung im Core                                               | Großer Dateifix mit kleiner Modellausgabe und bytegleicher Umgebung                              |
| F2  | [source_guidance.rs](../../crates/a3-application/src/agent_turn/source_guidance.rs) entfernt nach vier Ergebnissen die Evidencewahl, auch ohne Original                                                                                    | Fortschritt über tatsächliche aktuelle Bereiche, gezielte Lücken und echte Zerlegung            | Metadaten-/Leerreads erzeugen keine Editbereitschaft                                             |
| F3  | Allgemeines Kriterium, erster verfügbarer Prüfcommand und Diff-Fallback in [Materialisierung](../../apps/desktop/src-tauri/src/agent_session_manager.rs)                                                                                   | Kriterien pro Ergebnis und passende Prüfzuordnung                                               | Grün bleibender Linter, null Tests und falsches Verhalten können kein funktionales Done erzeugen |
| F4  | [automatic_replan_steps](../../apps/desktop/src-tauri/src/production_agent_run_executor.rs) kopiert Ergebnis und Verifikation mit neuen IDs                                                                                                | Geänderte kleine Schritte und Evidenzpflichten mit erhaltener Abdeckung                         | Nach Fehlschlag wirklich anderer Arbeitszuschnitt und unveränderte History                       |
| F5  | Mehrstufige Aktionswahl in [staged.rs](../../crates/a3-application/src/agent_turn/staged.rs), Bytezähler in [ModelProfile](../../crates/a3-domain/src/project/model_profile.rs)                                                            | Ein schmaler Phasenauftrag pro Modellentscheidung und gemessener vollständiger Kontext          | Positive 4k-Aufgaben statt ausschließlich erfolgreicher Budgetablehnung                          |
| F6  | Vorgegebene Patches in [coding_eval](../../tests/agent-harness/tests/coding_eval/mod.rs), Begriffsprüfungen in [Researchmatrix](../../apps/desktop/src-tauri/src/agent_research_matrix.rs)                                                 | Reale Modelle ab Nutzerauftrag und unabhängige semantische beziehungsweise operationale Abnahme | Richtiges Ergebnis getrennt von Schema, Quellenlieferung und Controller-Done                     |

Die historische [Zwei-Modul-Abnahme](10-RESEARCH_VALIDATION.md) erreichte elf commandgebundene Done-Zustände, von denen nur sieben die Zusatzprüfung bestanden. Diese Zahlen sind keine aktuelle Erfolgsquote von SourceGuided. Die [V10-Nachbesserung](21-TASK-SCOPE-AND-PERMISSIONS.md) ist bereits im Bestand und darf nicht nochmals als offener Vollschemafehler behandelt werden. Sie löst nicht die übrigen sechs Ziele.

## 3 Was bleibt und was ersetzt wird

| Bestand                                                                           | Entscheidung                                                                          | Konkrete Umbaugrenze                                                                                                  |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `a3-domain`: AgentRun, GoalContract, TaskLedger, VerificationSpec, Evidence       | Behalten und gezielt erweitern                                                        | Kriterienabdeckung und strukturierte kleine Schritte; keine zweite Aufgabenautorität                                  |
| `AdvanceAgentController`, endliche Zustände                                       | Behalten                                                                              | Phasenaufträge innerhalb desselben Controllers, kein freier Chatloop als Codingsteuerung                              |
| `a3-repo-index`, Exact/Lexical/Graph, Task Lens                                   | Behalten                                                                              | Quellen nach aktueller Arbeit liefern; keine zweite Suche oder Indexierung                                            |
| `a3-context`, Freshness, Secretprüfung, Digest                                    | Mechanismen behalten, Packing umbauen                                                 | Volltextüberhang durch explizite aktuelle Teilsicht ersetzen; Codefenster nach Bedarf statt zwei pauschalen Seiten    |
| `agent_turn/*`, SourceWork/AfterChange/Choice/Arguments                           | Regulären neuen Modellpfad ersetzen                                                   | Kleine versionierte Phasenverträge; bekannte IDs und deterministische Übergänge im Core                               |
| `AgentWorkPlan` aus Markdown und Materializer                                     | Für neue Tasks ersetzen                                                               | Strukturierter Auftrag mit Kriterien und kleinen Umsetzungsschritten; Markdown nur Darstellung/Legacy                 |
| `ResearchWorkState` und ResearchHandoff                                           | Fachliche Pflichten/Quellenbezüge wiederverwenden, zwingenden Agent-Vorlauf entfernen | Ask und Agent teilen Quellenlieferung; kein obligatorischer kompletter Recherche-/Entwurfsdurchlauf vor erster Arbeit |
| `a3-workspace`, PatchUpdate, ProcessRunner, Policy, Approval, MutationCoordinator | Behalten                                                                              | Neue Editbeschreibung wird vor bestehender Grenze in denselben vollständigen Patch kompiliert                         |
| Acceptance-/Step-Verifier                                                         | Behalten, Auswahl und Abdeckung korrigieren                                           | Kein beliebiger Erstcommand als semantischer Erfolgsbeweis                                                            |
| libSQL, CAS, Journal, bestehende Projekt-/Recoverydaten                           | Behalten; neuen Datenbestand frisch initialisieren                                                       | Neue gebundene Felder; globale Chatdaten getrennt; neue Historie unveränderlich; keine Altübernahme                          |
| Provider/Ollama, Profile, Cancellation/Timeout                                    | Behalten                                                                              | Kleine Phasenschemas, tatsächliche Wirekosten und effektive Limits prüfen                                             |
| `ProductionAgentRunExecutor`, `AgentSessionManager`, Composition Root             | Verschlanken                                                                          | Fachregeln in Application, Desktop verbindet Ports und projiziert Zustände                                            |
| Chatdarstellung, Approval, Review, Queue, UI-Scheduler                            | Wiederverwenden                                                                       | Drei neue sichtbare Modi, Scope und klarer tatsächlicher Nachweisstatus                                               |
| Deep Map, Embeddings, Atlas, Flowansichten                                        | Bestehen lassen, für Kernpfad optional                                                | Keine automatische Voraussetzung; nur nachgewiesene aktuelle Navigationshilfe                                         |
| Alte Ausgabeverträge und Plan-Sessions                                            | Im neuen Produkt entfernen; P0-Reproduktion erhalten                                              | Keine zweite produktive Legacyengine; keine Übernahme alter Sessions oder offener Arbeit                                      |

## 4 Zielablauf und erste vertikale Scheibe

Die erste vollständige Scheibe ist bewusst klein: Ein lokales Modell erhält einen echten Bugauftrag in einem unbekannten kleinen Pythonprojekt, lokalisiert Code und vorhandenen Test, plant eine begrenzte Korrektur, erzeugt einen Bereichsedit und erreicht nach realer Prüfung den korrekten Abschluss. Die Lösung und Patchbytes dürfen nicht im Testmodell vorgegeben sein. Ein unabhängig falscher Patch darf kein Done erreichen.

Darauf bauen große Dateien, mehrere Module, Greenfield und lange Arbeit auf. Die Reihenfolge vermeidet einen neuen großen Unterbau ohne früh belegte Kernfunktion.

Für Chat, Ask und Agent gelten die Fähigkeiten aus dem [Zielvertrag](../SMALL_LOCAL_MODEL_AGENT_CORE.md). Freier Chat läuft als begrenzter Modelljob, Ask als read-only Projektarbeit, Agent über den bestehenden einzelnen mutierenden Controller. Planning bleibt eine echte Modellaufgabe, verteilt auf wenige Ergebnisgruppen und einen kleinen konkreten Horizont.

## 5 Lokale Modelle und eingefrorene Profile

Bei der anfänglichen Bestandsprüfung am 2026-11-06 wurden `ollama list`, `ollama --version` und ausgewählte `ollama show`-Metadaten lokal gelesen. Diese Prüfung startete weder Generation noch Pull und änderte keine Nutzereinstellung. Der abgefragte Server meldete Ollama 0.40.14. Die anschließend ausdrücklich beauftragten lokalen P0-Modellläufe sind im [P0-Arbeitsstand](22-P0_BASELINE_STATUS.md) getrennt dokumentiert.

| Modelltag genau wie verfügbar beziehungsweise angefragt | Aufgabe im Vergleich                                                             | Gelesener Metadatenstand                                                                                                                        |
| ------------------------------------------------------- | -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `ornith-1.5:9b`                                         | Primärmodell für kleine lokale Modelle; Ausgangsprofil 8192/2048                 | Vorhanden, 9.0B, Q4_K_M, Kurz-ID `e5df7dcdd8a2`; effektives Profil vor Run setzen/prüfen                                                        |
| `qwen38-27b-iq3s:latest`                                | Zweites Primärmodell; vergleichbares Ausgangsprofil 8192/2048                    | Vorhanden, 26.9B, IQ3_S, Kurz-ID `3b8e5b0c7386`; Tagdefault num_ctx 65536 wird im isolierten Test explizit auf 8192 gebunden                    |
| `qwen38-4k:latest`                                      | Verpflichtender positiver 4k-Schnitt, 4096/1024                                  | Vorhanden, num_ctx 4096, Kurz-ID `45fe387a57ba`                                                                                                 |
| `qwen38-8k:latest`                                      | Erweiterte Kontextkontrolle, 8192/2048                                           | Vorhanden, num_ctx 8192, Kurz-ID `43cbd3535073`                                                                                                 |
| `qwen38-16k:latest`                                     | Erweiterte Kontextkontrolle, 16384/4096                                          | Vorhanden, num_ctx 16384, Kurz-ID `095cb6be6a6f`                                                                                                |
| `bonsai-2:27b`                                          | Zusätzliches bereits verfügbares Modell für gezielte Profil-/Quantisierungstests | Vorhanden nach Nutzerhinweis erneut geprüft, 26.9B, Q2_0, Kurz-ID `f3c4f4208659`; Thinkingdefault medium, effektives Testprofil explizit binden |

4k, 8k und 16k bezeichnen hier Kontextvarianten und werden nicht als drei unabhängige Modellfamilien gezählt. Gleiche Gewichte oder Templates werden erst nach Digest-/Metadatenprüfung behauptet. Architektonische Maximalwerte aus `ollama show` ersetzen nicht das effektive Laufprofil.

Vor jeder Vergleichsserie wird ein Manifest eingefroren: vollständiger Modell-Digest, Template-/Parameterbezug, Quantisierung, Serverversion, Hardware, effektives num_ctx/num_predict, Thinking-/Schema-Verhalten, Sampling, Prompt-/Compiler-/Toolpolicyrevision, Gitcommit, Aufgabenrevision, Zeit-/Aktions-/Reparaturbudget und OS. Dasselbe Modell hat im alten und neuen Kern identische Modellparameter. Thinking wird nicht implizit gratis gerechnet oder während des Vergleichs umgeschaltet.

Primärprofile verwenden 8k, damit ein größeres Kontextfenster nicht die Orchestrierung kaschiert. Bei nachgewiesener Profilunfähigkeit wird diese vor Kandidatenmessung dokumentiert; das Profil oder Primärmodell wird nicht nachträglich passend ausgewählt. Nutzerkatalog und gespeicherte Providerkonfiguration bleiben unangetastet. GPU-Läufe sind seriell und besitzen Cancellation, feste Frist und begrenzte Ausgaben. Gepaarte Vergleiche wechseln vorab festgelegt zwischen AB und BA, mit gleichem Warmup-/Lastzustand und identischen äußeren Zeit-, Modelltoken- und Werkzeugbudgets.

Vor der ersten P0-Generation ist das lokale Vergleichsprofil ausdrücklich festgelegt: FormatFieldOnly, Temperatur 0, Top-p 1, Parallelität 1, keine Stopsequenzen und der bestehende konservative Bytezähler. Das ist ein bewusstes Benchmarkprofil, keine Behauptung über den Desktopstandard RepeatSchemaInPrompt. Der unveränderte Ollama-Adapter sendet für diese Tags think=false; geerbte Tagparameter und die getrennten kleineren Probegrenzen werden im Laufmanifest gehalten. Alt- und Neukern verwenden später dasselbe eingefrorene Profil.

## 6 Reihenfolge der Arbeitspakete

Genau ein Paket wird gleichzeitig umgesetzt. Jedes Paket nennt Ergebnis, Belege und Stopbedingung. Ein Häkchen ist erst nach echter Abnahme zulässig. P0 ist verifiziert und eingefroren. P1 bis P6 sind verifiziert. Aktuell wird P7 bearbeitet.

Für P2 bis P10 gilt ausdrücklich: Neue Steuerungsverträge haben echte Integrationstestverbraucher, bleiben aber ausschließlich aus der Kandidaten-Testkomposition erreichbar. Produktiv ausgewählte Materializer-/SourceGuided-/Controllerpfade werden in diesen Paketen nicht vorzeitig umgeschaltet. Neue Datenbestände werden ausschließlich in isoliertem temporärem AppData eingerichtet und geprüft. P11 ist die einzige Produktionsumschaltung mit frisch initialisiertem A^3-Datenbestand gemäß ADR-0119; alte Entwicklungsdaten werden nicht migriert. Gemeinsame Sicherheitskorrekturen dürfen den alten Pfad nur mit dessen unveränderten Vertragsregressionen berühren; es gibt kein öffentliches Feature Flag und kein alternatives Produktrouting.

### P0 Ausgangsmessung und unabhängige Prüfungen

Abhängigkeit: keine Architekturänderung; bestehende Profile und Toolgrenzen verwenden.

Aktueller Aufbau, verifizierte Kontrollen und offene Messgrenzen stehen im
[P0-Arbeitsstand](22-P0_BASELINE_STATUS.md). Das [Freeze-Manifest](../../fixtures/local-agent-eval-v1/p0-freeze.json) pinnt Eingaben, Testbinary und alle sechs Versuche. P0 ist abgeschlossen; die Produkt- und Modellabnahme bleiben offen.

- [x] Aktuelles HEAD mit produktivem SourceGuided einfrieren; historische SingleAction-Fixturedefaults nicht als Produktbaseline verwenden.
- [x] Die Aufgaben aus Abschnitt 7 mit Sollverhalten, erwarteten Kriterien, geschützten Dateien, Zeit-/Aktionsgrenzen und Orakeln pinnen.
- [x] Die tatsächlichen ausführbaren Tests und deren erwartete IDs/Anzahl prüfen; der vorhandene Mini-`pytest.py` ist kein echtes pytest und Exit 0 bei null Tests kein Testbeleg.
- [x] Kleinen Vorlauf pro Primärmodell und 4k-Profil über den echten Produktionspfad ausführen; das vollständige Hauptpaket danach ohne weitere Profilauswahl einfrieren.
- [x] Baseline einschließlich Fehlversuchen, Nichtverfügbarkeit, Schemafehlern, Tokens, tatsächlichen Quellen, Aktionen und unabhängigen Ergebnissen speichern.

Belege: reproduzierbarer Runner, versionierte Fixtures/Erwartungen und content-sichere normalisierte Ergebnisberichte. Kontrollzustände mit vorgegebenen Modellaktionen bleiben als Infrastrukturtests separat bezeichnet. Dateien/Oracleinputs außerhalb des Worktrees sind keine behauptete OS-Sandbox.

Gate: Jeder verpflichtende Fall besitzt ein unabhängiges entscheidbares Soll und eine tatsächlich startbare Messung. Ein bereits scheiternder alter Kern ist ein Ausgangswert, kein Grund zur Entfernung der Aufgabe. Falls Messinfrastruktur nicht funktioniert, zuerst diese reparieren und keinen neuen Kern bauen.

### P1 Architekturvorschlag annehmen und Arbeitsvertrag ausrichten

Abhängigkeit: vor P2; P0 kann vorher stattfinden.

- [x] ADR-0119 mit seinen präzisen Teilablösungen durch den Maintainer entscheiden lassen. Ausdrückliche Annahme und Umsetzungsauftrag am 2026-11-06; im ADR festgehalten.
- [x] Nach Annahme AGENTS.md, Architekturregeln, Produktanforderungen, Memory-/Kontext-, Security-, Daten-/Persistenz- und IPC-Dokumentation konsistent auf die neuen Verträge ausrichten.
- [x] Fortbestehende Sicherheits-/Recoveryinvarianten und lediglich obsolete historische Prosaformen in einer Entfernungsliste unterscheiden.

Gate: Keine stillschweigende Lockerung vollständiger aktiver Constraints, 22-Prozent-Outputreserve, Einzelrepair, Root-/Secretprüfung, ADR-0117 oder ADR-0118. Ohne Annahme bleibt Produktimplementierung angehalten; die konkrete vorgeschlagene Entscheidung liegt bereits reviewbar vor.

Nachweis P1 am 2026-11-06: Die genannten Regeltexte sowie Systemarchitektur und
Aufgaben-/Berechtigungsdetail sind ausgerichtet. Die
[Ablösungsliste](../SMALL_LOCAL_MODEL_AGENT_CORE.md#ablösungsliste-und-erhaltene-invarianten)
nennt ersetzte Steuerungsformen, weiter gültige Grenzen und Entfernung erst in P11.
Zwei unabhängige lesende Gegenprüfungen gegen ADR-0119 fanden keinen Widerspruch.
`node scripts/check-markdown-links.mjs` prüfte 185 Dokumente/923 lokale Links;
`git diff --check` bestand. Die 20 eingefrorenen P0-Eingaben sind weiterhin
bytegleich, historische akzeptierte ADRs unverändert. P1 ändert Dokumentation,
keine Laufzeit; die in P0 bestandenen Rust-/Python-Gates werden dadurch nicht als
Nachweis neuer Produktfähigkeiten umgedeutet.

### P2 Kriterien und kleine strukturierte Arbeitsaufträge

Abhängigkeit: P1, P0-Fixtures.

- [x] Intake entlang begrenzter Originalauftragsteile modellieren; Ergebnisanforderungen, Constraints, Nicht-Ziele und echte offene Nutzerentscheidungen erhalten.
- [x] Unveränderliche Originalteile mit Bereichsreferenzen im bestehenden Goal-Store halten; 64-KiB-Grenze explizit prüfen, heutige 8-KiB-Kürzung und GoalObjective als alleinigen Ursprung vermeiden.
- [x] Acceptance Criteria an einzelne Ergebnisse und konkrete Nachweisarten binden; vollständige Abdeckung zum Originalauftrag führen.
- [x] Neue Pläne strukturiert statt über `Implementation Changes`/`Test Plan` aufnehmen; kleine Vorschau und nächsten konkreten Schritt materialisieren.
- [x] Kriterien-/Schrittzuordnung und benötigte Metadaten über bestehende Ports mit CAS im versionierten neuen Schema speichern; Neuinitialisierung/Reopen/Rollback testen, ohne Upgradepflicht alter Entwicklungsstände.
- [x] Den generischen letzten-Kriterium-/Erstcommand-Erfolg im Materializer ersetzen; bestehende Verifier nur um tatsächlich fehlende Abdeckungsregeln erweitern.
- [x] Rollierenden Horizont im Kandidaten schließen: offene globale Kriterien nach letzten aktuellen Schritten führen per Ledger-/Journal-CAS zu weiterer Planung; Budgets und ausstehende Planung crashsicher halten, Acceptance erst bei Gesamtabdeckung.
- [x] Produktive Verifikationsrezepte und benötigte Evidencearten konkret zuordnen; fehlende Rezeptfähigkeit als Implementierungsbedarf und offenen Nachweis behandeln, nicht durch Eval-Orakel ersetzen.

Belege: echte Goal-/Ledger-/Storageintegration für einen Änderungsauftrag, einen AlreadySatisfied-Fall und einen Auftrag ohne neue Tests. Ein grüner Linter bei unverändert falschem Verhalten bleibt offen; eine pure Dokumentationsänderung erhält keine erfundene Laufzeitpflicht. Auftrag über 16 KiB, späte Einschränkung, Reopen und Löschen des Präsentationsverlaufs verlieren keinen Ursprungsteil. Mehr als drei notwendige Schritte und ein Crash zwischen Horizontabschluss und neuer Planung erhalten Kriterien, Verbrauch und richtige Fortsetzung.

Die erste Speicherscheibe ist verifiziert. `MaterializeOriginalTask` materialisiert
den vollständigen Originalauftrag, quellengebundene Kriterien und höchstens drei
passende Umsetzungsschritte bis zu einem dauerhaft gespeicherten Ready-Ledger.
Sie startet keinen Run und behauptet keine Modellinterpretation oder vollständige
Auftragserfüllung. Legacy-Goals erhalten ausdrücklich keinen aus dem früher
gekürzten Objective erfundenen Ursprung. Der Unicode-Grenzfall benötigt bis zu
33 Teile; Gesamtlimit und Teilgröße bleiben 64 KiB beziehungsweise 2048 Bytes.

Die erste feste Kandidaten-Schemaeinrichtung V43/V44 war ausschließlich adapterintern
in Tests erreichbar. Produktions-`CURRENT` bleibt V42. Jeder Kandidatenstand akzeptiert nur
seine exakte Version oder ein tatsächlich leeres V0; inkompatible Bestände werden
read-only unverändert abgewiesen. Ein normales Produktopen weist neuere Kandidaten ebenfalls
vor Writes zurück. Das folgt der ergänzten ADR-0119 zum frischen Datenbestand;
es besteht keine Übernahmeverpflichtung alter Entwicklungsstände. Original,
Teile und Kriteriumsbindung werden in derselben Goaltransaktion geschrieben.
Echte libSQL-Tests belegen vollständiges Store-Reopen einschließlich 64-KiB-
UTF-8-Auftrag und später Anforderung, atomaren Rollback, Revisions-CAS, erhaltene
Historie, verweigerte Herkunfts-/Nachweisabschwächung und begrenztes Lesen
beschädigter Daten. Ein fehlgeschlagenes Ledger-Schreiben lässt den gültigen
Goal erhalten; erst ein expliziter erneuter Aufruf kann denselben Auftrag weiter
materialisieren.

Bestandene Prüfungen dieses Schnitts:
`cargo test -p a3-domain -p a3-application --offline --locked` (587 Tests),
`cargo test -p a3-storage-libsql --lib plan22 --offline --locked` (16 Tests),
`cargo test -p a3-storage-libsql --test connection_lifecycle --offline --locked`,
der Desktop-Grenztest `original_goal_binding_failures_keep_the_existing_closed_ui_error`,
`cargo test --workspace --all-features --offline --locked`,
`cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings`,
`cargo fmt --all -- --check`, Markdown-Links und `git diff --check`.
Die Tests liegen in `plan22_goal_contract_tests.rs`, `plan22_candidate_open_tests.rs`
sowie den Domain-/Application-Vertragstests; sie sind keine Modellqualitätsabnahme.

Die anschließende Source-Lifecycle-Scheibe ist ebenfalls verifiziert:

- Origins hängen auch an Constraints, Nicht-Zielen und bestätigten Entscheidungen;
  vollständige strukturelle Abdeckung wird aus allen vier Herkunftsfamilien abgeleitet.
  Ungedeckte Bytes einschließlich Whitespace und Regeln ohne Herkunft bleiben offen.
- Der einzige Acceptance-Verifier prüft Bindungen und ursprüngliche Nachweispflichten
  vor Evidence-Reads. Abgeschwächte Lint-/Command-/Diff-Specs ersetzen keinen Runtimebeleg.
  Source-Evidence muss zudem exakt zum aktuellen Snapshot gehören; unveränderte
  Testdateien verdecken eine Änderung am getesteten Produktionscode nicht.
- Eine gemeinsame Admission im ProductionExecutor und AgentTurn sperrt unvollständige
  Source-Intakes vor Compiler, Provider und Werkzeugen. Sie sperrt keine dauerhafte
  Speicherung unfertiger Anforderungen. Source-Revision, History und Store weisen
  reine Owner-Neubindungen ohne inhaltliche Änderung ab.
- Ein nativer AlreadySatisfied-Fall führt zwei vorhandene Python-Unittests über echte
  Discovery, Ask-Request/Grant, ProcessRunner, BuiltinFactory und Ledger/Journal aus.
  Evidence als erster Port nach vollständigem Reopen genügt dem einzigen Verifier;
  Done wird persistiert und erneut geladen. Alle Projekt-/Testbytes bleiben gleich.
  Das qualifiziert den `All`-Selector, keine echten `Exact`-Testnamen.

Nachweise dieses Standes: 632 Domain-/Application-Tests, 29 native P2-Storagekontrollen,
`connection_lifecycle`, Workspace mit 1585 bestandenen/25 bestehenden ignorierten Tests,
Clippy aller Targets/Features mit `-D warnings`, Formatierung, Links und Diffprüfung.
Der Workspace-Nachweis stammt aus einem sequentiellen Lauf nach einem dokumentierten
Compiler-ICE in unveränderten Tests; deren 13 Netzwerk-/Prozesskontrollen bestanden
auch separat. Ein Katalogtest setzte eine Kurzsuche fälschlich nur zum Blattnamen in
Beziehung; der reproduzierte Parent-/PID-Treffer ist durch dauerhaften Parent-17-
Gegenfall samt korrekter Pfadmatch-/Pagingprüfung geschlossen, Produktsuche unverändert.
Die fehlgeschlagenen Läufe bleiben als Diagnose erhalten.

Die Capture-/Promotion- und echten Änderungsverbraucher sind danach verifiziert:
`CaptureGoalIntake` speichert exakte Rawbytes create-only im selben Goal-Lebenszyklus,
bevor irgendeine Modellinterpretation benötigt wird. Pending hat keinen GoalDraft,
kein Kriterium, keinen Ledger und keinen Run. Die atomare CAS-Promotion validiert
Task/Initialreferenz, unveränderte Capturezeit, Digest, Bytes und vollständige
Bindung; ein Fehler nach dem Consume stellt Pending per Rollback wieder her.
Normales Create/Materialize kann die reservierte Taskidentität nicht umgehen.
Der feste Kandidat öffnete in dieser Scheibe ausschließlich sein V45 oder wirklich leeres V0;
V43/V44 bleiben unveränderte interne Aufbauschritte, kein Altupgradeangebot.

Ein echter falscher-Code-Gegenfall belegt FailedExit, offenen Schritt und verweigerte
Acceptance nach Reopen. Ein echter Änderungsfall durchläuft Patchpreview/-Grant,
Apply/Index B und den realen vorhandenen Context Compiler; der Test-Schritt bleibt
bis zum tatsächlichen Test InProgress. Frisch entdeckter Catalog, neue Bestätigung
und eigener Prozessgrant führen anschließend zu passender Evidence und Source-Done.
Testdateien bleiben bytegleich. Das 16k/4k-Native-Contextfixture wird keinem Modell
übergeben und ist kein Beleg für P4 oder ein 4k-Modellprofil. Die zusätzliche direkte
Tokio-Testkante stellt ausschließlich den tatsächlich erforderlichen Checkpoint-
Runtimeowner bereit; keine Bibliothek oder Paketversion kam hinzu.

Bestandener Stand dieser Scheibe: 652 Domain-/Application-Tests, 39 native P2-Kontrollen,
der neue geschlossene Desktop-Errormappingtest, `connection_lifecycle`,
`cargo test --workspace --all-features --offline --locked` mit 1616 bestandenen
und 25 bestehenden ignorierten Tests, anschließend sequentiell Clippy aller
Targets/Features mit `-D warnings`, Formatierung, 185 Markdown-Dokumente/933 Links
und Diffprüfung. Die 20 P0-Snapshotdateien und der eingefrorene Worker sind erneut
hashgleich; sechs ursprüngliche Modellversuche bleiben erhalten.

Die anschließende V46-Scheibe trennt Addresses/Fortschritt und Proves als
abschließenden Kriteriumsnachweis. Beide Rollen bleiben über CAS und Reopen
erhalten; Source-Admission verlangt passende eigene Schrittverifikation, bei
Proves zusätzlich die ursprüngliche globale Nachweispflicht. Nur Proves zählt
für globale Acceptance und noch ungeplante Nachweise. Ein real genehmigter
Patch mit Diff-Evidence schließt deshalb den lokalen Addresses-Schritt ab,
während globale Acceptance `InsufficientEvidence` bleibt. Ein späterer passender
Test-Proves-Schritt erreicht nach zwei echten bestehenden Tests und Reopen Done.
Die Testdateien bleiben unverändert. Der Replan ist im Komponententest explizit
gewählt, kein Beleg für automatische Horizontplanung oder Modellleistung.

Der aktuelle Testkandidat akzeptiert ausschließlich sein V46 oder leeres V0;
frühere Aufbauschritte bleiben unverändert und werden nicht als Altupgrade
angeboten. Die ergänzte begrenzte Intake-Liste findet ohne Session-Task-ID
ausstehende Arbeit wieder: höchstens 32 IDs pro Seite und ein Lookahead,
Worktreebindung und kein Rawtext-Lesen. Der spätere gebundene Load ist Pflicht;
Discovery erteilt keine Ausführungsfreigabe. Zwei native Fälle belegen Reopen als
ersten Portaufruf, 40 Captures mit Promotion/Paging, anderen Worktree und begrenzte
korruptionsfeste ID-Projektion. Eine unabhängige lesende Gegenprüfung fand keinen
Blocker. Capture/Discovery führen weiterhin null Modellaufrufe aus.

Nachweis dieses Standes: die 699 Rollen-Domain-/Application-/Context-Tests,
zusätzlicher Application-Seitengrenztest und 52 native P2-Kontrollen bestanden.
`cargo test --workspace --all-features --offline --locked --jobs 2` besteht mit
1641 Tests und 25 bestehenden ignorierten Tests. Clippy aller Targets/Features
mit `-D warnings`, der separate `connection_lifecycle`, Formatierung,
185 Markdown-Dokumente/933 Links und Diffprüfung bestehen. Ein früherer nativer
Kindprozessabbruch bleibt mit ungeklärter Ursache erhalten; isolierter Test und
anschließender Gesamtbatch bestanden. Die separat reproduzierte Fixture-
Reservierungslücke ist durch atomare begrenzte Namensreservierung geschlossen
(vier Regressionen); Kindfehler bleiben durch `--nocapture` sichtbar. Das belegt
keine Ursache des ursprünglichen Abbruchs. Der erste Workspace-Lauf scheiterte an
belegter Windows-Speichererschöpfung und Auslagerungsdateigrenze. Der nächste
Lauf wurde durch eine Quelländerung während der Doc-Tests inkonsistent;
anschließend bestand der vollständige unveränderte Stand mit zwei Buildjobs.
Alle fehlgeschlagenen Logs bleiben erhalten.

Der V48-Kandidat verbindet Intake-Review, Originalauftrag, Ready-Ledger und
dauerhafte Horizontplanung. Die kleine Planvorschau zeigt bis zu drei beabsichtigte
Ergebnisse mit Status und Prüfart. Sie markiert den ersten Ready-Schritt und hält
die noch nicht geplanten globalen Nachweise sichtbar. Der native Run-Start
verwendet diese Auswahl und bindet sie per Ledger-/Journal-CAS.

Ein tatsächlicher Vier-Schritte-Fall führt drei separat genehmigte Änderungen
aus. Der Core fordert anschließend weitere Planung an und öffnet den Store neu.
Nach dem Anhängen folgt ein zweites Reopen. Die Aktivierung verwendet die bereits
gespeicherte Planung, denselben Run und seine ursprünglichen Budgetmetadaten.
Zwei bestehende Tests belegen alle vier Originalkriterien. Die Ausgangs-Usage ist
kontrolliert vorgegeben. Die Datei- und Prüfwirkungen stammen aus den echten
Adaptern.

Ein separater Auftrag ändert ausschließlich `README.md` und erreicht nach Reopen
mit aktueller Diff-Evidence und ohne Prozesslauf `Done`. Der 64-KiB-Fall löscht eine
tatsächliche Agent-Präsentation und öffnet den Store vollständig neu. Alle
Originalbytes, die späte Einschränkung, der weiterhin offene R30-Nachweis und die
Vorschau bleiben erhalten. Die Intake-Kontrollen prüfen außerdem echte offene
Fragen, unveränderte Vorschlagsquellen und atomare Promotionssperren bei einer
ausgelassenen Einschränkung.

Der erste integrierte V48-Lauf endete während der Windows-Abmeldung. 35 Fälle
bestanden und 33 Kindprozesse scheiterten vor ihrem Teststart mit
`STATUS_DLL_INIT_FAILED_LOGOFF`. Das Windows-Systemprotokoll bestätigt die
Abmeldung zum Ende des Logs. Der unveränderte Wiederholungslauf bestand mit
68 Fällen. Der erweiterte integrierte Lauf mit den beiden Abnahmetests und der
Vorschau bestand mit 69 nativen Fällen. Am 2026-11-09 bestand das abschließende
Workspace-Gate mit 1.702 Tests und 25 bereits ignorierten Fällen. Die P2-Abnahme
umfasst außerdem den separaten nativen Verbindungstest, Clippy mit verweigerten
Warnungen, Formatierung, Markdown-Links und `git diff --check`.

- `cargo test --workspace --all-features --offline --locked --jobs 2`
- `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`
- `cargo test -p a3-storage-libsql --test connection_lifecycle --offline --locked --jobs 2`
- `cargo fmt --all -- --check`, `node scripts/check-markdown-links.mjs`, `git diff --check`

P4 ergänzt den modellgestützten Intake und die dauerhafte Job- und
Verbrauchsreservierung. Der Core muss vor dem Commit eines Findings die passende
Nachweispflicht liefern oder die spätere Konkretisierung gesondert auditieren.
P9 ergänzt tatsächliche Nutzerantworten mit ihrem eigenen Originalbezug.

Die [Rezeptmatrix](../SMALL_LOCAL_MODEL_AGENT_CORE.md#produktive-rezepte-und-offene-verbraucher)
ordnet die tatsächlich vorhandenen Evidence-Erzeuger und fehlenden Fähigkeiten
ihren konkreten Verbrauchern zu. Die abgehakte Zuordnung behauptet keine
Implementierung der offenen CLI-/HTTP-/manuellen Rezepte. Für P5 ist zunächst
der vorhandene Python-All-Pfad am unveränderten A01 zu schließen: dessen
`pyproject.toml` verhindert heute die unittest-Fallback-Discovery. Die grünen
manifestfreien Natives ersetzen diesen Nachweis nicht. Exact/gezielte Python-
Commands, Mixed-Roots, Rust und Node folgen vor den jeweils genannten Verbrauchern.

Gate: F3 ist durch gezielte Gegenbeispiele geschlossen. Keine neue abstrakte Planplattform und keine dauerhafte zweite Anforderungsquelle. Bereits jetzt existiert ein konkreter getesteter Verbraucher der neuen Typen.

### P3 Exakter Bereichsedit durch die bestehende Werkzeuggrenze

Abhängigkeit: P2.

Für die sichtbare Freigabe eines Bereichsedits gilt
[ADR-0120](../adrs/0120-positionsgebundene-patchvorschau.md). E3 wählt im Kandidaten
den tatsächlichen Änderungsausschnitt unter denselben Größen- und Freigabegrenzen.

- [x] Core-gebundene Editfenster aus tatsächlich gelieferten aktuellen Bytes erzeugen.
- [x] Kleinen Modellvertrag Alias plus Ersatztext strikt dekodieren; daraus bestehenden PatchUpdate kompilieren.
- [x] Bestehende E3-Vorschau mit exaktem Änderungsausschnitt nach ADR-0120 verwenden. Policy, Permissionrevision, Mutationslease, Livehashprüfung, Publish und Unknown-Recovery bleiben derselbe Ausführungspfad.
- [x] Add kleiner neuer Dateien und Insert am aktuellen Dateischluss über dieselbe Scopeprüfung ermöglichen.
- [x] Alte Vollfile-Modellausgabe für neue Updateaufträge ausschließen; intern bleibt der vollständige sichere Patch erhalten.

Belege: echte Workspace-Porttests für >200-KB-Datei, Ziel im mittleren Abschnitt, CRLF, Unicode, große unveränderte Wächterbereiche, doppelte Textvorkommen, Aliasfälschung, Überlappung, Live-Edit zwischen Vorschau und Apply und partiellen Fehler. Separate Fälle bearbeiten die erste Zeile einer BOM-Datei und fügen am gebundenen EOF ein; BOM bleibt erhalten, CR wird nicht doppelt konvertiert. Bytes außerhalb zugelassener Fenster bleiben identisch.

Der erste native Lauf erreichte bei zwei Python-Fixtures die Editphase noch nicht.
Der initiale Indexaufbau einer 252.086-Byte-Datei mit 9.000 kurzen Kommentarzeilen
überschritt ein bestehendes Zeitlimit. Eine gültige 42-Byte-Python-Datei mit BOM
scheiterte mit `InvalidResult`. Der Python-Adapter übernimmt den Tree-sitter-Bereich
des Moduls, bindet die synthetische Auswahl jedoch fest an Byte null. Das widerspricht
der Auswahlgrenze, sobald Tree-sitter die BOM vor dem Modulstart überspringt.
Die P3-Vorschau- und Editgrenzen werden getrennt von diesen Parserfällen geprüft.
Beide Fehler bleiben für das unveränderte A02 vor P7 relevant.

Der gezielte native Wiederholungslauf bestand mit neun Fällen. Der mittige Edit
in der 252.086-Byte-Markdown-Datei zeigt den absoluten Hunk vor der echten
Freigabe. Die Außenbytes und das erste identische Vorkommen bleiben nach Apply
und Reopen erhalten. Die Kurzfälle decken Markdown mit BOM sowie Python-EOF
und Python-Add ab. Ein absichtlich gescheiterter Ergebnis-Commit nach dem
Schreiben erzwingt beim Reopen echte Unknown-Reconciliation. Die Apply-
Konfliktfälle prüfen den publizierten Fremdhash vor der erneuten Kontextbildung
und erhalten die unterschiedliche Disposition ohne Wirkung oder mit Teilwirkung.
Der Workspace-Patchvertrag bestand mit 17 Fällen.

Im ersten Workspace-Gesamtlauf fehlte beim bestehenden Timeout-Test die
PID-Datei, mit der die Fixture einen gestarteten Kindprozess nachweist. Sie erfasst die PID
direkt nach erfolgreichem Spawn und wartet bei Cancellation auf einen
vollständigen Eintrag. Der Test lässt unter Workspace-Last fünf Sekunden für
beide Prozessstarts und prüft weiterhin Timeout, Prozessende und Ereignisfolge.
Der separat geprüfte kurze Timeout sowie der Produkt-Runner behalten ihre
bisherigen Grenzen.

Abnahme am 2026-11-09. Das wiederholte Workspace-Gesamtgate bestand mit 1.720
Tests und 25 bereits ignorierten Fällen. Der Prozessvertrag bestand mit 11
Fällen. Clippy mit verweigerten Warnungen, Formatierung, Markdown-Links und
`git diff --check` bestanden. Der native Editverbraucher liefert eine
kontrollierte Antwort im kleinen Phasenvertrag. Die echte Modellanfrage und
ihr Budget sind Gegenstand von P4.

- `cargo test -p a3-workspace --test workspace_patch_contract --offline --locked --jobs 2`
- `cargo test -p a3-storage-libsql --lib --offline --locked --jobs 2 -- plan22_goal_contract_tests::existing_test_tests::changed_tests::`
- `cargo test -p a3-workspace --test process_runner_contract --all-features --offline --locked --jobs 2`
- `cargo test --workspace --all-features --offline --locked --jobs 2`
- `cargo clippy --workspace --all-targets --all-features --offline --locked --jobs 2 -- -D warnings`

Gate: F1 geschlossen. Kein unscharfer Ersatz und keine automatische Konfliktübernahme. Neue Vollfile-/Fensterbeschreibungen dürfen nicht unterschiedlich strenge Policywege besitzen.

### P4 Kleine Phasenverträge und passende aktuelle Quellen

Abhängigkeit: P2/P3.

- [x] Deterministische aktive Auftragssicht auf die dauerhaften Kriterien und den aktuellen Schritt herstellen; alle geltenden Constraints und relevante Fehler erhalten.
- [x] Je Modellphase genau ihren kleinen Vertrag senden; IDs, Hashes und deterministische Phasenwahl im Core halten.
- [x] Auch beim Planen/Editieren eine kleine reguläre NeedEvidence-/TooLarge-Rückgabe zulassen; keine Mutation und kein Strukturrepair allein wegen nachträglich erkannter fachlicher Lücke.
- [x] Vorhandene Task Lens und Safe Reader für explizite Ziele, zusammengehörige Funktionsbereiche und benötigte Aufrufer verwenden.
- [x] Tatsächlich gelieferte Bereiche, Quellenlücken und Neuheit nachhalten; Vier-Read-Änderungszwang entfernen.
- [x] Einen typisierten Livekonflikt vor der Patch-Policy mit kontrollierter Neuaufnahme der aktuellen Quellen oder einem belegten Halt behandeln. Aus diesem Konflikt folgt kein automatisches Rebinding desselben Editvorschlags.
- [x] Die vollständige Requestprojektion einschließlich Formatfeld, möglicher Schemawiederholung, Thinking-/Protokollkosten und Outputreserve prüfen.
- [x] Vor tatsächlicher Provider-I/O jobgebundenen Verbrauch und Einzelrepair dauerhaft reservieren; Intakeverbrauch und ursprüngliche Frist ohne Reset in den Run übernehmen. Modellaufrufe und Core-Werkzeugaktionen getrennt korrekt zählen, ohne erfundene Modellinteraktionen zur Budgetbuchung.
- [x] Den 4k-Entwurf aus dem Detaildokument für jede Phase mit tatsächlichen Wireverträgen beweisen; nötige aktive Quellen dürfen nicht für einen formalen Compile-Erfolg unbrauchbar gekürzt werden.

Belege: deterministische Digests, Secret-/Stale-Negativfälle, tatsächliche Quellenlieferung ohne Modellstubgedächtnis und ein langer Gesamtauftrag, dessen passende aktuelle Teilsicht im 4k-Profil mit nutzbarem Editfenster arbeitet. Auch NeedEvidence/TooLarge passt in den echten 4k-Vertrag. Mindestens 30 dauerhafte Anforderungen blähen die Restabdeckungsprojektion nicht zur vollständigen Promptliste auf; ein später globaler Constraint bleibt wirksam. Ein read-only Bedarf bleibt auch nach mehreren Metadatenreads darstellbar. Wiederholungen ohne Neuheit werden endlich behandelt.

Der erste echte 4k-Editversuch am unveränderten A10-Entwicklungsauftrag steht in
`target/plan22-p4-runs/a10-first-edit-001`. Der gespeicherte Auftrag hatte 34.371
Bytes und 30 Originalbezüge. Sechs aktive Kriterien, die späten Constraints und
53 aktuelle Quellbytes ergaben 2.568 tatsächlich serialisierte Requestbytes.
Ollama zeigte 4.096 Kontexttokens. Der Aufruf war vor dem Versand mit 2.952
Prompt- und 1.024 Ausgabetokens reserviert. Der Server meldete 464 Eingabe-
und 91 Ausgabetokens und den regulären Stopp. Der Core speicherte
`invalid_structure` und ließ keinen Patch zu. Alle zwölf Fixturedateien blieben
bytegleich. Ollama lief mit 0.40.15, während P0 0.40.14 dokumentiert. Dieser
Versuch ist keine gepaarte Baseline-Messung.

Der veränderte Vertrag in `target/plan22-p4-runs/a10-first-edit-002` liefert
drei vollständige Schemaobjekte. Der Body mit allen aktiven Kriterien und
Constraints umfasst 2.641 Bytes. Die Reservierung von 3.025 Prompt- und 1.024
Ausgabetokens war vor dem einzigen Dispatch im Store sichtbar. Auf der GPU
lief das ausgewählte Modell mit 4.096 Kontexttokens und meldete 432 Eingabe-
und 46 Ausgabetokens. Es erzeugte beim ersten Aufruf eine gültige Änderung.
Der tatsächliche Controller genehmigte und veröffentlichte den Patch. Die
unabhängige Prüfung bestätigte die exakte Ganzzahlformel für vier Eingaben
einschließlich negativer und sehr großer Werte sowie elf geschützte Dateien.
Nach Reopen hält der Store den unveränderten Originalauftrag, den lokalen
Diff-Nachweis, den gebuchten Verbrauch und alle 24 offenen Laufzeitpflichten.
Der positive Befund gilt für diesen einzelnen 4k-Editauftrag.

Der erste echte 4k-Intake in `target/plan22-p4-runs/intake-readme-001`
zitierte die 35 Originalbytes vollständig und klassifizierte die geforderte
Änderung als bereits getroffene Entscheidung. Die gespeicherte Review enthielt
keine Pflichtanforderung. Die fachliche Prüfung stoppte vor Goal-Promotion und
Run-Start. Es gab keine Werkzeugaktion.

Der überarbeitete Intake-Prompt unterscheidet ausdrücklich angeforderte
Änderungen von bereits abgeschlossenen Entscheidungen. In
`target/plan22-p4-runs/intake-readme-002` erkannte dasselbe Modell den Auftrag
als Pflichtanforderung mit dem exakten Originalbereich. Der vollständige Body
umfasste 1.516 Bytes. Vor dem Versand waren 1.900 Prompt- und 1.024
Ausgabetokens reserviert. Ollama meldete 142 Eingabe- und 80 Ausgabetokens
bei 4.096 effektiv geladenen Kontexttokens. Der bestehende Goal-Store
übernahm die Review. Nach Reopen hielt der verknüpfte Run die ursprüngliche
absolute Frist und den gebuchten Intake-Aufruf. Die Projektdateien blieben
bytegleich. `target/plan22-p4-runs/intake-readme-003` wiederholte den Befund
mit dem im Repository hinterlegten Supervisor und demselben eingefrorenen
Worker. Beide Befunde gelten für einen Intake-Auftrag mit einem Quellfenster.
Der V50-Workspace-Gate lief mit 1.754 bestandenen und 27 explizit
ignorierten Tests, Clippy mit `-D warnings` und Formatprüfung durch.
Der vorbereitete Ledgerschritt dient hier der Budgetprüfung. Modellplanung,
mehrere Intakefenster und die vollständige P4-Abnahme bleiben offen.

V51 hat dieselbe Komponente mit dem neuen Store im Lauf `intake-readme-004`
erneut über Ollama bestätigt. Die native Reparaturprüfung zeigt zwei erhaltene
Intake-Aufrufe, einen Repair und dieselbe absolute Frist nach Promotion und
Run-Reopen. Der Core erlaubt keinen weiteren Erstaufruf über denselben oder
einen überlappenden Originalbereich. Die separate Vorschauprüfung ändert
außerhalb des Modellfensters eine Zeile mit gleicher Bytelänge. Das Ledger
hält den Konfliktcode, das Journal den Halt. Die fremden Bytes
bleiben erhalten, der Run stoppt vor einer neuen Policyentscheidung und es
entsteht kein Patch- oder Verifikationsversuch.
Der V51-Workspace-Gate war mit 1.757 bestandenen und 27 explizit ignorierten
Tests, Clippy mit `-D warnings`, Format- und Markdown-Linkprüfung grün.

Der erste begrenzte Planvertrag akzeptiert höchstens drei ausgewählte
Core-Angebote. Die Antwort liefert Schrittziel und echte Voraussetzungen.
Der Core bindet Kriterienrollen, Nachweispflichten und Identitäten. Sieben
Vertragstests und der native Durchschnittsfall prüfen den Vertrag. Dessen
echter Ollama-Encoder passt mit 4.096 Kontext- und 1.024 Ausgabetokens.
Eine vorgegebene Antwort führt über den vorhandenen Store zu einem nach
Reopen unveränderten Ledger. Die Modellplanung selbst bleibt bis zur
dauerhaften Plan-Reservierung offen.

Der native Quellenfall bestätigt die gerichtete Python-Aufrufkante und die
exakte UTF-8-Originalseite aus dem Safe Reader. Ein Metadaten-Inspect liefert
keine Originalseite. Ein gleich langer Fremdedit macht die alte Quelle
unbrauchbar. Der Journalbeleg für den tatsächlich gelesenen Bereich überlebt
Reopen. Die reine Erwerbsprüfung verbucht keine Modelllieferung.

Die ersten beiden lokalen Edit-Alternativen passten in die 4k-Wiregrenze.
In `edit-caller-001` schlug das Modell ohne Aufrufer einen Patch vor.
In `edit-too-large-001` meldete es eine fehlende Definition für das
vollständig beschriebene, zu große Einzelartefakt. Beide fachlichen Prüfungen
scheiterten vor einer Mutation. Die Berichte bleiben als Negativbelege erhalten.

`edit-caller-002` lieferte unter einer strengeren Anweisung die reguläre
Caller-Antwort. Der Core fand den gerichteten Aufruf und speicherte den
read-only Erwerb von 82 Originalbytes. Im Folgelauf
`edit-caller-continuation-001` war die Quellseite Teil einer zugelassenen
zweiten Wireanfrage. Das Modell meldete erneut fehlende Evidence. Die
strenge Anweisung führte in `a10-first-edit-003` ebenfalls zu einer
Quellenanforderung. Der überarbeitete kompakte Editauftrag in
`a10-first-edit-004` bestand wieder mit 34.371 Originalbytes, 30 Zuordnungen,
53 aktuellen Quellbytes und 4.096 geladenen Kontexttokens. Die 24 offenen
Laufzeitpflichten blieben erhalten. Mit dieser Anweisung schlug das Modell in
`edit-caller-003` und `edit-caller-continuation-002` ohne Aufrufer einen Patch
vor. `edit-too-large-002` und `edit-too-large-003` meldeten eine Definition.
Die reguläre TooLarge-Antwort und die sichere Fortsetzung mit gelieferter
Zusatzquelle bleiben Teil der offenen Abnahme.

Im ersten echten Planlauf `initial-plan-001` erkannte der 4k-Intake den
exakten Pflichtauftrag. Der reservierte Plan wählte Änderung und passenden
vorhandenen Test. Die Antwort ließ die fachlich bekannte Voraussetzung
zwischen beiden Schritten aus. Der Store übernahm beide als Ready, die
Abnahme stoppte vor dem Run. Der Core bindet diese Voraussetzung jetzt
am Testangebot und ergänzt sie bei der Übernahme der Modellauswahl.

`initial-plan-002` bestand den echten 4k-Ablauf. Der Plan-Body hatte 2.036
Bytes, vor dem Versand waren 2.420 Prompt- und 1.024 Ausgabetokens reserviert.
Ollama meldete 218 Eingabe- und 68 Ausgabetokens. Der atomar gespeicherte
Änderungsschritt war Ready, der vorhandene Test wartete auf ihn. Nach dem
Run-Start und Reopen waren 4.323 Prompt- und 2.048 Ausgabetokens aus zwei
Modellaufrufen gebucht. Startzeit plus verbleibende Laufzeit entsprach der
ursprünglichen Intake-Frist. Der Lauf führte weder Patch noch Testprozess aus.
Der V52-Workspace-Gate war mit 1.791 bestandenen und 31 explizit ignorierten
Tests, Clippy mit `-D warnings`, Format- und Markdown-Linkprüfung grün.

`intake-windows-001` übernahm zwei echte 4k-Antworten mit Reopen dazwischen.
Die Pflichtanforderung belegt Originalbytes `[0,39)`, einschließlich der
ASCII-Absatzgrenze. Die späte Schutzregel belegt `[39,64)`. Das Goal wurde
mit exakt 64 Originalbytes promoviert. Zwei gebuchte Aufrufe und dieselbe
Intake-Frist waren nach Reopen vorhanden. Dieser Komponentenlauf begann
keinen Run.

`edit-known-caller-001` zeigte die Quellenzulassung mit null Modellaufrufen
vor dem Read und 82 gelieferten neuen Bytes. Der Vorschlag bestand die
registrierte Abnahme nicht. `edit-known-caller-002` verlangt in der Fixture
eine unveränderte Signatur und einen einzelnen `min`-Aufruf. Die Einheit
ergab sich aus dem gelieferten Aufrufer. Der tatsächliche 4k-Aufruf schlug
die 30.000-ms-Grenze vor. Der unabhängige Check, die unveränderten Dateien
und die Modellbuchung nach Reopen wurden bestätigt. `edit-too-large-004`
und `edit-too-large-005` prüften ein einfaches unteilbares Stringliteral
als weitere Eingabe und lieferten eine fehlende Testquelle. Die reguläre
TooLarge-Modellantwort bleibt offen.

V53 verknüpft den tatsächlich gelesenen Originalbeleg mit der exakten
Editreservierung. `edit-known-caller-004` bestätigte den gültigen Vorschlag,
82 neue Quellbytes im Wire und dieselbe Liefergutschrift im kalt geöffneten
Store.
Der native Budgetfall führte zuerst fünf reine Metadatenaktionen aus. Zwei
spätere, tatsächlich gelesene und ungesendete Seiten stoppten die nächste
Quellenaufnahme nach Reopen. Eine reguläre Core-Aktion blieb noch möglich.
Die native Vorschauprüfung bestätigt zusätzlich die leere Vorschauanzeige
beim Halt vor der Policy.

Das V53-Workspace-Gate fand fünf Fehler in der Initial-Plan-Reservierung.
Der interne Featuretest akzeptierte exakt V52 und sperrte damit den aktuellen
Kandidaten. V54 behandelt V52 dort als Mindestversion der benötigten Tabellen.
Der Store-Eingang prüft weiterhin exakt die aktuelle Kandidatenversion.

Die ungepaarten 4k-Messungen für das unteilbare Literal mit `bonsai-2:27b`
und `ornith-1.5:9b` zeigen getrennte Fehler. Bonsai bestand die
Fähigkeitsprobe, der Adapter wies den eigentlichen Stream ab. Ornith
erreichte im ersten Aufruf die 1.024-Ausgabegrenze. Der Core buchte die
beobachtete unvollständige Antwort und verwarf den Teiloutput.

`edit-too-large-ornith-002` erreichte dieselbe Ausgabegrenze im Erstaufruf
und im einzigen autorisierten Strukturrepair. Beide Teilantworten wurden
verworfen. Die Berichte halten zusammen 4.055 reservierte Prompt- und 2.048
Ausgabetokens, zwei Aufrufe und einen Repair. Der reguläre Größenhinweis
blieb aus. Der folgende kompakte Editvertrag fordert die Größenentscheidung
vor der Erzeugung eines Ersatztexts.

Mit diesem allgemeinen Hinweis bestand `a10-first-edit-005` den langen
4k-Edit. `edit-too-large-006` meldete weiterhin eine fehlende Definition.
`edit-known-caller-005` meldete eine unzutreffende Größenüberschreitung für
den kleinen Edit mit vollständigem Aufrufer. Der allgemeine Hinweis wurde
deshalb zurückgenommen. Die Größenfixture beschreibt den vollständigen
Zielbereich und die konkrete Größenentscheidung im abgegrenzten Auftrag.

`edit-too-large-007` meldete mit Qwen eine fehlende Testquelle.
`edit-too-large-ornith-003` lieferte mit 4.096 tatsächlich geladenen
Kontexttokens regulär `TooLarge` im ersten Aufruf. Der vollständige Body
hatte 1.624 Bytes, reserviert waren 2.008 Prompt- und 1.024 Ausgabetokens.
Der Server meldete 213 Eingabe- und 42 Ausgabetokens. Nach Reopen blieb
die Laufzeitpflicht offen. Beide Projektdateien waren bytegleich. Es gab
keinen Strukturrepair, keine Policyentscheidung und keine Mutation. Der
ungepaarte Komponentenlauf belegt den gültigen Größenpfad für diesen
abgegrenzten Auftrag. Der native Zulassungstest prüft die bereits vom Core
bekannte Mindestgröße separat vor jedem Provideraufruf.

`initial-plan-004` bestätigte die Intake-/Plan-Buchungen und die ursprüngliche
Run-Frist erneut mit dem V54-Kandidaten. `edit-known-caller-006` lieferte
82 aktuelle Quellbytes und einen unabhängig geprüften 30.000-ms-Vorschlag.
`a10-first-edit-006` bestand mit der wiederhergestellten allgemeinen
Editanweisung und erhaltener offener Laufzeitverifikation. Drei native
Quellenfälle belegen gerichtete Aufruferbindung, exakte Wiregutschrift,
Nullgutschrift für Metadaten und den einmaligen Suchversuch auch nach
Reopen und Unterbrechung.

Das abschließende V54-Workspace-Gate bestand mit 1.815 Tests, null Fehlern und
33 explizit ignorierten Fällen in 98 Testzielen. Die gezielten nativen Fälle,
der separate `connection_lifecycle`, Clippy mit `-D warnings`, die Format- und
Linkprüfung sowie `git diff --check` bestanden. Das lokale libSQL-Quellarchiv
und seine dokumentierte enge Upstream-Abweichung wurden bytegenau geprüft.

Gate: F2 geschlossen und die technischen Voraussetzungen für F5 nachgewiesen. Größeres Kontextfenster, stilles Trimmen und ein tokenizerabhängiger Schönrechenfaktor sind keine Ersatzlösung. Eine neue Tokenizerdependency kommt nur nach Messung und gesonderter Begründung in Betracht.

### P5 Erster vollständiger Agent und sachliche Fehlerkorrektur

Abhängigkeit: P2/P3/P4.

- [x] Den vollständigen kleinen Bugfix ab exakt gebundenem Nutzerauftrag im isolierten Kandidaten über den bestehenden Controller und echte lokale Modelle komponieren.
- [x] Für das unveränderte A01 die manifestgebundene unittest-Discovery und den vorhandenen All-Evidence-Pfad im isolierten Kandidaten schließen; richtige und falsche Änderung durch echte Source-Acceptance prüfen, ohne Produktionsrouting oder eingefrorene Fixture anzupassen.
- [x] Nach einer tatsächlichen Änderung die gebundene passende Prüfung im Core anstoßen; keine erneute Modellentscheidung allein über bekannte Run-/Command-IDs.
- [x] Einen falschen ersten Änderungsversuch mit echter Prüfdiagnose berichtigen; Schema- und fachliche Reparatur getrennt budgetieren.
- [x] Zuerst den für diesen Bugfix benötigten produktiven Test-/CLI-Prüfweg mit typisierter Evidence, Policy, Timeout, Cancellation, Cleanup und Unknown-Disposition schließen; keine frei vom Modell gesetzten Wahrheitsprädikate. Noch unbenötigte HTTP-/manuelle Rezepte folgen vor ihren Verbrauchern in P7/P9.
- [x] Echte Zerlegung eines gescheiterten Schritts zulassen; neue IDs oder umformulierte gleiche Schritte als Stagnation ablehnen.
- [x] Sämtliche Werkzeugeffekte vor neuer Modellarbeit aktuell publizieren und evidenceabhängige Abschlüsse invalidieren.

Belege: mindestens drei vorab geplante reale Versuche des kleinen Bugfixes je Primärmodell sowie derselbe positive Fall mit 4k. Lösung und Code stammen jeweils vom Modell. Zusätzlich deterministisch injizierte Recoveryfälle mit neuer Fehler-/Caller-Evidence; diese werden getrennt von frei entstandenen Modellfehlern ausgewiesen.

Die deterministische Vorprüfung in
`target/plan22-p5-native/a01-constant-observation-002` materialisierte den
eingefrorenen A01-Entwicklungsfall und führte exakt seinen einzigen erlaubten
Testprozess aus. Der Originalcode scheiterte mit Exit 1. Eine Konstantantwort
bestand mit Exit 0 und genau einem Test. Geschützte Dateien blieben bytegleich.
Der Versuch hat keine Source-Acceptance aufgerufen. Für den fehlenden Nachweis
des nicht konstanten Ganzzahlverhaltens ist
[ADR-0121](../adrs/0121-revisionsgebundener-python-ganzzahlbeleg.md) seit
2026-11-09 angenommen. Der zusätzliche Quellbeleg und sein nativer Gegenfall
sind Teil der offenen P5-Abnahme.

Der P5-Messworker verwendet das bereits im Lockfile enthaltene `sha2 0.10.9`
als Testdependency. So bindet er Launch, Original und Quellbeleg an die
SHA-256-Identitäten des eingefrorenen Supervisors. Der Kandidatenstore
verwendet BLAKE3 für seine revisionsgebundenen Produktbelege.

`a01-v55-series-001` hat sieben Infrastrukturfehler festgehalten. Der Worker
erwartete eine noch uninitialisierte Git-Fixture, obwohl der Supervisor sie
bereits angelegt hatte. Beim Primär-Qwen änderte Ollama 0.40.15 die Reihenfolge
derselben numerischen Parameter. Die Folgeserie registrierte die beobachteten
Hashes nach dem Vergleich der exakten Optionswerte. Alle sieben echten
Erstaufrufe in `a01-v55-series-002` beendeten Intake regulär. Qwen zitierte den
fachlichen Teil ohne das vorangestellte Pfadziel und erhielt deshalb keinen
Beleg für den ganzen Bereich. Ornith klassifizierte diesen Teil als Nichtziel
und stellte eine Frage. Die Quelle blieb offen, ohne Ledger oder Mutation.

Der nächste P5-Vertrag übernimmt für den vorab exakt gebundenen A01-Auftrag
die bereits feststehenden Quellrollen in dieselbe Intake-Review. Alle 325
Originalbytes behalten ihre Bereiche. Die Scope-Transaktion pinnt Frist und
Budget mit null Modellaufrufen. Plan und Edit bleiben echte Modelljobs. Der
Komponentenlauf prüft diese gebundene Eingangsform. Die freie sprachliche
Intake-Interpretation bleibt durch P4 und die aufgezeichneten Fehlversuche
getrennt messbar.

Die vorab registrierte Serie `target/plan22-p5-runs/a01-v55-series-003`
bestand mit `ornith-1.5:9b` und `qwen38-27b-iq3s:latest` bei 8192/2048
jeweils 3/3 und mit `qwen38-4k:latest` bei 4096/1024 1/1. Alle sieben
Versuche hatten echte Plan- und Edit-Aufrufe, einen erfolgreichen gebundenen
unittest und aktuelle Produktbelege nach kaltem Öffnen. Der unabhängige
P0-Prüfer bestätigte die sieben Ergebnisse. Es gab kein falschgrünes Done
und keine Scope-Verletzung. Die Serie lief unter Ollama 0.40.15 und zählt
wegen der abweichenden Serverversion als ungepaart. Die beiden früheren
Serien bleiben mit allen Fehlversuchen erhalten.

Die getrennte kontrollierte Fehlerkorrektur
`target/plan22-p5-correction-runs/correction-001` hielt den echten roten
unittest mit `44` statt `42` fest. Nach kaltem Öffnen ersetzte der Core den
gescheiterten Schritt durch einen Edit und den davon abhängigen unveränderten
Test. `qwen38-4k:latest` schrieb die Korrektur mit einem Edit-Aufruf bei
4096/1024. Der Wire hatte 1666 Bytes. Ollama meldete 228 Eingabe- und 39
Ausgabetokens. Die bestehende Source-Acceptance bestätigte anschließend
den aktuellen Test- und Quellbeleg. Derselbe Run war nach erneutem Öffnen
`Done` mit seiner ursprünglichen Frist und ohne verbrauchte Schemareparatur.
Der erste falsche Edit war vorgegeben. Diese Messung zählt getrennt von den
sieben A01-Läufen.

Abnahme am 2026-11-09. Sieben gezielte native Kandidatenregressionen und
elf Prozessadapter-Vertragstests bestanden. Der Gegenfall mit Exit 0 und
null ausgeführten Tests endet nach kaltem Öffnen mit einem fehlgeschlagenen
Versuch und ohne TestEvidence. Der Grünlauf mit Konstantantwort behält
den wahren Prozessbeleg und scheitert am fehlenden Quellbeleg. Der
Workspace-Lauf mit allen Features und offline gesperrtem Lockfile umfasst
98 Testziele mit 1838 bestandenen, null fehlgeschlagenen und 35 ignorierten
Tests. Der Wiederholungslauf nutzte wegen des freien Platzes ein separates
Cargo-Zielverzeichnis ohne Debugsymbole und Inkrementalcaches.
Clippy mit `-D warnings`, der separate `connection_lifecycle`, der exakte
Archiv- und Patchvergleich für libSQL, `cargo fmt`, Markdownlinks und
`git diff --check` bestanden. Der Workspace-Bericht liegt unter
`target/plan22-p5-v55-workspace-nodebug.log`.

Gate: mindestens zwei von drei richtigen Lösungen je Modell in diesem frühen Slice, kein falschgrünes Done und keine Sicherheitsverletzung. Scheitert das, wird dieser Slice korrigiert; Greenfield, Chatintegration und weitere Produktflächen warten. F4 muss eine tatsächlich geänderte Zerlegung zeigen.

### P6 Projektfragen mit korrekter Quellenverwendung

Abhängigkeit: P4; nach P5 als nächstes Paket.

- [x] Ask auf den aktuellen Core-Projektscope begrenzen; kein Projekt und projektfremde Fragen klar behandeln.
- [x] Kleine Teilfragen, gezielte Originalversorgung und beleggebundene Antwortteile durch denselben Quellenpfad führen.
- [x] Quellenbindung/Freshness strikt prüfen, Interpretationen nicht automatisch als Facts persistieren.
- [x] Antworten bei neuen User-Edits neu belegen und zusammengesetzte Antworten aus ursprünglichen Belegen statt aus Zusammenfassungen von Zusammenfassungen erzeugen.

Belege: tatsächliche Writer-/Return-/Aufruferkette, falsche Prämisse, fehlender dynamischer Nachweis, Live-Edit und Quellen-Instruktionsversuch. Ein bloßer Name wie `save_tasks` darf keine Speicherung begründen. Worttreffer oder Zitatmitgliedschaft allein gelten nicht als richtige Antwort.

Der erste isolierte Q05-Schnitt nutzt den vorhandenen sicheren Reader, die
aktuelle Indexpublikation und Ask v1 mit Core-gebundenen Quellfenstern. Fünf
gezielte Anwendungstests und der native Vertrag für einen gleich langen Edit
während des Modellstreams sind grün. Der reale 4k-Lauf
`target/plan22-p6-runs/q05-001` hat vor dem Edit `json` und danach `sqlite`
aus den jeweiligen Originalen ausgegeben. Die beiden tatsächlichen Wirepakete
hatten 1280 und 1408 Bytes bei 4096/1024, beide ohne Strukturrepair. Die zweite
Antwort konnte den vorigen Wert aus der aktuellen Datei allein nicht ableiten.
Der Folgelauf `target/plan22-p6-runs/q05-002` stellte mit zwei echten
Modellaufrufen `sqlite` im aktuellen und `json` im historischen Abschnitt dar.
Der Core übernahm den früheren Antwortteil samt ursprünglichem Quellenanker.
Der Launcher prüft die getrennte Ausgabe, den exakten ursprünglichen und
geänderten Dateiinhalt, die gebundene Indexpublikation und das Wirebudget.
`python -B fixtures/plan22-p6/run_q05.py --help` dokumentiert den isolierten
Lauf. Der Launcher meldet strukturelle Resultate getrennt von der noch
ausstehenden unabhängigen semantischen Bewertung.

Die erste separate Q03-Kontrolle `target/plan22-p6-series/q03-control-001`
lieferte beiden Primärmodellen bei 8192/2048 exakt die aktuelle
`Storage.save_tasks`-Deklaration. Beide Antworten erkannten den Tupelreturn.
Qwen beschränkte die Aussage auf den gelieferten Code. Ornith leitete aus dem
Rückgabewert eine weitergehende Aussage über Speicherung ab und antwortete
auf Englisch. Diese beiden Strukturpässe sind keine semantische Abnahme.
Die Befunde begründen einen einzelnen engeren Antwortjob für allein gelieferte
Deklarationen. Die frühere Serie bleibt vollständig erhalten.

Die getrennten Kontrollen `q03-control-002`, `q01-control-001` und
`q05-control-001` unter `target/plan22-p6-series` haben jeweils zwei
strukturell gültige Läufe mit beiden Primärmodellen erhalten. Q03 lieferte
mit einem begrenzten Deklarationsjob zwei deutschsprachige Antworten zum
Tupelreturn und fehlenden Commit. Q01 lieferte alle drei Originalmodule.
Ornith ließ den Dispatch-Zwischenschritt aus, Qwen antwortete auf Englisch.
Im Q05-Lauf führte der eingefrorene Supervisor den exakten Edit erst nach
dem ersten Antwortbeleg aus. Beide Folgeantworten banden `sqlite` an die
neue Revision. Ihre zusätzlichen Hinweise zum fehlenden Vergleich begründen
die Begrenzung der Folgephase auf eine aktuelle Beobachtung. Die frühere
Antwort bleibt dem Core für den historischen Vergleich erhalten. Alle sechs
Läufe behalten den Status zur unabhängigen Bewertung.

`q01-control-004` und `q02-control-004` haben vier strukturell gültige
Antworten mit den beiden Primärmodellen geliefert. Q01 zitiert die exakte
Einstiegsdeklaration und fragt die sichtbare Writer-Kette sowie den
Storage-Return getrennt ab. Ein Ornith-Ergebnis ließ einen Schritt der
Writer-Kette aus. Q02 bindet die statischen Aufrufpfade und die
Zieldeklaration im Core und delegiert die separate Report-Deklaration an
das Modell. Die Kontrollbelege führen die jeweiligen Core-Teile ohne
Modellaufruf und alle tatsächlichen Modellaufrufe getrennt auf.

In `target/plan22-p6-series/matrix-001` endeten alle 36 Slots mit gültigen
Strukturbelegen und ohne Scope-Verstoß. Die 42 Antwortturns enthielten 51
Modellaufrufe einschließlich drei Strukturreparaturen. Alle Antworten
sind im Worksheet `target/plan22-p6-matrix-001-review.json` erhalten.
In allen drei Q01-Läufen von Ornith wiederholte sich eine widersprüchliche
Behauptung zum Einstieg in den Storage-Aufruf. Der folgende Schnitt
bindet bei einem eindeutigen Importziel die lokale Deklaration direkt und
liefert dem Modell für diesen Job nur das Importmodul. Die vollständige
erste Matrix bleibt erhalten. Ihr Workspace-Test hatte 1.861 bestandene,
37 explizit ausgenommene und keine fehlgeschlagenen Tests.

`q01-control-006` bestätigt mit beiden Primärmodellen das kleinere
Storage-Paket. Beide Antworten nennen den exakten Tupelreturn und
beschränken die Aussage zum fehlenden Schreibzugriff auf den sichtbaren
Körper. Die zuvor beobachtete falsche Einstiegszuordnung fehlt in beiden
Kontrollen. Pro Profil sank der Storage-Auftrag gegenüber `matrix-001`
um 435 Wire-Bytes und 111 gemeldete Prompt-Tokens. Die nativen
Regressionen prüfen getrennt das reine Rückgabemodul und die Variante
mit tatsächlichem `open` und `json.dump`.

`target/plan22-p6-series/matrix-002` hat 36 von 36 strukturell gültige
Slots ohne Scope-Verstoß. Die 42 Antwortturns enthielten 48 tatsächliche
Modellaufrufe, zwölf getrennte Core-Beobachtungen und keine
Strukturreparatur. Die Modellpakete aller 30 Nicht-Q01-Slots hatten
dieselben Wire-Fingerprints wie in der ersten Matrix. Alle sechs
Q01-Storage-Jobs nutzten ausschließlich das aktuelle Storage-Modul und
nannten den exakten Tupelreturn. In allen sechs Q06-Slots war die README
im tatsächlichen Modellpaket. Der Rust-Workspace-Test über 98 Ziele
ergab 1.862 bestandene, null fehlgeschlagene und 37 explizit
ausgenommene Tests. Format, Clippy mit `-D warnings`, Markdown-Links
und Diffprüfung waren grün.

Die 36 bindungsgeprüften Antwortpakete liegen gemeinsam in
`target/plan22-p6-matrix-002-review.json`. Für jedes Paket fehlt noch die
separate menschliche Rubrikbewertung. Der Auswerter meldet daher P6 als
offen. Die frühere Matrix bleibt als eigener Messstand erhalten.

Die Subagenten-Diagnose vom 2026-11-09 prüfte die zwölf unterschiedlichen
Antwortpakete der zweiten Matrix. Zehn erfüllten in dieser Diagnose alle
Rubrikpunkte. Q04 mit Qwen behauptete trotz fehlender Callback-Implementierung,
dass keine Dateioperationen ausgeführt würden. Das unveränderte
`dispatcher.py` schrieb in einer isolierten Gegenprobe über den Callback.
Bei Q06 mit Ornith bleibt die pauschale Persistenzaussage mehrdeutig.
Die isolierte Gegenprobe bestätigte den Tupelreturn von `save_tasks` und
den tatsächlichen Audit-Schreibzugriff auf dem `add_task`-Pfad. Q01 mit Qwen
nennt im Antworttext den lokalen Alias 1 der Storage-Phase. Der dazugehörige
Beleg ist in der zusammengesetzten Antwort als Quelle 3 gebunden.
`target/plan22-p6-matrix-002-subagent-diagnose.json` hält die einzelnen
Urteile und ihre Paketfingerprints fest. Die Diagnose besitzt keine
Abnahmeautorität im eingefrorenen menschlichen Prüfvertrag.

Die Korrektur vergibt pro Antwort stabile Quellnummern. Das Modell darf
nur die in seinem Auftrag gelieferten Fenster zitieren. Q04 bindet die
Originalzeile des unaufgelösten Callback-Aufrufs und rendert die offene
Wirkungsgrenze im Core. Q06 trennt den Rückgabeauftrag von der
Storage-Deklaration. Das vollständige README erreicht den zweiten
Modellauftrag als unprivilegierte Zusatzquelle. Für geschlossene reine
Tupelrückgaben in Q01 und Q06 prüft der Core das Originalzitat und
beschreibt die Rückgabe. Ein Schreibkörper behält seinen Wirkungsauftrag.
Die gezielten Negativtests prüfen falsche Quellnummern, die unbelegte
Callback-Aussage, ein README-only-Zitat und ein übergroßes gerendertes
Zitat. Die tatsächlichen Writer der Holdout-Varianten bleiben in ihren
jeweiligen Quellenpaketen sichtbar.

Die Vorabkontrollen `q01-control-007` und `q06-control-002` bleiben
erhalten. Sie zeigten bei Ornith einen ausgelassenen Writer und eine aus
dem Rückgabelabel abgeleitete Speicherwirkung. `q04-control-002`,
`q01-control-008` und `q06-control-003` lieferten mit beiden Primärmodellen
die gezielt korrigierten Antwortteile. Im zweiten Ornith-Storage-Auftrag
von Q06 war eine Strukturreparatur erforderlich.

`matrix-003` lieferte 36 strukturell gültige Slots ohne Scope-Verstoß,
57 Ollama-Aufrufe und drei Strukturreparaturen in Q06. Der Workspace-Test
bestand mit 1.868 Tests, null Fehlschlägen und 37 expliziten Ausnahmen.
Im Vergleich zu `matrix-002` blieb der Q01-Writer-Wire identisch. Der
gemeldete Promptverbrauch sank beim Q01-Storage-Job von 232 auf 136 Tokens
und bei Q04 von 161 auf 124. Q06 verbrauchte für beide Jobs einschließlich
eines gegebenenfalls nötigen Repairs je nach Modell 381 oder 589 Tokens
statt zuvor 467 in einem Job.

In der unabhängigen Subagenten-Diagnose zur Matrix 003 erfüllten elf von
zwölf Fall-/Modellgruppen alle Rubrikpunkte. Q01, Q04 und Q06 waren bei
beiden Profilen unbeanstandet. In Q02 ließ Qwen offen, ob sein Satz zu
fehlenden weiteren Aufrufen auch den im Rückgabeausdruck sichtbaren
`str(count)`-Aufruf ausschließt. Die Diagnose führt diese Mehrdeutigkeit
unter `no_unsupported_claims`. Der Report-Auftrag fordert daraufhin alle
sichtbaren Aufrufe einschließlich solcher im Rückgabeausdruck und
beschränkt negative Aussagen auf den direkten Aufruf des erfragten Ziels.
Die Urteile sind in `target/plan22-p6-matrix-003-subagent-diagnose.json`
separat von der menschlichen Abnahme erhalten.

In `q02-control-005` präzisierte Qwen den sichtbaren Aufruf, während
Ornith trotz des engeren Auftrags einen widersprüchlichen Satz ergänzte.
Der Core bindet deshalb bei einer verwandten Deklaration mit einer
einzigen direkten Rückgabe eines einfachen Aufrufs die exakte
Rückgabezeile. `q02-control-006` ließ beide Modelle die gebundene Zeile
liefern. Der Core beschreibt daraus `str(count)` und begrenzt die
Negativaussage auf den Namen `create_task` im gezeigten Funktionskörper.

`matrix-004` schloss alle 36 registrierten Slots strukturell gültig und
ohne Scope-Verstoß ab. Die 42 Antwortturns enthielten 60 tatsächliche
Ollama-Aufrufe und zwölf Core-Beobachtungen. Je drei Strukturreparaturen
entfielen auf Ornith in Q02 und Q06. Die 30 anderen Slots erhielten
dieselben Wire-Pakete wie in `matrix-003`. Q02 verwendet durchgehend
Quelle 5 und nennt den sichtbaren `str(count)`-Aufruf. In allen sechs
Q06-Slots gelangte das vollständige README tatsächlich zum Modell.

Die unabhängige Subagenten-Diagnose zur finalen Matrix bewertete alle
zwölf Fall-/Modellgruppen als richtig. Das entspricht 36 Slots mit 168
einzelnen Rubrikentscheidungen. Die Diagnose und das Prüfdokument sind
unter `target/plan22-p6-matrix-004-subagent-diagnose.json` und
`target/plan22-p6-matrix-004-review.json` separat erhalten. Der Maintainer
bestätigte am 2026-11-09 im Chat die fachliche Bewertung von Matrix 004
für Q01 bis Q06 bei beiden Modellen. Die separate bestätigte Arbeitsdatei
`target/plan22-p6-matrix-004-operator.json` bindet die bestätigten
Entscheidungen an alle 36 Antwortpakete. Das eingefrorene Orakel bestätigte
alle 36 Antworten und null Scope-Verstöße. `status` endete mit Exit 0 und
3/3 richtigen Antworten in jeder der zwölf Fall-/Modellgruppen. Der
Gatebeleg liegt in `target/plan22-p6-matrix-004-gate.json`. P6 ist abgenommen.

Die finale Fassung bestand 18 gezielte Anwendungstests, 13 native
Vertragstests und 1.869 Workspace-Tests über 98 Ziele. Es gab keine
Fehlschläge und 37 explizite Ausnahmen im Workspace-Lauf. Format,
Clippy mit `-D warnings`, Markdown-Links und Diffprüfung waren grün.
Die komprimierten Worker-Kopien behalten ihre registrierten
SHA-256-Fingerprints und Pfade.

Gate: mindestens zwei von drei richtigen Antworten je Kernfall und Primärmodell in der gezielten Vorabnahme. Ask erreicht keine Mutation, freie Prozessausführung oder allgemeine Netzwerkanfrage, auch bei Full machine. Der Modellzugriff der Messreihe erfolgt über den gebundenen lokalen Ollama-Port.

Die Entwicklungsmatrix umfasst 36 registrierte Slots für Q01 bis Q06 mit
je drei Durchläufen pro Primärprofil. Der Launcher trennt gebundene
Antworten, Strukturfehler und ausstehende menschliche Entscheidungen. Die
aktuelle Messung benötigt für jeden Slot einen gebundenen Abschlussbeleg.
Der eingefrorene Orakelvertrag verlangt für die fachliche Bewertung jedes
Antwortslots eine separate menschliche Rubrikentscheidung.
`python -B fixtures/plan22-p6/review_ask_series.py worksheet --help`
beschreibt das gemeinsame Prüfdokument mit Antworten, Originalausschnitten
und Phasenherkunft. `submit` übernimmt die unabhängig erstellten
Entscheidungen slotweise und `status` prüft die Schwelle pro Fall und Profil.

### P7 Greenfield, mehrere Module und große bestehende Projekte

Abhängigkeit: P5/P6.

Der enge NoChange-Slice ist technisch verifiziert: Ein wirklich byteidentisch
gesenkter aktueller Existing/Replace-Edit bleibt mit Original-/Window-/Wirebeleg
dauerhaft erhalten. Der bestehende Ledger-/Journal-CAS beendet den aktiven Versuch
als Failed/Replan und schließt den alten Read ohne Credit. Kaltes Replay,
vollständige CurrentIndex-Konflikte, Rollback, Kosten und ursprüngliche Frist sind
im nativen Vertrag geprüft. Er führt keinen neuen Modelljob aus und ändert weder
Originalauftrag noch konsumierte Budgets oder offene HTTP-Proofpflicht.
Die mechanischen Gates für Kandidat 67 sind in
`target/plan22-p7-runs/edit-nochange-mechanical-gates-004.json` an den eingefrorenen
Quellstand gebunden: Windows 2.139 erfolgreiche Tests, Linux 278 Testausführungen
über 18 Befehle, jeweils null Fehler, sowie Clippy, Format, Verbindungs-, Link- und
Whitespaceprüfung. Historische Modellmessungen bleiben unverändert. Eine echte
kleinere Folgezerlegung und deren lokale Modellabnahme bleiben offen; daraus folgt
kein zusätzlicher P7-Haken.

Der aktuelle [HTTP-Korrekturprüfstand](22-P7_HTTP_CORRECTION_STATUS.md) beschreibt
den nachfolgend implementierten kalten Herkunfts-/Aktivierungs-/Editanschluss,
seine echten nativen Nachweise und die noch offenen lokalen Modell-/Abschlussgates.
Die früheren Messungen dieses Abschnitts bleiben historische Teilnachweise.

- [x] Leeren publizierten Index als echte Bestandsaufnahme behandeln; keine erfundenen Dateien oder Pflichtrecherche.
- [x] Vor dem Serverfall das produktive geschlossene Start-/HTTP-/Stop-Rezept samt typisierter Evidence, gebundenem Loopbackziel, Policy, Timeout, Cancellation, Prozess-Cleanup und Unknown-Recovery schließen; das externe HTTP-Orakel allein genügt nicht.
- [ ] Kleine gemeinsame Schnittstellen/Entwurfsentscheidungen festhalten und anschließend Dateien in begrenzten Umsetzungsschritten erzeugen.
- [ ] Größere neue Dateien über kleine Gerüste und exakte Folgeedits bauen; keine Kontext-bedingte ungefragte Projektarchitekturänderung.
- [x] Vor A02 den tatsächlichen Indexaufbau seiner eingefrorenen großen Python-Datei mit BOM und vielen CRLF-Zeilen nachweisen. Die in P3 beobachteten Fälle für ungültige Modulbereiche und Parse-Zeitgrenzen getrennt diagnostizieren. Fixture, Modellprofil und Sicherheitsgrenzen beibehalten.
- [ ] APIänderung über mehrere Module mit Aufruferabdeckung, aktuellen Schnittstellenentscheidungen und passenden Prüfungen ausführen.
- [ ] Bestehende Fremdänderungen, rote Altchecks, fehlende Dependencies und angehaltene Installationsfreigaben korrekt behandeln.
- [ ] Nach Context Compaction und Appneustart Ziel, Schnittstellen, offene Kriterien und tatsächliche Verifikation wiederherstellen.

Der [A02-Indexprüfstand](22-P7_A02_INDEX_STATUS.md) dokumentiert den tatsächlich
reproduzierten Parse-Timeout, die gemessene Optimierung bestehender Parserpakete
und dreier Kindlisten-Schleifen sowie zwei erfolgreiche vollständige native
Windows-Indexierungen mit kaltem Reopen. Die Abschlussprüfung vom 2026-11-12
besteht mit 2047 Windows-Workspace-Tests über 103 Ziele und 60 gezielten Linux-Tests
über sieben Ziele, jeweils ohne Fehler. Beide A02-Varianten publizieren vollständig
und laden kalt identisch. Connection-Lifecycle, vollständiges Clippy mit verbotenen
Warnungen, Format-, Link-, Diff- und unabhängige libSQL-Herkunftsprüfung bestehen.
Das gebundene Gate liegt in
`target/plan22-p7-runs/a02-http-v3-combined-gate-001.json`.
Daraus folgt keine A02-Modell- oder P7-Abnahme.

Nachweis zum leeren Index vom 2026-11-09. Die isolierte V55-Fixture
publiziert einen frischen Git-Worktree ohne Dateirevisionen. Der Core
übergibt diese Bestandsaufnahme ohne Quellseite. Eine neue dateihaltige
Publikation weist den alten Plan ab. Auch bei übereinstimmender Lauf- und
Snapshot-ID muss der Leerstatus beim erneuten Laden übereinstimmen.
`qwen38-4k:latest` und `ornith-1.5:9b` erzeugten mit 4096 Kontexttokens
je einen passenden ersten README-Schritt. Jeder Lauf verbrauchte einen
Planaufruf und null Reparaturen. Der Schritt blieb nach kaltem Reopen
`Ready`. Beide Arbeitsverzeichnisse blieben leer. Der Modellbeleg liegt
in `target/plan22-p7-runs/inventory-ollama-001.json`.

Elf gezielte Planphasen-Tests, zwei native Vertragsfälle, Clippy mit
`-D warnings` und 1872 Workspace-Tests über 98 Ziele bestanden. Der
vollständige Workspace-Lauf nutzte einen kurzen temporären Pfad auf D:
und protokollierte null Fehler und 38 explizite Ausnahmen. Sein Beleg
liegt in `target/plan22-p7-runs/inventory-workspace-tests-002.log`.

Erster Nachweis der HTTP-Ausführungsgrenze. Fünf native Vertragsfälle auf
Windows und Linux prüfen einen echten Python-Dienst, positive und
negative HTTP-Antworten, fremde Listener, Abbruch, Timeout, Größenlimit,
Skriptänderung und das beobachtete Prozessende. Belege liegen in
`target/plan22-p7-runs/http-windows-contract-004.log` und
`target/plan22-p7-runs/http-linux-001/linux-contract-001.log`.

V56-Zwischenstand vom 2026-11-09. Der isolierte Store bindet
HTTP-Evidence an den exakten Originalbereich, die aktuelle
Skriptrevision, das vollständige Startkommando, die geordneten
Freigaben und den abgeschlossenen Journalversuch. Der Store weist
eine vollständig journalisierte Ausführung von `other.py` als
Nachweis für `server.py` ab. Der exakte Beleg bleibt nach kaltem
Reopen verfügbar. Elf native Speicherprüfungen sind in
`target/plan22-p7-runs/http-v56-native-004.log` protokolliert.

Die isolierte Plan-Fixture übernimmt das typisierte Rezept aus einer
vorgegebenen Intake-Bindung. Sie misst die Planphase ohne Dienststart
oder Dateiänderung. Ein Qwen-Lauf mit Steuerzeichen in der
Schrittbeschreibung scheiterte auch bei der einmaligen Strukturreparatur.
Nach der einzeiligen Modellanweisung erzeugten `qwen38-4k:latest` und
`ornith-1.5:9b` jeweils im ersten Aufruf einen gültigen Schritt.
Beide Ledger blieben nach kaltem Reopen `Ready` und enthielten das
exakte Rezept. Modellidentitäten und Belege liegen in
`target/plan22-p7-runs/http-v56-ollama-003.json`.

Die bestehende README-Planung mit beiden Modellen und der P4-Plan
mit Qwen bestanden erneut. Sechs HTTP-Adapterfälle liefen auf Windows
und Linux. Der Windows-Workspace-Lauf auf D: meldete 1896 bestandene
Tests über 99 Ziele, null Fehler und 39 explizit ignorierte Tests.
Clippy bestand mit `-D warnings`. Die Belege liegen in
`target/plan22-p7-runs/http-v56-plan-regressions-001.json`,
`target/plan22-p7-runs/http-v56-workspace-001.log`,
`target/plan22-p7-runs/http-v56-clippy-004.log` und
`target/plan22-p7-runs/http-linux-001/linux-contract-005.log`.

Der isolierte Kandidatencontroller führt den aktiven HTTP-Schritt
mit bereits erteilten Freigaben aus. Er hält die Worktree-Lease bis
zur Ergebnisverarbeitung, speichert den `Unknown`-Versuch vor der
Ausführung und prüft den Worktree nach dem Stop erneut. Der
Toolabschluss ist atomar mit dem HTTP-Beleg. Der nachfolgende
Ledger-/Run-CAS verlangt die weiterhin aktuelle Publikation.
Die native Prüfung bestätigt Rollback bei falscher Rezept-ID
und die Ablehnung eines Abschlusses nach neuer Publikation.

Die isolierte Linux-Fixture startet ein vorgegebenes Python-Skript
über den Kandidatencontroller. Die passende Antwort führt nach
kaltem Reopen zu `Done`. Ein falscher Body bleibt als
`VerificationFailed` gespeichert und führt zu `Replan`. Ein
kontrollierter unbekannter Cleanup sperrt weitere Mutationen.
In der ersten V56-Fixture prüften unter Windows kontrollierte
Beobachtungen dieselbe Policy-, Journal- und Ledger-Kette. Belege liegen in
`target/plan22-p7-runs/http-controller-native-004.log` und
`target/plan22-p7-runs/http-linux-001/linux-http-controller-native-004.log`.

Die erneute Planmessung zeigte eine unzulässige Selbstabhängigkeit
und unnötige Evidenzanforderungen. Das Schema schränkt die Zahl
zusätzlicher Abhängigkeiten auf die tatsächlich angebotenen
Vorgänger ein. Die Greenfield-Anweisung unterscheidet vorhandene
Quellen von der nachgelagerten, bereits gebundenen Prüfung.
`qwen38-4k:latest` und `ornith-1.5:9b` lieferten danach mit je 4096
Kontexttokens jeweils zweimal im ersten Aufruf einen gültigen
HTTP-Schritt. Alle vier Ledger hielten das exakte Rezept nach
Reopen. Beide README-Läufe und der frühere P4-Plan mit Qwen
bestanden erneut. Die Messungen stehen in
`target/plan22-p7-runs/http-controller-ollama-003.json` und
`target/plan22-p7-runs/http-controller-plan-regressions-001.json`.

Der Windows-Workspace-Lauf auf D: meldete 1899 bestandene Tests
über 99 Ziele, null Fehler und 39 explizit ignorierte Tests.
Clippy bestand mit `-D warnings`. Belege liegen in
`target/plan22-p7-runs/http-controller-workspace-001.log` und
`target/plan22-p7-runs/http-controller-clippy-002.log`.

Nach dem atomaren Toolabschluss kann derselbe aktive Schritt aus
dem erhaltenen HTTP-Beleg wiederaufgenommen werden. Der
Regressionstest verwirft den ersten Ledger-CAS und öffnet V56
kalt. Die allgemeine Recovery schützt den letzten Toolabschluss.
Der gezielte Einstieg verwendet denselben Beleg ohne zweiten
Toolversuch. Eine neu publizierte Revision verweigert den Abschluss.
Nach abgeschlossenem Schritt prüft auch die allgemeine Recovery
den typisierten HTTP-Beleg auf Aktualität. Sieben native Fälle
prüfen diese Übergänge auf Windows und Linux.
`target/plan22-p7-runs/http-resume-windows-native-005.log` und
`target/plan22-p7-runs/http-linux-001/linux-http-resume-and-invalid-start-005.log`
enthalten die Ergebnisse.

Nach Annahme von
[ADR-0122](../adrs/0122-besessener-windows-job-fuer-loopback-pruefungen.md)
prüft der Windows-Adapter den privaten Job und die numerische
TCP-Owner-Tabelle direkt. Die nativen Fälle prüfen Listener,
gehaltenen Socket, Nachkommen ohne Listener, gescheiterte
Job-Zuordnung, eine abgelaufene Cleanup-Frist und den extern
beendeten Worker. Der echte Python-Dienst ist auch unter Windows
mit dem V56-Controller verbunden. Der Windows-Adapterbeleg
liegt in `target/plan22-p7-runs/adr0122-windows-adapter-003.log`.

Eine weitere Ornith-Messung verlangte `Source` bei leerem Index.
Die Planphase begrenzt deshalb Schema und Decoder für genau diese
Publikation auf Umsetzungsschritt und `TooLarge`. Bei vorhandenem
Bestand bleibt `NeedEvidence` verfügbar. Qwen und Ornith lieferten
danach jeweils zweimal mit 4096 Kontexttokens im ersten Aufruf
einen gültigen HTTP-Schritt. Alle vier Ledger hielten das exakte
Rezept nach Reopen. Beide README-Pläne und Qwens P4-Plan bestanden
erneut. Die Belege liegen in
`target/plan22-p7-runs/http-resume-ollama-002.json` und
`target/plan22-p7-runs/http-resume-plan-regressions-001.json`.

Der abschließende Windows-Workspace-Lauf für diesen Zwischenstand
meldete 1905 bestandene Tests über 99 Ziele, null Fehler und 39
explizit ignorierte Tests. Clippy bestand mit `-D warnings`.
Die Ergebnisse stehen in
`target/plan22-p7-runs/adr0122-resume-workspace-001.log` und
`target/plan22-p7-runs/adr0122-resume-clippy-002.log`.

Das Approval Center des Kandidaten zeigt Start und jede geordnete
HTTP-Probe einzeln. `AllowOnce` speichert den passenden Grant.
Erst das letzte getrennte `Continue` erlaubt einen Werkzeugversuch.
Der native V56-Vertrag prüft echten Start, GET und POST, eine alte
Anzeigerevision, eine neue Quellpublikation, Verlust der letzten
Anzeige und eine zwischenzeitlich geänderte Berechtigungsrevision.
Die lokale Desktop-IPC der Altbaseline bleibt getrennt.
Je elf Fälle bestanden auf Windows und Linux. Die Belege stehen in
`target/plan22-p7-runs/http-approval-windows-native-002.log` und
`target/plan22-p7-runs/http-linux-001/linux-http-approval-002.log`.
Der erneute leere HTTP-Plan bestand mit `qwen38-4k:latest` und
`ornith-1.5:9b` bei jeweils 4096 Kontexttokens im ersten Aufruf.
Nach kaltem Reopen enthielten beide Ledger das exakte Rezept.
Diese getrennte Planmessung führte keinen Start aus.
`target/plan22-p7-runs/http-approval-ollama-001.json` hält die Belege.
Der vollständige Windows-Workspace-Lauf auf D: bestand mit 1912
Tests über 99 Ziele, null Fehlern und 39 explizit ignorierten Tests.
`cargo fmt --all -- --check` und Workspace-Clippy mit allen Zielen,
allen Features und `-D warnings` bestanden ebenfalls. Die Logs sind
`target/plan22-p7-runs/http-approval-windows-workspace-001.log` und
`target/plan22-p7-runs/http-approval-windows-clippy-001.log`.

V57 bindet den Fehlerexit bei ungültigem Portargument, den Fehlerexit
am gehaltenen expliziten Port, die HTTP-Prüfung nach explizitem Start
und die Prüfung des Standardstarts an eine Rezept-ID und dieselbe
Originalpflicht. Der Einzelstartpfad und sein Beleg bleiben für diese
Startabdeckung gesperrt.
[ADR-0123](../adrs/0123-exklusive-loopback-bindung-fuer-startfehler.md)
autorisiert die exklusive Bindung und Exit-Abfrage im vorhandenen
Windows-Modul. Zwei native Windows-Tests und elf gezielte Linux-Tests
bestanden. Die Plattform-Fixtures unterscheiden den tatsächlichen
Fehlerexit bei gehaltenem Port, den Start bei freiem Port und den
Abbruch. Die Bausteine laufen isoliert in Tests. Der gemeinsame
Controllerbeleg und die eingefrorene A04-Abnahme mit lokalem Modell
stehen aus. Belege liegen in
`target/plan22-p7-runs/adr0123-windows-native-001.log` und
`target/plan22-p7-runs/http-linux-001/linux-startup-native-001.log`.

Das vollständige Rezept bleibt nach kaltem Wiederöffnen in Intake,
Goal und Ledger erhalten. Ein mit passenden IDs, Skriptrevision,
Policyentscheidungen und Toolversuch konstruierter Einzelstartbeleg
rollt in V57 vollständig zurück. Der Versuch bleibt nach Reopen
`Unknown`. Die gezielten Speicherfälle bestanden auf Windows und
Linux. Belege stehen in
`target/plan22-p7-runs/startup-coverage-windows-storage-001.log` und
`target/plan22-p7-runs/http-linux-001/linux-startup-v57-storage-001.log`.

Der eingefrorene A04-Entwicklungsauftrag umfasst 1207 UTF-8-Bytes.
Im 4k-Planversuch 001 hielt der Wire-Budgetcheck beide Modelle vor
dem Planaufruf an. Versuch 002 passte ins Budget, materialisierte
aber `tooLarge` beziehungsweise `R1` als Schrittbeschreibung. Diese
beiden Ergebnisse zählen fachlich als Fehlversuche. Der Core weist
solche Platzhalter jetzt vor dem Ledger-Write ab. In Versuch 003
gaben beide Modelle `tooLarge` an der vorgesehenen Stelle zurück.
Die kurze Anweisung wurde daraufhin auf größere Einzelangebote
beschränkt und fordert den ersten konkreten Umsetzungsschritt an.
Die Fehlversuche stehen in
`target/plan22-p7-runs/a04-development-plan-ollama-001.json`,
`target/plan22-p7-runs/a04-development-plan-ollama-002.json` und
`target/plan22-p7-runs/a04-development-plan-ollama-003.json`.

Im Versuch 004 benötigte der tatsächlich serialisierte A04-Aufruf
2605 Bytes für `qwen38-4k:latest` und 2602 Bytes für `ornith-1.5:9b`.
Beide Profile verwendeten 4096 Kontexttokens und 1024 Ausgabetokens.
Qwen beschrieb Argumentverarbeitung und HTTP-Routing für `server.py`
im ersten Aufruf. Der `Ready`-Schritt behielt die vollständige
Verifikationsspezifikation nach kaltem Reopen. Ornith lieferte auch
nach der einmaligen Reparatur Steuerzeichen im Ergebnistext. Sein
Ledger blieb leer. Beide kurzen README-Aufträge bestanden erneut.
Die Messungen stehen in
`target/plan22-p7-runs/a04-development-plan-ollama-004.json` und
`target/plan22-p7-runs/a04-compact-readme-plan-ollama-002.json`.
17 gezielte Plan- und Reparaturtests sowie Workspace-Clippy mit
`-D warnings` bestanden auf Windows. Die native libSQL-Verbindungsprüfung
bestand auf D:. Belege stehen in
`target/plan22-p7-runs/a04-compact-plan-windows-targeted-005.log`,
`target/plan22-p7-runs/startup-coverage-windows-clippy-002.log` und
`target/plan22-p7-runs/startup-coverage-windows-connection-lifecycle-001.log`.

Der abschließende sequenzielle Workspace-Lauf für diesen Stand
bestand auf Windows mit 1921 Tests über 99 Ziele, null Fehlern und
40 explizit ignorierten Tests. Er verwendete alle Features,
`--offline --locked --no-fail-fast` und zwei Buildjobs auf D:.
Sein Beleg liegt in
`target/plan22-p7-runs/startup-coverage-windows-workspace-004.log`.
Ein separater Ollama-Formatprobeversuch mit Ornith konnte ein
dekodiertes Steuerzeichen trotz einer negierten Unicode-Control-Klasse
erzeugen. Dieser Providerprobeversuch ist keine Kandidatenabnahme.
Der Laufzeitdecoder bleibt maßgeblich. Der Probebericht liegt in
`target/plan22-p7-runs/ornith-single-line-schema-probe-001.json`.

V58 verbindet die vollständige Startabdeckung mit demselben
Mutationscontroller. Eine exakte Reservierungsfreigabe und getrennte
Freigaben für gehaltenen und freien Start binden trotz identischer argv
verschiedene Wirkungen. Danach folgen die expliziten Proben, Standardstart
mit GET und ungültiges Portargument unter einer gemeinsamen Frist.
Der gesamte Ablauf besitzt einen Unknown-Versuch vor dem ersten Bind;
vollständiger Beleg und Toolabschluss werden atomar gespeichert.
Eine beendete Teilwirkung liefert keinen vollständigen Funktionsbeleg.
Die neuen nativen Controllerprüfungen decken auch den E6-Konflikt mit
gezielter kalter Wiederaufnahme ohne neuen Start ab.

Die erneuten A04-Planläufe 005 bestanden mit beiden lokalen Modellen
im ersten Aufruf, ohne Repair und mit unverändertem vollständigem Rezept
nach Reopen. Qwen verwendete 2658 vollständige Requestbytes, Ornith 2655,
jeweils bei 4096 Kontext- und 1024 Ausgabetokens. Der Beleg
`target/plan22-p7-runs/a04-development-plan-ollama-005.json` gilt nur
für die Planphase; beide Projekte blieben leer und führten keinen Server aus.
Der vorherige Repairzusatz hätte für Qwen 2689 Bytes benötigt und damit
die 2688-Byte-Grenze überschritten. Der verkürzte Zusatz benötigt 14
JSON-kodierte Zusatzbytes. Der Regressionstest prüft 2672 Bytes innerhalb
derselben Grenze sowie unveränderten Auftrag, Schema und Modellprofil.

Für die folgende vollständige A04-Umsetzung ist die derzeitige einzelne
HTTP-Proves-Planung noch kein kleiner Erzeugungshorizont. Ein erstes
Addresses-Angebot mit DiffInvariant für `server.py` ist bereits möglich;
das originale globale HTTP-Kriterium bleibt bis zur echten Prüfung offen.
Für weitere Gerüst-/EOF-Horizonte fehlen noch der reservierte echte
Modell-Folgeplan am bestehenden Run und der revisions-/bereichsgebundene
Fortschrittsnachweis. Der vorhandene Stagnationsschutz verweigert zu Recht
einen zweiten gleichen Datei-/Kriterienauftrag mit bloß neuen IDs.
Auch der reguläre Folgeplan passt mit seiner heutigen Steuerlast noch
nicht in die gemessene 4k-Grenze. Diese Lücken bleiben offen; eine direkte
Weitergabe an den alten Arbeitsaufruf schließt sie nicht.

Das V58-Gate vom 2026-11-10 bestand auf Windows mit 1949 Tests über
101 Testziele, null Fehlern und 40 ausdrücklich ignorierten Fällen.
Der sequenzielle Workspace-Lauf verwendete alle Features und
`--offline --locked --no-fail-fast --jobs 2`. Clippy bestand für den gesamten
Workspace mit allen Targets/Features und `-D warnings`; Formatierung,
Markdown-Links und die separate native `connection_lifecycle` bestanden.
Die 22 Windows-HTTP-Speicher-/Controllerfälle, fünf Autorisierungsfälle und
13 nativen Workspace-Vertragsfälle bestanden gezielt. Linux bestand
dieselben 22 HTTP-Fälle, vier Codec- und acht Candidate-Open-Fälle sowie
13 Workspace-Vertragsfälle und elf native Prüfungen. Die erneute Linux-
Workspace-Prüfung bestätigt auch die aktuelle Box-API. Die bekannte
Linux-Warnung in `command_discovery.rs:118` bleibt dokumentiert;
ein vollständiges Linux-Clippy-Gate wurde hier nicht ausgeführt.
Die Belege stehen in `target/plan22-p7-runs/aggregate-windows-workspace-003.log`,
`aggregate-windows-clippy-003.log`, `aggregate-windows-connection-lifecycle-001.log`
und `http-linux-001/linux-startup-aggregate-storage-001.log` sowie
`http-linux-001/linux-startup-aggregate-contract-002.log` unter demselben
Runverzeichnis. Die damalige Abnahme des HTTP-Arbeitspakets wurde im
folgenden echten A04-Lauf wieder geöffnet: Die Speicher-/Controllerfälle
hatten nicht alle HTTP-Methoden durch die dauerhafte Freigabekette geführt.
Bei `DELETE` verlangte die Domain `Network/Critical`, während die bestehende
Klassen-/Risiko-Invariante und die Tabellen für `Network` ausschließlich
`High` erlauben. Der achte Request rollte vor Werkzeugbeginn zurück.
Sieben aktive HTTP-Grants, kein HTTP-Versuch und `AwaitApproval` blieben
korrekt erhalten. Der Befund liegt in
`target/plan22-p7-runs/a04-agent-ornith9b-001`.
`Critical` und die Netzwerk-, Schreib- und Destruktionsgrenzen müssen
bei der Korrektur erhalten bleiben. P7 und die vollständige
A04-Modellabnahme bleiben offen.

Fortsetzung vom 2026-11-10. `DELETE` verwendet jetzt die zulässige
Primärklasse `Destructive/Critical`; `Network` und `Write` bleiben zusätzliche
Restriktionen. Die Klassen-/Risiko-Invariante und SQL-Tabellen wurden nicht
aufgeweicht. Der gemeinsame Speichervertrag führt alle fünf Methoden in
beiden Berechtigungsmodi über Request, kaltes Laden, Grant, Verbrauch und
erneutes Laden. Deny und CAS-Rollback bleiben eigene Negativfälle. Die
gezielten Windows- und Linux-Prüfungen bestanden jeweils mit einem
Domainfall, 15 Anwendungsfällen und einem gemeinsamen Policyvertrag.
Die Linux-Ausführung war offline ohne Image-Pull; die bekannte unveränderte
`unused_mut`-Warnung ersetzt kein vollständiges Linux-Clippy-Gate.

Die Modell-Planphase begrenzt ihre Schritttitel auf 64 UTF-8-Bytes und weist
einen bloßen gebundenen Dateipfad als angebliches Änderungsziel zurück.
Der allgemeine Domaintexttyp bleibt unverändert. Der aktuelle einmalige
Planrepair ergänzt 26 JSON-kodierte Bytes und erhält Auftrag, Schema,
Kontext- und Outputprofil. Beim Add in einen nachweislich leeren publizierten
Index enthält das Ausgabeschema ausschließlich `replacement` oder `tooLarge`.
Ein unabhängiger Decoder verwirft dort auch schemawidrige Quellanforderungen.
Der Leerstatus ist im Fenster-/Jobdigest v2 gebunden und wird innerhalb
derselben Reservierungstransaktion gegen die aktuelle Publikation geprüft.
Bestands-Adds und Updates behalten ihre regulären Need-Varianten.

Die echte Encoder-Nachmessung `a04-edit-wire-projection-comparison-003.json`
verwendet denselben 1207-Byte-Originalauftrag, dieselben Profile und denselben
56-Byte-Titel wie die Vormessung. Empty-Add benötigt mit Qwen
2399/2406/2518 Bytes und mit Ornith 2396/2403/2515 Bytes für Initial-,
Struktur- und Trunkierungsrepair: jeweils 121 Bytes weniger. Die aktive
1425-Byte-Pflichtprojektion bleibt bytegleich. Ein tatsächliches Update
eines 31-Byte-Endfensters benötigt einschließlich Trunkierungsrepair
2676 beziehungsweise 2673 Bytes und passt in die unveränderte Grenze von
2688 Bytes. Die vollständige 277-Byte-Quelldatei bleibt als negative
Größenkontrolle zu groß.
Diese Messung ersetzt keine fachliche Modellabnahme.

`a04-agent-qwen4k-003` bestand mit dem eingefrorenen Worker aus Build 006:
echte Plan-/Edit-Aufrufe mit 4096/1024, zwei Calls ohne Repair, vollständiges
produktives Start-/HTTP-/Stop-Rezept, aktuelle typisierte Evidence,
dauerhaftes `Done` nach kaltem Reopen und bestandenes unabhängiges Orakel.
Intake war ausdrücklich an den geprüften vollständigen Originalauftrag
gebunden; ein Intake-Modellaufruf gehört nicht zu diesem Teilnachweis.
`a04-agent-ornith9b-002` schlug dagegen nach angewandtem Add beim
Indexaufbau fehl, vor DiffInvariant und HTTP. Dieser Fehlversuch bleibt
erhalten und terminal. Ein fachlicher HTTP-Fehler darf daraus nicht
abgeleitet werden.

Der Indexfehler wurde mit den unveränderten 1455 Modellbytes reproduziert:
Tree-sitter liefert eine gültige Moduldeklaration ab Byte 1, die feste
Auswahl an Byte 0 liegt außerhalb. Entfernen ausschließlich der ersten LF
in einer Arbeitsspeicherkopie lässt den alten Adapter bestehen. Dasselbe
Problem betrifft führende CRLF, BOM und reine Leerzeilen. Die minimale
Korrektur setzt die synthetische Modulauswahl an den tatsächlichen Beginn
der Deklaration; Bereichs- und Parsegrenzen bleiben erhalten. Zwei neue
Regressionen scheiterten vor dem Fix tatsächlich. Die Belege liegen in
`a04-python-module-repro-001.log` und
`a04-python-module-regression-before-001.log` unter demselben Runverzeichnis.
Das vollständige Python-Adapterziel bestand danach auf Windows und Linux
mit jeweils acht Tests einschließlich beider neuen Regressionen. Die
unveränderte Ornith002-Datei behält ihren ursprünglichen Quellhash.

Das gesonderte Code-Review `a04-independent-code-review-001.json` fand
zusätzlich einen Fehler im A04-Prüfbericht: ein verzögerter Done-Commit
durfte nicht trotz gescheitertem Gesamtversuch als formaler Erfolg gelten.
Die Deadline wird jetzt auch um den Commit geprüft. Der tatsächliche
dauerhafte Zustand bleibt sichtbar; formaler Erfolg verlangt zusätzlich
den erfolgreichen vollständigen Versuch und bestätigtes kaltes Done-Reopen.
Ein abgeschlossener verspäteter Commit wird nicht nachträglich zurückgerollt.
Die vier gezielten A04-Workerprüfungen bestanden. Die finalen
Workspace-/Clippy-Gates sind noch offen.
Der nächste begrenzte Korrekturslice ist in
`target/plan22-p7-runs/a04-followup-slice-audit-001.md` untersucht; er ist
noch nicht implementiert und öffnet keine alten terminalen Runs wieder.

Die erneuten tatsächlichen Läufe mit Worker 007 sind keine Modellabnahme:
`a04-agent-qwen4k-004` lieferte nach seinem einen Repair erneut eine
ungültige Planantwort; es entstanden kein Run, Werkzeug, Approval oder
Dateieffekt. Beide dauerhaften Reservierungen enthalten `invalid_structure`.
Der konkrete Decodergrund ist daraus nicht belegt. Eine bloße Dateinamen-
Caption darf nicht als erratene Ursache ausgegeben werden.
`a04-agent-ornith9b-003` publizierte dagegen nach dem Parserfix den neuen
Index und gelangte durch alle zwölf Rezeptfreigaben bis zur tatsächlichen
HTTP-Prüfung. Diese schlug fehl; der Run blieb `Failed`, ohne HTTP-Beleg
oder Abschluss. Die erzeugte Datei und das separate Orakel zeigen das
fehlende LF im Antwortkörper. Der Produktfehlerpfad bewahrt derzeit nur
einen generischen Prüfungsfehler und darf den konkreten Grund nicht aus
dem Orakel übernehmen.

Das unabhängige Subagentenreview
`a04-independent-model-review-001.json` bestätigt einen belegten Einzelpass
und drei erhaltene Fehlversuche, die vollständigen Originale, Quell-/Worker-
und Logbindungen sowie die kalten Datenbankzustände. Es prüft den
Qwen003-V2-Beleg strukturell gegen das gebundene Rezept und die zwölf
verbrauchten Rezeptentscheidungen. Sein BLAKE3-Siegel wurde vom unabhängigen
Prüfer nicht selbst neu berechnet. Ein konstantes Kompositionskennzeichen
`source_controller: true` beweist insbesondere bei Qwen004 keine erreichte
Controllerphase. Unterschiedliche Worker sind kein kontrollierter
Zuverlässigkeitsvergleich. P7, die A04-Modellabnahme und P10 bleiben offen.

Der finale Workspace-Lauf `a04-windows-workspace-002.log` schlug im nativen
Startvertrag fehl: `compound_startup_does_not_renew_the_deadline_between_phases`
erhielt nach Fristablauf `Unavailable` statt `TimedOut`; fünf weitere Fehler
waren Folgen des vergifteten Test-Locks. Die isolierte Wiederholung und
fünf direkte Wiederholungen des ursprünglichen Workspace-Testprogramms
bestanden, beseitigen diesen Befund aber nicht. Der Windows-TCP-Beobachter
meldet seinen Ablauf als `OutputUnavailable`; dessen pauschale Umwandlung
in `Unavailable` überspringt die anschließende Prüfung der ursprünglichen
Frist. Die Korrektur und deterministische Grenztests müssen vor einem
neuen vollständigen Gate bestehen. Das HTTP-Arbeitspaket bleibt offen.

Dieses erneut geöffnete HTTP-Arbeitspaket ist nach der Fristkorrektur wieder
verifiziert. Die Beobachtung prüft dieselbe absolute Effektfrist und
Cancellation vor und nach dem Betriebssystemaufruf; ein verspäteter Fehler
oder Erfolg kann keinen aktuellen Ownership-Nachweis liefern. Cleanup,
Unknown, Windows-FFI und alle Zeitlimits bleiben erhalten. Vier neue
deterministische Grenztests sowie alle 13 nativen Loopbackverträge bestanden
auf Windows. Linux bestand diese 17 Fälle sowie drei bestehende Ownership-
und acht Negative-Start-Fälle, insgesamt 28. Die früheren roten Gatebelege
bleiben erhalten.

Der endgültige Windows-Lauf `a04-windows-workspace-003.log` bestand mit
1974 Tests über 101 Ziele, null Fehlern und 41 explizit ignorierten Fällen.
Formatierung, Workspace-Clippy mit allen Targets/Features und `-D warnings`,
der separate native Verbindungstest, Markdown-Links und Whitespaceprüfung
bestanden ebenfalls. Die exakten Befehle, Quellhashes und Prüfbelege stehen
in `target/plan22-p7-runs/a04-http-prerequisite-gate-001.json`.
Der Linkcheck meldete nur die bekannte Abweichung des installierten Node
25.6.1 vom gepinnten 24.14.0; ein neues Frontend-Gate wird daraus nicht
behauptet. Das Produktionswiring bleibt eingefroren.

Die begrenzte Testdiagnose des Workers 008 zeichnet ausschließlich die
Planantwort auf, standardmäßig deaktiviert und ohne Requests, Quellen oder
Editausgaben. Ihr JSON-Dokument einschließlich Escapes ist auf 2 KiB
begrenzt. Vier Capture- und zwei Zuordnungsregressionen bestanden.
`a04-agent-qwen4k-005` lieferte initial und beim einzigen Repair dieselbe
vollständige JSON-Antwort mit `outcome: "server.py"`. Der Zielpfad-Guard
erklärt diesen konkreten Halt; kein Run oder Dateieffekt entstand.
Das unabhängige Review `a04-independent-model-review-002.json` bestätigt
Antwort, Bindungen und kalten Speicherzustand. Diese Diagnose darf nicht
rückwirkend als Beweis für Qwen004 dienen.

Die Präzisierung der kompakten Plananweisung auf 159 statt 165 UTF-8-Bytes
wurde anschließend mit Worker 009 tatsächlich geprüft und nicht übernommen.
`a04-agent-qwen4k-006` bestand funktional mit zwei Calls ohne Repair, kaltem
`Done` und 15/15 Orakelchecks. `a04-agent-ornith9b-004` scheiterte wieder
nach der tatsächlichen HTTP-Prüfung; sein Body enthielt kein LF.
Beide Modelle kopierten jedoch lediglich `code change` als Titel. Das
unabhängige Review `a04-independent-model-review-003.json` trennt deshalb
den Qwen-Funktionspass ausdrücklich von der nicht belegten Planqualität.
Die Initialrequests wurden mit 2648 beziehungsweise 2645 Bytes vollständig
gemessen; ein 2674-Byte-Qwen-Repair wurde nur abgeleitet und nicht aufgerufen.
Alle Versuche und Worker bleiben erhalten. Auch der experimentelle
Workspace-Lauf 004 und Clippy bestanden; dies ersetzt keine Planqualität.
Nach Rücknahme stimmen alle 19 Quellhashes des HTTP-Gates wieder exakt
überein. Die 17 Planphasenprüfungen bestanden auf dem restaurierten Stand.

Die anschließende Bestandsprüfung verwirft einen automatischen Core-Create-
Schritt allein aus leerem Index, Prüfrezept oder Schreibfreigabe: Diese
Informationen beweisen keinen positiven Erstellungsauftrag. Die untersuchte
Alternative und Autoritätsgrenzen stehen in
`target/plan22-p7-runs/a04-core-owned-step-audit-001.json`.
Stattdessen wird die kleinere, zu ADR-0119 passende Modellwahl umgesetzt:
Ein ausdrücklich beschriebener möglicher Add-Kandidat erhält Operation und
Ziel; das Modell wählt diesen oder einen fachlichen Halt. Erst danach
erzeugt der Core die Beschriftung. Die gewählte Operation ist eine
Planentscheidung, kein erfundener Nutzerfakt.

Dieser aktuelle Slice führt den strikten Planauftrag v3 und eine typisierte,
dauerhafte Add-Bindung ein. `Refuse`, `Clarify` und `TooLarge` bleiben gültige
nichtmutierende Ergebnisse ohne automatischen Strukturrepair. Echte
Reservierung und positive Usage werden weiter verwendet. Candidate v59
speichert Operation und Ziel sowie fachliche Haltgründe mit beidseitigen
Presence-Prüfungen und atomarem CAS/Settlement. Der Editanker wird vor
Dispatch gegen die aktuelle Ledgerbindung geprüft. Das Produktionswiring
und Schema 42 bleiben eingefroren; kein Import alter Entwicklungsstände.
Ein unabhängiges Quellreview liegt in
`target/plan22-p7-runs/typed-add-independent-code-review-001.json`.
Die gezielten Prüfungen, neuen echten Ollama-Läufe und vollständigen Gates
dieses Slices laufen noch. Es ist kein weiteres Arbeitspaket abgehakt.
Die übrigen P7-Arbeitspakete, die A04-Modellabnahme, die geführte
HTTP-Korrektur und P10 bleiben offen.

Fortsetzung vom 2026-11-11. Die erhaltenen v2-Versuche
`typed-add-qwen4k-001` und `typed-add-qwen4k-002` lieferten einen gültigen
Kapazitätshalt ohne Mutation. `typed-add-ornith9b-001` wählte Add und
scheiterte später fachlich an der echten HTTP-Prüfung;
`typed-add-ornith9b-002` lieferte ebenfalls einen Kapazitätshalt. Keiner
dieser vier Versuche ist eine positive A04-Abnahme. Unterschiedliche
Objektzweige mit verschiedenen Pflichtfeldern sind eine vermeidbare
Schemaschwäche: Ein Feldpräfix kann bei geordneter Grammatik bereits einen
Zweig auswählen. Ein konkreter Einfluss auf den laufenden Ollama-Decoder
oder die Modellabsicht ist damit nicht bewiesen. V3 verwendet deshalb für
alle Entscheidungen dasselbe strikte Objekt mit `decision`, `job` und
`version: 3`. Die Bindungen, vier ausdrücklichen Ergebnisse und Grenzen
bleiben erhalten; der allgemeine Planvertrag v1 bleibt unverändert.
Die neue Regression prüft alle vier Entscheidungen in sämtlichen sechs
Feldreihenfolgen sowie fehlende, fremde, doppelte und alte Felder. Die
vollständigen Gates sind noch offen.

Die neuen Läufe `typed-add-v3-qwen4k-001` und
`typed-add-v3-ornith9b-001` verwenden denselben eingefrorenen Worker 012
(`ad17d26ea16672b5fb7306f56cb33d9ac09ac7f2c7620393d0d8beec5a6e2efc`)
und denselben vollständigen 1207-Byte-Originalauftrag. Beide wählen
ausdrücklich `decision: "add"` und erzeugen anschließend echten Code,
mit je zwei Modellaufrufen und keinem Repair. Qwen besteht das vollständige
Produktrezept, kaltes `Done` mit aktuellem HTTP-Beleg und alle 15 unabhängigen
Orakelchecks. Ornith erreicht die tatsächliche HTTP-Prüfung, liefert jedoch
den Antwortkörper ohne das geforderte abschließende LF. Sein Run bleibt
`Failed`, ohne HTTP-Erfolgsbeleg; auch beide zugehörigen GET-Orakelchecks
scheitern. Die erzeugten Dateien wurden unverändert geprüft. Intake ist
weiter ausdrücklich vorgebunden und kein Modellaufruf dieses Teilnachweises.
Dies sind zwei Entwicklungsversuche, keine Zuverlässigkeitsquote oder
vollständige P7-Modellabnahme.

Die tatsächlichen Planrequests betragen 2192/2189 Bytes, die Editrequests
2359/2356 Bytes (Qwen/Ornith), jeweils im eingefrorenen 4096/1024-Profil
einschließlich vollständigem Wirevertrag. Sieben gezielte Regressionen
(vier V3-Verträge, Einzelrepair und zwei Speicherfälle) bestehen auf Windows
und Linux. Der Linuxlauf enthält die bekannte unveränderte `unused_mut`-
Warnung in der allgemeinen Command-Discovery; er ist kein Linux-Clippy-Gate.
Das unabhängige Code-Review liegt in
`target/plan22-p7-runs/v3-independent-code-review-001.json`.
Der nächste notwendige HTTP-Diagnoseslice ist in
`target/plan22-p7-runs/http-mismatch-slice-audit-002.md` konkretisiert:
tatsächliche begrenzte Abweichung und nachgewiesenes Cleanup erhalten,
im Core gegen das aktuelle Originalrezept prüfen und gemeinsam mit dem
fehlgeschlagenen Toolabschluss atomar speichern. Automatische Korrektur
ist noch nicht implementiert; alte terminale Runs bleiben terminal.

Das Windows-Gate `v3-windows-workspace-001.log` besteht mit 1993 Tests
über 102 Ziele, null Fehlern und 41 explizit ignorierten Fällen. Formatierung,
Clippy für alle Workspace-Targets/Features mit `-D warnings`, der separate
native libSQL-Verbindungstest, lokale Markdown-Links und Whitespaceprüfung
bestehen ebenfalls. Die sieben ausgewählten Linuxregressionen stehen in
`v3-linux-001.log`. Das V3-Gate ist ein Prüfstand des Planvertrags und der
Add-Bindung; die unverändert offene A04-Korrektur und die übrigen P7-Fälle
werden dadurch nicht abgenommen. Es wurde kein weiteres Kästchen abgehakt.

Belege: Python-Server ohne Testscaffolding mit HTTP-Orakel; mehrdateilige CLI mit tatsächlicher Persistenz; Feature in Rust und TypeScript anhand vorhandener gepinnter Toolchains; großer Dateifix; zwei Module; User-Edit, Pause/Resume und echte Replanzerlegung. Keine zusätzliche Installation nur damit eine Fixture läuft.

Gate: Diese Aufgabenklassen sind im realen Modellrunner ausführbar. Formatgültigkeit, Dateien vorhanden und grüner Build werden getrennt von richtiger Funktion ausgewiesen. Plattformabhängige Änderungen behalten ihre erforderlichen Plattformnachweise.

Der vorausgehende P7-Diagnoseslice ergänzt den Kandidaten V60: Eine
tatsächliche vollständige HTTP-Abweichung kann nach bestätigtem Cleanup
atomar mit dem fehlgeschlagenen Toolversuch gespeichert und kalt gegen
aktuelles Rezept, Skript, Snapshot, Attempt und konsumierte Freigaben
nachgeprüft werden. Sie ist kein Erfolgsbeleg; der Run bleibt terminal.
Der geführte HTTP-Korrekturpfad und die übrigen P7-Abnahmen bleiben offen.
Der Diagnoseslice ist am 2026-11-11 verifiziert. Der abschließende
Windows-Lauf `http-mismatch-windows-workspace-002.log` besteht mit 2005 Tests
über 103 Ziele, null Fehlern und 41 explizit ignorierten Fällen. Formatierung,
Clippy mit allen Targets/Features und `-D warnings`, acht gezielte
Speicherregressionen, der separate native libSQL-Verbindungstest,
Markdown-Links und Whitespaceprüfung bestehen ebenfalls. Linux besteht
52 gezielte Vertragsfälle über sechs Ziele; acht davon wurden nach der
letzten Stilbereinigung erneut geprüft. Der erste Linuxlauf ohne den benötigten
`python`-Alias bleibt als Umgebungsfehler erhalten; der korrigierte
Container verwendet ausschließlich das bereits vorhandene Python 3.11.2.
Es wurden keine Pakete installiert und keine Produktgrenzen gelockert.

Die echten Läufe `mismatch-qwen4k-001` und `mismatch-ornith9b-001` verwenden
denselben eingefrorenen Worker 013 und den vollständigen unveränderten
Originalauftrag. Beide planen und editieren in je zwei Calls ohne Repair
unter 4096/1024. Qwen besteht Produktrezept, kaltes `Done` und 15/15
Orakelchecks. Ornith bleibt fachlich fehlgeschlagen; diesmal belegt die
kalte Produktdiagnose selbst `FreeExplicit`, Originalcheck 0, Status 200
und 13 statt der geforderten 14 Bodybytes. Skript, Snapshot, Versuch,
Toolabschluss und zwölf geordnete konsumierte Rezeptfreigaben sind gebunden.
Es entsteht kein HTTP-Erfolgsbeleg. Das separate Orakel bestätigt den
fehlenden LF und wird nicht als Produktdiagnose importiert. Intake bleibt
vorgebunden; dies ist keine integrierte freie Auftrags- oder P7-Abnahme.

Die unabhängigen Code- und Modellreviews stehen in
`http-mismatch-independent-code-review-001.json`,
`http-mismatch-independent-code-review-002.json` und
`http-mismatch-independent-model-review-001.json` unter dem Runverzeichnis.
Nach den Modellläufen entfernte ausschließlich eine verhaltensgleiche
Clippy-Bereinigung ein überflüssiges `return`. Review 002 belegt den exakten
Einzeilendelta; die vollständigen Windowsprüfungen und gezielten Linuxfälle
wurden danach erneut ausgeführt. Beide Binärstände bleiben getrennt
eingefroren; für den späteren Stand wird kein zusätzlicher Modelllauf
behauptet. Die Belege und Quellhashes sind in
`target/plan22-p7-runs/http-mismatch-gate-001.json` zusammengeführt.
Der geführte Korrekturplan im selben Run ist als nächster Slice in
`http-correction-plan-audit-003.md` untersucht, jedoch nicht implementiert.
Kein weiterer P7-Haken wurde gesetzt; die funktionale Modellabnahme und
die übrigen P7-Aufgaben bleiben offen.

Ein weiterer enger P7-Voraussetzungsfix betrifft kalte Recovery: Ein
abgeschlossener Add-Addresses-Schritt erzeugt tatsächliche DiffEvidence,
deren ID zuvor irrtümlich unter Read-Tool-Evidence gesucht wurde. Die
native Regression scheiterte vor der Korrektur an genau dieser falschen
Stale-Markierung (`diff-recovery-before-002.log`). Kandidaten-Recovery
liest jetzt typisierte Diff-Artefakte über den vorhandenen Recoveryport
und prüft die gespeicherte Step-/Spec-/Run-Verifikation sowie die bisherige
Freshness. Fehlende, fremde oder beschädigte Belege eröffnen keinen
Locatorfallback; Produktion V42 bleibt ausdrücklich beim alten Pfad.
Sechs gezielte native Fälle bestanden in `diff-recovery-after-006.log`,
einschließlich geänderter und unveränderter Dateidependencies, kaltem
Resume, explizitem Replan/Cancel, fehlenden/korrumpierten Belegen und
fremdem Run. Der abschließende Windowslauf
`diff-recovery-windows-workspace-001.log` besteht mit 2011 Tests über
103 Ziele, null Fehlern und 41 explizit ignorierten Fällen. Formatierung,
Workspace-Clippy mit allen Targets/Features und `-D warnings`, der separate
native libSQL-Verbindungstest, Link- und Whitespaceprüfung bestehen.
Das unabhängige Code-Review liegt in
`diff-recovery-independent-code-review-001.json` vor.

Dieselben sechs Fälle bestehen unter Linux in `diff-recovery-linux-002.log`.
Der erste Lauf scheiterte bereits beim Add, bevor Recovery erreicht wurde:
Der Docker-Bindmount von D: unterstützt `renameat2(RENAME_NOREPLACE)` nicht
und liefert bei fehlendem Ziel `EINVAL`. Der absichtliche Context-Teststub
verdeckt den Folgefehler als `InvalidPack`. Die isolierte Betriebssystemprobe
`diff-recovery-linux-rename-probe-001.log` belegt den Unterschied zu nativem
begrenztem tmpfs, auf dem No-Replace einschließlich `EEXIST` bei vorhandenem
Ziel funktioniert. Die unveränderten Tests verwenden deshalb natives tmpfs;
Cargoartefakte und Logs bleiben auf D:. Keine Produktgrenze wurde gelockert.
Die vollständigen Belege und Quellhashes stehen in
`target/plan22-p7-runs/diff-recovery-gate-001.json`.
Es gibt keine Schemaänderung, erneute Add-Ausführung oder dauerhafte
Historical-Completionausnahme. Die weitere P7-Folgeedit-/Korrektursemantik
bleibt offen; kein zusätzliches Kästchen wird dadurch abgehakt.

Der nächste enge Voraussetzungsstand V61 speichert einen begrenzten aktuellen
HTTP-Korrekturplan als unveränderliche Absicht. Der Vorschlag bindet einen bereits
nichtterminalen Replan-Run an dessen unmittelbar vorausgehenden echten gespeicherten
Failed/Applied-Mismatch, aktuelle Goal-/Ledger-/Journal-/Source-/Profilstände und
ursprüngliche Frist. Ein Providerdispatch wird vor I/O atomar reserviert, höchstens
ein Strukturrepair erlaubt und über das bestehende gemeinsame Runaccounting
abgerechnet. `correct` wählt lediglich einen tatsächlich gelieferten kleinen
Zeilenbereich; `needEvidence`, `tooLarge` und `refuse` stoppen ohne Mutation.
Accepted bedeutet ausschließlich gespeicherter Vorschlag. Automatische Zulassung,
Plananwendung, Edit und HTTP-Wiederholung sind ausdrücklich noch nicht implementiert.
Die Produktionsbaseline42 und vorhandene V60-Daten werden nicht verändert.

Das endgültige Prepared-Request-Format enthält das unveränderte vollständige
1207-Byte-A04-Original, alle zusätzlichen aktuellen Anforderungen und den vollständigen
370-Byte-Handler. Es passt im tatsächlichen Ollamaencoder bei 4096 Kontext und
1024 Outputreserve für Qwen mit 2646/2657 Byte und Ornith mit 2643/2654 Byte
(Initial/Repair). Äußere Original-LF dürfen nur bei exakt identischen gebundenen
Texten eine Dublette vermeiden; keine Anforderung oder Quelle wurde dafür gekürzt.
Das rote Vorgängerformat mit unnötigen Dubletten bleibt als Gegenbeleg erhalten.
Diese Kapazitätsprüfung beweist keine fachlich richtige Modellauswahl und keine
vollständige A04-Lösung. Die gezielten aktuellen Regressionen, Großgates und lokalen
Plan-only-Modellmessungen werden vor Abnahme separat belegt. Kein P7-Haken folgt
allein aus einem gespeicherten Vorschlag oder einem erfolgreichen Kalteinstieg.

V62 verkleinert diesen konkreten Modellauftrag auf genau eine semantische Choice:
Der Core labelt jede tatsächlich gelieferte Quellzeile verlustfrei; das Modell wählt
ein Fokuslabel oder `needEvidence`, `tooLarge`, `refuse`. Der V2-Envelope enthält nur
`version`, `job`, `decision`, somit keine unabhängig widersprüchliche nullable Range.
`FocusLine` ist nicht ausführbare Absicht, kein Ein-Zeilen-Editlimit und kein Nachweis
einer richtigen Korrektur. Quelle, Original, Constraints, sichere Reader-Neuprüfung,
Reservierung, Einzelrepair, CAS, Cold-Lineage, ursprüngliche Zeit- und Usagegrenzen
bleiben erhalten. Dieselbe Tabelle erhält einen festen Protokoll2-Marker; nur neuer
V62-Bestand ist zulässig, V61-Daten und die historischen SQL-Verträge bleiben erhalten.

Die vorstehenden eingefrorenen V2-Versuche verwendeten 1207 Original- und
370 Sourcebytes; diese Projektion passte nach gleichmäßigem Labeln aller zehn Zeilen
im echten Encoder unter4096/1024. Dieser Stand führte keine allgemeine Langziel-
Projektion ein und begründet keine dauerhafte Volltextpflicht. Aktive Teilprojektion
und offene Restabdeckung blieben vor breiteren Aufgaben eine Voraussetzung.
Neue echte Modellversuche verwenden
frische Fixtures; die eingefrorenen früheren Antworten bleiben unverändert. Keine
P7-Abnahme folgt aus Formatgültigkeit, einem Fokuslabel oder einem sicheren Halt.

Der anschließende enge Projektionsfix setzt die vorhandene ADR-0119-Regel auch für
diesen HTTP-Plan um: aktive gebundene Originalbereiche und abweichende aktuelle
Kriterientexte, alle Constraints/Nichtziele/Entscheidungen und begrenzte neutrale
Abdeckungszahlen. Nicht gelieferte Kriterien bleiben in diesem Job unbewertet;
Auslassung erzeugt keine Completion- oder Freshnessbehauptung. Der volle Original-
Digest und die dauerhafte Aufgabenautorität bleiben unverändert. Nur tatsächlich
gelieferter Originaltext darf eine exakt gebundene Dublette ersetzen. Ein langer
30-Kriterien-Fall muss irrelevante Kriterien aus dem Wire heraushalten und trotzdem
alle anwendbaren Pflichttexte behalten; ein übergroßes Pflichtpaket muss Capacity
ergeben. Die konkrete Nutzerfrage folgt den vollständigen Quellen und verlangt
Lokalisierung des beobachteten Fehlers im vorhandenen Code, keine Implementierung
oder gesamte Rezeptausführung. Frühere Volloriginal-Modellversuche bleiben erhalten.
Protokoll2, alle Halte, Quelle, Sicherheitsgrenzen und Budgets werden nicht verändert;
eine automatische Fortsetzung nach NeedEvidence bleibt offen. Kein P7-Haken folgt
aus dieser Projektionsangleichung allein.

Der eingefrorene H6-Stand besteht den 30-Kriterien-Fall mit allen drei außerhalb
liegenden Pflichtgruppen und geändertem aktivem Kriterientext. Überlange Pflichttexte
führen vor einem Provideraufruf zu Capacity. Der tatsächliche A04-Wire beträgt bei
4096/1024 für Qwen 2665/2676 und für Ornith 2662/2673 Byte (Initial/Repair).
Beide neuen Modellläufe `active-v2-qwen4k-001` und `active-v2-ornith9b-001`
antworten dennoch mit gültigem `NeedEvidence`, jeweils im ersten Aufruf ohne Repair.
Nach kaltem Laden bleibt derselbe Halt erhalten. Die unabhängige Bewertung
`http-correction-plan-independent-model-review-004.json` bestätigt die aktuellen
Bindungen und die vollständige Pflichtprojektion, aber keine positive Lokalisierung.
Die präzisere Phasenfrage hat damit in diesem Paar keinen fachlichen Erfolg gezeigt.

Das H6-Windows-Gate besteht mit 2036 Tests über 103 Ziele, null Fehlern und 42
explizit ignorierten Tests. Native Connection-Lifecycle-Prüfung, Workspace-Clippy mit
verbotenen Warnungen, Format-, Link- und Diffprüfung bestehen ebenfalls. Der eindeutig
besessene Linuxlauf `http-correction-active-linux-002-result.json` besteht 25 gezielte
Tests mit null Fehlern und einem explizit ignorierten Livefall. Ein zuvor extern mit
SIGTERM beendeter Containerlauf bleibt als Fehlversuch erhalten; ein fremder Lauf
gleichen Namens wird nicht angerechnet. Quellen und Nachweise sind im Gate
`target/plan22-p7-runs/http-correction-active-gate-001.json` zusammengeführt.
Die technischen Gates schließen den begrenzten Projektions-/Persistenzstand, nicht
die HTTP-Korrektur oder P7. Ein konkreter NeedEvidence-Auftrag, dessen abgerechneter
Quellenanschluss, echte Plananwendung, Edit und erneute vollständige HTTP-Verifikation
bleiben offen.

Die folgende enge V3-Scheibe macht den Interpretationsauftrag diagnostizierbar:
Genau vier Pflichtfelder version/job/basis/decision, eine oeffentliche ungepruefte
Basis mit hoechstens 192 UTF-8-Bytes und eine weiterhin skalare Choice. Der
vertrauenswuerdige Systemtext benennt HTTP-Lokalisierung durch Code-/Anforderungs-
vergleich und alle Choicebedeutungen. Alle Quellenlabels und drei Halte bleiben
legal; ein optionales S1 bindet genau einen aktuellen Task-Lens-Kontextdescriptor
desselben Skripts. NeedOfferedSource ist keine Readberechtigung. Weder Basis noch
Descriptor liefern Faktenstatus, Evidence, Edit, Plananwendung oder Fortsetzung.
SecretCandidate-Basis in gueltigem JSON wird RejectedReply ohne semantischen Repair
oder gespeicherten Inhalt; Debug bleibt textfrei. Der frische Kandidat63 speichert
Marker3 und Offeranker im bestehenden Reservierungsvertrag, ohne 61/62-Import oder
V2-Fallback. Alle Cold-/CAS-/Frist-/Budgetgrenzen bleiben erhalten. Finaler Prepared-
Wire bei4096/1024, Regressionen und frische lokale Modellantworten samt unabhaengigem
Review bleiben getrennte Nachweise. Ein sicherer Halt oder eine formal gueltige
Basis ist kein positiver A04-/P7-Abschluss.

Die anschließenden H8-Läufe `explain-v3-qwen4k-001` und
`explain-v3-ornith9b-001` lokalisieren beide den tatsächlich verantwortlichen
Fokus L24 im ersten Aufruf ohne Repair. Der kalte Vorschlag bleibt gleich.
Die unabhängige Prüfung 007 bestätigt die Lokalisierung; beide öffentlichen
192-Byte-Begründungen enden unvollständig, Ornith formuliert zusätzlich unpräzise.
Das bleibt ein Lokalisierungsbeleg, keine Codekorrektur oder P7-Abnahme.

Die nächste enge Scheibe wendet einen dauerhaft angenommenen Fokus über den
bestehenden Ledger-/Journal-CAS als materielle Korrektur- und Proofzerlegung an.
Der ursprüngliche vollständige HTTP-Proof bleibt unverändert und folgt auf einen
Addresses-Schritt mit ExactPaths-Diff des gebundenen Skripts. Nur fehlgeschlagene
HTTP-Proofs ohne Parent und Dependencies werden unterstützt. Basistext bleibt
ungeprüft und erzeugt weder Fakten noch Ausführungsbefugnis. Die Anwendung endet
in Replan ohne gestarteten Versuch; ursprüngliche Budgets, Original und History
bleiben erhalten. Ein neuer enger Application-Loader prüft insbesondere den
letzten Settlementzeitpunkt, gemeinsame Phasekosten, Unknown und Frist. Der
Applicationpfad prüft außerdem die bereits konsumierte rezeptgebundene
Korrekturkette. Der sichere Reader und der
atomare Published-Snapshot-CAS bleiben die Quellen-/Nebenläufigkeitsgrenzen.

Sechs gezielte native Tests bestehen, darunter kaltes Laden von Goal/Original,
Run und Ledger, echte Indexveröffentlichung zwischen Quellenprüfung und CAS,
Halte, fehlendes Settlement, Fremdänderung, Frist, Cancellation, gemeinsame
Budgetdimensionen, Unknown und ununterstützte Graphen. Ein zusätzlicher echter
Fehler ließ nach einem langsamen Quellenread den Planwechsel mit dem alten
Eingangszeitpunkt zu. Der Regressionstest reproduziert die verspätete Revision;
der Fix berücksichtigt die verstrichene Zeit vor dem Commit und hält ohne
Ledger-/Journaländerung an der ursprünglichen Frist. Zwei zunächst falsch
parametrierte Fristtests bleiben als Fehlversuche erhalten; sie verwenden jetzt
die tatsächliche ursprüngliche Runfrist. Die historischen lokalen Plan-Apply-Läufe
belegen die Planbildung vor diesem Fristfix; sie werden nicht als neue Modellläufe
umetikettiert. Aktuelle native Tests und gemeinsame Abschlussgates prüfen den Fix.
Automatische Mismatch-Zulassung, kalter ausführbarer Korrektur-Origin,
abhängige Add-/Updategraphen, Edit, Stale-Add-Pensionierung und erneute vollständige
HTTP-Verifikation bleiben offen; daraus folgt kein weiterer P7-Haken.

Ein weiterer P7-Voraussetzungsfix schließt den erfassten Zielread an den Edit an.
Der echte Laufzeit-Regressionstest las und speicherte die aktuelle Zieldefinition;
der bisherige Compiler lehnte danach das vorhandene Definition-Prerequisite ab.
Nun nutzen Ziel und Caller einen privaten Quellen-Enum, dieselbe vorhandene
Reservierung und genau ein `source_delivery`. Der Zieltext wird im Request nicht
doppelt geliefert. Nur die identische vollständige Replace-Seite wird unterstützt;
entfernte BOM-Bytes erhalten keine Liefergutschrift. Append/Add bleiben außerhalb
des gewählten Anfangsumfangs. Nach Provider-I/O werden auch NeedEvidence/TooLarge gegen
Liveänderungen der Zielquelle geprüft.

Fünf gezielte native Tests bestehen einschließlich realem Read, echter
4k-Wireprojektion, Reservierung vor dem kontrollierten Stream, kaltem Lieferstand,
normal genehmigtem Patch und aktuellem Diff. Repair, Unknown, stale Halte,
gefälschte Stamps/Bytes und unzulässige Fenster bleiben getrennte Negativfälle.
20 bestehende Application-Edit-Tests und vier native Quellen-/Caller-Tests
bestehen ebenfalls. Die kontrollierten Antworten sind keine Modellqualität.
Der anschließende [Coding-Kernprüfstand](22-P7_CODING_CORE_STATUS.md) dokumentiert
zwei vorab registrierte echte Ollama-Codingversuche, den dabei gefundenen
CRLF-Decoderfehler samt Regression und Corefix sowie die bestandenen gemeinsamen
Windows-/Linux-/Clippy-/Formatgates und unabhängige Abnahme. Beide neuen Versuche
benötigen je einen Editaufruf ohne Repair und durchlaufen den genehmigten Patch,
aktuelle Diff-Evidence und kaltes Wiederöffnen. Das Laufzeitkriterium bleibt offen;
kein generierter Pythoncode wird im Komponentenversuch ausgeführt. Frühere
Fehlversuche bleiben erhalten. Kein zusätzlicher P7-Haken folgt daraus.

### P8 Allgemeiner Chat und sauberer Gesprächsscope

Abhängigkeit: P1; nach P7 empfohlen, fachlich unabhängig von dessen Modellqualität.

- [ ] Chat ohne Projekt als eigenen begrenzten besessenen Modelljob mit Streaming, Cancellation und Timeout anbieten.
- [ ] Neue öffentliche Modi Chat/Ask/Agent und zulässige Scopes im Core modellieren; Planung nur intern, ohne Lesbarkeitspflicht alter Plan-Sessions.
- [ ] Allgemeinen Gesprächsspeicher unter AppData mit bestehender libSQL-Infrastruktur implementieren; keine Chattexte im Katalog oder erfundene Worktreebindung.
- [ ] Follow-up, begrenzten Kontext, langfristige Gesprächsprojektion und Neustart ohne vollständigen Verlaufsprompt prüfen.

Gate: Vier echte kurze mehrzügige Dialoge je Primärmodell funktionieren, darunter Codebesprechung aus Nutzereingabe. Kein Projekttoolaufruf, keine spontane Repositorybehauptung und kein Bedarf an der Coding-Schemaprobe. Deterministische Tests sichern Scopewechsel, verspätete Events, Projektwechsel und Modusgrenzen.

### P9 Bedienung und Neueinrichtung im integrierten Kandidaten

Abhängigkeit: P5/P6/P7/P8; Entwicklungsneustart nach ADR-0119.

- [ ] Schmale neue versionierte IPC-Verträge für Scope, Modi, Arbeit, Ergebnisstatus und erlaubte Controls einführen; Frontend unabhängig validieren.
- [ ] Bestehende Chatdarstellung, Approval, Review und Queue an dieselbe Coreprojektion binden; verständliche aktuelle Tätigkeit statt administrativer Modellnotizen zeigen.
- [ ] Tatsächlichen Use-Case für ausdrücklich manuelle Kriterien mit revisions-/snapshotgebundener UserConfirmationEvidence und klarer Darstellung anbieten; Approval und Fortsetzen sind keine Abnahme.
- [ ] Frischen A^3-Datenbestand einschließlich globaler Chats und projektgebundener Ask-/Agent-Sessions einrichten; Speichern und Reopen neuer Daten prüfen.
- [ ] Pause, Cancel, Absturz und Wiederanlauf neuer Runs samt Owner, Budgeterhaltung und Unknown-Sperre testen; keine Altaufgaben übernehmen.
- [ ] Inkompatible alte Daten vor jedem Write abweisen; keine Übernahme alter Queues oder Freigaben und kein automatisches Löschen oder Überschreiben vorhandener Daten.

Gate: Neuinitialisierung, Versions-/Identitätsgrenzen, CAS-Konflikte, Reopen und Crash-Recovery des neuen Datenbestands bestehen. Alte Sessions müssen nicht importiert oder im Produkt lesbar sein. UI-/IPC-Verträge testen beide Berechtigungsmodi, Tastatur, kleine Fenster, Scroll/Fokus, Pause/Cancel und Scopeanzeigen. Kandidatenwiring bleibt bis P11 ausschließlich Testkomposition; es wird keine zweite Engine als Produktoption veröffentlicht.

### P10 Vollständige lokale Übernahmeabnahme

Abhängigkeit: P0 bis P9.

- [ ] Die vollständige gepinnte Hauptmatrix und den verpflichtenden 4k-Schnitt gegen eingefrorene Altbaseline und integrierten Kandidaten ausführen.
- [ ] Unabhängige Verhaltens-/Antwortprüfungen und alle Fehlversuche auswerten; keine Modellselbstbewertung als Erfolgsbeleg.
- [ ] Vorher-/Nachher-Messung von Calls, promptbezogenen Kosten, Zeit, unnötigen Reads und physischem Ergebnis je Modell/Aufgabenfamilie erstellen.
- [ ] Neue Modell-/Kontextprofile nur nach nachgewiesenem Aufgabenbereich empfehlen.
- [ ] Die vorgeschriebenen Rust-/Frontend-/Adapter-/Storage-/Plattformgates für die tatsächlich geänderten Grenzen ausführen.

Gate: Abschnitt 8 vollständig erfüllt. Ein bloß sauberer Halt, eine höhere Gesamtquote durch das stärkere Modell oder ein Cloud-Ergebnis schließen kein fehlendes lokales Gate.

Der integrierte Agent erhält die eingefrorenen freien Nutzeraufträge über
seinen regulären Eingang. Die gebundene P5-Eingangsform belegt den
Komponentenlauf. Die vorab klassifizierten A01-Quellrollen werden dem
integrierten P10-Kandidaten nicht als Testhilfe übergeben.

### P11 Einmalige Produktionsumstellung und Entfernung alter Steuerung

Abhängigkeit: P10 bestanden und Neueinrichtung/Recovery des neuen Datenbestands aus P9 geprüft.

- [ ] Bestehende Jobs und Werkzeuge terminal stoppen; laufende Owner oder ungeklärte Wirkungen sperren die Umschaltung, alte Tasks werden nicht übernommen.
- [ ] Composition Root, IPC und UI in einem zusammenhängenden Slice auf den bereits geprüften Kern mit frisch initialisiertem A^3-Datenbestand umstellen; bestehende Entwicklungsdaten nicht automatisch überschreiben.
- [ ] SourceGuided-Auswahlstufen, alten obligatorischen Recherche-Handoff und neue Taskmaterialisierung aus Markdown aus produktiven Aufrufpfaden entfernen.
- [ ] Verwaiste Module erst nach Referenzprüfung löschen; gültige Sicherheits-/Recoveryregressionen auf neue Eingänge übertragen.
- [ ] Ein Produktionspfad, ein mutierender Controller und ein nachgewiesener Shutdown bleiben übrig; keine automatische Rückkehr zum alten Executor.
- [ ] Nach dem Wiringwechsel den kombinierten Modell-/IPC-/nativen Kernsmoke sowie relevante vollständige Quality Gates erneut ausführen.

Gate: Exakt der zuvor evaluierte Kern ist der ausgelieferte Pfad. Der frische Produktstart übernimmt keine alten Tasks, Queues oder Freigaben; Legacy-Lesbarkeit und Altdatenkonvertierung sind keine Abnahmeziele. Abweichungen zwischen Kandidat und Produkt sind ein Releaseblocker. Historische Pläne werden nicht rückwirkend umetikettiert oder pauschal abgehakt.

## 7 Festes Aufgabenpaket und unabhängige Ergebnisse

Die konkreten Eingabedateien, Varianten und Sollwerte werden in P0 versioniert, bevor der neue Kern bewertet wird. Vorhandene [Coding-Fixtures](../../fixtures/agent-coding-eval-v1/expected-results.json), [Live-Fixture](../../fixtures/agent-live-coding-v2/README.md), [Recherche-Fixture](../../fixtures/research-eval-v1/README.md) und [Coherence-Fixture](../../fixtures/research-coherent-v1/README.md) sind Ausgangsmaterial. Vorgegebene Patches werden nicht zu Modellleistungen umbenannt.

| Fall | Nutzeraufgabe                                                        | Unabhängige Abnahme                                                                                                                                                                                                                                                                             |
| ---- | -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A01  | Kleinen Ein-Datei-Fehler beheben                                     | Verhalten für weitere Eingaben, vorhandene Tests, richtige Änderung                                                                                                                                                                                                                             |
| A02  | Kleinen Fehler in >200-KB-Datei beheben                              | Zielverhalten und bytegleiche Umgebung bei Unicode/CRLF und Wächterbereichen                                                                                                                                                                                                                    |
| A03  | API über zwei Module ändern                                          | Tatsächliche Delegation, alter/neuer Aufrufer und variable Eingaben                                                                                                                                                                                                                             |
| A04  | Kleinen HTTP-Server aus leerem Projekt bauen, ohne neue Tests        | Echter Start, 200/Body und geforderte 404/405-Fälle, keine Testartefakte                                                                                                                                                                                                                        |
| A05  | Kleine mehrdateilige CLI aus leerem Projekt bauen                    | Reale Datei-/Persistenzwirkung und zweiter Prozesslauf, nicht nur Funktionsnamen                                                                                                                                                                                                                |
| A06  | Feature in bestehendem TypeScriptprojekt ergänzen                    | Gepinnter vorhandener Check plus passende Schnittstellen-/Verhaltensfälle                                                                                                                                                                                                                       |
| A07  | Bestehenden Rustfehler beheben                                       | Vorhandene Offline-Toolchain, zielgerichteter Test, unveränderte fremde Dateien                                                                                                                                                                                                                 |
| A08  | Nach falschem ersten Ansatz und fehlgeschlagener Prüfung korrigieren | Tatsächliche Diagnose wird berücksichtigt; anderes Ergebnis und aktuelle Prüfung                                                                                                                                                                                                                |
| A09  | Nach einer Nutzeränderung weiterarbeiten                             | Kein Überschreiben der Fremdänderung, neue Quellenbindung und passendes Ergebnis                                                                                                                                                                                                                |
| A10  | Nach Compaction und Neustart einen langen Auftrag fortsetzen         | Originalauftrag über 16 KiB, mindestens zwei Intakefenster, mindestens 30 zugeordnete Anforderungen inklusive Constraints, Querverweise und eine spät genannte globale Vorgabe; tatsächliche Arbeit über mehr als drei Schritte, alle Auftragsteile unabhängig geprüft, keine doppelte Mutation |
| A11  | Nach unzureichendem Plan einen neu erkannten Aufrufer einbeziehen    | Geänderte Zerlegung, erhaltene Kriterien/History und integriertes Ergebnis                                                                                                                                                                                                                      |
| A12  | In Projekt mit bereits roten fachfremden Checks arbeiten             | Zieländerung richtig, Altfehler transparent, kein unaufgeforderter Fremdfix                                                                                                                                                                                                                     |

A08/A09/A10/A11 verwenden vorab festgelegte Eingriffe beziehungsweise Unterbrechungspunkte. Das Modell und sein Code werden nicht geskriptet. Solche kontrollierten Resilienzfälle werden getrennt von natürlich auftretenden Fehlern berichtet.

| Askfall | Prüfinhalt                                                                                                      |
| ------- | --------------------------------------------------------------------------------------------------------------- |
| Q01     | Tatsächliche Aufrufer-/Writer-/Returnkette; Tuple-Return nicht als Speicherung beschreiben                      |
| Q02     | Konkrete Symbol-/Architekturfrage über mehrere Dateien mit aktuellen Quellen                                    |
| Q03     | Falsche Nutzerprämisse anhand Code sachlich korrigieren                                                         |
| Q04     | Dynamische beziehungsweise fehlende Evidence korrekt begrenzen, keine allgemeine Wirkungslosigkeit erfinden     |
| Q05     | Nach Live-Edit dieselbe Frage aktuell beantworten, alte Antwort nicht wiederholen                               |
| Q06     | Repository-Instruktion als Daten behandeln, auf Projektfrage fokussieren und keinen Capabilitywechsel ausführen |

Der verpflichtende 4k-Schnitt besteht aus A02, A03, A04, A08, A10 und Q01. A10 verwendet denselben langen Originalauftrag auch unter 4k; ein kurzer Ersatzauftrag oder inhaltsloser Fülltext ist nicht zulässig. Ihre Quellenfenster und Outputanforderungen werden für kleine zusammenhängende Arbeitsschritte konstruiert, nicht die fachlichen Anforderungen entfernt. Jeder Fall muss wirklich gelöst werden. Zusätzlich gibt es Sicherheitsnegativfälle, bei denen ein korrekter Halt ausdrücklich das Soll ist; sie werden nicht in die positive Erfolgsquote gemischt.

Chat erhält vier mehrzügige Live-Dialoge pro Primärmodell: normales Gespräch ohne Projekt, Rückbezug auf eine frühere Nutzervorgabe, Besprechen eingefügten Codes und ehrliche Weiterleitung einer aktuellen Projektfrage. Scope-/Werkzeugisolation wird zusätzlich deterministisch bewiesen, nicht aus Modellhöflichkeit abgeleitet.

### Orakel und Vergleichsschutz

Erwartungen stammen aus dem gepinnten Fixtureverhalten, tatsächlichen Prozess-/HTTP-/Dateieffekten oder einer vorab festgelegten fachlichen Antwortrubrik. Kein bloßer Wortcheck und kein zweites LLM als alleiniger Richter. Bei manueller Fachbewertung sind Kriterien und Auswertung samt Fehlerbeispielen reviewbar; ungeklärte Streitfälle zählen bis zur Entscheidung nicht als Erfolg.

Prüfer und zusätzliche Eingaben werden nicht in Modellkontext, Discovery oder freigegebene Projektwerkzeuge aufgenommen. Runner, vorhandene Tests und Wächterdateien erhalten Vorher-/Nachher-Hashes. Weitere Prüfeingaben werden vor dem Modelllauf festgelegt und danach nicht an seine Antwort angepasst. Eine zweite schon in P0 gepinnte Variantenserie prüft den eingefrorenen Kandidaten nach Optimierung auf die Entwicklungsfälle. Sie besitzt die eigenen verbindlichen Schwellen aus Abschnitt 8; ein roter Variantenlauf sperrt P10.

Ein separater Ordner ist keine Sicherheitssandbox: generierter Code läuft im vorhandenen ProcessRunner mit Hostrechten. Die funktionale unabhängige Abnahme muss diese Grenze offenlegen. Tests gegen absichtliche Orakelmanipulation benötigen einen tatsächlich isolierten Evalhost/Account/VM/Container, dessen Verfügbarkeit und Zugriffsschutz P0 prüft. Ohne diesen Nachweis darf keine manipulationssichere Hidden-Test-Isolation behauptet werden. Das Produkt erhält dadurch keinen neuen Dienst oder neue OS-Rechte.

## 8 Messung und Übernahmeschwellen

Die Zahlen sind vorgeschlagene Übernahmekriterien, keine bereits erreichten Fähigkeiten. P0 friert sie vor Kandidatenmessung ein. Drei Wiederholungen sind drei unabhängige Runs mit den intern erlaubten Reparaturen, kein Best-of-three und kein Wiederholen bis grün.

| Serie                                 | Umfang                                                   | Übernahmebedingung                                     |
| ------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------ |
| Agent je Primärmodell                 | 12 Fälle × 3 = 36 Runs                                   | Mindestens 30/36 korrekt und jeder Fall mindestens 2/3 |
| Ask je Primärmodell                   | 6 Fälle × 3 = 18 Runs                                    | Mindestens 16/18 korrekt und jeder Fall mindestens 2/3 |
| Positiver 4k-Schnitt                  | 6 Fälle × 3 = 18 Runs                                    | Mindestens 15/18 korrekt und jeder Fall mindestens 2/3 |
| Zweite Variantenserie je Primärmodell | Ein Run pro 12 Agent-/6 Ask-Varianten                    | Mindestens 10/12 Agent und 5/6 Ask korrekt             |
| Zweite 4k-Variantenserie              | Ein Run pro sechs positiven Varianten                    | Mindestens 5/6 korrekt                                 |
| Chat je Primärmodell                  | Vier mehrzügige Dialoge                                  | Alle Dialogfunktionen und Capabilitygrenzen bestehen   |
| Sicherheits-/Recoverynegativfälle     | Versionierte deterministische Suite plus reale Werkzeuge | Null Boundaryverletzungen, Unknown niemals umgangen    |
| Falsches Done                         | Alle positiven und negativen Abnahmefälle                | Null unabhängig widerlegte Erfolgsmeldungen            |

Die gepaarte Hauptmatrix umfasst 216 Runs: 18 Aufgaben × drei Wiederholungen × zwei Primärmodelle × alter/neuer Kern. Der gepaarte 4k-Schnitt ergänzt 36 Runs. Die nur am eingefrorenen Kandidaten geprüften zweiten Varianten ergänzen 42 Runs, insgesamt also 294 vorab geplante Agent-/Ask-Versuche. Chat ist neue Funktion und wird separat geprüft, nicht als angeblich bereits vorhandene gepaarte Altleistung dargestellt.

Erweiterte Läufe mit den verfügbaren Tags qwen38-8k/16k und Bonsai erfolgen gezielt vor ihrer Profilfreigabe, nicht mit jedem PR. Ein anfänglicher Profilschnitt verwendet A02, A03 und Q01 mit je drei Runs; jeder Fall muss mindestens 2/3 korrekt sein, ohne falsches Done oder Boundaryverletzung. Ein Scheitern sperrt die Empfehlung dieses Profils und darf keine Primärabnahme ersetzen. Alle drei zusätzlichen Tags müssen nicht bei jeder Änderung laufen.

Pro Modell werden ausgewiesen: unabhängig korrektes Ergebnis, formaler Controllerzustand, Formatgültigkeit/Repair, tatsächliche Quellenlieferung, fachlicher Fortschritt, ungeklärte Evidence, unnötige Reads, Kontextablehnung, Nutzerhalte nach Ursache, Prozesse/Mutationen, echte Providerusage, konservative vollständige Requestschätzung, Laufzeit/Timeout und Abbruchdisposition. Providerusage und Bytekostenschätzung sind unterschiedliche Größen.

Jeder geplante Versuch bleibt im Bericht, auch Provider- und Infrastrukturfehler. Vorab nicht verfügbare optionale Tags werden separat als ausstehend geführt. Fehler während verpflichtender Runs werden typisiert ausgewiesen und dürfen die End-to-End-Verfügbarkeit nicht durch Entfernen aus dem Nenner verbessern. Fachlich richtige Ergebnisse eines stärkeren Modells kompensieren keine Fehler des kleineren.

Der neue Kern muss die absoluten Schwellen erreichen. Für jeden einzelnen Fall mit mindestens 2/3 richtigen Baselineläufen darf er nicht weniger richtige Runs erreichen; andere Fälle oder Modelle kompensieren keinen Rückschritt. Die durch F1 bis F5 reproduzierten Fehlerfälle müssen sich verbessern. Wenn ein alter Fehler im eingefrorenen Fall nicht auftritt, gelten stattdessen dessen gezielte Gegenprobe und die absolute Zielschwelle; F6 wird durch die unabhängige Messung selbst geschlossen. Bei identischem fachlichem Erfolg entscheidet nachgewiesener geringerer unnötiger Modell-/Werkzeugaufwand; keine behauptete Beschleunigung ohne Messung. Kleine Stichproben erlauben keine allgemeine statistische Zuverlässigkeitsgarantie.

## 9 Qualitätsgates und Dokumentationspflichten

Nach jeder vertikalen Änderung zuerst gezielte Regressionen und relevante Adapter-/Boundaryverträge. Für geänderten Rustcode danach mindestens:

```text
cargo fmt --all -- --check
cargo test --workspace --all-features --offline --locked --no-fail-fast
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
```

Bei Persistenzänderungen zusätzlich der unveränderte native `connection_lifecycle`-Vertrag und Herkunftsprüfung des bestehenden libSQL-Patches. Bei Frontend-/IPCänderungen `pnpm ci:frontend`, relevante Rust-/TypeScript-Negativverträge und native UI-Smokes. `pnpm check:links` und `git diff --check` gehören zum Abschluss. Es werden keine zusätzlichen Dependencies installiert, um einen fehlenden Nachweis zu verdecken.

Neue plattformspezifische Werkzeuge oder Prozess-/Dateipfade benötigen die Plattformnachweise aus den Qualitätsgates. Die frühere Windowsausnahme von Plan 21 wird nicht stillschweigend auf diesen neuen Auftrag übertragen. Ein nicht prüfbarer geschützter Plattformpfad bleibt ein ausgewiesener Blocker.

Öffentliche Modi, Prozess-/Edit-/Antwortschemas, persistente Felder, Stateübergänge, Autorisierungsfingerprints und Schema-/Neueinrichtungsgrenzen werden im selben Slice dokumentiert. Der finale Diff wird auf Secrets, generierte Nebenprodukte, unbeabsichtigte Fremdänderungen und neue privilegierte UI-Capabilities geprüft.

## 10 Gegenprüfung des Entwurfs

Die Architekturprüfung vor Umsetzung muss insbesondere die folgenden Fehlkonstruktionen ausschließen. Die Detailverträge enthalten dazu bereits Entscheidungen; die jeweilige Implementierung braucht weiterhin den genannten Beleg.

| Kritische Frage                                            | Entscheidung im Plan                                                                              | Noch erforderlicher Beleg                                                        |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Wird aus weniger Kontext heimlich weniger Nutzerziel?      | Originalauftrag und Kriterien vollständig dauerhaft, explizite Teilsicht und offene Restabdeckung | Langer Auftrag, Querverweis und globaler Constraint bleiben bis zum Ende erfüllt |
| Verwechselt der Core Zitate mit Wahrheit?                  | Provenienzprüfung getrennt von semantischer Aufgabenabnahme                                       | Q01 bis Q06 samt falscher Speicherinterpretation                                 |
| Wird 4k durch Ablehnen schöngezählt?                       | Positive 4k-Aufgaben müssen gelöst werden, Mindestreserve bleibt                                  | Tatsächliche Wirekosten und 15/18 richtige Runs                                  |
| Können grüne bestehende Checks ohne Umsetzung genügen?     | Kriteriengebundene Ergebnis-/Prüfevidence, AlreadySatisfied separat                               | Lint-grün/Verhalten-falsch und null Tests verhindern funktionales Done           |
| Entsteht dieselbe Komplexität unter neuen Namen?           | Keine neuen Crates/Dienste, nur konkrete Verbraucher, eine Aufgabenautorität und ein Controller   | Referenzprüfung und entfernter SourceGuided-Produktpfad                          |
| Wird Replan zum Budgetreset?                               | Echte geänderte Zerlegung, ein äußeres Budget, gleiche Fehler erkennen                            | Fehlerfortsetzung und Neustart erneuern keine Grenzen                            |
| Werden Sicherheit oder Testwünsche für Erfolg geopfert?    | ADR-0117/0118 erhalten, keine versteckten Paketinstallationen                                     | Beide Modi, expliziter Testverzicht, genaue Policy-/Adaptertests                 |
| Erzwingt Greenfield riesige Modellausgaben?                | Kleine Gerüste und exakte Folgeedits, integriertes Ergebnis bleibt offen                          | A04/A05 und unveränderte Schnittstellenentscheidungen                            |
| Starten alte Pläne oder Unknown-Wirkungen automatisch neu? | Frischer Datenbestand ohne Altübernahme, terminale Owner, Recovery neuer Runs vor Mutation                                       | Reopen/Crash/CAS; alte Planqueue und Freigaben werden nicht importiert                                              |
| Ist der Kandidat ein anderer Kern als das Produkt?         | Einmaliger Wiringwechsel und erneuter kombinierter Produktnachweis                                | P11 gegen ausgeliefertes Binary und tatsächliche IPC                             |

Ein logisch konsistenter Plan beweist noch keine Modellqualität. Wenn die erste reale Scheibe in P5 oder die volle Abnahme in P10 scheitert, bleibt die Produktion auf dem bisherigen Pfad. Geändert wird dann die nachgewiesene Engstelle; Aufgaben, Profile oder Erfolgskriterien werden nicht nachträglich passend gemacht.

## 11 Fertigstellung des Umbaus

Die zweite unabhängige Gegenprüfung des geschriebenen Entwurfs führte am 2026-11-06 zu konkreten Ergänzungen: dauerhafter Übergang über einen kleinen Planungshorizont hinaus; produktive CLI-/HTTP-/manuelle Evidence statt ausschließlich externer Orakel; vollständiger unveränderlicher Originalspeicher statt bestehender Textkürzung; fachliche Rückgabeoption auch aus Editphasen; BOM-Schutz am Dateianfang; strikt begrenzte Restabdeckung; langer Auftrag als echter 4k-Modellfall; verbindliche zweite Variantenserie; eindeutig isolierte Kandidatenmigration und definierte Vergleichsreihenfolge. Diese Punkte sind im Plan nachgezogen. Die technische Machbarkeit und Modellqualität bleiben an die jeweiligen Umsetzungs- und Livegates gebunden.

Der Umbau ist erst abgeschlossen, wenn alle P-Pakete objektiv verifiziert sind, die lokalen Übernahmegates bestehen, die drei Modi im ausgelieferten Desktop mit frisch initialisierten Daten nutzbar sind, Neustart und Recovery neuer Runs funktionieren und die alte produktive Steuerung entfernt ist. Eine Altdatenmigration ist gemäß ADR-0119 kein Abschlusskriterium. Der Abschlussbericht verlinkt die eingefrorenen Manifeste und Ergebnisse und nennt verbleibende Modell-/Aufgabengrenzen. Eine abgeschlossene Planerstellung ist davon getrennt.
