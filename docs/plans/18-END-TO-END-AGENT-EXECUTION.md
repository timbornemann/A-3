# Plan 18: Durchgängige Agentenausführung aus realer Recherche

Status: Complete · 2026-09-22

## Ziel

Der im Chat gestartete Agent setzt einen Auftrag aus dem vollständigen realen Recherche-Handoff
heraus um. In einem leeren Repository erreicht der exakte Hello-World-Auftrag mit dem
konfigurierten `gpt-5.6-luna` echte Dateien, lokale Verifikation und `Done`.

## Akzeptanzkriterien

- [x] Der produktive Recherche→Materialisierung→Executor-Pfad reproduziert den bisherigen Abbruch
      vor dem ersten Coding-Modellturn mit seiner typisierten Ursache.
- [x] Goal und Ledger bewahren Nutzerziel, alle Planergebnisse und vollständige
      Verifikationsabdeckung, ohne jeden Schritttext im Pflichtanker zu duplizieren.
- [x] Der reale Luna-Pfad erreicht Patch-Freigaben, wendet die erlaubten Änderungen an und führt
      den nachgelagert entdeckten lokalen Prüfcommand aus.
- [x] Der Run endet nur mit frischer Step- und Acceptance-Evidence in `Done`; unabhängige
      Dateisystem-, Test- und HTTP-Orakel bestehen.
- [x] Kontext-, Modell-, Tool- und Policyfehler erscheinen als konkrete geschlossene
      Haltegründe; insbesondere wird ein Kontextkapazitätsfehler nicht als allgemeines
      Runtime-Unavailable dargestellt.
- [x] Der Produktstandard verwendet die quellengelenkte, zweistufig validierte Aktionserzeugung
      und kann vorhandene Dateien aktualisieren sowie in einem leeren Index neue Dateien anlegen.
- [x] Fehlende, fehlgeschlagene oder hängende lokale Prüfungen führen nach frischem Reindex in
      einen begrenzten Reparaturpfad; mehrere manifestfreie Python-Testwurzeln bleiben prüfbar.
- [x] Gezielte Regressionen und alle vorgeschriebenen Quality Gates sind grün.

## Constraints und Non-Goals

- Keine Reduktion von Ausgabe- oder Sicherheitsreserven und keine profilspezifische
  Kontextvergrößerung.
- Keine freie Shell, Paketinstallation, externe Netzwerkverbindung oder ungeprüfte Dateipfade.
- Keine Erfolgsbehauptung allein aus Modelltext, Patch oder Diff.
- Keine Abschaffung der schrittweisen Ledger- und Approval-Grenzen.

## Vertikale Arbeitsschritte

- [x] Kombinierten opt-in Live-Test ergänzen und die echte Grenze vor dem Modellturn messen.
- [x] Redundante Schritt-zu-Goal-Kopie durch ein zielweites, evidencegebundenes Muss-Kriterium
      ersetzen.
- [x] Vor-Aktionsfehler typisiert bis zur Sessionmeldung projizieren und regressionsprüfen.
- [x] Produktstandard auf SourceGuided umstellen und ActionChoice, Argumentprojektion,
      Original-/Readgrenzen sowie aktuelle Patchanker regressionsprüfen.
- [x] Deferred-Command-Reparatur, feste Command-Timeouts, Prozessbaumbeendigung und
      manifestfreie Multi-Root-`unittest`-Erkennung schließen.
- [x] Kombinierten Lauf über Freigaben, Reindex, Deferred-Command-Bindung und Acceptance bis
      `Done` erweitern.
- [x] Engste Checks, vollständige Gates und finales Diff-/Sicherheitsaudit ausführen.

## Prüfweg

1. Unit- und Contract-Tests für Goal-/Step-Abbildung, Budgetzulassung und Fehlerprojektion.
2. Reales Luna-A/B: alter getrennter Coding-Smoke gegen vollständigen Recherche-Handoff.
3. Isolierter leerer Git-Worktree mit begrenzten Patchpfaden und direkten lokalen Python-argv.
4. Unabhängige Tests und HTTP-Orakel, die nicht in den Modellkontext gelangen.
5. Format, Workspace-Tests, Clippy sowie die unveränderten Frontend-Gates.
