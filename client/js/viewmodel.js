// Die Waffe in der eigenen Hand.
//
// Reine Darstellung: das Modell zuckt beim Schuss zurueck und wandert beim
// Nachladen aus dem Bild. Ob geschossen werden darf, entscheidet weiterhin
// allein der Server - hier wird nur gezeigt, was er gemeldet hat.

import * as THREE from "../vendor/three.module.min.js";
import { MAT, grip, part, ribs, sights, triggerGroup } from "./parts.js";

/** Ruhelage vor der Kamera: rechts unten, knapp ausserhalb der Zielachse. */
const REST = new THREE.Vector3(0.255, -0.235, -0.5);

/** Leichte Drehung nach innen, damit die Waffe zum Fadenkreuz zeigt. */
const REST_YAW = 0.07;

/** Wie schnell der Rueckstoss abklingt (1/s). */
const RECOIL_DECAY = 12;

/** Wie weit das Modell beim Nachladen absinkt. */
const RELOAD_DROP = 0.34;

/**
 * Textmarker-Pistole.
 *
 * Ein Textmarker, dem jemand einen Rahmen, einen Griff und ein Visier
 * verpasst hat. Die Keilspitze sitzt schraeg wie bei einem echten Marker -
 * das eine gedrehte Teil, das die ganze Silhouette traegt.
 */
function buildTextmarker() {
  const g = new THREE.Group();

  // --- Markerkoerper ---
  part(g, { w: 0.058, h: 0.058, d: 0.25, color: MAT.marker, y: 0.026, z: -0.095 });
  part(g, { w: 0.044, h: 0.044, d: 0.05, color: MAT.marker, y: 0.022, z: -0.232 });
  // Kappenring, an dem der Marker frueher aufging.
  part(g, { w: 0.064, h: 0.064, d: 0.016, color: MAT.markerCap, y: 0.026, z: -0.206 });
  // Werbebanderole in der Hausfarbe. Kein Buerogegenstand ohne.
  part(g, { w: 0.061, h: 0.061, d: 0.03, color: MAT.brand, y: 0.026, z: -0.135 });
  // Tintenfenster an der Seite.
  part(g, { w: 0.011, h: 0.026, d: 0.085, color: MAT.ink, x: 0.028, y: 0.022, z: -0.06 });
  part(g, { w: 0.011, h: 0.026, d: 0.085, color: MAT.ink, x: -0.028, y: 0.022, z: -0.06 });
  // Drei Griffrippen am Schaft.
  ribs(g, { n: 3, from: -0.03, pitch: 0.02, w: 0.056, h: 0.056, d: 0.006,
            color: MAT.markerCap, y: 0.026 });

  // --- Spitze ---
  part(g, { w: 0.04, h: 0.04, d: 0.014, color: MAT.steel, y: 0.022, z: -0.258 });
  part(g, { w: 0.032, h: 0.026, d: 0.055, color: MAT.felt, y: 0.019, z: -0.29, rx: 0.32 });

  // --- Rahmen, Griff, Abzug ---
  // Der Rahmen bleibt bewusst schmal: die Waffe ist ein Textmarker, dem
  // jemand einen Griff angeschraubt hat, und nicht umgekehrt.
  part(g, { w: 0.042, h: 0.036, d: 0.1, color: MAT.polymer, y: -0.012, z: -0.01 });
  part(g, { w: 0.048, h: 0.01, d: 0.115, color: MAT.gunmetal, y: 0.004, z: -0.02 });
  // Auswurffenster.
  part(g, { w: 0.005, h: 0.016, d: 0.04, color: MAT.black, x: 0.023, y: -0.006, z: -0.04 });
  grip(g, { y: -0.072, z: 0.026, w: 0.044, h: 0.115, d: 0.052 });
  triggerGroup(g, { y: -0.03, z: -0.012 });
  // Magazinboden schaut unten aus dem Griff.
  part(g, { w: 0.048, h: 0.008, d: 0.058, color: MAT.gunmetal, y: -0.134, z: 0.037 });

  sights(g, { y: 0.055, front: -0.185, rear: 0.036 });
  return g;
}

/**
 * Locher-Schrotflinte.
 *
 * Ein Buerolocher auf Lauflaenge gebracht: die beiden Stanzstempel sind die
 * Muendungen, der Hebel ist der Spannhebel, und der ausklappbare
 * Papieranschlag dient als Visierschiene.
 */
function buildLocher() {
  const g = new THREE.Group();

  // --- Grundplatte und Korpus ---
  part(g, { w: 0.13, h: 0.02, d: 0.17, color: MAT.steel, y: -0.052, z: -0.07 });
  part(g, { w: 0.104, h: 0.088, d: 0.3, color: MAT.steel, y: 0.004, z: -0.13 });
  // Seitliche Blechkanten.
  part(g, { w: 0.112, h: 0.014, d: 0.28, color: MAT.chrome, y: 0.045, z: -0.13 });

  // --- Doppellauf aus zwei Stanzstempeln ---
  part(g, { w: 0.08, h: 0.05, d: 0.028, color: MAT.gunmetal, y: 0.0, z: -0.276 });
  for (const x of [-0.026, 0.026]) {
    part(g, { w: 0.021, h: 0.021, d: 0.1, color: MAT.gunmetal, x, y: 0.0, z: -0.32 });
    part(g, { w: 0.025, h: 0.025, d: 0.012, color: MAT.chrome, x, y: 0.0, z: -0.366 });
    part(g, { w: 0.013, h: 0.013, d: 0.006, color: MAT.black, x, y: 0.0, z: -0.371 });
  }

  // --- Hebel mit Scharnier und geriffeltem Ende ---
  part(g, { w: 0.086, h: 0.03, d: 0.25, color: MAT.punchRed, y: 0.076, z: -0.14, rx: -0.1 });
  part(g, { w: 0.1, h: 0.02, d: 0.02, color: MAT.gunmetal, y: 0.062, z: -0.008 });
  ribs(g, { n: 3, from: -0.245, pitch: 0.016, w: 0.09, h: 0.012, d: 0.006,
            color: MAT.punchRed, y: 0.094 });
  // Feder unter dem Hebel, drei Windungen angedeutet.
  ribs(g, { n: 3, from: -0.06, pitch: 0.018, w: 0.03, h: 0.012, d: 0.008,
            color: MAT.chrome, y: 0.056 });

  // --- Konfettifenster und Papieranschlag ---
  part(g, { w: 0.008, h: 0.036, d: 0.1, color: MAT.black, x: 0.055, y: 0.0, z: -0.1 });
  part(g, { w: 0.02, h: 0.008, d: 0.15, color: MAT.brand, x: -0.062, y: -0.03, z: -0.12 });
  ribs(g, { n: 4, from: -0.18, pitch: 0.03, w: 0.024, h: 0.01, d: 0.004,
            color: MAT.chrome, x: -0.062, y: -0.024 });

  // --- Vorderschaft, Griff, Abzug ---
  part(g, { w: 0.07, h: 0.05, d: 0.11, color: MAT.polymer, y: -0.058, z: -0.2 });
  ribs(g, { n: 4, from: -0.24, pitch: 0.026, w: 0.076, h: 0.05, d: 0.008,
            color: MAT.rubber, y: -0.058 });
  grip(g, { y: -0.105, z: 0.038, w: 0.058, h: 0.155, d: 0.07 });
  triggerGroup(g, { y: -0.05, z: -0.01 });

  // --- Zwei Papierpatronen im Halter ---
  for (const z of [0.0, 0.03]) {
    part(g, { w: 0.018, h: 0.018, d: 0.026, color: MAT.paper, x: -0.062, y: 0.03, z });
    part(g, { w: 0.02, h: 0.02, d: 0.01, color: MAT.brass, x: -0.062, y: 0.03, z: z + 0.017 });
  }

  sights(g, { y: 0.064, front: -0.27, rear: -0.01 });
  return g;
}

/**
 * Passiv-aggressive E-Mail.
 *
 * Ein Briefumschlag auf einem Klemmbrett, das man wie einen Wurfarm haelt.
 * Der Umschlag sitzt vorn und fliegt beim Schuss davon - deshalb ist er ein
 * eigenes Teil, das der Rueckstoss mitnimmt.
 */
function buildEmail() {
  const g = new THREE.Group();

  // Klemmbrett als Griffplatte.
  part(g, { w: 0.13, h: 0.012, d: 0.2, color: MAT.polymer, y: -0.03, z: -0.03 });
  part(g, { w: 0.135, h: 0.008, d: 0.03, color: MAT.steel, y: -0.021, z: -0.115 });
  // Klemme.
  part(g, { w: 0.06, h: 0.014, d: 0.022, color: MAT.chrome, y: -0.014, z: -0.115 });
  grip(g, { y: -0.1, z: 0.03, tilt: 0.24 });

  // Der Umschlag, aufgerichtet, mit angedeuteter Lasche.
  part(g, { w: 0.11, h: 0.075, d: 0.006, color: MAT.paper, y: 0.012, z: -0.15, rx: 0.3 });
  part(g, { w: 0.098, h: 0.03, d: 0.004, color: 0xe4e0d4, y: 0.028, z: -0.152, rx: 0.3 });
  // Rote Dringlichkeitsmarke - der ganze Witz der Waffe.
  part(g, { w: 0.024, h: 0.016, d: 0.004, color: MAT.punchRed, x: 0.036, y: 0.031, z: -0.153, rx: 0.3 });

  // Stapel Blaetter unter der Klemme: die Waffe hat vier Schuss.
  ribs(g, { n: 3, from: -0.035, pitch: 0.006, w: 0.105, h: 0.004, d: 0.14, color: MAT.paper, z: -0.04, axis: "y" });
  return g;
}

/**
 * Kaffeevollautomat-Minigun.
 *
 * Ein Bruehkopf mit rotierendem Buendel Auslaufduesen, Wassertank obendrauf
 * und einem Manometer, das die Hitze anzeigt. Die Duesen sitzen in einer
 * eigenen Gruppe, damit sie sich beim Feuern drehen koennen.
 */
function buildKaffee() {
  const g = new THREE.Group();

  // Gehaeuse. Kurz genug, dass das Duesenbuendel davor sichtbar bleibt - in
  // der ersten Fassung war es zweihundert Millimeter lang und verdeckte die
  // Duesen vollstaendig, sodass die Waffe von hinten wie eine Kiste aussah.
  part(g, { w: 0.1, h: 0.1, d: 0.15, color: MAT.gunmetal, y: -0.01, z: 0.01 });
  part(g, { w: 0.105, h: 0.024, d: 0.15, color: MAT.chrome, y: 0.045, z: 0.01 });
  // Wassertank hinten oben, halbdurchsichtig gemeint, hier hell.
  part(g, { w: 0.07, h: 0.07, d: 0.065, color: 0x7fa8c9, y: 0.075, z: 0.055 });
  part(g, { w: 0.075, h: 0.01, d: 0.07, color: MAT.steel, y: 0.113, z: 0.055 });
  // Manometer an der Seite.
  part(g, { w: 0.01, h: 0.034, d: 0.034, color: MAT.chrome, x: 0.055, y: 0.0, z: -0.02 });
  part(g, { w: 0.004, h: 0.024, d: 0.024, color: MAT.punchRed, x: 0.061, y: 0.0, z: -0.02 });

  grip(g, { y: -0.1, z: 0.06, tilt: 0.2, h: 0.15 });
  triggerGroup(g, { y: -0.045, z: 0.03 });

  // Duesenbuendel: sechs Rohre im Kreis.
  const duesen = new THREE.Group();
  duesen.position.set(0, -0.005, -0.155);
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2;
    part(duesen, {
      w: 0.015, h: 0.015, d: 0.16, color: MAT.steel,
      x: Math.cos(a) * 0.029, y: Math.sin(a) * 0.029,
    });
  }
  part(duesen, { w: 0.028, h: 0.028, d: 0.02, color: MAT.gunmetal, z: 0.075 });
  g.add(duesen);
  g.userData.duesen = duesen;
  return g;
}

/**
 * Whiteboard.
 *
 * Keine Waffe, sondern eine Platte am Griff. Sie steht quer vor dem Gesicht -
 * das ist der Punkt: wer sie traegt, sieht schlechter und ist besser geschuetzt.
 */
function buildWhiteboard() {
  const g = new THREE.Group();

  // Die Platte, quer und schraeg, damit sie nicht das halbe Bild fuellt.
  const platte = new THREE.Group();
  platte.position.set(-0.02, 0.04, -0.16);
  platte.rotation.set(0.12, -0.34, 0.05);
  part(platte, { w: 0.46, h: 0.34, d: 0.012, color: MAT.felt });
  // Rahmen ringsum.
  part(platte, { w: 0.48, h: 0.022, d: 0.02, color: MAT.steel, y: 0.176 });
  part(platte, { w: 0.48, h: 0.022, d: 0.02, color: MAT.steel, y: -0.176 });
  part(platte, { w: 0.022, h: 0.36, d: 0.02, color: MAT.steel, x: 0.235 });
  part(platte, { w: 0.022, h: 0.36, d: 0.02, color: MAT.steel, x: -0.235 });
  // Stiftablage mit einem Marker darauf.
  part(platte, { w: 0.16, h: 0.014, d: 0.03, color: MAT.steel, y: -0.184, z: 0.016 });
  part(platte, { w: 0.09, h: 0.014, d: 0.014, color: MAT.marker, y: -0.17, z: 0.022 });
  // Ein Rest vom letzten Meeting.
  part(platte, { w: 0.16, h: 0.008, d: 0.002, color: 0x3f7fd0, x: -0.06, y: 0.06, z: -0.008 });
  part(platte, { w: 0.1, h: 0.008, d: 0.002, color: 0x3f7fd0, x: -0.09, y: 0.03, z: -0.008 });
  g.add(platte);

  // Zwei Haltegriffe, an der Unterkante statt mitten auf der Flaeche: von der
  // Kamera aus liegen sie vor der Tafel, und auf halber Hoehe sahen sie aus
  // wie zwei aufgemalte Balken.
  part(g, { w: 0.024, h: 0.075, d: 0.024, color: MAT.polymer, x: 0.075, y: -0.145, z: -0.09, rx: 0.35 });
  part(g, { w: 0.024, h: 0.075, d: 0.024, color: MAT.polymer, x: -0.115, y: -0.115, z: -0.13, rx: 0.35 });
  // Unterarm, der die Tafel haelt.
  part(g, { w: 0.055, h: 0.05, d: 0.13, color: MAT.skin, x: -0.02, y: -0.185, z: -0.02, rx: 0.2 });
  return g;
}

/**
 * Was der Server als Waffe meldet, und wie sie sich anfuehlt.
 *
 * Vorher standen Modell und Rueckstoss in zwei getrennten Tabellen - eine
 * neue Waffe musste an zwei Stellen eingetragen werden, und wer die zweite
 * vergass, bekam kommentarlos den Vorgabewert.
 */
const WEAPONS = {
  Textmarker: { build: buildTextmarker, recoil: 0.035 },
  Locher: { build: buildLocher, recoil: 0.12 },
  // `offset` ruecht sperrige Modelle ins Bild. Die Ruhelage ist auf eine
  // Pistole ausgelegt; ein Klemmbrett, ein Bruehkopf und eine halbe
  // Wandtafel haben ihre Masse woanders und ragen sonst unten aus dem Bild.
  // Bewusst als Zahl in der Tabelle und nicht im Modell versteckt: so steht
  // die Lage aller Waffen an einer Stelle nebeneinander.
  Email: { build: buildEmail, recoil: 0.06, offset: [-0.03, 0.07, 0.0] },
  // Wenig Rueckstoss je Schuss, aber sechzehn Schuss je Sekunde - in der
  // Summe zittert die Waffe dauernd.
  Kaffeevollautomat: { build: buildKaffee, recoil: 0.022, offset: [-0.05, 0.05, -0.02] },
  // Weit nach links und oben: das Whiteboard soll die Sicht wirklich
  // einschraenken, sonst waere es ein Schild ohne Preis.
  Whiteboard: { build: buildWhiteboard, recoil: 0, offset: [-0.19, 0.16, -0.02] },
};

export class ViewModel {
  /** @param {THREE.Camera} camera Modell haengt an der Kamera und folgt ihr */
  constructor(camera) {
    this.root = new THREE.Group();
    this.root.rotation.y = REST_YAW;
    // Etwas kleiner als frueher: die Modelle bestehen jetzt aus zwanzig statt
    // drei Teilen und sind entsprechend laenger, sonst schoebe sich der Griff
    // aus dem Bild.
    this.root.scale.setScalar(0.82);
    camera.add(this.root);

    this.current = null;
    this.weaponId = null;
    this.recoil = 0;
    this.drop = 0;
  }

  /** Wechselt das Modell, wenn der Server eine andere Waffe meldet. */
  setWeapon(weaponId) {
    if (weaponId === this.weaponId) return;
    this.weaponId = weaponId;

    if (this.current) {
      this.root.remove(this.current);
      this.current.traverse((o) => {
        o.geometry?.dispose();
        o.material?.dispose();
      });
    }
    const weapon = WEAPONS[weaponId];
    this.current = weapon ? weapon.build() : null;
    if (this.current) {
      const [x, y, z] = weapon.offset ?? [0, 0, 0];
      this.current.position.set(x, y, z);
      this.root.add(this.current);
    }
  }

  /** Ein eigener Schuss ist bestaetigt worden. */
  kick(weaponId) {
    this.recoil = Math.min(0.3, this.recoil + (WEAPONS[weaponId]?.recoil ?? 0.05));
  }

  update(dt, reloading) {
    this.recoil = Math.max(0, this.recoil - this.recoil * RECOIL_DECAY * dt);

    // Beim Nachladen weich absenken statt hart umschalten.
    const target = reloading ? RELOAD_DROP : 0;
    this.drop += (target - this.drop) * Math.min(1, dt * 9);

    this.root.position.set(
      REST.x,
      REST.y - this.drop,
      REST.z + this.recoil,
    );
    this.root.rotation.x = this.recoil * 2.2 - this.drop * 1.1;
    this.root.rotation.z = this.drop * 0.5;
  }

  dispose() {
    this.setWeapon(null);
    this.root.parent?.remove(this.root);
  }
}
