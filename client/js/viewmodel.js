// Die Waffe in der eigenen Hand.
//
// Reine Darstellung: das Modell zuckt beim Schuss zurueck und wandert beim
// Nachladen aus dem Bild. Ob geschossen werden darf, entscheidet weiterhin
// allein der Server - hier wird nur gezeigt, was er gemeldet hat.

import * as THREE from "../vendor/three.module.min.js";

/** Ruhelage vor der Kamera: rechts unten, knapp ausserhalb der Zielachse. */
const REST = new THREE.Vector3(0.29, -0.29, -0.55);

/** Leichte Drehung nach innen, damit die Waffe zum Fadenkreuz zeigt. */
const REST_YAW = -0.07;

/** Rueckstoss je Schuss und wie schnell er abklingt (1/s). */
const RECOIL_PER_SHOT = { Textmarker: 0.035, Locher: 0.12 };
const RECOIL_DECAY = 12;

/** Wie weit das Modell beim Nachladen absinkt. */
const RELOAD_DROP = 0.34;

function box(w, h, d, color) {
  return new THREE.Mesh(
    new THREE.BoxGeometry(w, h, d),
    new THREE.MeshLambertMaterial({ color }),
  );
}

/** Textmarker-Pistole: ein Marker mit Griff, mehr Prototyp als Waffe. */
function buildTextmarker() {
  const group = new THREE.Group();
  const barrel = box(0.05, 0.05, 0.3, 0xf2d024);
  barrel.position.set(0, 0.02, -0.1);
  group.add(barrel);
  const tip = box(0.035, 0.035, 0.05, 0xffffff);
  tip.position.set(0, 0.02, -0.27);
  group.add(tip);
  const grip = box(0.05, 0.13, 0.06, 0x2f343b);
  grip.position.set(0, -0.06, 0.02);
  group.add(grip);
  return group;
}

/** Locher-Schrotflinte: Buerolocher, auf Lauflaenge gebracht. */
function buildLocher() {
  const group = new THREE.Group();
  const body = box(0.1, 0.09, 0.34, 0x8d939c);
  body.position.set(0, 0.0, -0.12);
  group.add(body);
  const lever = box(0.08, 0.035, 0.26, 0xc4302b);
  lever.position.set(0, 0.07, -0.14);
  lever.rotation.x = -0.12;
  group.add(lever);
  const grip = box(0.06, 0.14, 0.07, 0x2f343b);
  grip.position.set(0, -0.09, 0.03);
  group.add(grip);
  return group;
}

const BUILDERS = {
  Textmarker: buildTextmarker,
  Locher: buildLocher,
};

export class ViewModel {
  /** @param {THREE.Camera} camera Modell haengt an der Kamera und folgt ihr */
  constructor(camera) {
    this.root = new THREE.Group();
    this.root.rotation.y = REST_YAW;
    this.root.scale.setScalar(0.9);
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
    const build = BUILDERS[weaponId];
    this.current = build ? build() : null;
    if (this.current) this.root.add(this.current);
  }

  /** Ein eigener Schuss ist bestaetigt worden. */
  kick(weaponId) {
    this.recoil = Math.min(0.3, this.recoil + (RECOIL_PER_SHOT[weaponId] ?? 0.05));
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
