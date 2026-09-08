// Vorhersage der eigenen Bewegung.
//
// Ohne sie zeigt die Kamera die Position aus dem letzten Snapshot: bis eine
// Taste sichtbar wirkt, vergehen ein halber Tick, die Umlaufzeit und die
// Zeitkonstante der Kameraglaettung. Im LAN sind das rund 60 ms, ueber das
// Internet eher 100 bis 150 - dort ist die Vorhersage keine Politur, sondern
// Voraussetzung.
//
// Gerechnet wird dabei **nicht** in JavaScript. Dieses Modul laedt
// `predict.wasm`, in dem dieselbe Rust-Funktion steckt, die auch der Server
// ausfuehrt. Eine zweite, hier gepflegte Fassung der Bewegung gaebe es sonst
// genau so lange, bis die beiden auseinanderlaufen -
// `scripts/gleichlauf.sh` haelt fest, dass sie es nicht tun.
//
// Vorhergesagt wird ausschliesslich die eigene Bewegung. Schuesse, Treffer,
// Leben und fremde Spieler bleiben unveraendert serverautoritativ.

/** Reihenfolge der Felder im geteilten Speicher, siehe `crates/predict`. */
const POS_X = 0, POS_Y = 1, POS_Z = 2;
const VEL_Y = 4;
const YAW = 6, PITCH = 7, ON_GROUND = 8;
const DASH_TIMER = 9, DASH_DIR_X = 10, DASH_DIR_Z = 11, DASH_COOLDOWN = 12;
const STATE_FLOATS = 13;

/**
 * Wie viele gesendete Eingaben aufgehoben werden.
 *
 * Nachgespielt wird alles ab der letzten Bestaetigung des Servers, also
 * ungefaehr eine Umlaufzeit. 120 Bilder sind bei 30 Hz vier Sekunden - mehr,
 * als jede brauchbare Verbindung braucht.
 */
const RINGGROESSE = 120;

export class Prediction {
  constructor(instance) {
    this.x = instance.exports;
    this._pending = [];
    this._ready = false;
    // Sicht auf den Zustand erst nach dem Laden anlegen: das Parsen der Karte
    // belegt Speicher, und waechst der WASM-Speicher, wird jede aeltere Sicht
    // entkoppelt.
    this._state = null;
  }

  /**
   * Laedt das Modul. Liefert `null`, wenn das nicht geht - dann bleibt es beim
   * bisherigen Verhalten, statt dass das Spiel ausfaellt.
   */
  static async load(url = "../vendor/predict.wasm") {
    try {
      const antwort = await fetch(new URL(url, import.meta.url));
      if (!antwort.ok) throw new Error(`HTTP ${antwort.status}`);
      // `instantiateStreaming` braucht den richtigen MIME-Typ; ueber den
      // Umweg des Puffers ist es unabhaengig davon, wie der Server ihn setzt.
      const { instance } = await WebAssembly.instantiate(
        await antwort.arrayBuffer(),
        {},
      );
      return new Prediction(instance);
    } catch (fehler) {
      console.warn("Vorhersage nicht verfuegbar:", fehler.message);
      return null;
    }
  }

  /** Uebergibt Karte und Konfiguration, unveraendert wie vom Server gekommen. */
  init(map, config) {
    const ok =
      this._reichEin(JSON.stringify(map), this.x.load_level) &&
      this._reichEin(JSON.stringify(config), this.x.load_config);
    if (!ok) {
      console.warn("Vorhersage: Karte oder Konfiguration abgelehnt");
      return false;
    }
    this._state = new Float32Array(
      this.x.memory.buffer,
      this.x.state_ptr(),
      STATE_FLOATS,
    );
    this._ready = true;
    return true;
  }

  _reichEin(text, fn) {
    const bytes = new TextEncoder().encode(text);
    if (bytes.length > this.x.scratch_len()) return false;
    // Erst den Zeiger holen, dann die Sicht anlegen - der Aufruf kann den
    // Speicher wachsen lassen und eine vorher angelegte Sicht entkoppeln.
    const ptr = this.x.scratch_ptr();
    new Uint8Array(this.x.memory.buffer).set(bytes, ptr);
    return fn(bytes.length) === 1;
  }

  get ready() {
    return this._ready;
  }

  /** Merkt sich eine gesendete Eingabe, bis der Server sie bestaetigt. */
  record(frame) {
    this._pending.push(frame);
    if (this._pending.length > RINGGROESSE) this._pending.shift();
  }

  /**
   * Setzt auf den Serverzustand zurueck und spielt alle unbestaetigten
   * Eingaben erneut durch.
   *
   * Genau das macht die Vorhersage ehrlich: der Server behaelt recht, und was
   * seit seiner letzten Antwort eingegeben wurde, wird darauf neu gerechnet -
   * mit demselben Code, den er selbst benutzt.
   */
  reconcile(self, local, ackSeq) {
    if (!this._ready || !self || !local) return;
    // Vor dem ersten Abgleich steht im Zustand nur der Nullpunkt. Ihn als
    // Position auszugeben hiesse, die Kamera einen Wimpernschlag lang in die
    // Mitte der Karte zu stellen.
    this._synced = true;

    const s = this._state;
    s[POS_X] = self.pos[0];
    s[POS_Y] = self.pos[1];
    s[POS_Z] = self.pos[2];
    // Waagerecht wird je Tick neu aus der Eingabe gesetzt, senkrecht
    // integriert - deshalb schickt der Server nur `vel_y`.
    s[3] = 0;
    s[VEL_Y] = local.vel_y ?? 0;
    s[5] = 0;
    s[YAW] = self.yaw;
    s[PITCH] = self.pitch;
    s[ON_GROUND] = local.on_ground ? 1 : 0;
    s[DASH_TIMER] = local.dash_timer ?? 0;
    s[DASH_DIR_X] = local.dash_dir_x ?? 0;
    s[DASH_DIR_Z] = local.dash_dir_z ?? 0;
    s[DASH_COOLDOWN] = local.dash_cooldown_remaining ?? 0;

    // Alles Bestaetigte kann weg.
    while (this._pending.length && this._pending[0].seq <= ackSeq) {
      this._pending.shift();
    }

    let prev = 0;
    for (const frame of this._pending) {
      this._step(frame, prev, self.alive);
      prev = frame.buttons;
    }
    this._lastButtons = prev;
  }

  /** Rechnet eine gerade abgeschickte Eingabe sofort voraus. */
  advance(frame, alive) {
    if (!this._ready) return;
    this._step(frame, this._lastButtons ?? 0, alive);
    this._lastButtons = frame.buttons;
  }

  _step(frame, prevButtons, alive) {
    this.x.step(
      frame.move_x,
      frame.move_z,
      frame.yaw,
      frame.pitch,
      frame.buttons,
      prevButtons,
      alive ? 1 : 0,
    );
  }

  /** Vorhergesagte Fussposition, oder `null` solange nichts geladen ist. */
  position() {
    if (!this._ready || !this._synced) return null;
    return { x: this._state[POS_X], y: this._state[POS_Y], z: this._state[POS_Z] };
  }
}
