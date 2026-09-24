// Sichtbare Rueckmeldung zu Schuessen.
//
// Der Server schickt zu jedem Schuss die tatsaechlichen Projektilbahnen mit.
// Der Client zeichnet genau diese Bahnen - er berechnet nichts nach und kann
// deshalb auch nicht etwas anderes anzeigen, als serverseitig passiert ist.

import * as THREE from "../vendor/three.module.min.js";

/** Farbe der Leuchtspur je Waffe. */
const TRACER_COLOR = {
  Highlighter: 0xf5e663,
  HolePunch: 0xdfe6f0,
  CoffeeMachine: 0x8a5a2b,
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
const MAX_BURSTS = 16;

/**
 * Feste Menge vorab angelegter Effekte, die reihum wiederverwendet werden.
 *
 * Vorher entstanden je Schuss eine neue Linie samt Geometrie und Material und
 * je Einschlag ein neues Material - beim Kaffeevollautomaten sechzehnmal je
 * Sekunde und Spieler, und jedes davon wollte kurz darauf wieder entsorgt
 * werden. Jetzt wird nur umgefaerbt und verschoben.
 *
 * Reihum heisst: ist alles belegt, trifft es den aeltesten Eintrag. Das ist
 * dieselbe Regel wie vorher mit den Obergrenzen, nur ohne Buchfuehrung.
 */
class Pool {
  constructor(scene, size, create) {
    this.entries = Array.from({ length: size }, () => {
      const object = create();
      object.visible = false;
      object.frustumCulled = false;
      scene.add(object);
      return { object, age: 0, life: 0, active: false, cloud: null };
    });
    this.next = 0;
  }

  take(life) {
    const entry = this.entries[this.next];
    this.next = (this.next + 1) % this.entries.length;
    entry.age = 0;
    entry.life = life;
    entry.active = true;
    entry.cloud = null;
    entry.object.visible = true;
    return entry;
  }

  /** Blendet aus, was noch laeuft, und gibt Abgelaufenes frei. */
  advance(dt, startOpacity) {
    for (const entry of this.entries) {
      if (!entry.active) continue;
      entry.age += dt;
      if (entry.age >= entry.life) {
        entry.active = false;
        entry.object.visible = false;
        continue;
      }
      const t = entry.age / entry.life;
      entry.object.material.opacity = startOpacity * (1 - t);
      if (entry.cloud) {
        entry.object.scale.setScalar(entry.cloud.from + (entry.cloud.to - entry.cloud.from) * t);
      }
    }
  }

  dispose(scene) {
    for (const { object } of this.entries) {
      scene.remove(object);
      object.material.dispose();
    }
  }
}

const fadingMaterial = (Material, opacity) =>
  new Material({ color: 0xffffff, transparent: true, opacity, depthWrite: false });

export class Effects {
  constructor(scene) {
    this.scene = scene;
    /** Geschosse in der Luft, nach ihrer Nummer vom Server. */
    this.inFlight = new Map();

    this._impactGeometry = new THREE.SphereGeometry(0.055, 6, 4);
    this._letterGeometry = new THREE.BoxGeometry(0.19, 0.13, 0.02);
    this._letterMaterial = new THREE.MeshLambertMaterial({ color: 0xf7f5ef });
    this._burstGeometry = new THREE.SphereGeometry(1, 10, 7);

    this.tracers = new Pool(scene, MAX_TRACERS, () => {
      // Je Linie eine eigene Geometrie mit zwei Punkten, die bei jeder
      // Wiederverwendung ueberschrieben werden.
      const geometry = new THREE.BufferGeometry();
      const position = new THREE.BufferAttribute(new Float32Array(6), 3);
      position.setUsage(THREE.DynamicDrawUsage);
      geometry.setAttribute("position", position);
      return new THREE.Line(geometry, fadingMaterial(THREE.LineBasicMaterial, 0.95));
    });
    this.impacts = new Pool(
      scene,
      MAX_IMPACTS,
      () => new THREE.Mesh(this._impactGeometry, fadingMaterial(THREE.MeshBasicMaterial, 0.9)),
    );
    this.bursts = new Pool(scene, MAX_BURSTS, () => {
      const mesh = new THREE.Mesh(
        this._burstGeometry,
        fadingMaterial(THREE.MeshBasicMaterial, 0.55),
      );
      mesh.material.color.setHex(0xf1e5c8);
      return mesh;
    });
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
    const mesh = new THREE.Mesh(this._letterGeometry, this._letterMaterial);
    mesh.position.set(event.pos[0], event.pos[1], event.pos[2]);
    this.scene.add(mesh);
    this.inFlight.set(event.projectile, {
      mesh,
      vel: [event.vel[0], event.vel[1], event.vel[2]],
      gravity,
      // Notbremse: geht die Einschlagsmeldung verloren, raeumt der Client
      // spaetestens hier ab, statt einen Brief bis ans Ende der Karte fliegen
      // zu lassen.
      remaining: 6.0,
    });
  }

  /** Nimmt ein Geschoss aus der Luft und zeichnet die Explosion. */
  addBurst(event) {
    this._removeProjectile(event.projectile);

    const entry = this.bursts.take(BURST_LIFETIME);
    entry.object.position.set(event.pos[0], event.pos[1], event.pos[2]);
    entry.object.material.opacity = 0.55;
    // Die Wolke waechst beim Verblassen auf die Groesse, in der der Schaden
    // gewirkt hat - so sieht man, wen es getroffen haben kann.
    entry.cloud = { from: event.radius * 0.3, to: event.radius };
    entry.object.scale.setScalar(entry.cloud.from);
  }

  _removeProjectile(id) {
    const entry = this.inFlight.get(id);
    if (!entry) return;
    this.scene.remove(entry.mesh);
    this.inFlight.delete(id);
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
    const { object: line } = this.tracers.take(TRACER_LIFETIME);
    const position = line.geometry.attributes.position;
    position.setXYZ(0, from[0], from[1], from[2]);
    position.setXYZ(1, to[0], to[1], to[2]);
    position.needsUpdate = true;
    line.material.color.setHex(color);
    line.material.opacity = 0.95;
  }

  _addImpact(pos, color) {
    const { object: dot } = this.impacts.take(IMPACT_LIFETIME);
    dot.position.set(pos[0], pos[1], pos[2]);
    dot.material.color.setHex(color);
    dot.material.opacity = 0.9;
  }

  /** Blendet alle Effekte aus und raeumt abgelaufene weg. */
  update(dt) {
    // Geschosse weiterfliegen lassen. Dieselbe Rechnung wie auf dem Server,
    // nur ohne Kollision - wo es endet, sagt der Server mit `Burst`.
    for (const [id, p] of this.inFlight) {
      p.vel[1] -= p.gravity * dt;
      p.mesh.position.x += p.vel[0] * dt;
      p.mesh.position.y += p.vel[1] * dt;
      p.mesh.position.z += p.vel[2] * dt;
      // Der Brief taumelt; ein starr fliegendes Rechteck sieht tot aus.
      p.mesh.rotation.x += dt * 6.0;
      p.mesh.rotation.y += dt * 2.5;
      p.remaining -= dt;
      if (p.remaining <= 0) this._removeProjectile(id);
    }

    this.tracers.advance(dt, 0.95);
    this.impacts.advance(dt, 0.9);
    this.bursts.advance(dt, 0.55);
  }

  dispose() {
    for (const id of [...this.inFlight.keys()]) this._removeProjectile(id);
    for (const { object } of this.tracers.entries) object.geometry.dispose();
    this.tracers.dispose(this.scene);
    this.impacts.dispose(this.scene);
    this.bursts.dispose(this.scene);
    this._letterGeometry.dispose();
    this._letterMaterial.dispose();
    this._burstGeometry.dispose();
    this._impactGeometry.dispose();
  }
}
