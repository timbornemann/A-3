# ADR-0066: Mehrere Provider und gemeinsame Modellauswahl

Status: Accepted\
Datum: 2026-09-06\
Ersetzt die Einzelprovider- und Festorigin-Regeln aus ADR-0026, ADR-0028 und ADR-0032;\
die historischen ADRs bleiben unverändert.

## Entscheidung

A^3 verwaltet genau drei kanonische Provider-Slots: Ollama, Google Gemini und OpenAI.
Jeder Slot besitzt eigene URL-, Credential-, Konfigurationsrevisions-, Health- und
Aktivierungsdaten. Alle Slots sind im initialen Settings-Snapshot vorhanden und zunächst
deaktiviert. Aktivierung ist erst nach expliziter, erfolgreicher Katalogabfrage möglich.

Kataloge sind flüchtig, werden nur auf ausdrückliche Nutzeraktion geladen und sind an die
jeweilige Provider-Konfigurationsrevision gebunden. Coding, Deep Map und Embeddings wählen
je genau ein Modell als Tupel aus Provider-ID und Modell-ID. Deaktivierung entfernt nur die
Rollenprofile dieses Providers; URL, Credential und der letzte Verbindungsnachweis bleiben
erhalten. Ein späterer fehlgeschlagener Retest setzt den Health-Status auf `Unreachable`,
ohne Aktivierung oder bestehende Rollenprofile zu löschen. Nur URL- oder Credentialänderungen
invalidieren Verbindungsnachweis, Katalog und Rollenprofile.

Gemini- und OpenAI-Ziele sind pathlose HTTPS-Origins ohne Query, Fragment oder Userinfo.
Nicht-offizielle Origins werden nach Adaptervalidierung in einem nativen, nicht von der
WebView kontrollierten Bestätigungsdialog mit Provider und exakter Origin bestätigt. Die
gespeicherte Origin wird bei jeder Anfrage exakt geprüft; Redirects und Umgebungsproxys
bleiben deaktiviert. Credentialwerte verbleiben ausschließlich im nativen Keyring und sind
an einen Origin-Fingerprint gebunden.

## Persistenz und Schnittstelle

`SettingsV2` liefert exakt die drei kanonisch sortierten Providerzeilen. V2-Commands tragen
die geschlossene Providerart und dürfen keine vom WebView vorgegebenen Endpoint-, Capability-
oder Zeitwerte für Probe/Discovery akzeptieren. Eine erfolgreiche Discovery ist zugleich der
content-freie Verbindungsnachweis. Die globale abbrechbare Modelloperations-Sperre bleibt
serialisiert erhalten.

Catalog-Schema V8 ergänzt append-only Providerzeilen je Settingsrevision. V7-Snapshots bleiben
lesbar und werden beim nächsten Write verlustarm in die Mehrproviderform überführt. Settings-
Reads und Appstart führen keinerlei Providerzugriff aus; die laufgebundene Netzwerkfreigabe
für Agent- und Deep-Map-Anfragen bleibt separat.

## Folgen

Die Oberfläche zeigt dauerhaft drei eigenständige Karten mit unabhängigen Aktionen. Gleiche
Modell-IDs verschiedener Provider bleiben eindeutig. Neue Authentifizierungsarten, mehrere
Instanzen derselben Providerart, automatische Hintergrunderkennung und persistierte Kataloge
sind nicht Bestandteil dieser Entscheidung.
