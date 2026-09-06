# ADR-0064: Budgetierte Bestandsübergabe an Entwürfe

Status: Accepted\
Datum: 2026-09-06\
Freigabe: Fortgesetzter Nutzerauftrag zu Plan 10 einschließlich direkter Modelltestkorrekturen und zugehöriger ADRs.

## Befund

Der Qwen-8k-Mehrmodusnachtest auf ADR-0063 kennt in Q1 den konkreten Audit-Pfad,
behauptet aber in Q2, er sei nicht bestimmt. Der Core übergibt Bestandsinterpretationen
pauschal nur mit 384 Bytes. Die entscheidende Passage liegt hinter dieser Grenze;
optionale Originalfenster füllen den Rest nach Quellenreihenfolge und enthalten den
Audit-Körper in der Entwurfsphase nicht mehr. Dies belegt einen Informationsverlust
des Packings, nicht die alleinige Ursache aller inhaltlichen Modellfehler.

## Entscheidung

Diese Entscheidung ersetzt die pauschale Vorschaugrenze ausschließlich für aktuelle
Voraussetzungen einer Designfrage. Der unveränderte Auftrag, aktive Pflicht und alle
abhängigen Designentscheidungen behalten ihren vollständigen Vorrang nach ADR-0050.
Bestandsinterpretationen erhalten den danach tatsächlich verfügbaren Platz vor
optionalen Originalfenstern. Die vollständige Darstellung wird zuerst versucht;
bei Überlauf verwendet die bestehende partitionierte Ansicht eine deterministische,
begrenzte Verteilung über die Bestandsvoraussetzungen. Unvermeidbare Textauszüge
werden explizit markiert. Repositoryfragen behalten ihre begrenzten Vorschauen und
den Vorrang aktueller Originale.

Der persistierte Ergebnistext, seine epistemische Art, Quellen und Freshness ändern
sich nicht. Eine Interpretation bleibt eine quellengebundene Modellaussage, kein
verifizierter Fakt; ein Auszug beweist weder Vollständigkeit noch das Fehlen einer
Angabe im Original. Bindende Entwürfe dürfen durch zusätzliche Bestandsdetails nie
gekürzt werden. Echt unpassende Pflichtdaten bleiben ein ContextLimit. Es gibt keine
zusätzlichen Modellaufrufe, Reads, Repairs, Token, Rechte oder neue Zusammenfassung.

## Verifikation

Ein direkter Packing-Vertrag und ein echter Git-/Index-/Safe-Reader-/libSQL-Vertrag
reproduzieren den Verlust einer späten Unicode-haltigen Bestandsangabe zunächst rot.
Der Mehrmodustest verwendet das tatsächliche 8.192/2.048-Profil und verlangt dieselbe
Information in Q2 und Q3 bei drei Aufrufen, null zusätzlichen Reads und bytegleichen
Originalen. Zusätzliche Überlauffälle prüfen vollständige lange Designentscheidungen,
markierte UTF-8-sichere Auszüge, Determinismus und unveränderten dauerhaften Zustand.
Bestehende Provider-, ContextLimit-, Freshness- und Research-Gates bleiben verbindlich.
Liveantworten und Ablaufzähler werden weiterhin getrennt ausgewertet.
