// Die Spielerfigur.
//
// Vorher waren Mitspieler vier Kisten: ein Rumpf, ein dunkler Block als Beine,
// ein Wuerfel als Kopf, ein Schild als Blickrichtung. Das reichte, um zu sehen,
// *dass* dort jemand steht - nicht, was er tut.
//
// Gebaut aus denselben Bauteilen wie die Waffen (`parts.js`), aber als
// Hierarchie statt als Stapel: Huefte, Ober-, Unterschenkel, Schulter, Ober-,
// Unterarm haengen ineinander. Erst dadurch laesst sich ein Glied als Ganzes
// drehen, statt jede Box einzeln nachzurechnen.

import * as THREE from "../vendor/three.module.min.js";
import { MAT, part } from "./parts.js";
import { limbPose } from "./walk-cycle.js";

/** Farben, die nicht aus der Waffenwelt kommen. */
const HOSE = 0x2b3038;
const SHOE = 0x101317;

// --- Masse ----------------------------------------------------------------
// Die Figur ist 1.94 m hoch. Der Server rechnet mit 1.80 m Kollisionshoehe und
// 1.62 m Augenhoehe; die Figur darf etwas darueber hinausragen, weil Kopf und
// Haar keine Kollision haben. Wichtig ist die Augenhoehe: auf 1.62 muss ein
// Kopf sein, sonst zielt man ins Leere.

const HIP_Y = 0.92;
const THIGH = 0.44;
const SHIN = 0.40;
const SHOULDER_Y = 1.52;
const UPPER_ARM = 0.30;
const FOREARM = 0.28;

/** Ein Bein, aufgehaengt an der Huefte. Gibt Ober- und Unterschenkel zurueck. */
function leg(parent, side) {
  const top = new THREE.Group();
  top.position.set(side * 0.115, HIP_Y, 0);
  part(top, { w: 0.17, h: THIGH, d: 0.19, color: HOSE, y: -THIGH / 2 });

  const bottom = new THREE.Group();
  bottom.position.y = -THIGH;
  part(bottom, { w: 0.145, h: SHIN, d: 0.16, color: HOSE, y: -SHIN / 2 });
  // Der Schuh steht etwas nach vorn ueber, wie ein Schuh das tut.
  part(bottom, {
    w: 0.155, h: 0.075, d: 0.27,
    color: SHOE,
    y: -SHIN - 0.037,
    z: -0.045,
  });
  top.add(bottom);

  parent.add(top);
  return { top, bottom };
}

/** Ein Arm, aufgehaengt an der Schulter. */
function arm(parent, side, tint) {
  const top = new THREE.Group();
  top.position.set(side * 0.31, SHOULDER_Y, 0);
  // Leicht nach aussen, sonst steckt der Arm im Rumpf.
  top.rotation.z = side * 0.07;
  part(top, { w: 0.13, h: UPPER_ARM, d: 0.145, color: tint, y: -UPPER_ARM / 2 });

  const bottom = new THREE.Group();
  bottom.position.y = -UPPER_ARM;
  part(bottom, { w: 0.12, h: FOREARM, d: 0.135, color: tint, y: -FOREARM / 2 });
  part(bottom, { w: 0.115, h: 0.11, d: 0.13, color: MAT.skin, y: -FOREARM - 0.055 });
  top.add(bottom);

  parent.add(top);
  return { top, bottom };
}

/**
 * Baut eine Figur in Teamfarbe.
 *
 * @returns {{root: THREE.Group, torso: THREE.Group, limbs: object}}
 *   `root` traegt Position und Blickrichtung, `torso` das Wippen und das
 *   Umfallen - so bleibt die Drehung um die Hochachse frei von beidem.
 */
export function buildFigure(teamColor) {
  const root = new THREE.Group();
  const torso = new THREE.Group();
  root.add(torso);

  const legs = {
    left: leg(torso, 1),
    right: leg(torso, -1),
  };

  // Huefte und Rumpf. Der Rumpf traegt die Teamfarbe: er ist die groesste
  // Flaeche und aus jeder Richtung zu sehen.
  part(torso, { w: 0.42, h: 0.16, d: 0.26, color: HOSE, y: HIP_Y + 0.06 });
  part(torso, { w: 0.50, h: 0.47, d: 0.29, color: teamColor, y: 1.30 });
  part(torso, { w: 0.60, h: 0.11, d: 0.29, color: teamColor, y: 1.51 });

  const arms = {
    left: arm(torso, 1, teamColor),
    right: arm(torso, -1, teamColor),
  };

  // Hals und Kopf haengen in einer eigenen Gruppe: nur sie folgt dem Blick
  // nach oben und unten. Auf die ganze Figur angewandt kippte sie nach vorn.
  const head = new THREE.Group();
  head.position.y = 1.60;
  part(head, { w: 0.12, h: 0.09, d: 0.12, color: MAT.skin, y: 0.045 });
  part(head, { w: 0.255, h: 0.27, d: 0.26, color: MAT.skin, y: 0.235 });
  // Haaransatz - ohne ihn ist von hinten nicht zu sehen, wohin jemand schaut.
  part(head, { w: 0.265, h: 0.075, d: 0.27, color: 0x3a2f28, y: 0.355 });
  // Brille als Blickrichtung, wie bisher das Sichtschild.
  part(head, { w: 0.22, h: 0.055, d: 0.045, color: MAT.black, y: 0.245, z: -0.135 });
  torso.add(head);

  return { root, torso, limbs: { legs, arms, head } };
}

/**
 * Setzt die Figur auf einen Stand des Laufzyklus.
 *
 * @param {object} limbs aus [`buildFigure`]
 * @param {THREE.Group} torso aus [`buildFigure`]
 * @param {{phase: number, amplitude: number}} state
 * @param {number} pitch Blickneigung in Radiant
 */
export function poseFigure(limbs, torso, state, pitch) {
  const w = limbPose(state);

  limbs.legs.left.top.rotation.x = w.legLeft;
  limbs.legs.right.top.rotation.x = w.legRight;
  limbs.legs.left.bottom.rotation.x = w.kneeLeft;
  limbs.legs.right.bottom.rotation.x = w.kneeRight;

  limbs.arms.left.top.rotation.x = w.armLeft;
  limbs.arms.right.top.rotation.x = w.armRight;
  // Der Unterarm bleibt leicht angewinkelt, sonst haengen die Arme wie Bretter.
  limbs.arms.left.bottom.rotation.x = 0.25 + Math.abs(w.armLeft) * 0.5;
  limbs.arms.right.bottom.rotation.x = 0.25 + Math.abs(w.armRight) * 0.5;

  // Der Kopf folgt dem Blick, aber nur zur Haelfte und gedeckelt: der Server
  // laesst +-89 Grad zu, und ein Hals, der so weit kippt, sieht gebrochen aus.
  limbs.head.rotation.x = Math.max(-0.6, Math.min(0.6, pitch * 0.5));

  torso.position.y = w.bob;
  // Die Sturzpose vollstaendig zuruecknehmen - auch die Schulterdrehung, sonst
  // steht die Figur nach dem Wiedereinstieg mit abgespreizten Armen da.
  torso.rotation.x = 0;
  limbs.arms.left.top.rotation.z = 0.07;
  limbs.arms.right.top.rotation.z = -0.07;
}

/**
 * Legt die Figur hin. Kein Ragdoll - eine Drehung, ein Absenken, eine Pose.
 *
 * Rueckwaerts, nicht vorwaerts: baeuchlings verschwinden Beine und Arme hinter
 * dem Rumpf, und von vorn ist nur noch ein Haufen Kisten zu sehen. Auf dem
 * Ruecken bleibt die Silhouette einer Person erkennbar - und darum geht es,
 * denn eine Leiche ist eine Nachricht: hier war eben jemand.
 */
export function layFigureDown(limbs, torso) {
  torso.rotation.x = Math.PI / 2;
  // Nach dem Kippen liegt die Figur mit der Mitte auf Hoehe null; das
  // Anheben bringt sie um ihre halbe Dicke ueber den Boden.
  torso.position.y = 0.16;
  limbs.head.rotation.x = -0.35;
  for (const side of ["links", "rechts"]) {
    // Beine leicht angewinkelt und gespreizt, Arme vom Koerper weg: sonst
    // liegt da ein Brett.
    limbs.legs[side].top.rotation.x = -0.22;
    limbs.legs[side].bottom.rotation.x = 0.45;
    limbs.arms[side].top.rotation.x = 0.55;
    limbs.arms[side].bottom.rotation.x = 0.35;
  }
  limbs.arms.left.top.rotation.z = 0.5;
  limbs.arms.right.top.rotation.z = -0.5;
}
