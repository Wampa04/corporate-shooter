// Aufbau der Szene aus der Levelbeschreibung des Servers.
//
// Der Client kennt den Grundriss nicht: er baut ihn aus der `MapDesc`, die mit
// der Willkommensnachricht kommt. Aendert sich die Karte serverseitig, aendert
// sie sich hier ohne eine einzige Zeile Anpassung.

import * as THREE from "../vendor/three.module.min.js";

/**
 * Die Hausfarbe.
 *
 * Ein Unternehmen, das sich eine solche Farbe gibt, bringt sie ueberall an:
 * auf Stuhlpolstern, als Band auf Fluren, als Leitstreifen auf dem Boden. Die
 * Karte verteilt sie entsprechend.
 */
const BRAND = 0x9fd356;

/**
 * Darstellung je `BrushKind`.
 *
 * `roughness` und `metalness` sind das, was Buerooberflaechen voneinander
 * unterscheidet: matter Teppich, halbglaenzender Lack, mattes Blech.
 */
const MATERIALS = {
  // Huelle
  Floor:         { color: 0x767f8b, roughness: 0.95, texture: "carpet" },
  Ceiling:       { color: 0xeef1f5, roughness: 0.9,  texture: "ceiling" },
  Wall:          { color: 0xccd3dc, roughness: 0.85 },
  Glass:         { color: 0xa8dcea, roughness: 0.05, opacity: 0.15, transparent: true },
  // Milchglasband auf Brusthoehe: verdeckt den Rumpf, laesst Kopf und Beine frei.
  FrostedGlass:  { color: 0xdfe7ec, roughness: 0.45, opacity: 0.82, transparent: true },
  Pillar:        { color: 0xd8dee6, roughness: 0.8 },

  // Arbeitsplatz
  Desk:          { color: 0xbb9061, roughness: 0.6 },   // Buche, wie ueberall
  Worktop:       { color: 0xe4e7ea, roughness: 0.3 },   // helle Platte mit Kante
  Paper:         { color: 0xf7f5ef, roughness: 0.95 },
  Mug:           { color: BRAND,    roughness: 0.25 },  // Tasse in Hausfarbe
  Cubicle:       { color: 0x8e9aa7, roughness: 1.0 },   // Stoffbespannung
  Monitor:       { color: 0x1b1f25, roughness: 0.25 },
  Keyboard:      { color: 0x2b3038, roughness: 0.6 },

  // Buerostuhl: Polster, Schale, Gestell. Anthrazit und Aluminium - die
  // Hausfarbe bleibt den Waenden, Tassen und Leitstreifen vorbehalten.
  Chair:         { color: 0x3a3f47, roughness: 0.85 },
  ChairShell:    { color: 0x1e2228, roughness: 0.5 },
  ChairFrame:    { color: 0x8b929b, roughness: 0.35, metalness: 0.55 },
  Whiteboard:    { color: 0xffffff, roughness: 0.15 },  // beschreibbar, also glatt
  Cabinet:       { color: 0xb9c1cb, roughness: 0.45, metalness: 0.35 },
  Shelf:         { color: 0x9c7852, roughness: 0.7 },
  Printer:       { color: 0xe6eaee, roughness: 0.5 },

  // Sonderraeume
  CoffeeMachine: { color: 0x3b4149, roughness: 0.35, metalness: 0.5 },
  ServerRack:    { color: 0x2a2f36, roughness: 0.5,  metalness: 0.3 },
  // Yuccapalme: Uebertopf, Rand, Erde, Stamm und zwei Gruentoene fuer die
  // Wedel - erst der Unterschied zwischen Krone und Blatt macht sie zur
  // Pflanze statt zum gruenen Kasten.
  Plant:         { color: 0xb0aa9e, roughness: 0.85 },  // Sichtbeton-Uebertopf
  PlantRim:      { color: 0xc9c3b7, roughness: 0.7 },
  Soil:          { color: 0x3b3027, roughness: 1.0 },
  Stem:          { color: 0x6b5a3e, roughness: 0.9 },
  Foliage:       { color: 0x4e9e57, roughness: 1.0 },
  FoliageDark:   { color: 0x35754a, roughness: 1.0 },

  // Dekoration
  LightPanel:    { color: 0xffffff, roughness: 1.0, emissive: 0xfff4d6 },
  Vent:          { color: 0x9aa3ad, roughness: 0.6, metalness: 0.4 },
  Trim:          { color: 0xb6bec8, roughness: 0.6 },
  AccentPanel:   { color: BRAND,    roughness: 0.75 },
  FloorStripe:   { color: BRAND,    roughness: 0.85 },
};

/** Ersatzdarstellung fuer ein `BrushKind`, das dieser Client noch nicht kennt. */
const FALLBACK = { color: 0xff00ff, roughness: 1.0 };

/**
 * Arten, deren Textur sich nach der Groesse der Flaeche richten muss.
 *
 * Bei allen anderen genuegt die Grundfarbe. Boden und Decke sind die beiden
 * groessten Flaechen im Bild - eine gestreckte Textur faellt genau dort auf.
 */
const TILED = new Set(["Floor", "Ceiling"]);

/** Kantenlaenge einer Boden- beziehungsweise Deckenplatte in Metern. */
const TILE_SIZE = 1.2;

/**
 * Arten, die kein Licht abfangen sollen: die Huelle des Raums.
 *
 * Das Licht soll aus der Decke kommen. Warfe die Huelle Schatten, laege der
 * halbe Boden im Schatten der Aussenwaende - das dunkelt alles gleichmaessig
 * ab, statt Kontur zu geben, und kostet trotzdem die volle Rechenzeit.
 * Schatten werfen soll nur, was im Raum steht.
 */
const NO_SHADOW_CAST = new Set([
  "Ceiling",
  "Wall",
  "Floor",
  "LightPanel",
  "FloorStripe",
  "AccentPanel",
  "Vent",
  "Trim",
]);

/**
 * Erzeugt eine kachelbare Textur mit Fugenraster.
 *
 * Kein Bildmaterial noetig: ein paar Linien auf einem Canvas genuegen, damit
 * grosse Flaechen nicht als einfarbige Bloecke wirken.
 */
function gridTexture({ line, noise }) {
  const size = 128;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");

  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, size, size);

  if (noise > 0) {
    // Feine Koernung, damit Teppich nicht wie lackiertes Blech aussieht.
    const image = ctx.getImageData(0, 0, size, size);
    for (let i = 0; i < image.data.length; i += 4) {
      const d = (Math.random() - 0.5) * noise;
      image.data[i] += d;
      image.data[i + 1] += d;
      image.data[i + 2] += d;
    }
    ctx.putImageData(image, 0, 0);
  }

  ctx.strokeStyle = line;
  ctx.lineWidth = 2;
  ctx.strokeRect(1, 1, size - 2, size - 2);

  const texture = new THREE.CanvasTexture(canvas);
  texture.wrapS = THREE.RepeatWrapping;
  texture.wrapT = THREE.RepeatWrapping;
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  return texture;
}

const TEXTURES = {
  carpet: () => gridTexture({ line: "rgba(0,0,0,0.13)", noise: 26 }),
  ceiling: () => gridTexture({ line: "rgba(0,0,0,0.12)", noise: 0 }),
};

function makeMaterial(spec, map) {
  return new THREE.MeshStandardMaterial({
    color: spec.color,
    roughness: spec.roughness ?? 0.8,
    metalness: spec.metalness ?? 0.0,
    emissive: spec.emissive ?? 0x000000,
    transparent: spec.transparent === true,
    opacity: spec.opacity ?? 1,
    // Ohne beidseitige Darstellung verschwinden Glaswaende, sobald man von
    // der falschen Seite kommt.
    side: spec.transparent ? THREE.DoubleSide : THREE.FrontSide,
    depthWrite: !spec.transparent,
    map: map ?? null,
  });
}

const size = (aabb, axis) => aabb.max[axis] - aabb.min[axis];
const middle = (aabb, axis) => (aabb.min[axis] + aabb.max[axis]) / 2;

/**
 * Baut Szene, Licht und Levelgeometrie.
 *
 * Gleichartige Boxen werden zu je einem `InstancedMesh` zusammengefasst: aus
 * gut zweihundert Einzelobjekten werden rund zwanzig Zeichenaufrufe. Nur Boden
 * und Decke entstehen einzeln, weil ihre Textur zur Flaechengroesse passen
 * muss.
 *
 * `shadows` schaltet den Schattenwurf ab - fuer Rechner, denen er zu viel ist.
 */
export function buildScene(map, { shadows = true } = {}) {
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0xaeb7c2);
  // Der Dunst setzt erst jenseits der Raumdiagonale spuerbar ein. Naeher
  // gesetzt wuerde er die gegenueberliegende Bueroseite ausbleichen, und
  // gerade dort steht, worauf man schiesst.
  scene.fog = new THREE.Fog(0xaeb7c2, 58, 130);

  // Buerobeleuchtung ist flach, hell und gnadenlos. Das Umgebungslicht ist
  // dabei unverzichtbar: ohne es faellt jede nach unten zeigende Flaeche -
  // allen voran die Deckenunterseite, die man staendig sieht - ins Schwarze.
  //
  // Mit Schattenwurf faellt es etwas schwaecher aus und das gerichtete Licht
  // dafuer staerker: sonst waescht die Grundhelligkeit genau die Schatten weg,
  // fuer die gerechnet wird. Zu weit herunter darf es aber nicht, sonst
  // erscheint die Hausfarbe auf abgewandten Flaechen dunkelgruen statt hell.
  scene.add(new THREE.AmbientLight(0xffffff, shadows ? 0.9 : 1.3));
  scene.add(new THREE.HemisphereLight(0xf4f8ff, 0x8d959f, shadows ? 1.0 : 1.2));

  const key = new THREE.DirectionalLight(0xffffff, shadows ? 1.5 : 0.85);
  // Licht und Ziel wandern mit der Mitte des Grundrisses. Der Anbau im Osten
  // reicht bis x = 29; bliebe das Ziel im Ursprung, fiele er aus der
  // Schattenkamera und waere als einziger Raum schattenlos.
  const mitteX = middle(map.bounds, 0);
  const mitteZ = middle(map.bounds, 2);
  key.position.set(mitteX + 7, 26, mitteZ + 5);
  key.target.position.set(mitteX, 0, mitteZ);
  scene.add(key.target);
  key.castShadow = shadows;
  key.shadow.mapSize.set(1024, 1024);
  key.shadow.bias = -0.0015;
  // Die Schattenkamera muss das ganze Stockwerk fassen, aber nicht mehr:
  // je enger sie sitzt, desto schaerfer wird der Schatten bei gleicher
  // Texturgroesse.
  // Halbe Ausdehnung des Grundrisses plus etwas Luft - so wächst die
  // Schattenkamera mit der Karte, statt bei der nächsten Erweiterung wieder
  // zu klein zu sein.
  const reach =
    Math.max(size(map.bounds, 0), size(map.bounds, 2)) * 0.5 + 6;
  Object.assign(key.shadow.camera, {
    left: -reach,
    right: reach,
    top: reach,
    bottom: -reach,
    near: 1,
    far: 70,
  });
  key.shadow.camera.updateProjectionMatrix();
  key.userData.isKeyLight = true;
  scene.add(key);

  const fill = new THREE.DirectionalLight(0xd6e2f2, 0.35);
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
    const mayCast = !NO_SHADOW_CAST.has(kind) && spec.transparent !== true;

    if (TILED.has(kind)) {
      // Einzeln, damit die Fugen ueberall gleich gross bleiben.
      for (const aabb of boxes) {
        const texture = TEXTURES[spec.texture]();
        texture.repeat.set(
          Math.max(1, Math.round(size(aabb, 0) / TILE_SIZE)),
          Math.max(1, Math.round(size(aabb, 2) / TILE_SIZE)),
        );
        const mesh = new THREE.Mesh(unitBox, makeMaterial(spec, texture));
        mesh.scale.set(size(aabb, 0), size(aabb, 1), size(aabb, 2));
        mesh.position.set(middle(aabb, 0), middle(aabb, 1), middle(aabb, 2));
        mesh.userData.mayCastShadow = mayCast;
        mesh.castShadow = shadows && mayCast;
        mesh.receiveShadow = shadows;
        mesh.frustumCulled = false;
        scene.add(mesh);
      }
      continue;
    }

    const mesh = new THREE.InstancedMesh(unitBox, makeMaterial(spec), boxes.length);
    mesh.name = kind;
    boxes.forEach((aabb, i) => {
      matrix.makeScale(size(aabb, 0), size(aabb, 1), size(aabb, 2));
      matrix.setPosition(middle(aabb, 0), middle(aabb, 1), middle(aabb, 2));
      mesh.setMatrixAt(i, matrix);
    });
    mesh.instanceMatrix.needsUpdate = true;
    mesh.userData.mayCastShadow = mayCast;
    mesh.castShadow = shadows && mayCast;
    mesh.receiveShadow = shadows;
    // Statische Geometrie: die Kamera bewegt sich, das Buero nicht.
    mesh.frustumCulled = false;
    scene.add(mesh);
  }

  return scene;
}

/**
 * Schaltet den Schattenwurf im laufenden Betrieb um.
 *
 * Wird gebraucht, weil erst zur Laufzeit feststeht, ob der Rechner ihn
 * verkraftet - und weil sich das nicht vorhersagen laesst, ohne ihn zu kennen.
 *
 * Die Beleuchtung wird mitgezogen: ohne Schatten darf die Grundhelligkeit
 * hoeher liegen, mit Schatten muss sie niedriger sein, damit die Schatten
 * ueberhaupt sichtbar bleiben.
 */
export function setShadowsEnabled(scene, renderer, on) {
  renderer.shadowMap.enabled = on;

  scene.traverse((object) => {
    if (object.isDirectionalLight && object.shadow) {
      // Nur das Hauptlicht wirft; das Aufhelllicht hat nie geworfen.
      object.castShadow = on && object.userData.isKeyLight === true;
    }
    if (object.isAmbientLight) object.intensity = on ? 0.9 : 1.3;
    if (object.isHemisphereLight) object.intensity = on ? 1.0 : 1.2;
    if (object.userData.isKeyLight) object.intensity = on ? 1.5 : 0.85;

    if (object.userData.mayCastShadow !== undefined) {
      object.castShadow = on && object.userData.mayCastShadow;
      object.receiveShadow = on;
    }
    // Three erzeugt die Shader je nach Schattenlage neu - ohne diesen Hinweis
    // bliebe der alte Shader stehen und die Umschaltung waere wirkungslos.
    if (object.material) object.material.needsUpdate = true;
  });
}
