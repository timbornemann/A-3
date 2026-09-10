# ADR-0106: Gezielte Wiederherstellung ungültiger Modellprofile

Status: Accepted\
Datum: 2026-09-10\
Freigabe: ausdrücklicher Auftrag zur Fehlerkorrektur und nutzerbedienbaren Wiederherstellung;
vorliegende Zustimmung zu den dazugehörigen ADRs.

## Entscheidung

Der normale Settings-Read bleibt strikt. V8 validiert jede Rollenbindung gegen
ihren eigenen vollständigen Provider-Slot gemäß ADR-0066; Legacy-Daten behalten
ihre Einzelprovider-Regeln. Eine gültige gemischte Konfiguration benötigt keine
Migration, Neuverifikation oder Rücksetzung.

Ein separater lokaler Diagnosepfad darf höchstens die drei ungültigen Rollen
Coding, Mapping und Embedding sowie die aktuelle Settingsrevision anzeigen.
Er validiert zuerst sämtliche aktuellen Provider-, Credential- und Legacyanker.
Physische Korruption, unvollständige Providergruppen, SQL-/Lesefehler und neuere
Schemas werden nicht durch Überspringen von Daten als reparierbar ausgegeben.

Erst ein expliziter zweistufiger Nutzerklick autorisiert einen engen Command:
erneute Diagnose innerhalb einer kurzen Schreibtransaktion, exakter Revisions-CAS,
Entfernen ausschließlich der als ungültig erkannten Rollen aus einem neuen
append-only Snapshot. Gültige Rollen, deren Probezeiten und Fähigkeiten, Provider,
Credentialgenerationen und Privacy bleiben gleich. Der aktuelle Originalsnapshot
bleibt historisch erhalten. Keine ältere Konfiguration oder Credentialgeneration
wird als Fallback aktiviert. Gesunde Settings erzeugen keinen Recovery-Write.

Die WebView liefert nur Version und zuvor sichtbare Revision, weder Rolle,
Profil, Pfad, SQL, Providerstatus noch Capabilitybehauptung. Dieselbe native
Operationssperre wie bei Provideränderungen verhindert konkurrierende Modellprobes.
Recovery liest oder verändert keine Schlüssel und startet keine Provideranfrage.
Entfernte Rollen müssen anschließend regulär ausgewählt und geprüft werden.

## Grenzen und Nachweise

Die UI erklärt betroffene Rollen, Erhalt von Projektwissen/Schlüsseln und den
Bedarf einer erneuten Rollenprüfung vor dem bestätigenden Klick. Ablehnung und
erneute Diagnose bleiben erreichbar. Nicht reparierbare Katalogfehler brauchen
eine unterstützte Wiederherstellung; ein pauschaler Datenbankreset ist nicht Teil
dieses Commands. Echte Storage-, CAS-/Rollback-, Mehrprovider-/Reopen-, IPC- und
Frontendtests sind verbindlich. Normale Modell- und Werkzeuggrenzen bleiben zu.
