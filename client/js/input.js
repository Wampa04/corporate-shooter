// Tastatur und Maus.
//
// Hier entsteht ausschliesslich der `InputFrame`, den der Server auswertet.
// Die einzige Groesse, die der Client selbst fuehrt, ist die Blickrichtung -
// sie muss ohne Wartezeit auf den Server reagieren, sonst fuehlt sich das
// Umsehen zaeh an. Der Server begrenzt sie ohnehin noch einmal.

import { BUTTON } from "./net.js";

/** Radiant pro Pixel Mausbewegung. */
const SENSITIVITY = 0.0022;

/** Grenze der Blickneigung, etwas unter dem Serverwert von 1.55. */
const MAX_PITCH = 1.54;

const MOVEMENT_KEYS = {
  KeyW: ["move_z", 1],
  KeyS: ["move_z", -1],
  KeyD: ["move_x", 1],
  KeyA: ["move_x", -1],
};

const BUTTON_KEYS = {
  Space:      BUTTON.JUMP,
  ShiftLeft:  BUTTON.DASH,
  ShiftRight: BUTTON.DASH,
  KeyE:       BUTTON.HEAL,
  KeyR:       BUTTON.RELOAD,
};

export class InputController {
  /**
   * @param {HTMLCanvasElement} canvas Element, das den Mauszeiger einfaengt
   * @param {Array} weapons Waffenliste aus der Serverkonfiguration
   */
  constructor(canvas, weapons) {
    this.canvas = canvas;
    this.yaw = 0;
    this.pitch = 0;
    this.locked = false;
    this.scoreboardVisible = false;
    /** 0 bedeutet "keine Aenderung", bis zum ersten Tastendruck. */
    this.weaponSlot = 0;

    this._seq = 0;
    this._keys = new Set();
    this._mouseDown = false;
    // Tasten, die seit dem letzten gesendeten Frame gedrueckt wurden.
    //
    // Eingaben werden mit der Tickrate abgetastet (30 Hz), Tastendruecke
    // dauern aber oft kuerzer als 33 ms. Ohne dieses Gedaechtnis fiele ein
    // schneller Druck auf Nachladen, Sprung oder Wellness-Tag komplett unter
    // den Tisch. Gehaltene Tasten decken sich damit, kurze nicht.
    this._latched = 0;
    this._slots = new Set(weapons.map((w) => w.slot));

    this.onLockChange = () => {};

    this._bind();
  }

  /** Setzt die Blickrichtung, etwa nach einem Respawn an anderer Stelle. */
  setLook(yaw, pitch) {
    this.yaw = yaw;
    this.pitch = pitch;
  }

  requestLock() {
    this.canvas.requestPointerLock?.();
  }

  _bind() {
    document.addEventListener("pointerlockchange", () => {
      this.locked = document.pointerLockElement === this.canvas;
      if (!this.locked) {
        // Beim Verlassen alles loslassen, sonst laeuft die Figur weiter, waehrend
        // man in einem anderen Fenster ist.
        this._keys.clear();
        this._mouseDown = false;
        this._latched = 0;
      }
      this.onLockChange(this.locked);
    });

    document.addEventListener("mousemove", (event) => {
      if (!this.locked) return;
      // Nach rechts ziehen dreht nach rechts, also im Uhrzeigersinn - und das
      // heisst bei dieser Yaw-Konvention: kleiner werdender Winkel.
      this.yaw -= event.movementX * SENSITIVITY;
      this.pitch -= event.movementY * SENSITIVITY;
      this.pitch = Math.max(-MAX_PITCH, Math.min(MAX_PITCH, this.pitch));
    });

    this.canvas.addEventListener("mousedown", (event) => {
      if (event.button === 0) {
        this._mouseDown = true;
        this._latched |= BUTTON.FIRE;
      }
    });
    document.addEventListener("mouseup", (event) => {
      if (event.button === 0) this._mouseDown = false;
    });
    // Rechtsklick oeffnet sonst mitten im Feuergefecht das Kontextmenue.
    this.canvas.addEventListener("contextmenu", (event) => event.preventDefault());

    document.addEventListener("keydown", (event) => {
      if (event.repeat) return;
      if (event.code === "Tab") {
        event.preventDefault();
        this.scoreboardVisible = true;
        return;
      }
      const digit = /^Digit([1-9])$/.exec(event.code);
      if (digit) {
        const slot = Number(digit[1]);
        if (this._slots.has(slot)) this.weaponSlot = slot;
        return;
      }
      if (event.code === "Space") event.preventDefault();
      this._keys.add(event.code);
      const button = BUTTON_KEYS[event.code];
      if (button) this._latched |= button;
    });

    document.addEventListener("keyup", (event) => {
      if (event.code === "Tab") {
        event.preventDefault();
        this.scoreboardVisible = false;
        return;
      }
      this._keys.delete(event.code);
    });

    // Fensterwechsel: Tasten koennen sonst "haengen" bleiben.
    window.addEventListener("blur", () => {
      this._keys.clear();
      this._mouseDown = false;
      this._latched = 0;
    });
  }

  /** Baut den naechsten `InputFrame`. */
  nextFrame() {
    let move_x = 0;
    let move_z = 0;
    let buttons = 0;

    if (this.locked) {
      for (const code of this._keys) {
        const movement = MOVEMENT_KEYS[code];
        if (movement) {
          if (movement[0] === "move_x") move_x += movement[1];
          else move_z += movement[1];
        }
        const button = BUTTON_KEYS[code];
        if (button) buttons |= button;
      }
      if (this._mouseDown) buttons |= BUTTON.FIRE;
      buttons |= this._latched;
    }
    // Auch bei nicht gefangener Maus zuruecksetzen, sonst wird ein alter
    // Druck nachtraeglich wirksam, sobald die Maus wieder gefangen wird.
    this._latched = 0;

    return {
      seq: ++this._seq,
      move_x: Math.max(-1, Math.min(1, move_x)),
      move_z: Math.max(-1, Math.min(1, move_z)),
      yaw: this.yaw,
      pitch: this.pitch,
      buttons,
      weapon_slot: this.weaponSlot,
    };
  }
}
