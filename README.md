# Corporate Shooter

Ein LAN-Shooter im Großraumbüro. Internes Spaßprojekt, kein Produkt.

Marketing gegen Engineering, drei Etagenabschnitte, Textmarker und Locher.
Der Server hält den gesamten Spielzustand, die Kolleg:innen öffnen eine URL.

## Loslegen

```sh
cargo run --release -p server
```

Der Server gibt beim Start aus, unter welchen Adressen er erreichbar ist:

```
Corporate Shooter laeuft.
  Karte:   Großraumbüro, 3. OG
  Takt:    30 Hz
  Client:  client
  lokal:   http://localhost:4200
  im LAN:  http://192.168.1.42:4200
```

Die LAN-Adresse weitergeben, fertig. Es gibt keinen Build-Schritt für den
Client und kein npm.

### Optionen

| Option | Bedeutung |
| --- | --- |
| `--port <n>` | Port für HTTP und WebSocket (Standard 4200, `0` wählt einen freien) |
| `--bind <ip>` | Adresse, an die gebunden wird (Standard `0.0.0.0`) |
| `--name <text>` | Name, unter dem der Server im LAN erscheint |
| `--tick-rate <hz>` | Simulationsschritte pro Sekunde (Standard 30) |
| `--client-dir <pfad>` | Verzeichnis mit dem Browser-Client, falls es nicht gefunden wird |
| `--no-mdns` | mDNS-Bekanntmachung abschalten |
| `--seed <n>` | Fester Startwert für Streuung und Spawnauswahl (für Tests) |

## Steuerung

| Taste | Wirkung |
| --- | --- |
| W A S D | Laufen |
| Maus | Umsehen, Linksklick feuert |
| Leertaste | Springen |
| Umschalt | **Agile Sprint** (Dash) |
| E | **Wellness-Tag** (Heilung, langer Cooldown) |
| R | Nachladen |
| 1 / 2 | Textmarker-Pistole / Locher-Schrotflinte |
| Tab | Rangliste |
| Esc | Maus freigeben |

## Was drin ist

* Autoritativer Server, 30 Hz, Clients schicken nur Eingaben
* Karte „Großraumbüro, 3. OG“ mit Kaffeeküche, verglastem Serverraum und
  erhöhter Chef-Etage
* Textmarker-Pistole und Locher-Schrotflinte
* Agile Sprint und Wellness-Tag
* Team Deathmatch: Marketing gegen Engineering, kein Friendly Fire
* Tod, Wartezeit und Wiedereinstieg an einem gegnerfernen Spawnpunkt
* Rangliste, Killfeed, Trefferanzeige

## Was noch fehlt

Aus dem ursprünglichen Entwurf ist bewusst noch nicht umgesetzt:

* **Waffen:** Passiv-aggressive E-Mail (Wurfgeschoss mit Verzögerung),
  Kaffeevollautomat-Minigun (Überhitzung), Whiteboard als tragbares Schild.
  Das Whiteboard steht bisher nur als feste Deckung im Level.
* **Spielmodi:** „Deadline“ (Capture the Flag) und „Layoff Royale“
  (schrumpfendes Feld). Es gibt bisher nur Team Deathmatch, und zwar ohne
  Rundenende und ohne Punktegrenze.
* **Ultimate:** der teambasierte „Synergie-Boost“.
* **Client-seitige Vorhersage.** Im LAN (unter 1 ms Laufzeit) fällt die
  fehlende Vorhersage kaum auf; über das Internet wäre sie nötig.
  `InputFrame::seq` und `Snapshot::ack_seq` sind dafür bereits vorgesehen.

## Aufbau

```
crates/protocol/   Wire-Typen, von Server und Client gemeinsam benutzt
crates/server/     Autoritative Simulation (Bevy, headless) und Netzwerkschicht
client/            Browser-Client (Three.js, ES-Module, kein Build-Schritt)
```

Der Server ist die einzige Quelle der Wahrheit. Er schickt dem Client beim
Verbinden nicht nur den Spielzustand, sondern auch die **Levelgeometrie** und
alle **Balancing-Werte**. Der Client baut sein Rendering daraus. Das ist die
Antwort auf den Haupteinwand gegen einen separaten JS-Client: es gibt keine
zweite, in JavaScript gepflegte Kopie des Levels oder der Waffenwerte, die
auseinanderlaufen könnte. In `client/js/` steht keine Spiellogik.

Einzige Ausnahme ist die Blickrichtung: sie entsteht lokal aus der
Mausbewegung, weil Umsehen nicht auf eine Netzwerkantwort warten darf. Der
Server begrenzt sie und behandelt sie für alles Weitere als verbindlich.

### Warum WebSocket und nicht UDP

Der ursprüngliche Entwurf sah UDP über `laminar` oder `renet` vor. Das geht
mit einem Browser-Client nicht: **Browser haben keine UDP-Sockets.** Weder
Bevy-zu-WASM noch Three.js ändert daran etwas — beide sitzen in derselben
Sandbox.

Die Alternativen waren:

* **WebTransport** (QUIC/HTTP3) hat unreliable Datagrams und damit kein
  Head-of-Line-Blocking, erfüllt also die eigentliche Absicht. Es braucht
  aber zwingend TLS, im LAN also ein selbst erzeugtes ECDSA-Zertifikat mit
  höchstens 14 Tagen Gültigkeit, dessen Hash der Client kennen muss.
* **Nativer Client** mit echtem UDP — dann ist es aber kein Browser-Spiel
  mehr, und jede:r müsste eine Binärdatei starten.
* **WebSocket** über TCP hat Head-of-Line-Blocking. Bei 30 Hz in einem LAN
  mit unter 1 ms Laufzeit und praktisch ohne Paketverlust ist das nicht
  messbar.

Gewählt wurde WebSocket, weil der Nachteil hier theoretisch bleibt und der
Aufwand aller anderen Wege real ist. Die Transportschicht ist auf
`crates/server/src/net/` und `client/js/net.js` begrenzt; ein Wechsel auf
WebTransport betrifft nichts darüber hinaus.

### Warum mDNS trotzdem drin ist

`mdns-sd` meldet den Server unter `_corpshoot._tcp.local.` an. Ein Browser
kann davon nichts sehen — er hat keinen Zugriff auf Multicast. Die
Bekanntmachung richtet sich an Werkzeuge, die im LAN suchen (`avahi-browse`,
ein späterer Launcher). Damit trotzdem niemand IP-Adressen raten muss, gibt
der Server seine erreichbaren URLs beim Start aus.

## Entwicklung

```sh
cargo test --workspace   # Simulation, Protokoll und ein End-to-End-Duell
cargo clippy --all-targets
cargo fmt --all
```

Der End-to-End-Test startet den echten Serverprozess, verbindet sich per
WebSocket wie ein Browser und spielt eine Runde durch. Er hat bereits zwei
Fehler gefunden, die den Unit-Tests entgangen waren — es lohnt sich, ihn
laufen zu lassen.

Der Browser-Client hat keine automatisierten Tests. Änderungen daran gehören
im Browser angesehen; die Konsole muss dabei fehlerfrei bleiben.

Rust 1.95 oder neuer (Vorgabe von Bevy 0.19).
