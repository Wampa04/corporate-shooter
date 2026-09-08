# Corporate Shooter

Ein Browser-Shooter im Großraumbüro. Internes Spaßprojekt, kein Produkt.

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
  Takt:    60 Hz
  Client:  client
  lokal:   http://localhost:4200
  im LAN:  http://192.168.1.42:4200
```

Adresse weitergeben, fertig. Es gibt keinen Build-Schritt für den Client und
kein npm.

Die Zeile „im LAN" erscheint nur, wenn mDNS eingeschaltet ist; für einen
gehosteten Server ist sie ohne Belang.

### Optionen

| Option | Bedeutung |
| --- | --- |
| `--port <n>` | Port für HTTP und WebSocket (Standard 4200, `0` wählt einen freien) |
| `--bind <ip>` | Adresse, an die gebunden wird (Standard `0.0.0.0`) |
| `--name <text>` | Name, unter dem der Server erscheint |
| `--tick-rate <hz>` | Simulationsschritte pro Sekunde (Standard 60) |
| `--snapshot-interval <n>` | Simulationsschritte je Snapshot (Standard 2) |
| `--client-dir <pfad>` | Verzeichnis mit dem Browser-Client, falls es nicht gefunden wird |
| `--no-mdns` | mDNS-Bekanntmachung abschalten (beim Hosten sinnvoll) |
| `--max-players <n>` | Höchstzahl gleichzeitiger Spieler (Standard 16) |
| `--seed <n>` | Fester Startwert für Streuung und Spawnauswahl (für Tests) |

### Mit Docker Compose

Der bequemste Weg:

```sh
cp .env.example .env    # optional, alles hat Vorgaben
docker compose up -d
docker compose logs     # gibt die Adresse aus, die weiterzugeben ist
```

Vorgabe ist der Serverbetrieb: gewöhnliches Bridge-Netz, ein veröffentlichter
Port, mDNS aus. Wer im eigenen Netz spielt, nimmt das Profil `lan` — es
benutzt `network_mode: host`, weil mDNS Multicast braucht und im NAT der
Bridge nichts davon im LAN ankommt:

```sh
docker compose --profile lan up -d server-lan
```

### Auf einem öffentlich erreichbaren Server

Davor gehört ein Reverse Proxy, der TLS beendet (Caddy, nginx, Traefik) und
das Upgrade auf WebSocket durchreicht. Am Client ist dafür nichts zu tun: er
wählt `wss` von selbst, sobald die Seite über `https` ausgeliefert wird. Der
Server bindet dann sinnvollerweise nur an `127.0.0.1`.

Was dabei zu beachten ist:

* `--max-players` ist keine Formalie mehr. Ohne Grenze hält ein einzelner
  Gegenüber beliebig viele Verbindungen offen, und jede kostet einen Platz in
  jedem Snapshot.
* Eine WebSocket-Nachricht ist auf 4 KiB begrenzt. Ein `InputFrame` wiegt gut
  hundert Byte; die Vorgabe der Bibliothek läge bei 64 MiB.
* Der Client wird gzip-komprimiert ausgeliefert — statt 800 KiB gehen rund
  200 KiB über die Leitung, der Löwenanteil davon Three.js.
* Die Kartenbeschreibung wiegt beim Beitreten rund 109 KiB und geht
  unkomprimiert über die WebSocket-Verbindung. Einmal je Beitritt, also
  vertretbar; ein Kandidat für später.

Alles ist über Umgebungsvariablen einstellbar, `.env.example` listet sie mit
Erklärung auf:

| Variable | Bedeutung |
| --- | --- |
| `CORPSHOOT_PORT` | Port für HTTP und WebSocket |
| `CORPSHOOT_BIND` | Adresse, an die gebunden wird |
| `CORPSHOOT_NAME` | Name, unter dem der Server erscheint |
| `CORPSHOOT_TICK_RATE` | Simulationsschritte pro Sekunde |
| `CORPSHOOT_NO_MDNS` | mDNS-Bekanntmachung abschalten |
| `CORPSHOOT_CLIENT_DIR` | Verzeichnis mit dem Browser-Client |
| `CORPSHOOT_SEED` | Fester Startwert des Zufallsgenerators (für Tests) |

Jede Variable entspricht einer Kommandozeilenoption; wird beides angegeben,
gewinnt die Kommandozeile. Die Vorgabewerte stehen nur an einer Stelle,
nämlich im Server — `.env.example` nennt sie als Hinweis, setzt sie aber
nicht.

### Mit Docker allein

Das Image enthält Server **und** Client. Empfohlen ist `--network host`: der
Server liegt damit direkt im LAN, die mDNS-Bekanntmachung funktioniert, und
die beim Start ausgegebene Adresse stimmt.

```sh
docker run --rm --network host ghcr.io/wampa04/corporate-shooter:latest
```

Ohne `--network host` bleibt der Container hinter NAT. Dann den Port
veröffentlichen und mDNS abschalten:

```sh
docker run --rm -p 4200:4200 ghcr.io/wampa04/corporate-shooter:latest --no-mdns
```

In diesem Fall ist die ausgegebene „LAN-Adresse“ die des Containers
(`172.17.x.x`) und nicht die des Rechners — dann die Host-IP weitergeben.

Alle Serveroptionen werden durchgereicht:

```sh
docker run --rm --network host ghcr.io/wampa04/corporate-shooter:latest \
  --port 8080 --name "Daily Standup"
```

Selbst bauen:

```sh
docker build -t corporate-shooter .
```

Der Build ist zweistufig und nutzt [cargo-chef], damit eine Quelltextänderung
nicht das Übersetzen von Bevy nach sich zieht — ein Rebuild dauert dann
Sekunden statt Minuten. Das Laufzeitbild ist `debian-slim` ohne
nachinstallierte Pakete: die Binärdatei braucht nur libc, libm und libgcc. Der
Server läuft als unprivilegierter Nutzer.

[cargo-chef]: https://github.com/LukeMathWalker/cargo-chef

### Darstellung

Der Client misst laufend seine eigene Bildzeit und regelt danach die
Grafikstufe. Bleibt der Median eines Messfensters über 28 ms - also unter gut
35 Bildern je Sekunde -, fällt er eine Stufe; bleibt er über mehrere Fenster
unter 18 ms, steigt er wieder. Die Bildrate steht im HUD neben dem Ping.

Die 18 ms sind kein runder Wert, sondern ein Deckel: bei 60 Hz wartet der
Browser auf den Bildwechsel, kein Bild kann schneller als 16,7 ms fertig
werden. Eine Schwelle darunter wäre unerreichbar — der Regler könnte fallen,
aber nie wieder steigen.

Die Stufen, von schön nach schnell: voller Schattenwurf, ohne Schattenwurf,
dann in zwei Schritten weniger Bildpunkte. Der Schattenwurf fällt zuerst, weil
er das ganze Stockwerk ein zweites Mal zeichnet und damit unabhängig von der
Bildgröße kostet; die Auflösung sinkt zuletzt, weil man das sieht. Auf einem
Bildschirm mit Pixelverhältnis 1 entfällt die erste Auflösungsstufe, weil sie
dort nichts änderte.

Gemessen im Prüflauf (SwiftShader, ein und dieselbe Sitzung): 83 ms bei voller
Stufe, 67 ms ohne Schattenwurf, 50 ms bei 75 Prozent Auflösung - zwölf,
fünfzehn, zwanzig Bilder je Sekunde.

Gemessen wird der Median, nicht der Mittelwert: ein einzelnes langes Bild -
eine Speicherbereinigung, die Rückkehr aus einem anderen Tab - sagt nichts
darüber, ob die Maschine die Stufe trägt. Und wer zweimal von derselben Stufe
herunter musste, kommt nicht mehr hinauf, sonst pendelt eine Maschine an der
Grenze im Sekundentakt.

Erzwingen lässt sich das über die Adresse:

| Adresse | Wirkung |
| --- | --- |
| `…:4200/` | misst laufend und regelt nach |
| `…:4200/?grafik=schoen` | immer die volle Stufe |
| `…:4200/?grafik=einfach` | immer die schnellste Stufe |

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

* Autoritativer Server, 60 Hz simuliert, 30 Snapshots je Sekunde;
  Clients schicken nur Eingaben
* Karte „Großraumbüro, 3. OG“ mit Kaffeeküche, verglastem Serverraum,
  erhöhter Chef-Etage und einem Ostflügel aus Besprechungsraum und zwei
  verschieden eingerichteten Einzelbüros
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
* **Client-seitige Vorhersage.** Bis eine Taste sichtbar wirkt, vergehen ein
  halber Tick, die Umlaufzeit und die Zeitkonstante der Kameraglättung. Im LAN
  sind das rund 60 ms, über das Internet eher 100 bis 150 ms — dort ist die
  Vorhersage keine Politur mehr, sondern Voraussetzung.
  `InputFrame::seq` und `Snapshot::ack_seq` sind dafür bereits vorgesehen.
  Nachgebaut wird die Bewegung dabei **nicht** in JavaScript: die vorhandene
  Rust-Funktion soll nach WASM übersetzt und im Browser dieselbe bleiben.


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

### Vorhersage und Lag-Kompensation

Zwei Dinge, die über das Internet nötig werden und im LAN entbehrlich waren:

**Simulation und Versand sind entkoppelt.** Der Server rechnet mit 60 Hz,
verschickt aber nur jeden zweiten Schritt. Der feine Takt halbiert Eingabeweg
und Schrittgröße in der Vorhersage; die Bandbreite bleibt die von 30 Hz.
Gemessen: 0,38 ms je Tick von 16,7 ms Budget bei acht Spielern.

**Die eigene Bewegung wird vorhergesagt.** Der Client wartet nicht auf die
Antwort des Servers, sondern rechnet selbst weiter und gleicht bei jedem
Snapshot ab. Gerechnet wird dabei nicht in JavaScript: `crates/predict`
übersetzt dieselbe Rust-Funktion nach WebAssembly, die auch der Server
ausführt. `scripts/gleichlauf.sh` hält fest, dass beide dasselbe rechnen.

**Schüsse werden zurückgespult.** Fremde Spieler werden im Client bewusst
verzögert gezeigt, damit ihre Bewegung nicht ruckelt; dazu kommt die Laufzeit.
Wer auf einen Kopf zielt, zielt also auf eine Vergangenheit. Der Client meldet
mit jeder Eingabe, welchen Serverstand er gerade sah, und der Server wertet den
Schuss gegen diesen Stand aus statt gegen den aktuellen. Ohne das müsste man
über das Internet um die eigene Umlaufzeit vorhalten — bei 80 ms und 5,4 m/s
gut 40 cm.

Das Rückspulen ist auf 12 Ticks (400 ms) gedeckelt. Der gewünschte Zeitpunkt
kommt vom Client, und ohne Deckel könnte jemand behaupten, er habe den Stand
von vor einer Minute gesehen. Die übliche Kehrseite bleibt: wer gerade hinter
eine Ecke gelaufen ist, kann dort noch getroffen werden, wo der Schütze ihn
sah. Das ist der Preis dafür, dass Zielen überhaupt funktioniert.

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
  messbar. Über das Internet wird es das: ein verlorenes Paket hält alle
  nachfolgenden Snapshots auf, bis es erneut angekommen ist. Bei spürbarem
  Paketverlust ist WebTransport der nächste Schritt — die Transportschicht
  liegt dafür gekapselt.

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

Rust 1.95 oder neuer (Vorgabe von Bevy 0.19). Das `Dockerfile` nagelt die
Compiler-Version fest; wird Bevy angehoben, ist dort `RUST_VERSION`
nachzuziehen.

### Continuous Integration

`.github/workflows/image.yml` läuft bei jedem Push und Pull Request in zwei
Stufen:

1. **Tests** — `cargo test --workspace --locked`.
2. **Image** — bauen, starten und prüfen, dass es den Client ausliefert.

Die zweite Stufe hängt an der ersten: aus rotem Code entsteht erst gar kein
Image. Veröffentlicht wird nur vom Standardbranch und von `v*`-Tags, nach
`ghcr.io/<repo>`.

`cargo fmt --all -- --check` und `cargo clippy` laufen **nicht** in der CI und
gehören vor dem Commit gelaufen.
