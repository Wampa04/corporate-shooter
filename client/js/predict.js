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

/**
 * Wie schnell ein Korrekturfehler ausgeblendet wird (1/s).
 *
 * Der Server hat immer recht, aber seine Korrektur muss nicht sichtbar sein.
 * Wird sie hart gesetzt, springt das Bild - und ein paar Zentimeter je
 * Snapshot fuehlen sich dann an wie Paketverlust. Zwoelf entspricht rund
 * achtzig Millisekunden: schnell genug, dass niemand an falscher Stelle
 * spielt, langsam genug, dass es niemand als Ruck wahrnimmt.
 */
const FEHLER_ABKLINGEN = 12;

/**
 * Ab diesem Abstand wird gesprungen statt ausgeblendet.
 *
 * Wiedereinstieg und Teleport sind keine Fehler, die man verstecken sollte -
 * sie langsam einzublenden waere eine Rutschpartie quer durchs Buero.
 */
const SPRUNG_AB = 1.5;

export class Prediction {
  constructor(instance) {
    this.x = instance.exports;
    this._pending = [];
    this._ready = false;
    // Sicht auf den Zustand erst nach dem Laden anlegen: das Parsen der Karte
    // belegt Speicher, und waechst der WASM-Speicher, wird jede aeltere Sicht
    // entkoppelt.
    this._state = null;
    /** Sichtbarer Rest einer Korrektur, klingt ab. */
    this._fehler = [0, 0, 0];
    /** Vorheriger und aktueller Schritt, zum Zwischenbild-Ausgleich. */
    this._vor = null;
    this._jetzt = null;
    this._seitSchritt = 0;
    /**
     * Statistik ueber alle Abgleiche.
     *
     * Nicht abgetastet, sondern beim Entstehen gezaehlt: Korrekturen treten je
     * Snapshot auf, und eine Abtastung von aussen verfehlt sie regelmaessig.
     * Diese Zahlen sind das Guetemass der Vorhersage.
     */
    this.stats = { anzahl: 0, summe: 0, max: 0, ueber5cm: 0 };
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
    this._tickDt = 1 / config.tick_rate;
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
    // Wie weit der Server der Vorhersage widerspricht. Diese Zahl ist das
    // Mass fuer die Guete der Vorhersage: bleibt sie klein, merkt niemand
    // etwas; springt sie, ruckelt das Bild.
    const vorher = this._synced
      ? [this._state[POS_X], this._state[POS_Y], this._state[POS_Z]]
      : null;
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

    if (vorher) {
      // Die Korrektur nicht sofort zeigen, sondern als Rest mitfuehren und
      // ausblenden. `vorher` ist, was gerade zu sehen war; die Differenz zum
      // neuen Stand wandert in den Rest und verschwindet von dort.
      const dx = vorher[0] - this._state[POS_X];
      const dy = vorher[1] - this._state[POS_Y];
      const dz = vorher[2] - this._state[POS_Z];
      this.lastError = Math.hypot(dx, dy, dz);
      this.pendingCount = this._pending.length;
      this.stats.anzahl++;
      this.stats.summe += this.lastError;
      this.stats.max = Math.max(this.stats.max, this.lastError);
      if (this.lastError > 0.05) this.stats.ueber5cm++;

      if (this.lastError > SPRUNG_AB) {
        this._fehler = [0, 0, 0];
      } else {
        this._fehler[0] += dx;
        this._fehler[1] += dy;
        this._fehler[2] += dz;
      }
      // Der Ausgleich zwischen zwei Schritten geht vom neuen Stand aus; die
      // Differenz zum alten steckt jetzt im abklingenden Rest.
      this._vor = this._lesePos();
      this._jetzt = this._vor.slice();
    }
  }

  /** Rechnet eine gerade abgeschickte Eingabe sofort voraus. */
  advance(frame, alive) {
    if (!this._ready) return;
    this._vor = this._jetzt ?? this._lesePos();
    this._step(frame, this._lastButtons ?? 0, alive);
    this._lastButtons = frame.buttons;
    this._jetzt = this._lesePos();
    this._seitSchritt = 0;
  }

  _lesePos() {
    return [this._state[POS_X], this._state[POS_Y], this._state[POS_Z]];
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

  /**
   * Darzustellende Fussposition, oder `null` solange nichts geladen ist.
   *
   * Das ist die vorhergesagte Position plus dem, was von der letzten Korrektur
   * noch nicht ausgeblendet ist. Gespielt wird auf der vorhergesagten,
   * gezeigt wird die geglaettete - der Unterschied betraegt hoechstens ein
   * paar Zentimeter und ist nach achtzig Millisekunden weg.
   */
  position(dt = 0) {
    if (!this._ready || !this._synced) return null;

    if (dt > 0) {
      const rest = Math.exp(-FEHLER_ABKLINGEN * dt);
      this._fehler[0] *= rest;
      this._fehler[1] *= rest;
      this._fehler[2] *= rest;
      this._seitSchritt += dt;
    }

    // Zwischen zwei Vorhersageschritten ausgleichen.
    //
    // Vorhergesagt wird mit der Tickrate, gezeichnet mit der Bildrate. Ohne
    // diesen Ausgleich stuende die Kamera bei 120 Bildern je Sekunde drei
    // Bilder still und spraenge dann achtzehn Zentimeter - das liest sich als
    // Ruckeln, obwohl die Vorhersage stimmt.
    //
    // Bewusst zwischen zwei bekannten Staenden statt darueber hinaus: eine
    // Fortschreibung schoebe die Kamera an einer Wand in die Wand hinein und
    // beim Stehenbleiben ueber das Ziel. Der Preis ist ein halber Tick
    // Verzoegerung gegenueber gar keinem Ausgleich - bei den 60 bis 150 ms,
    // die die Vorhersage einspart, ein guter Handel.
    const a = this._vor && this._jetzt
      ? Math.min(this._seitSchritt / this._tickDt, 1)
      : 1;
    const vor = this._vor ?? this._lesePos();
    const jetzt = this._jetzt ?? vor;

    return {
      x: vor[0] + (jetzt[0] - vor[0]) * a + this._fehler[0],
      y: vor[1] + (jetzt[1] - vor[1]) * a + this._fehler[1],
      z: vor[2] + (jetzt[2] - vor[2]) * a + this._fehler[2],
    };
  }
}
