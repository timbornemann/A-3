# ADR-0118: Appweite Agent-Berechtigungsmodi

Status: Accepted  
Datum: 2026-09-30  
Entscheider: Tim Bornemann

Annahme: Tim hat am 2026-09-30 ADR-0117 und ADR-0118 einschließlich Maschinen-
Recovery ausdrücklich angenommen und die Fortsetzung der Umsetzung beauftragt.
Die bewusste UI-Auswahl aktiviert den Modus ohne zusätzlichen Bestätigungsdialog.

## Entscheidung

Diese Entscheidung ersetzt die unveränderliche Freigabebaseline aus ADR-0012 und den
Ausschluss breiter Modi aus ADR-0022 sowie die entsprechende Prozessfreigabepflicht
aus ADR-0112 ausschließlich für nachstehend automatisch erlaubte Aktionen.

- Der privilegierte Core besitzt einen revisionierten, appweit gespeicherten Modus
  `AskPermissions` oder `FullMachine`. Neue und migrierte Installationen beginnen mit
  `AskPermissions`. Ein erfolgreicher expliziter UI-Wechsel gilt auch nach Neustarts.
- Die WebView darf über einen schmalen versionierten Settings-Command genau diesen
  Modus gegen die sichtbare Revision ändern. Diese bewusste Erweiterung der Settings-
  Trust-Boundary ist keine generische Betriebssystem-Capability. Modellaktionen,
  Repositorytexte und Slash Commands dürfen den Modus nicht ändern.
- Lesen und Ableiten im Projekt sind automatisch. Im Ask-Modus benötigen Änderungen,
  Prozesse und Outside-Root-Aktionen eine exakte Einmalfreigabe.
- Full machine erlaubt nichtdestruktive Dateiänderungen und vom Core eingeordnete
  Prozesse, Maschinenzugriffe, Netzwerk und Installationen automatisch. Unbekannte
  Skripte, unklare Wirkungen, Destruktion und Veröffentlichung benötigen weiterhin
  exakte Einzelabfragen. Kombinierte Aktionen behalten ihre strengste Teilwirkung.
- Automatische Aktionen erhalten echte aktionsgebundene Policyentscheidungen mit
  Einstellungsrevision. Automatische Freigaben werden nicht als ApprovalGrant simuliert.
- Jeder Werkzeugbeginn revalidiert den aktuellen Modus. Bereits begonnene Aktionen
  dürfen kontrolliert enden. Ein Wechsel zu Full machine darf einen wartenden sicheren
  Vorgang nach erneuter Validierung ohne zusätzlichen Fortsetzen-Klick starten.
- Ask und Plan bleiben read-only. Hash-, Freshness-, Secret-, Pfad-, Ressourcen-,
  Cancellation- und Mutation-Serialisierungsgrenzen bleiben bestehen.
- Maschinenzugriff verwendet ausschließlich die Rechte des angemeldeten Nutzers.
  Er ist keine Betriebssystem-Sandbox und keine Privilegienerhöhung.

## Maschinenwerkzeuge, Evidence und Recovery

ADR-0019 versöhnt unbekannte Wirkungen mit einem vollständigen Repositoryscan. Dieser
Scan beobachtet keine externen Dateien, Installationen oder Netzwerkzustände. Die
nachfolgende Erweiterung ersetzt diesen Teil von ADR-0019 ausschließlich für neue
maschinenbezogene Aktionen; bestehende Patch-/Prozess-Recovery bleibt erhalten.

- Neue Modellwerkzeuge bekommen einen geschlossenen, versionierten Rust-Vertrag.
  Der Core bindet jedes vorgeschlagene Werkzeug an den aktiven Run und Schritt,
  prüft Parameter und kanonische Ressourcen und bestimmt die Wirkung. Ein Modell
  darf weder seine eigene Risikoklasse noch eine Policyentscheidung angeben.
- Zusätzlich angebotene Programme und Parameter werden durch geschlossene Core-
  Regeln eingeordnet. Katalogisierte bestätigte Prüfkommandos bleiben verwendbar.
  Nicht abgedeckte Programme, Skripte, Parameter oder kombinierte Wirkungen behalten
  die exakte Einzelabfrage. Destruktion und Veröffentlichung haben Vorrang.
- Automatisch erlaubtes Netzwerk beschränkt sich auf eingeordnete, begrenzte
  Leseoperationen mit exaktem Ziel und ohne Secrets oder verdeckte Redirects.
  Installation darf nur über eine eingeordnete Core-Regel automatisch erfolgen;
  mögliche nicht eingeordnete Installationsskripte benötigen Einzelabfrage.
- Vor jeder neuen mutierenden Werkzeuggrenze werden die exakte Aktionsart, der
  Fingerprint und eine begrenzte Ressourcenbeschreibung dauerhaft als `Unknown`
  gespeichert. Quelltext, Patchinhalte, Secrets und rohe Prozessausgaben werden
  dafür nicht persistiert. Ein Storefehler verhindert den Werkzeugbeginn.
- Reale Dateiquittungen enthalten kanonische Ressourcenidentität und beobachteten
  Hash beziehungsweise belegte Abwesenheit. Prozess-/Netzwerkquittungen belegen
  ausschließlich tatsächlich beobachtete Ergebnisse; sie beweisen keine beliebigen
  Nebenwirkungen. Externe Evidence darf keine Repositoryrevision imitieren.
- Nach unklarer Wirkung gibt es weder automatisches Retry noch Zurücksetzen. Die
  bisherigen Worktree-Sperren bleiben bestehen. Ein Repositoryscan allein darf
  eine Maschinenaktion nicht versöhnen. Bekannte externe Ressourcen benötigen
  zusätzliche aktuelle, autorisierte Beobachtungen. Diese Wiederlesevorgänge
  unterliegen dem aktuellen Berechtigungsmodus.
- Nicht vollständig beobachtbare Wirkungen bleiben ausdrücklich unbekannt. Ein
  Fortsetzen benötigt eine sichtbare, exakte Recovery-Entscheidung des Nutzers mit
  den verbleibenden Beobachtungslücken und einen Replan. Die Entscheidung bestätigt
  keinen Erfolg der ursprünglichen Aktion. Cancel bleibt erreichbar.
- Dateiwerkzeuge und zusätzliche Prozesse teilen den einzigen Worktree-Mutations-
  Lease. Prozessbaumbeendigung, Timeouts, Cancellation, begrenzte Ausgaben und
  Secret-Prüfungen bleiben auch bei Full machine verbindlich.

## Stand der Umsetzung zur Annahme

Der prüfbare Arbeitsstand enthält den Settings-/UI-/Policy-Pfad und echte Patch-/
Prüfprozess-Regressionsfixtures. Ein begrenzter externer Dateiadapter ist separat
geprüft. Er ist noch nicht als Modellwerkzeug im Produktionsharness verfügbar.
Die erweiterten Prozess-/Netzwerkverträge und Maschinen-Recovery bleiben offen;
der Modusschalter darf diese fehlenden Fähigkeiten nicht als vorhanden ausgeben.

## Nachweise

Policy-, Storage-, IPC-, Adapter-, Executor- und UI-Verträge prüfen beide Modi,
unbekannte Prozesse, kombinierte Risiken, Moduswechsel, Neustart und stale Autorisierung.
Die übrigen Einmalfreigabe- und Recovery-Verträge bleiben verbindlich.

## Referenzen

- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0022](0022-task-bound-approval-center.md)
- [ADR-0019](0019-durable-mutation-reconciliation.md)
- [ADR-0112](0112-nachgelagerte-greenfield-verifikation.md)
- [Plan 21](../plans/21-TASK-SCOPE-AND-PERMISSIONS.md)
