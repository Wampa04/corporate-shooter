// Bauteile fuer Modelle aus Boxen.
//
// Waffe und Spielerfigur bestanden bisher aus handgeschriebenen
// `new THREE.Mesh(new THREE.BoxGeometry(...))`-Zeilen, in `viewmodel.js` und
// `players.js` getrennt voneinander. Hier steht das gemeinsame Vokabular:
// Farben mit Namen, ein Teil mit Lage und Drehung, und die Baugruppen, die
// sich an jeder Waffe wiederholen.
//
// Bewusst ohne Material-Cache: `ViewModel.setWeapon` entsorgt beim Wechsel
// Geometrie und Material der alten Waffe. Wuerden sich zwei Waffen ein
// Material teilen, waere die zweite nach dem ersten Wechsel schwarz. Vierzig
// kleine Boxen je Modell kosten nichts.

import * as THREE from "../vendor/three.module.min.js";

/** Farben, die im Buero vorkommen. */
export const MAT = {
  polymer: 0x2f343b,   // Griffschalen, Rahmen
  gunmetal: 0x4a5058,  // brueniertes Metall
  steel: 0x9aa3ad,     // Blech
  chrome: 0xc9d0d8,    // blanke Teile
  black: 0x14171c,
  rubber: 0x1c1f24,
  brass: 0xc9a227,
  brand: 0x9fd356,     // die Hausfarbe, auch auf dem Werbekugelschreiber
  marker: 0xf2d024,    // Textmarker-Gelb
  markerCap: 0xd8a900,
  ink: 0x7a5c00,
  felt: 0xfdfdfa,
  punchRed: 0xc4302b,
  paper: 0xf7f5ef,
  skin: 0xd9b48f,
};

/** Eine Box. Der einzige Baustein, aus dem hier alles besteht. */
export function box(w, h, d, color) {
  return new THREE.Mesh(
    new THREE.BoxGeometry(w, h, d),
    new THREE.MeshLambertMaterial({ color }),
  );
}

/**
 * Ein Teil an seinen Platz setzen.
 *
 * Kuerzt die drei Zeilen ab, die sonst jedes Mal dastehen: bauen, verschieben,
 * anhaengen.
 */
export function part(group, { w, h, d, color, x = 0, y = 0, z = 0, rx = 0, ry = 0, rz = 0 }) {
  const mesh = box(w, h, d, color);
  mesh.position.set(x, y, z);
  mesh.rotation.set(rx, ry, rz);
  group.add(mesh);
  return mesh;
}

/** Eine Reihe schmaler Rippen: Griffriffelung, Pumpvorderschaft, Feder. */
export function ribs(group, { n, from, pitch, w, h, d, color, x = 0, y = 0, axis = "z" }) {
  for (let i = 0; i < n; i++) {
    const at = from + i * pitch;
    part(group, {
      w, h, d, color, x: axis === "x" ? at : x,
      y: axis === "y" ? at : y,
      z: axis === "z" ? at : 0,
    });
  }
}

/**
 * Griff mit Riffelung und Abschlusskappe.
 *
 * Beide Waffen haben einen, und beide hatten ihn bisher als eigene, um
 * Hundertstel abweichende Box.
 */
export function grip(group, { x = 0, y = -0.075, z = 0.02, w = 0.052, h = 0.145, d = 0.062,
                              tilt = 0.18, color = MAT.polymer } = {}) {
  const g = new THREE.Group();
  g.position.set(x, y, z);
  g.rotation.x = tilt;
  part(g, { w, h, d, color });
  // Riffelung an der Vorderkante, wo der Daumen liegt.
  ribs(g, {
    n: 4, from: -d * 0.5 + 0.012, pitch: 0.014,
    w: w + 0.004, h: 0.016, d: 0.006, color: MAT.rubber, y: 0.005,
  });
  // Abschlusskappe unten.
  part(g, { w: w + 0.006, h: 0.012, d: d + 0.006, color: MAT.gunmetal, y: -h * 0.5 - 0.004 });
  group.add(g);
  return g;
}

/** Abzug und Abzugsbuegel. */
export function triggerGroup(group, { x = 0, y = -0.03, z = -0.02, color = MAT.polymer } = {}) {
  part(group, { w: 0.012, h: 0.032, d: 0.01, color: MAT.gunmetal, x, y: y - 0.008, z });
  // Buegel: vorn, unten, hinten.
  part(group, { w: 0.04, h: 0.008, d: 0.008, color, x, y: y + 0.012, z: z - 0.035 });
  part(group, { w: 0.04, h: 0.008, d: 0.06, color, x, y: y - 0.028, z: z - 0.006 });
}

/** Kimme und Korn. */
export function sights(group, { y = 0.05, front = -0.24, rear = 0.02, color = MAT.black } = {}) {
  part(group, { w: 0.008, h: 0.014, d: 0.008, color, y, z: front });
  part(group, { w: 0.03, h: 0.012, d: 0.008, color, y, z: rear });
  part(group, { w: 0.008, h: 0.014, d: 0.008, color: MAT.gunmetal, y: y + 0.002, x: -0.011, z: rear });
  part(group, { w: 0.008, h: 0.014, d: 0.008, color: MAT.gunmetal, y: y + 0.002, x: 0.011, z: rear });
}
