# ADR-0119 Kleinschrittiger Agentenkern für lokale Modelle

Status: Accepted  
Datum: 2026-11-06  
Ergänzt mit ausdrücklicher Maintainerfreigabe: 2026-11-07  
Entscheider: Tim Bornemann

Annahme: Tim Bornemann hat ADR-0119 am 2026-11-06 ausdrücklich angenommen und die schrittweise Umsetzung von [Plan 22](../plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md), beginnend mit P0 und lokalen Ollama-Modellen, beauftragt. Der bestehende Produktionspfad bleibt bis zur geprüften Umstellung in P11 aktiv.

## Kontext

A^3 besitzt einen abgesicherten Index-, Werkzeug-, Policy-, Persistenz- und Recovery-Unterbau. Der modellnahe Ablauf verlangt jedoch vollständige Ersatzdateien bei teilweise geliefertem Code, mehrere vorgeschaltete Aktionsentscheidungen, einen starren Vier-Read-Übergang und teilweise nur allgemeine Abschlussproxies. Vorhandene Integrationstests prüfen diese Mechanismen stärker als selbstständige richtige Aufgabenlösungen kleiner lokaler Modelle.

Das Produkt benötigt drei verständliche Arbeitsweisen: allgemeines Chatten, projektbezogene Fragen und ausführende Agentenarbeit. Planung und fachliche Entscheidungen bleiben Modellaufgaben, werden aber in einzeln bearbeitbare Aufträge zerlegt. Sicherheit, Zustandsführung, Quellenlieferung, Dateierhaltung und die Entscheidung über einen verifizierten Abschluss bleiben Core-Aufgaben.

## Entscheidung

1. Öffentliche Modi sind `Chat`, `Ask` und `Agent`. Chat funktioniert ohne Projekt und ohne Projektwerkzeuge. Ask liest ausschließlich das gebundene geöffnete Projekt. Agent arbeitet über genau den vorhandenen endlichen Controller, den Scheduler und die zentrale Werkzeuggrenze. Planung ist ein sichtbarer interner Arbeitsschritt des Agenten. Alte Entwicklungs-Plan-Sessions müssen im neuen Kern nicht lesbar bleiben und werden niemals automatisch ausführbar.
2. Ein Modellaufruf bearbeitet genau einen vom Core bestimmten Phasenauftrag: Anforderungen klären, nächsten kleinen Umsetzungsschritt planen, eine konkrete Quellenlücke benennen, einen begrenzten Editvorschlag erzeugen oder eine Antwort aus gelieferten Quellen formulieren. Kein vollständiger globaler AgentAction-Vertrag und keine obligatorische Folge aus Arbeitswahl, Aktionswahl und Argumentwahl für dieselbe bereits feststehende Arbeit.
3. Goal Contract, Acceptance Criteria, Task Ledger, unveränderliche Versuche und Journal bleiben die einzige dauerhafte Aufgabenautorität. Ein vollständiger Originalauftrag bleibt unveränderlich mit geordneten Bereichsreferenzen im Goal-Store erhalten; eine ausdrückliche Größenobergrenze ersetzt stille Kürzung. Jeder aktive Modellturn erhält eine deterministische, revisionsgebundene Sicht auf das aktuelle Teilziel, seine Kriterien, geltende Constraints, offenen relevanten Fehler und eine größenbegrenzte Projektion der globalen Abdeckung. Ausgelassene Auftragsteile bleiben explizit offen; eine Modellsicht darf sie weder löschen noch erfüllen. Sind nur die aktuellen Schritte abgeschlossen, führen offene globale Kriterien im selben Controller per dauerhafter CAS zu weiterer Planung und nicht zu Acceptance.
4. Pflichtregeln, deren Geltung für den aktuellen Schritt nicht sicher ausgeschlossen werden kann, bleiben im aktiven Auftrag vollständig. Passt dessen Pflichtinhalt nicht, wird der Schritt kleiner zerlegt oder mit einem konkreten Kapazitätsgrund angehalten. Weder stille Trunkierung noch ein größer eingestelltes Kontextfenster lösen den Konflikt. Die Ausgabereserve bleibt mindestens 22 Prozent beziehungsweise das höhere konfigurierte Outputlimit; sämtliche tatsächlich gesendeten Schema-, System- und Protokollkosten zählen mit.
5. Quellenfortschritt wird anhand neuer aktueller Bereiche und bearbeiteter Anforderungen geführt. Ein Readlimit begründet keine ausreichende Informationslage und erzwingt keine Änderung. Auf fehlende Belege folgen begrenzte gezielte Lokalisierung, echte Zerlegung oder ein belegter Erkenntnis-/Kapazitätshalt.
6. Für vorhandene Dateien erzeugt das Modell eine Ersatzfolge für ein tatsächlich geliefertes, exakt gebundenes kleines Editfenster oder meldet typisiert fehlende Evidence beziehungsweise zu großen Scope. Der Core bindet Identitäten, Pfad, Snapshot, vollständigen Dateihash und Ausgangsbytes, erhält BOM/Encoding sowie alle Bytes außerhalb des Fensters und erzeugt daraus den bestehenden vollständigen `PatchUpdate`. Vorschau, exakter Fingerprint, Policy, Lease, Live-Revalidierung, atomare Veröffentlichung und Recovery bleiben unverändert. Keine unscharfe Suche, Regexmutation oder automatische Konfliktübernahme.
7. Ein Implementierungsabschluss verlangt zum Auftrag passende aktuelle Ergebnisnachweise. Dafür existieren produktive geschlossene Verifikationsrezepte und typisierte Evidence-Erzeuger einschließlich Policy, Cancellation, Cleanup und Recovery; externe Eval-Orakel sind kein Ersatz. Ein grüner beliebiger Command, ein nicht leerer Diff oder Modelltext allein erfüllt keine fachliche Laufzeitforderung. Die Regel zur Testerstellung aus ADR-0117 bleibt vollständig erhalten. Nicht prüfbare Wirkungen bleiben offen oder benötigen eine ausdrücklich begrenzte revisionsgebundene Nutzerabnahme; sie werden nicht als automatisch verifiziert dargestellt.
8. Nach einem fachlichen Fehlschlag kann der nächste begrenzte Replan kleinere oder andere Umsetzungsschritte und veränderte Evidenzpflichten vorschlagen. Der Core prüft Zielerhaltung, Abdeckung, echte Änderung gegenüber dem gescheiterten Plan und endliche Budgets. Neue IDs allein sind kein Replanfortschritt. Pro ungültigem Ausgabedokument bleibt genau ein Strukturrepair erlaubt; automatische Neustarts dürfen diese Grenze nicht umgehen.
9. Allgemeine Chats erhalten einen eigenen lokalen Gesprächsspeicher unter dem geprüften AppData-Root, mit bestehendem libSQL-Adapter, getrennt von Katalogmetadaten und projektlokalen Knowledge-Datenbanken. Keine erfundene Projektidentität, kein zusätzlicher Dienst und keine automatische Übernahme von Chattext als Projektwissen.
10. Der neue Ablauf wird zunächst ausschließlich in Integrations- und Modellvergleichen komponiert. Der Produktionswechsel ersetzt den bisherigen Pfad in einem zusammenhängenden Slice; keine zweite produktive Engine und kein Feature Flag für einen unvollständigen Kern. P11 startet mit frisch initialisierten A^3-Daten; bestehende Entwicklungsaufgaben und Sessions werden nicht übernommen. Recovery neuer Runs bleibt erforderlich.
11. Lokale Modelle sind die verbindliche Zielabnahme. Ein festes 4k-Aufgabenpaket muss richtige Lösungen erreichen; ein Kapazitätshalt zählt dort als Misserfolg. Formatfähigkeit, Quellenlieferung, Controller-Done und unabhängige Aufgabenkorrektheit werden getrennt gemessen.

### Ergänzung vom 2026-11-07: Entwicklungsneustart ohne Altdatenmigration

Tim hat ausdrücklich festgelegt, dass A^3 ausschließlich in Entwicklung ist und
bestehende Daten nicht übernommen werden müssen. Er hat die direkte Änderung
dieser angenommenen ADR freigegeben. Die ursprüngliche Annahme bleibt dokumentiert;
für Altbestandsübernahme und Kompatibilität gilt ab jetzt die folgende Entscheidung.

1. Plan 22 startet den neuen Kern mit einem frisch initialisierten A^3-Datenbestand;
   alte Entwicklungs-Chats, Tasks, Ledger, Journale, Queues, Freigaben, Indizes und
   Einstellungen müssen weder konvertiert noch im neuen Produkt lesbar gehalten werden.
   Es gibt keine zugesagte automatische Übernahme alter Providerkonfigurationen.
2. P9 integriert Bedienung, IPC und die sichere Neueinrichtung; P11 schaltet nach
   bestandener P10-Abnahme einmalig auf diesen geprüften Kern und Datenbestand um.
   Ein neuer Datenbestand kann über einen getrennten geprüften AppData-Speicherbereich
   eingerichtet werden; alte Dateien müssen dazu nicht automatisch gelöscht werden.
   Vor der Umschaltung müssen die bisherigen Jobs und ihre Werkzeuge terminal beendet sein.
3. Es entstehen keine neuen Legacy-Importer, Altdaten-Decoder oder Adapter allein
   für diese Übernahme. Solche ausschließlich dafür benötigten Pfade dürfen nach
   Referenzprüfung entfallen. Das bedeutet nicht, dass bestehende Laufzeit- oder
   Storagepfade bereits mit dieser Dokumentationsänderung entfernt werden.
4. Neuinitialisierung, eindeutige Schema-/Datenbestandsversion, Identitätsprüfung,
   atomare Writes, CAS, Reopen, Crash-Recovery, erhaltene Budgets und Unknown-Reconciliation
   des neuen Kerns bleiben verbindlich. Unbekannte beziehungsweise inkompatible alte
   Daten werden abgewiesen und weder automatisch konvertiert noch überschrieben.
5. Interne SQL-Schemaaufbauschritte dürfen für die Einrichtung einer leeren Datenbank
   wiederverwendet werden; deren Bezeichnung als Migration erzeugt keine Pflicht,
   alte Entwicklungsstände aufzurüsten. Eine Upgrade-Matrix aller alten Versionen,
   Legacy-Lesbarkeit und ein Backup-/Restorepfad für deren Konvertierung sind keine
   neuen Abnahmekriterien von Plan 22.
6. P0-Baseline, eingefrorene Eingaben, Berichte und historisch akzeptierte ADRs bleiben
   unverändert reproduzierbar. P2 bis P10 bleiben isolierte Kandidaten-Testkomposition;
   es entsteht kein zweiter produktiver Executor und keine vorzeitige Umschaltung.
7. Diese Ergänzung erlaubt keine Löschung oder Änderung von Nutzerprojekten,
   Arbeitsverzeichnissen, Secrets oder beliebigen AppData-Dateien. Eine konkrete
   Datenbereinigung ist ein eigener abgegrenzter Vorgang, kein Nebenprodukt des Umbaus.


## Präzise Ablösung bisheriger Entscheidungen

Die folgenden Ablösungen gelten für neue Verträge. Andere historische ADRs und P0-Artefakte bleiben unverändert; die direkte Ergänzung dieser ADR ist ausdrücklich freigegeben. Die eingefrorene bisherige Implementierung bleibt die Altbaseline.

| ADR                                                                                                                                                                   | Zu ersetzender Teil                                                                                                                                                  | Erhaltene Entscheidung                                                                                                                      |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| [0005](0005-worktree-scoped-storage.md)                                                                                                                               | Ausschließlich katalog- und worktreegebundener Speicherumfang                                                                                                        | Geprüftes AppData, Trennung von Quellenableitung und dauerhaften Daten, Worktreeisolation                                                   |
| [0010](0010-single-controller-state-machine.md)                                                                                                                       | Ergänzung des Übergangs Verify nach Plan bei offenem globalem Auftrag und ausgeschöpftem aktuellem Planungshorizont                                                  | Dieselben groben Zustände, genau ein Controller, endliche Budgets, keine zweite Mutation                                                    |
| [0009](0009-context-compiler.md), [0077](0077-vollstaendige-agent-anker-im-konfigurierten-kontext.md), [0080](0080-pflichtmemory-vor-optionaler-kontextverteilung.md) | Volltextdarstellung sämtlicher Goal-/Ledger-/Fehlermemory im aktiven Modellpack statt verlustfreier dauerhafter Haltung plus expliziter Teilsicht                    | Determinismus, Freshness, vollständige aktive Constraints, keine stille Kürzung, mindestens 22 Prozent Outputreserve                        |
| [0033](0033-chatbasierter-agent-workspace.md), [0041](0041-sichere-moduswechsel-und-dauerhafte-nachrichtenwarteschlange.md)                                           | Modustripel Ask/Plan/Agent und ausschließliche Projektbindung jeder Unterhaltung                                                                                     | Capability-Envelopes, nächstes unabhängiges Work Item, FIFO, Owner, keine Ausführungsautorität der UI                                       |
| [0042](0042-adaptiver-agent-arbeitsplan.md)                                                                                                                           | Markdownabschnitte als regulärer Planeingang, feste Verkettung und Kopieren offener Schritte beim Replan                                                             | Begrenzte verifizierbare Schritte, Core-IDs, append-only Revisionen, endliche Replans                                                       |
| [0047](0047-verbindlicher-recherchearbeitsstand.md), [0049](0049-core-planpflichten-und-statusnotizen.md), [0109](0109-autonomer-agent-einstieg.md)                   | Verpflichtender Bestands-/Entwurfs-/Testentwurfs-Handoff als Voraussetzung jedes Agentstarts                                                                         | Stabile Anforderungen, Quellenaktualität, Trennung von Fakt und Interpretation, kein Modellabschlussrecht, autonomer Einstieg im Agentmodus |
| [0093](0093-originalquellen-im-agentenkontext.md), [0096](0096-originalbudget-vor-optionalen-metadaten.md), [0097](0097-grosszuegigere-optionale-codeversorgung.md)   | Feste Seiten-/Zeilen- und Bereichsverteilung für den normalen Agentturn, soweit durch auftragsgebundene Fenster ersetzt                                              | Sichere aktuelle Originalbytes, begrenzte Ressourcen, Quellen vor optionaler Historie                                                       |
| [0115](0115-ausfuehrbarer-plananker-im-modellkontext.md)                                                                                                              | SourceGuided als Produktstandard, vorgeschaltete Auswahlstufen, Vier-Read-Zwang, generisches Zielkriterium nur am letzten Schritt und obligatorisches Handoffpacking | Echte kombinierte E2E-Abnahme, aktuelle Originale, konkrete Kapazitätsfehler, fehlertolerante Verifikationsbindung                          |
| [0012](0012-safe-tools-and-approval-policy.md)                                                                                                                        | Erweiterung der modellseitigen Patchbeschreibung um gebundene Editfenster                                                                                            | Bestehende vollständige Patchwirkung, Hash-/Pfadschutz, argv, Policy und Freigaben einschließlich ADR-0118                                  |

Historische Experimente und Reparaturformen, deren alleiniger Verbraucher entfällt, werden im Code entfernt. Ihre weiterhin gültigen Sicherheits- und Fehlerregressionen werden auf neue Eingänge übertragen. Diese Tabelle ist keine pauschale Aufhebung aller Folge-ADRs.

Unverändert gelten insbesondere modularer Monolith, libSQL hinter Adapter, deterministischer Index, epistemische Evidenz, ein endlicher mutierender Controller, Providergrenzen, Goal/Ledger/Journal, Unknown-Recovery, Schedulerbesitz sowie [ADR-0117](0117-auftragsbezogene-tests-und-verifikation.md) und [ADR-0118](0118-appweite-agent-berechtigungsmodi.md). Deren Berechtigungsregeln werden nicht erweitert.

## Konsequenzen

Der Umbau verändert Orchestrierung, Modellverträge, Anforderungszuordnung, Gesprächsscope und das Schema frisch initialisierter Daten. Eine Migration alter Entwicklungsdaten, Legacy-Lesbarkeit und dafür benötigte Kompatibilitätsschichten entfallen. Sicherheitsadapter sowie unveränderliche Historie und Recovery neuer Runs bleiben erhalten. Globale Chats verursachen einen zusätzlichen lokalen Speicherlebenszyklus. Teilsichten benötigen eigene Abdeckungs- und Constrainttests, damit kleine Prompts keinen Zielverlust verstecken.

Beliebige semantische Richtigkeit kann der Core nicht aus Quellenlabels beweisen. Quellenbindung verhindert erfundene Provenienz, unabhängige Aufgabenprüfungen messen richtige Interpretation und Umsetzung. Ein Modell unterhalb der gemessenen Mindestfähigkeit erhält keine unbelegte Zuverlässigkeitsempfehlung.

## Alternativen

- Vollständiger Neustart: verwirft abgesicherte Infrastruktur, ohne die modellnahen Fehlentscheidungen zu beseitigen.
- Fortgesetzte Einzelkorrekturen an SourceGuided: erhält den vollständigen Dateiausgabevertrag und die bisherigen Abschlussproxies.
- Größere Prompts, Modelle oder unbeschränkte Retries: verlagert Harnessverantwortung und verschiebt die Zielabnahme.
- Zweite produktive Engine mit automatischem Modellrouting: verdoppelt Zustands-, Sicherheits- und Recoverypfade.

## Compliance

P9 prüft die Neueinrichtung in isoliertem AppData, Version-/Identitätsgrenzen, CAS,
Reopen und Recovery des neuen Datenbestands. P11 prüft terminalen Shutdown, frischen
Produktstart und den kombinierten Modell-/IPC-/nativen Kernsmoke. Alte Entwicklungsdaten
werden weder automatisch übernommen noch durch einen Reset überschrieben. Es ist
keine Upgrade-Matrix alter Entwicklungsstände für den neuen Kern erforderlich.


Es gelten die Arbeitspakete, unabhängigen Orakel, lokalen Modellprofile und Umschaltbedingungen aus [Plan 22](../plans/22-SMALL-LOCAL-MODEL-AGENT-CORE.md) und die Verträge aus [Kleiner Agentenkern](../SMALL_LOCAL_MODEL_AGENT_CORE.md). Produktcode, aktuelle Architekturregeln und AGENTS.md werden erst nach ausdrücklicher ADR-Annahme entsprechend angepasst. Kein Implementierungspunkt dieses Plans gilt durch die Erstellung dieses ADRs als abgeschlossen.
