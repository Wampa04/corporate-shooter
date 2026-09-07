// Aufbau der Szene aus der Levelbeschreibung des Servers.
//
// Der Client kennt den Grundriss nicht: er baut ihn aus der `MapDesc`, die mit
// der Willkommensnachricht kommt. Aendert sich die Karte serverseitig, aendert
// sie sich hier ohne eine einzige Zeile Anpassung.

import * as THREE from "../vendor/three.module.min.js";

/**
 * Darstellung je `BrushKind`.
 *
 * `blocksBullets` steht hier nur, um Deckung optisch von Dekoration zu
 * unterscheiden - die Entscheidung darueber faellt der Server.
 */
const MATERIALS = {
  Floor:         { color: 0x707a86 },  // Teppichfliese, blaugrau
  Ceiling:       { color: 0xe9edf2 },  // Rasterdecke
  Wall:          { color: 0xccd3dc },
  Glass:         { color: 0xa8dcea, opacity: 0.15, transparent: true },
  Desk:          { color: 0xbb9061 },  // Buche, wie überall
  Cubicle:       { color: 0x94a0ad },
  Whiteboard:    { color: 0xffffff },
  Shelf:         { color: 0x9c7852 },
  Printer:       { color: 0xe6eaee },
  CoffeeMachine: { color: 0x3b4149 },
  ServerRack:    { color: 0x2a2f36 },
  Plant:         { color: 0x4e9e57 },
};

/** Ersatzdarstellung fuer ein `BrushKind`, das dieser Client noch nicht kennt. */
const FALLBACK = { color: 0xff00ff };

/**
 * Baut Szene, Licht und Levelgeometrie.
 *
 * Gleichartige Boxen werden zu je einem `InstancedMesh` zusammengefasst: aus
 * knapp zweihundert Einzelobjekten wird gut ein Dutzend Zeichenaufrufe.
 */
export function buildScene(map) {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0xaeb7c2);
  // Leichter Dunst ueber die volle Diagonale des Bueros: gibt Tiefe, ohne dass
  // etwas im Nichts verschwindet.
  scene.fog = new THREE.Fog(0xaeb7c2, 30, 78);

  // Buerobeleuchtung ist flach, hell und gnadenlos. Entscheidend ist das
  // Umgebungslicht: ohne es faellt jede nach unten zeigende Flaeche - allen
  // voran die Deckenunterseite, die man staendig sieht - ins Schwarze.
  scene.add(new THREE.AmbientLight(0xffffff, 1.35));
  scene.add(new THREE.HemisphereLight(0xf4f8ff, 0x8d959f, 1.5));
  const key = new THREE.DirectionalLight(0xffffff, 0.85);
  key.position.set(12, 24, 8);
  scene.add(key);
  const fill = new THREE.DirectionalLight(0xd6e2f2, 0.4);
  fill.position.set(-14, 16, -10);
  scene.add(fill);

  const byKind = new Map();
  for (const brush of map.brushes) {
    if (!byKind.has(brush.kind)) byKind.set(brush.kind, []);
    byKind.get(brush.kind).push(brush.aabb);
  }

  const unitBox = new THREE.BoxGeometry(1, 1, 1);
  const matrix = new THREE.Matrix4();

  for (const [kind, boxes] of byKind) {
    const spec = MATERIALS[kind] ?? FALLBACK;
    const material = new THREE.MeshLambertMaterial({
      color: spec.color,
      transparent: spec.transparent === true,
      opacity: spec.opacity ?? 1,
      // Ohne beidseitige Darstellung verschwinden Glaswaende, sobald man von
      // der falschen Seite kommt.
      side: spec.transparent ? THREE.DoubleSide : THREE.FrontSide,
      depthWrite: !spec.transparent,
    });

    const mesh = new THREE.InstancedMesh(unitBox, material, boxes.length);
    mesh.name = kind;
    boxes.forEach((aabb, i) => {
      const size = [
        aabb.max[0] - aabb.min[0],
        aabb.max[1] - aabb.min[1],
        aabb.max[2] - aabb.min[2],
      ];
      matrix.makeScale(size[0], size[1], size[2]);
      matrix.setPosition(
        (aabb.min[0] + aabb.max[0]) / 2,
        (aabb.min[1] + aabb.max[1]) / 2,
        (aabb.min[2] + aabb.max[2]) / 2,
      );
      mesh.setMatrixAt(i, matrix);
    });
    mesh.instanceMatrix.needsUpdate = true;
    // Statische Geometrie: die Kamera bewegt sich, das Buero nicht.
    mesh.frustumCulled = false;
    scene.add(mesh);
  }

  return scene;
}
