// Verbindung zum Server.
//
// Der Client schickt ausschliesslich Absichten und bekommt Zustaende zurueck.
// Hier steht deshalb kein Spielwissen, nur Transport, ein Puffer der letzten
// Snapshots (fuer die Interpolation) und eine Laufzeitmessung.

/** Tastenbits, identisch zu `protocol::message::buttons`. */
export const BUTTON = {
  FIRE:   1 << 0,
  JUMP:   1 << 1,
  DASH:   1 << 2,
  HEAL:   1 << 3,
  RELOAD: 1 << 4,
};

/** Wie viele Snapshots aufgehoben werden. Bei 30 Hz gut zwei Sekunden. */
const SNAPSHOT_HISTORY = 64;

/** Abstand zwischen zwei Laufzeitmessungen in Millisekunden. */
const PING_INTERVAL = 1000;

/**
 * Fuehrt den eigenen Stand (`Local`) und den oeffentlichen `Snapshot`
 * desselben Ticks zu dem Objekt zusammen, mit dem der Rest des Clients
 * arbeitet: `{tick, ack_seq, players, local, events, match}`.
 *
 * Der Server schickt beides getrennt, damit er den Snapshot fuer alle
 * Empfaenger nur einmal kodieren muss. `Local` kommt immer unmittelbar davor.
 * Passt der Tick nicht, ist etwas grundsaetzlich schiefgelaufen - dann lieber
 * einen Snapshot auslassen, als die Vorhersage mit dem Stand eines anderen
 * Ticks abzugleichen.
 *
 * @param {{tick: number, ack_seq: number, local: object} | null} local
 * @param {{tick: number}} snapshot
 * @returns {object | null}
 */
export function mergeSnapshot(local, snapshot) {
  if (!local || local.tick !== snapshot.tick) return null;
  snapshot.ack_seq = local.ack_seq;
  snapshot.local = local.local;
  return snapshot;
}

export class Connection {
  constructor() {
    this.socket = null;

    /** Ringpuffer der letzten Snapshots, aelteste zuerst. */
    this.snapshots = [];
    /** Zuletzt gemessene Laufzeit in Millisekunden, `null` bis zum ersten Pong. */
    this.ping = null;

    this.onSnapshot = () => {};
    this.onClose = () => {};

    this._pingTimer = null;
    /** Eigener Stand, wartet auf den Snapshot desselben Ticks. */
    this._local = null;
  }

  /**
   * Baut die Verbindung auf und meldet sich an.
   *
   * Die WebSocket-Adresse wird aus der aufgerufenen Seite abgeleitet: der
   * Server liefert Client und Spiel unter derselben Herkunft aus, es gibt
   * also nichts zu konfigurieren.
   */
  connect(name) {
    const scheme = location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${scheme}//${location.host}/ws`;

    return new Promise((resolve, reject) => {
      let settled = false;
      const socket = new WebSocket(url);
      this.socket = socket;

      socket.addEventListener("open", () => {
        socket.send(JSON.stringify({ t: "Join", d: { name } }));
      });

      socket.addEventListener("message", (event) => {
        let message;
        try {
          message = JSON.parse(event.data);
        } catch {
          return; // Unlesbares verwerfen statt die Verbindung zu kappen.
        }
        this._handle(message, (welcome) => {
          if (settled) return;
          settled = true;
          this._startPinging();
          resolve(welcome);
        }, (reason) => {
          if (settled) return;
          settled = true;
          reject(new Error(reason));
        });
      });

      socket.addEventListener("error", () => {
        if (settled) return;
        settled = true;
        reject(new Error("Server nicht erreichbar."));
      });

      socket.addEventListener("close", () => {
        this._stopPinging();
        if (!settled) {
          settled = true;
          reject(new Error("Verbindung wurde vom Server geschlossen."));
          return;
        }
        this.onClose();
      });
    });
  }

  _handle(message, resolve, reject) {
    switch (message.t) {
      case "Welcome":
        resolve(message.d);
        break;

      case "Local":
        this._local = message.d;
        break;

      case "Snapshot": {
        const snapshot = mergeSnapshot(this._local, message.d);
        this._local = null;
        if (!snapshot) break;
        snapshot.recvTime = performance.now();
        // Einmal je Snapshot statt in jedem Bild: Kamera, HUD, Interpolation
        // und Vorhersage fragen alle "welcher Spieler ist das?".
        snapshot.byId = new Map(snapshot.players.map((p) => [p.id, p]));
        this.snapshots.push(snapshot);
        if (this.snapshots.length > SNAPSHOT_HISTORY) this.snapshots.shift();
        this.onSnapshot(snapshot);
        break;
      }

      case "Pong":
        this.ping = performance.now() - message.d.client_time_ms;
        break;

      case "Rejected":
        reject(message.d.reason);
        this.socket.close();
        break;
    }
  }

  get latest() {
    return this.snapshots[this.snapshots.length - 1] ?? null;
  }

  sendInput(frame) {
    if (this.socket?.readyState !== WebSocket.OPEN) return;
    this.socket.send(JSON.stringify({ t: "Input", d: frame }));
  }

  _startPinging() {
    this._stopPinging();
    const send = () => {
      if (this.socket?.readyState !== WebSocket.OPEN) return;
      this.socket.send(
        JSON.stringify({ t: "Ping", d: { client_time_ms: performance.now() } }),
      );
    };
    send();
    this._pingTimer = setInterval(send, PING_INTERVAL);
  }

  _stopPinging() {
    if (this._pingTimer !== null) {
      clearInterval(this._pingTimer);
      this._pingTimer = null;
    }
  }
}
