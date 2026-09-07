// Sichtbare Rueckmeldung zu Schuessen.
//
// Der Server schickt zu jedem Schuss die tatsaechlichen Projektilbahnen mit.
// Der Client zeichnet genau diese Bahnen - er berechnet nichts nach und kann
// deshalb auch nicht etwas anderes anzeigen, als serverseitig passiert ist.

import * as THREE from "../vendor/three.module.min.js";

/** Farbe der Leuchtspur je Waffe. */
const TRACER_COLOR = {
  Textmarker: 0xf5e663,
  Locher:     0xdfe6f0,
};

/** Standzeit einer Leuchtspur in Sekunden. */
const TRACER_LIFETIME = 0.09;

/** Standzeit eines Einschlags in Sekunden. */
const IMPACT_LIFETIME = 0.22;

/**
 * Obergrenze gleichzeitiger Leuchtspuren. Eine Locher-Salve erzeugt acht auf
 * einmal; bei acht Spielenden mit Dauerfeuer bleibt hier noch Luft.
 */
const MAX_TRACERS = 160;
const MAX_IMPACTS = 80;

export class Effects {
  constructor(scene) {
    this.scene = scene;
    this.tracers = [];
    this.impacts = [];

    // Einmal angelegte Geometrie fuer alle Einschlaege.
    this._impactGeometry = new THREE.SphereGeometry(0.055, 6, 4);
  }

  /** Zeichnet die Bahnen eines `Shot`-Ereignisses. */
  addShot(event) {
    const color = TRACER_COLOR[event.weapon] ?? 0xffffff;
    for (const tracer of event.tracers) {
      this._addTracer(tracer.from, tracer.to, color);
      if (!tracer.hit_player) this._addImpact(tracer.to, color);
    }
  }

  /** Zeichnet einen Treffer an einem Spieler. */
  addHit(event) {
    this._addImpact(event.pos, 0xe8503a);
  }

  _addTracer(from, to, color) {
    if (this.tracers.length >= MAX_TRACERS) this._retire(this.tracers, 0);

    const geometry = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(from[0], from[1], from[2]),
      new THREE.Vector3(to[0], to[1], to[2]),
    ]);
    const material = new THREE.LineBasicMaterial({
      color,
      transparent: true,
      opacity: 0.95,
      depthWrite: false,
    });
    const line = new THREE.Line(geometry, material);
    line.frustumCulled = false;
    this.scene.add(line);
    this.tracers.push({ object: line, age: 0, life: TRACER_LIFETIME });
  }

  _addImpact(pos, color) {
    if (this.impacts.length >= MAX_IMPACTS) this._retire(this.impacts, 0);

    const material = new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity: 0.9,
      depthWrite: false,
    });
    const dot = new THREE.Mesh(this._impactGeometry, material);
    dot.position.set(pos[0], pos[1], pos[2]);
    this.scene.add(dot);
    this.impacts.push({ object: dot, age: 0, life: IMPACT_LIFETIME });
  }

  /** Blendet alle Effekte aus und raeumt abgelaufene weg. */
  update(dt) {
    this._advance(this.tracers, dt, 0.95);
    this._advance(this.impacts, dt, 0.9);
  }

  _advance(list, dt, startOpacity) {
    for (let i = list.length - 1; i >= 0; i--) {
      const entry = list[i];
      entry.age += dt;
      if (entry.age >= entry.life) {
        this._retire(list, i);
        continue;
      }
      entry.object.material.opacity = startOpacity * (1 - entry.age / entry.life);
    }
  }

  _retire(list, index) {
    const [entry] = list.splice(index, 1);
    this.scene.remove(entry.object);
    entry.object.material.dispose();
    // Die Geometrie der Einschlaege ist geteilt und wird nicht mitentsorgt.
    if (entry.object.isLine) entry.object.geometry.dispose();
  }

  dispose() {
    while (this.tracers.length) this._retire(this.tracers, 0);
    while (this.impacts.length) this._retire(this.impacts, 0);
    this._impactGeometry.dispose();
  }
}
