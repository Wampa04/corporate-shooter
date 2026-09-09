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
import { gliedmassen } from "./laufzyklus.js";

/** Farben, die nicht aus der Waffenwelt kommen. */
const HOSE = 0x2b3038;
const SCHUH = 0x101317;

// --- Masse ----------------------------------------------------------------
// Die Figur ist 1.94 m hoch. Der Server rechnet mit 1.80 m Kollisionshoehe und
// 1.62 m Augenhoehe; die Figur darf etwas darueber hinausragen, weil Kopf und
// Haar keine Kollision haben. Wichtig ist die Augenhoehe: auf 1.62 muss ein
// Kopf sein, sonst zielt man ins Leere.

const HUEFTE_Y = 0.92;
const OBERSCHENKEL = 0.44;
const UNTERSCHENKEL = 0.40;
const SCHULTER_Y = 1.52;
const OBERARM = 0.30;
const UNTERARM = 0.28;

/** Ein Bein, aufgehaengt an der Huefte. Gibt Ober- und Unterschenkel zurueck. */
function bein(eltern, seite) {
  const oben = new THREE.Group();
  oben.position.set(seite * 0.115, HUEFTE_Y, 0);
  part(oben, { w: 0.17, h: OBERSCHENKEL, d: 0.19, color: HOSE, y: -OBERSCHENKEL / 2 });

  const unten = new THREE.Group();
  unten.position.y = -OBERSCHENKEL;
  part(unten, { w: 0.145, h: UNTERSCHENKEL, d: 0.16, color: HOSE, y: -UNTERSCHENKEL / 2 });
  // Der Schuh steht etwas nach vorn ueber, wie ein Schuh das tut.
  part(unten, {
    w: 0.155, h: 0.075, d: 0.27,
    color: SCHUH,
    y: -UNTERSCHENKEL - 0.037,
    z: -0.045,
  });
  oben.add(unten);

  eltern.add(oben);
  return { oben, unten };
}

/** Ein Arm, aufgehaengt an der Schulter. */
function arm(eltern, seite, farbe) {
  const oben = new THREE.Group();
  oben.position.set(seite * 0.31, SCHULTER_Y, 0);
  // Leicht nach aussen, sonst steckt der Arm im Rumpf.
  oben.rotation.z = seite * 0.07;
  part(oben, { w: 0.13, h: OBERARM, d: 0.145, color: farbe, y: -OBERARM / 2 });

  const unten = new THREE.Group();
  unten.position.y = -OBERARM;
  part(unten, { w: 0.12, h: UNTERARM, d: 0.135, color: farbe, y: -UNTERARM / 2 });
  part(unten, { w: 0.115, h: 0.11, d: 0.13, color: MAT.skin, y: -UNTERARM - 0.055 });
  oben.add(unten);

  eltern.add(oben);
  return { oben, unten };
}

/**
 * Baut eine Figur in Teamfarbe.
 *
 * @returns {{wurzel: THREE.Group, koerper: THREE.Group, glieder: object}}
 *   `wurzel` traegt Position und Blickrichtung, `koerper` das Wippen und das
 *   Umfallen - so bleibt die Drehung um die Hochachse frei von beidem.
 */
export function baueFigur(teamfarbe) {
  const wurzel = new THREE.Group();
  const koerper = new THREE.Group();
  wurzel.add(koerper);

  const beine = {
    links: bein(koerper, 1),
    rechts: bein(koerper, -1),
  };

  // Huefte und Rumpf. Der Rumpf traegt die Teamfarbe: er ist die groesste
  // Flaeche und aus jeder Richtung zu sehen.
  part(koerper, { w: 0.42, h: 0.16, d: 0.26, color: HOSE, y: HUEFTE_Y + 0.06 });
  part(koerper, { w: 0.50, h: 0.47, d: 0.29, color: teamfarbe, y: 1.30 });
  part(koerper, { w: 0.60, h: 0.11, d: 0.29, color: teamfarbe, y: 1.51 });

  const arme = {
    links: arm(koerper, 1, teamfarbe),
    rechts: arm(koerper, -1, teamfarbe),
  };

  // Hals und Kopf haengen in einer eigenen Gruppe: nur sie folgt dem Blick
  // nach oben und unten. Auf die ganze Figur angewandt kippte sie nach vorn.
  const kopf = new THREE.Group();
  kopf.position.y = 1.60;
  part(kopf, { w: 0.12, h: 0.09, d: 0.12, color: MAT.skin, y: 0.045 });
  part(kopf, { w: 0.255, h: 0.27, d: 0.26, color: MAT.skin, y: 0.235 });
  // Haaransatz - ohne ihn ist von hinten nicht zu sehen, wohin jemand schaut.
  part(kopf, { w: 0.265, h: 0.075, d: 0.27, color: 0x3a2f28, y: 0.355 });
  // Brille als Blickrichtung, wie bisher das Sichtschild.
  part(kopf, { w: 0.22, h: 0.055, d: 0.045, color: MAT.black, y: 0.245, z: -0.135 });
  koerper.add(kopf);

  return { wurzel, koerper, glieder: { beine, arme, kopf } };
}

/**
 * Setzt die Figur auf einen Stand des Laufzyklus.
 *
 * @param {object} glieder aus [`baueFigur`]
 * @param {THREE.Group} koerper aus [`baueFigur`]
 * @param {{phase: number, ausschlag: number}} zustand
 * @param {number} pitch Blickneigung in Radiant
 */
export function stelleFigur(glieder, koerper, zustand, pitch) {
  const w = gliedmassen(zustand);

  glieder.beine.links.oben.rotation.x = w.beinLinks;
  glieder.beine.rechts.oben.rotation.x = w.beinRechts;
  glieder.beine.links.unten.rotation.x = w.knieLinks;
  glieder.beine.rechts.unten.rotation.x = w.knieRechts;

  glieder.arme.links.oben.rotation.x = w.armLinks;
  glieder.arme.rechts.oben.rotation.x = w.armRechts;
  // Der Unterarm bleibt leicht angewinkelt, sonst haengen die Arme wie Bretter.
  glieder.arme.links.unten.rotation.x = 0.25 + Math.abs(w.armLinks) * 0.5;
  glieder.arme.rechts.unten.rotation.x = 0.25 + Math.abs(w.armRechts) * 0.5;

  // Der Kopf folgt dem Blick, aber nur zur Haelfte und gedeckelt: der Server
  // laesst +-89 Grad zu, und ein Hals, der so weit kippt, sieht gebrochen aus.
  glieder.kopf.rotation.x = Math.max(-0.6, Math.min(0.6, pitch * 0.5));

  koerper.position.y = w.wippen;
  // Die Sturzpose vollstaendig zuruecknehmen - auch die Schulterdrehung, sonst
  // steht die Figur nach dem Wiedereinstieg mit abgespreizten Armen da.
  koerper.rotation.x = 0;
  glieder.arme.links.oben.rotation.z = 0.07;
  glieder.arme.rechts.oben.rotation.z = -0.07;
}

/**
 * Legt die Figur hin. Kein Ragdoll - eine Drehung, ein Absenken, eine Pose.
 *
 * Rueckwaerts, nicht vorwaerts: baeuchlings verschwinden Beine und Arme hinter
 * dem Rumpf, und von vorn ist nur noch ein Haufen Kisten zu sehen. Auf dem
 * Ruecken bleibt die Silhouette einer Person erkennbar - und darum geht es,
 * denn eine Leiche ist eine Nachricht: hier war eben jemand.
 */
export function legeFigurHin(glieder, koerper) {
  koerper.rotation.x = Math.PI / 2;
  // Nach dem Kippen liegt die Figur mit der Mitte auf Hoehe null; das
  // Anheben bringt sie um ihre halbe Dicke ueber den Boden.
  koerper.position.y = 0.16;
  glieder.kopf.rotation.x = -0.35;
  for (const seite of ["links", "rechts"]) {
    // Beine leicht angewinkelt und gespreizt, Arme vom Koerper weg: sonst
    // liegt da ein Brett.
    glieder.beine[seite].oben.rotation.x = -0.22;
    glieder.beine[seite].unten.rotation.x = 0.45;
    glieder.arme[seite].oben.rotation.x = 0.55;
    glieder.arme[seite].unten.rotation.x = 0.35;
  }
  glieder.arme.links.oben.rotation.z = 0.5;
  glieder.arme.rechts.oben.rotation.z = -0.5;
}
