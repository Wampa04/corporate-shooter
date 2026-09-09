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
  Kaffeevollautomat: 0x8a5a2b,
};

/** Standzeit einer Explosionswolke in Sekunden. */
const BURST_LIFETIME = 0.4;

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
    /** Geschosse in der Luft, nach ihrer Nummer vom Server. */
    this.fliegende = new Map();

    // Einmal angelegte Geometrie fuer alle Einschlaege.
    this._impactGeometry = new THREE.SphereGeometry(0.055, 6, 4);
    this._briefGeometry = new THREE.BoxGeometry(0.19, 0.13, 0.02);
    this._briefMaterial = new THREE.MeshLambertMaterial({ color: 0xf7f5ef });
    this._burstGeometry = new THREE.SphereGeometry(1, 10, 7);
  }

  /**
   * Schickt ein Geschoss auf die Reise.
   *
   * Der Server meldet nur Abschuss und Einschlag; dazwischen fliegt der Client
   * selbst. Die Bahn ist vollkommen vorhersagbar - Ort, Geschwindigkeit und
   * Fallbeschleunigung genuegen -, und sie dreissigmal je Sekunde zu
   * verschicken waere Bandbreite fuer nichts.
   *
   * @param {object} event `Launched`-Ereignis
   * @param {number} gravity Fallbeschleunigung aus der Waffenbeschreibung
   */
  addLaunch(event, gravity) {
    const mesh = new THREE.Mesh(this._briefGeometry, this._briefMaterial);
    mesh.position.set(event.pos[0], event.pos[1], event.pos[2]);
    this.scene.add(mesh);
    this.fliegende.set(event.projectile, {
      mesh,
      vel: [event.vel[0], event.vel[1], event.vel[2]],
      gravity,
      // Notbremse: geht die Einschlagsmeldung verloren, raeumt der Client
      // spaetestens hier ab, statt einen Brief bis ans Ende der Karte fliegen
      // zu lassen.
      rest: 6.0,
    });
  }

  /** Nimmt ein Geschoss aus der Luft und zeichnet die Explosion. */
  addBurst(event) {
    this._entferneGeschoss(event.projectile);

    const mesh = new THREE.Mesh(
      this._burstGeometry,
      new THREE.MeshBasicMaterial({
        color: 0xf1e5c8,
        transparent: true,
        opacity: 0.55,
        depthWrite: false,
      }),
    );
    mesh.position.set(event.pos[0], event.pos[1], event.pos[2]);
    mesh.scale.setScalar(event.radius * 0.3);
    this.scene.add(mesh);
    if (this.impacts.length >= MAX_IMPACTS) this._retire(this.impacts, 0);
    // Gleiche Form wie ein Einschlag, damit `_advance` und `_retire` sie ohne
    // Sonderfall abraeumen. `wolke` traegt nur den Zielradius: sie waechst
    // beim Verblassen auf die Groesse, in der der Schaden gewirkt hat - so
    // sieht man, wen es getroffen haben kann.
    this.impacts.push({
      object: mesh,
      age: 0,
      life: BURST_LIFETIME,
      wolke: { von: event.radius * 0.3, bis: event.radius },
    });
  }

  _entferneGeschoss(id) {
    const eintrag = this.fliegende.get(id);
    if (!eintrag) return;
    this.scene.remove(eintrag.mesh);
    this.fliegende.delete(id);
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
    // Geschosse weiterfliegen lassen. Dieselbe Rechnung wie auf dem Server,
    // nur ohne Kollision - wo es endet, sagt der Server mit `Burst`.
    for (const [id, p] of this.fliegende) {
      p.vel[1] -= p.gravity * dt;
      p.mesh.position.x += p.vel[0] * dt;
      p.mesh.position.y += p.vel[1] * dt;
      p.mesh.position.z += p.vel[2] * dt;
      // Der Brief taumelt; ein starr fliegendes Rechteck sieht tot aus.
      p.mesh.rotation.x += dt * 6.0;
      p.mesh.rotation.y += dt * 2.5;
      p.rest -= dt;
      if (p.rest <= 0) this._entferneGeschoss(id);
    }

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
      const t = entry.age / entry.life;
      entry.object.material.opacity = startOpacity * (1 - t);
      if (entry.wolke) {
        entry.object.scale.setScalar(entry.wolke.von + (entry.wolke.bis - entry.wolke.von) * t);
      }
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
    for (const id of [...this.fliegende.keys()]) this._entferneGeschoss(id);
    this._briefGeometry.dispose();
    this._briefMaterial.dispose();
    this._burstGeometry.dispose();
    while (this.tracers.length) this._retire(this.tracers, 0);
    while (this.impacts.length) this._retire(this.impacts, 0);
    this._impactGeometry.dispose();
  }
}
