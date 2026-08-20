# ADR-0028: OS-gespeicherte Provider-Credentials und expliziter Gemini-Remotezugriff

Status: Accepted

Datum: 2026-08-20

Entscheider: Tim Bornemann

Supersedes: Credential-, Endpoint- und WebView-Eingabeteile von ADR-0027; ergänzt ADR-0005,
ADR-0012, ADR-0018, ADR-0023 und ADR-0026

## Kontext

ADR-0027 lädt Gemini-API-Schlüssel ausschließlich aus Prozess-Umgebungsvariablen. Damit kann ein
normal gestarteter Desktopprozess die Verbindung nicht zuverlässig konfigurieren. Außerdem ließ
die erste Adapterfassung neben dem kanonischen Google-Origin beliebige HTTPS-Origins zu und band
die Remote-Ausnahme in Application-Code an den String `gemini`. Das gefährdet die enge
Credential- und Netzwerkgrenze und lässt sich nicht providerneutral persistieren.

Google authentifiziert die Gemini Developer API über `x-goog-api-key`. Ein Schlüssel ist ein
Secret und darf weder in libSQL, Logs, Fehlern, Frontendzustand noch Providerprofilen landen. Die
WebView ist weiterhin unprivilegiert; für eine brauchbare Desktopkonfiguration muss sie jedoch den
vom Nutzer gerade eingegebenen Schlüssel genau einmal an einen schmalen Core-Befehl übergeben
können.

## Entscheidung

- Provider-Credentials liegen ausschließlich im nativen OS-Schlüsselspeicher. Der
  Application-Layer besitzt dafür den Port `ProviderCredentialStore`; ein nativer Adapter nutzt
  Windows Credential Manager, macOS Keychain oder Linux Secret Service. Es gibt keinen Datei-,
  libSQL-, Environment- oder Synchronisations-Fallback.
- libSQL speichert nur die Credential-Anforderung, eine content-freie Lebenszyklusphase und eine
  monotone Generation. Der Keyring-Eintrag enthält dieselbe Generation zusammen mit dem Secret.
  Providerzugriff ist nur bei einem konsistenten `Configured`-Paar zulässig.
- Setzen und Löschen verwenden kurze append-only Settings-Übergänge vor und nach der externen
  Keyring-Wirkung. Ein Abbruch zwischen beiden Stores bleibt als `RecoveryRequired` sichtbar und
  sperrt Providerzugriffe, bis der Nutzer den Schlüssel ersetzt oder löscht.
- Der Core gibt Credentials niemals an die WebView zurück. Ein dediziertes unkontrolliertes
  Passwortfeld darf eine neue Nutzereingabe nur bis zum schmalen Set-Command halten. Frontend und
  Core löschen ihre verwalteten temporären Bytepuffer bestmöglich; Responses, `Debug`, Fehler und
  Telemetrie enthalten ausschließlich content-freien Status.
- Gemini akzeptiert in Produktion ausschließlich den exakten Origin
  `https://generativelanguage.googleapis.com`. Der API-Key wird erst nach dieser erneuten
  Adapterprüfung als `x-goog-api-key` angefügt. Redirects und Umgebungsproxies bleiben aus.
  Loopback ist ausschließlich über eine explizite test-only Policy ohne produktiven Keyring-Key
  zulässig.
- Das Speichern oder Lesen von Settings erzeugt keinen Netzwerkzugriff. Modell-Discovery und
  Capability-Probe sind jeweils bewusst gestartete, revisionsgebundene Aufrufe auf den
  kanonischen Origin. Diese Einstellung ist keine allgemeine oder wiederverwendbare
  Netzwerkfreigabe für Agentenläufe.
- Beim Wechsel weg von Gemini oder beim Entfernen der Verbindung wird zuerst der Credential-
  Eintrag gelöscht. Ein Keyring-Fehler blockiert die Provideränderung sichtbar und wiederholbar.

## Konsequenzen

### Positiv

- Gemini lässt sich aus der Desktopoberfläche konfigurieren, ohne Secrets dauerhaft in WebView
  oder Datenbank abzulegen.
- Ein kompromittiertes Frontend kann weder vorhandene Schlüssel lesen noch einen gespeicherten
  Schlüssel an einen frei wählbaren Host umleiten.
- Crash- und Cross-Store-Fehler sind erkennbar und fail-closed statt als scheinbar konfigurierte
  Verbindung weiterzulaufen.

### Negativ

- Das Betriebssystem-Credential-Backend ist eine zusätzliche plattformspezifische Abhängigkeit.
- Während der manuellen Eingabe existiert der neue Schlüssel unvermeidbar kurzzeitig im DOM und
  im serialisierten IPC-Speicher. Diese Kopien können nicht kryptografisch garantiert überschrieben
  werden; A^3 minimiert Lebensdauer und behält sie nie in reaktivem Zustand.
- Eine gesperrte oder fehlende Keychain macht Gemini sicher nicht verfügbar, auch wenn die
  content-freien Settings eine frühere Konfiguration dokumentieren.

## Verworfene Alternativen

- API-Key in `catalog.db` speichern oder mit einem app-eigenen Schlüssel verschlüsseln — verlagert
  das Schlüsselverwaltungsproblem nur und widerspricht ADR-0005.
- Environment-Variablen als Fallback behalten — erzeugt eine zweite, nicht revisionierte
  Autorität und kann Profile ohne sichtbare Settingsänderung einem anderen Google-Projekt zuordnen.
- Beliebige HTTPS-Gateways erlauben — würde dem Frontend mittelbar die Zielwahl für ein Secret
  geben.
- Einen generischen Keyring- oder Netzwerkbefehl an die WebView geben — umgeht den
  use-case-orientierten IPC-Vertrag.

## Compliance

- Domain-, Store- und IPC-Verträge prüfen Eingabegrenzen, Redaction, Generation-Mismatch,
  Zwischenzustände, CAS-Konflikte und die Abwesenheit des Testsecrets in libSQL und Responses.
- Adapterverträge beweisen, dass Auth-Header ausschließlich am kanonischen Google-Origin
  entstehen und beliebige HTTPS- sowie produktive Loopback-Ziele vor einem Request abgelehnt
  werden.
- Component-Tests prüfen das nicht vorausgefüllte Passwortfeld, sofortiges Leeren, Status-Gating,
  Ersetzen, bestätigtes Löschen und blockierten Providerwechsel bei Keyring-Fehlern.
- Plattform-Smokes schreiben, lesen und löschen einen zufälligen isolierten Testeintrag auf
  Windows, Linux und macOS. Live-Gemini-Smokes bleiben opt-in und schreiben keine Secrets in
  Artefakte.

## Referenzen

- [ADR-0005](0005-worktree-scoped-storage.md)
- [ADR-0012](0012-safe-tools-and-approval-policy.md)
- [ADR-0018](0018-model-provider-port-ownership.md)
- [ADR-0023](0023-local-settings-and-model-activation.md)
- [ADR-0026](0026-explicit-local-provider-model-discovery.md)
- [ADR-0027](0027-google-gemini-model-provider.md)
- [Gemini API keys](https://ai.google.dev/gemini-api/docs/api-key)
- [Gemini GenerateContent](https://ai.google.dev/api/generate-content)

