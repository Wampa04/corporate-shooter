// Ton.
//
// Alle Klaenge entstehen zur Laufzeit aus Oszillatoren und Rauschen - es gibt
// keine einzige Tondatei. Das passt zum Rest des Clients, dessen Texturen
// ebenfalls auf einem Canvas entstehen, haelt das Repository frei von
// Binaerdateien und Lizenzfragen, und ist fuer Bueroeraeusche ohnehin das
// richtige Mittel: ein Locher klingt nach gefiltertem Rauschen mit einem
// Bums darunter.
//
// Wie `effects.js` liest dieses Modul nur die Ereignisse des Servers. Es
// leitet daraus keinen Spielzustand ab.

/** Lautstaerke beim ersten Start, falls nichts gespeichert ist. */
const DEFAULT_VOLUME = 0.7;

/** Laufstrecke zwischen zwei Schritten in Metern. */
const STEP_DISTANCE = 0.78;

/**
 * Hoechste Geschwindigkeit, die noch als Laufen durchgeht (m/s).
 *
 * Ein Wiedereinstieg versetzt eine Figur quer durchs Buero; ohne Grenze loeste
 * jeder Respawn eine Salve von Schritten aus. Die Grenze haengt am Zeitschritt
 * und nicht an einer festen Strecke: bei zwanzig Bildern je Sekunde legt ein
 * Sprintender zwischen zwei Bildern fast einen Meter zurueck, was eine feste
 * Grenze faelschlich als Sprung verwerfen wuerde.
 *
 * Etwas ueber `dash_speed` (16 m/s), damit ein Agile Sprint nicht durchfaellt.
 */
const MAX_STEP_SPEED = 20;

/** Groesste Strecke, die bei einem Zeitschritt noch als Laufen zaehlt. */
function maxStrecke(dt) {
  return MAX_STEP_SPEED * Math.max(dt, 1 / 120);
}

/** Reichweite, ab der ein Klang praktisch nicht mehr zu hoeren ist. */
const MAX_DISTANCE = 45;

export class Audio {
  /**
   * @param {number|null} forcedVolume Vorgabe aus der URL, `null` fuer gespeichert
   */
  constructor(forcedVolume = null) {
    const Ctx = window.AudioContext ?? window.webkitAudioContext;
    /** Ohne Web-Audio bleibt alles still, aber nichts geht kaputt. */
    this.ctx = Ctx ? new Ctx() : null;
    this.onStatus = () => {};

    this._panners = [];
    this._walkers = new Map();
    this._ownDistance = 0;
    this._reloading = false;
    this._ambience = null;

    const gespeichert = Number.parseFloat(localStorage.getItem("corpshoot.volume"));
    this.volume = forcedVolume ?? (Number.isFinite(gespeichert) ? gespeichert : DEFAULT_VOLUME);

    if (!this.ctx) return;

    this.bus = this.ctx.createGain();
    this.bus.gain.value = this.volume;
    this.bus.connect(this.ctx.destination);

    this.noise = this._noiseBuffer(2.0);
    this._bindKeys();
  }

  // -------------------------------------------------------------------------
  // Leben und Lautstaerke
  // -------------------------------------------------------------------------

  /**
   * Gibt den Ton frei.
   *
   * Browser starten einen `AudioContext` erst nach einer Nutzergeste. Diese
   * Methode haengt deshalb an derselben Stelle wie das Einfangen der Maus -
   * inklusive des Wegs zurueck aus der Pause.
   */
  resume() {
    if (!this.ctx) return;
    if (this.ctx.state === "suspended") this.ctx.resume();
    if (!this._ambience && this.volume > 0) this._startAmbience();
  }

  setVolume(value) {
    this.volume = Math.min(1, Math.max(0, value));
    localStorage.setItem("corpshoot.volume", String(this.volume));
    if (this.bus) this.bus.gain.value = this.volume;
    if (this.volume > 0) this.resume();
  }

  _bindKeys() {
    this._onKey = (event) => {
      if (event.repeat || event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.code === "KeyM") {
        if (this.volume > 0) {
          // Die alte Lautstaerke merken, damit das Einschalten sie
          // zurueckbringt statt auf die Vorgabe zu springen.
          this._vorher = this.volume;
          this.setVolume(0);
          this.onStatus("Ton aus");
        } else {
          this.setVolume(this._vorher ?? DEFAULT_VOLUME);
          this.onStatus("Ton an");
        }
      } else if (event.code === "Period" || event.code === "Comma") {
        const schritt = event.code === "Period" ? 0.1 : -0.1;
        this.setVolume(Math.round((this.volume + schritt) * 10) / 10);
        this.onStatus(`Ton ${Math.round(this.volume * 100)} %`);
      }
    };
    window.addEventListener("keydown", this._onKey);
  }

  // -------------------------------------------------------------------------
  // Ereignisse
  // -------------------------------------------------------------------------

  /**
   * Vertont die Ereignisse eines Snapshots.
   *
   * @param {Array} events Ereignisliste
   * @param {number} selfId eigene Spieler-ID
   */
  handleEvents(events, selfId) {
    if (!this.ctx) return;

    for (const event of events) {
      const d = event.d;
      switch (event.t) {
        case "Shot": {
          // Die eigene Waffe laeuft nicht durch den Panner: sie klebt an der
          // Kamera, und ein Klang, der im Kopf herumwandert, wirkt falsch.
          const pos = d.shooter === selfId ? null : d.tracers[0]?.from;
          if (d.weapon === "Locher") this._shotLocher(pos);
          else if (d.weapon === "Kaffeevollautomat") this._shotKaffee(pos);
          else this._shotTextmarker(pos);
          break;
        }
        case "Launched":
          this._wurf(d.shooter === selfId ? null : d.pos);
          break;
        case "Burst":
          this._burst(d.pos);
          break;
        case "Hit":
          if (d.attacker === selfId) this._hitmarker();
          if (d.target === selfId) this._tookDamage();
          else this._impact(d.pos);
          break;
        case "Death":
          this._death(d.victim === selfId, this._walkerPos(d.victim));
          break;
        case "Dashed":
          this._dash(d.id === selfId ? null : this._walkerPos(d.id));
          break;
        case "Healed":
          if (d.id === selfId) this._heal();
          break;
        case "Spawned":
          if (d.id === selfId) this._spawn();
          break;
      }
    }
  }

  /** Nachladen erzeugt zwei Klacks: einen beim Beginn, einen am Ende. */
  setReloading(reloading) {
    if (!this.ctx || reloading === this._reloading) return;
    this._reloading = reloading;
    this._click(reloading ? 320 : 520);
  }

  // -------------------------------------------------------------------------
  // Je Bild
  // -------------------------------------------------------------------------

  /**
   * Setzt den Hoerer und erzeugt Schritte.
   *
   * @param {number} dt Zeitschritt in Sekunden
   * @param {THREE.Camera} camera Kamera, an der der Hoerer haengt
   * @param {{x: number, y: number, z: number}} feet eigene Fussposition
   * @param {boolean} onGround eigener Bodenkontakt
   * @param {Map} avatars Mitspieler aus `PlayerViews.avatars`
   */
  update(dt, camera, feet, onGround, avatars) {
    if (!this.ctx) return;

    this._placeListener(camera);
    this._ownFootsteps(dt, feet, onGround);
    this._otherFootsteps(dt, avatars);

    // Abgelaufene Panner abraeumen - dieselbe Buchfuehrung wie in `effects.js`.
    const jetzt = this.ctx.currentTime;
    for (let i = this._panners.length - 1; i >= 0; i--) {
      if (this._panners[i].until > jetzt) continue;
      this._panners[i].node.disconnect();
      this._panners.splice(i, 1);
    }
  }

  _placeListener(camera) {
    const l = this.ctx.listener;
    const p = camera.position;
    // Blickrichtung aus der Kameradrehung; `getWorldDirection` wuerde eine
    // Matrixaktualisierung je Bild erzwingen.
    const fx = -Math.sin(camera.rotation.y);
    const fz = -Math.cos(camera.rotation.y);

    if (l.positionX) {
      l.positionX.value = p.x;
      l.positionY.value = p.y;
      l.positionZ.value = p.z;
      l.forwardX.value = fx;
      l.forwardY.value = 0;
      l.forwardZ.value = fz;
      l.upX.value = 0;
      l.upY.value = 1;
      l.upZ.value = 0;
    } else {
      // Aeltere Browser (Safari) kennen nur die alte Form.
      l.setPosition(p.x, p.y, p.z);
      l.setOrientation(fx, 0, fz, 0, 1, 0);
    }
  }

  _ownFootsteps(dt, feet, onGround) {
    if (!feet) return;
    if (this._lastOwn && onGround) {
      const strecke = Math.hypot(feet.x - this._lastOwn.x, feet.z - this._lastOwn.z);
      if (strecke <= maxStrecke(dt)) {
        this._ownDistance += strecke;
        this._ownDistance = this._schritte(this._ownDistance, () => this._footstep(null, 1.0));
      }
    }
    this._lastOwn = { x: feet.x, z: feet.z };
  }

  /**
   * Loest alle faelligen Schritte aus und gibt den Rest der Strecke zurueck.
   *
   * Der Rest muss erhalten bleiben: wuerde er auf null gesetzt, ginge bei
   * jedem Bild ein Stueck Weg verloren, und bei niedriger Bildrate - wenn also
   * je Bild mehr als ein Schritt faellig waere - bliebe es bei einem einzigen.
   * Genau daran sind hier drei statt siebzehn Schritten herausgekommen.
   */
  _schritte(strecke, spiele) {
    let rest = strecke;
    // Obergrenze, damit ein Sprung in der Buchfuehrung keine Salve ausloest.
    for (let i = 0; rest >= STEP_DISTANCE && i < 4; i++) {
      rest -= STEP_DISTANCE;
      spiele();
    }
    return rest;
  }

  _otherFootsteps(dt, avatars) {
    if (!avatars) return;
    const grenze = maxStrecke(dt);

    for (const [id, entry] of avatars) {
      const p = entry.group.position;
      const zuvor = this._walkers.get(id);
      const eintrag = { x: p.x, y: p.y, z: p.z, weg: zuvor?.weg ?? 0 };
      this._walkers.set(id, eintrag);
      if (!zuvor || !entry.group.visible) continue;

      const strecke = Math.hypot(p.x - zuvor.x, p.z - zuvor.z);
      // Nur am Boden: in der Luft macht niemand Schritte.
      if (strecke > grenze || Math.abs(p.y - zuvor.y) > 0.08) continue;

      // Fremde Schritte laut genug, um sie taktisch nutzen zu koennen - die
      // Entfernung daempft sie ohnehin.
      eintrag.weg = this._schritte(zuvor.weg + strecke, () =>
        this._footstep([p.x, p.y + 0.1, p.z], 1.4),
      );
    }

    for (const id of [...this._walkers.keys()]) {
      if (!avatars.has(id)) this._walkers.delete(id);
    }
  }

  _walkerPos(id) {
    const w = this._walkers.get(id);
    return w ? [w.x, w.y + 0.9, w.z] : null;
  }

  // -------------------------------------------------------------------------
  // Die Klaenge
  // -------------------------------------------------------------------------

  /** Textmarker: ein quietschendes Blip mit trockenem Klick. */
  _shotTextmarker(pos) {
    const dest = this._dest(pos, 0.2);
    this._tone({ type: "square", from: 720, to: 240, dur: 0.07, peak: 0.16, dest });
    this._noise({ dur: 0.035, peak: 0.1, type: "highpass", freq: 2200, dest });
  }

  /** Locher: schweres Klacken mit Bums und Metallping. */
  _shotLocher(pos) {
    const dest = this._dest(pos, 0.45);
    this._noise({ dur: 0.09, peak: 0.34, type: "bandpass", freq: 1600, q: 1.1, dest });
    this._tone({ type: "sine", from: 120, to: 55, dur: 0.16, peak: 0.4, dest });
    this._tone({ type: "triangle", from: 2400, to: 1800, dur: 0.05, peak: 0.06, dest });
  }

  /**
   * Kaffeevollautomat: Dampf und Rattern.
   *
   * Leiser als die anderen Schuesse, und das ist Absicht: bei sechzehn Schuss
   * je Sekunde summiert sich alles, was einzeln passend klingt, zu Krach.
   */
  _shotKaffee(pos) {
    const dest = this._dest(pos, 0.25);
    this._noise({ dur: 0.05, peak: 0.09, type: "highpass", freq: 3200, dest });
    this._tone({ type: "sawtooth", from: 190, to: 130, dur: 0.045, peak: 0.055, dest });
  }

  /** E-Mail unterwegs: Papier, das durch die Luft geht. */
  _wurf(pos) {
    const dest = this._dest(pos, 0.35);
    this._noise({ dur: 0.22, peak: 0.1, type: "bandpass", freq: 1400, q: 0.6, dest });
    this._tone({ type: "sine", from: 300, to: 480, dur: 0.18, peak: 0.05, dest });
  }

  /** E-Mail angekommen: dumpfer Schlag mit Papierrascheln hinterher. */
  _burst(pos) {
    const dest = this._dest(pos, 0.8);
    this._tone({ type: "sine", from: 150, to: 42, dur: 0.42, peak: 0.5, dest });
    this._noise({ dur: 0.3, peak: 0.28, type: "lowpass", freq: 900, dest });
    this._noise({ dur: 0.35, peak: 0.12, type: "highpass", freq: 2600, dest, delay: 0.05 });
  }

  /** Einschlag in der Einrichtung. */
  _impact(pos) {
    const dest = this._dest(pos, 0.2);
    this._noise({ dur: 0.06, peak: 0.13, type: "bandpass", freq: 900, q: 0.8, dest });
  }

  /** Bestaetigung eines eigenen Treffers - klingt im Kopf, nicht im Raum. */
  _hitmarker() {
    this._tone({ type: "sine", from: 880, dur: 0.045, peak: 0.12, dest: this.bus });
    this._tone({ type: "sine", from: 1320, dur: 0.06, peak: 0.09, dest: this.bus });
  }

  /** Eigener Schaden: ein Stoss, kein heller Ton. */
  _tookDamage() {
    this._tone({ type: "sawtooth", from: 180, to: 70, dur: 0.22, peak: 0.3, dest: this.bus });
    this._noise({ dur: 0.12, peak: 0.16, type: "lowpass", freq: 500, dest: this.bus });
  }

  /** Abgang: ein absteigender Ton. Der eigene klingt im Kopf, fremde im Raum. */
  _death(eigener, pos) {
    this._tone({
      type: "sawtooth",
      from: eigener ? 380 : 300,
      to: 110,
      dur: 0.45,
      peak: eigener ? 0.3 : 0.18,
      dest: eigener ? this.bus : this._dest(pos, 0.5),
    });
  }

  /** Agile Sprint: Rauschen durch einen wandernden Bandpass. */
  _dash(pos) {
    const dest = this._dest(pos, 0.4);
    this._noise({
      dur: 0.32,
      peak: pos ? 0.18 : 0.24,
      type: "bandpass",
      freq: 240,
      freqTo: 2200,
      q: 1.6,
      dest,
    });
  }

  /** Wellness-Tag: zwei aufsteigende Toene. */
  _heal() {
    this._tone({ type: "sine", from: 520, dur: 0.18, peak: 0.14, dest: this.bus });
    this._tone({ type: "sine", from: 780, dur: 0.26, peak: 0.12, dest: this.bus, delay: 0.09 });
  }

  _spawn() {
    this._tone({ type: "sine", from: 440, to: 660, dur: 0.2, peak: 0.12, dest: this.bus });
  }

  _click(freq) {
    this._tone({ type: "square", from: freq, to: freq * 0.6, dur: 0.04, peak: 0.1, dest: this.bus });
  }

  _footstep(pos, lautstaerke) {
    const dest = this._dest(pos, 0.15);
    // Teppich: kurzes, dumpfes Rauschen ohne Ausschwingen.
    // Ein Tiefpass bei 750 Hz nimmt weissem Rauschen fast den ganzen Pegel:
    // gemessen lag ein Schritt damit auf Hoehe des Grundgeraeuschs. Deshalb
    // deutlich lauter und etwas offener - Teppich bleibt dumpf, aber hoerbar.
    this._noise({
      dur: 0.08,
      peak: 0.55 * lautstaerke,
      type: "lowpass",
      freq: 1100 + Math.random() * 400,
      q: 0.7,
      dest,
    });
  }

  /**
   * Grundgeraeusch: Lueftung und Leuchtstoffroehren.
   *
   * Laeuft endlos und sehr leise. Es faellt erst auf, wenn es fehlt - dann
   * wirkt das Buero wie ein Standbild.
   */
  _startAmbience() {
    const rauschen = this.ctx.createBufferSource();
    rauschen.buffer = this.noise;
    rauschen.loop = true;

    const tief = this.ctx.createBiquadFilter();
    tief.type = "lowpass";
    tief.frequency.value = 420;

    const pegel = this.ctx.createGain();
    pegel.gain.value = 0.02;
    rauschen.connect(tief).connect(pegel).connect(this.bus);
    rauschen.start();

    // Das Brummen der Leuchten, eine Oktave unter Netzfrequenz.
    const brummen = this.ctx.createOscillator();
    brummen.type = "sine";
    brummen.frequency.value = 100;
    const brummPegel = this.ctx.createGain();
    brummPegel.gain.value = 0.006;
    brummen.connect(brummPegel).connect(this.bus);
    brummen.start();

    this._ambience = { rauschen, brummen };
  }

  // -------------------------------------------------------------------------
  // Bausteine
  // -------------------------------------------------------------------------

  /** Zwei Sekunden weisses Rauschen, von allen Klaengen geteilt. */
  _noiseBuffer(sekunden) {
    const rate = this.ctx.sampleRate;
    const buffer = this.ctx.createBuffer(1, Math.floor(rate * sekunden), rate);
    const daten = buffer.getChannelData(0);
    for (let i = 0; i < daten.length; i++) daten[i] = Math.random() * 2 - 1;
    return buffer;
  }

  /**
   * Zielknoten fuer einen Klang.
   *
   * Ohne Position geht es direkt auf den Bus - der Klang sitzt dann im Kopf.
   * Mit Position kommt ein `PannerNode` davor, der nach Ablauf wieder
   * abgeraeumt wird.
   */
  _dest(pos, standzeit) {
    if (!pos) return this.bus;

    const panner = this.ctx.createPanner();
    panner.panningModel = "HRTF";
    panner.distanceModel = "inverse";
    panner.refDistance = 3.5;
    panner.maxDistance = MAX_DISTANCE;
    panner.rolloffFactor = 1.1;
    if (panner.positionX) {
      panner.positionX.value = pos[0];
      panner.positionY.value = pos[1];
      panner.positionZ.value = pos[2];
    } else {
      panner.setPosition(pos[0], pos[1], pos[2]);
    }
    panner.connect(this.bus);
    this._panners.push({ node: panner, until: this.ctx.currentTime + standzeit + 0.3 });
    return panner;
  }

  /**
   * Huellkurve: schneller Anstieg, exponentieller Abfall.
   *
   * Exponentielle Rampen duerfen nicht auf null laufen, daher der winzige
   * Endwert.
   */
  _env(t0, dur, peak) {
    const g = this.ctx.createGain();
    g.gain.setValueAtTime(0.0001, t0);
    g.gain.exponentialRampToValueAtTime(Math.max(peak, 0.0002), t0 + 0.004);
    g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
    return g;
  }

  _tone({ type, from, to, dur, peak, dest, delay = 0 }) {
    const t0 = this.ctx.currentTime + delay;
    const osc = this.ctx.createOscillator();
    osc.type = type;
    osc.frequency.setValueAtTime(from, t0);
    if (to) osc.frequency.exponentialRampToValueAtTime(to, t0 + dur);

    const env = this._env(t0, dur, peak);
    osc.connect(env).connect(dest);
    osc.start(t0);
    osc.stop(t0 + dur + 0.02);
    osc.onended = () => env.disconnect();
  }

  _noise({ dur, peak, type, freq, freqTo, q, dest, delay = 0 }) {
    const t0 = this.ctx.currentTime + delay;
    const src = this.ctx.createBufferSource();
    src.buffer = this.noise;
    // Zufaelliger Einstieg, sonst klingt jeder Schuss identisch.
    const offset = Math.random() * (this.noise.duration - dur - 0.01);

    const filter = this.ctx.createBiquadFilter();
    filter.type = type;
    filter.frequency.setValueAtTime(freq, t0);
    if (freqTo) filter.frequency.exponentialRampToValueAtTime(freqTo, t0 + dur);
    if (q) filter.Q.value = q;

    const env = this._env(t0, dur, peak);
    src.connect(filter).connect(env).connect(dest);
    src.start(t0, Math.max(offset, 0), dur + 0.02);
    src.onended = () => {
      filter.disconnect();
      env.disconnect();
    };
  }

  dispose() {
    window.removeEventListener("keydown", this._onKey);
    if (!this.ctx) return;
    this._ambience?.rauschen.stop();
    this._ambience?.brummen.stop();
    this._ambience = null;
    for (const { node } of this._panners) node.disconnect();
    this._panners.length = 0;
    this.ctx.close();
  }
}
