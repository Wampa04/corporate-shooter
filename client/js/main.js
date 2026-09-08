// Einstiegspunkt: verbindet Netz, Eingabe, Darstellung und Oberflaeche.
//
// Aufgabenteilung: der Server simuliert, der Client zeigt an. Die einzige
// Groesse, die der Client sofort selbst fuehrt, ist die Blickrichtung - sie
// wird jeden Tick an den Server gemeldet, der sie begrenzt und fuer alles
// Weitere als verbindlich behandelt.

import * as THREE from "../vendor/three.module.min.js";
import { Connection } from "./net.js";
import { buildScene, setShadowsEnabled } from "./world.js";
import { PlayerViews } from "./players.js";
import { Effects } from "./effects.js";
import { InputController } from "./input.js";
import { ViewModel } from "./viewmodel.js";
import { Hud } from "./hud.js";
import { Audio } from "./audio.js";

/**
 * Wie schnell die Kamera der autoritativen Position folgt (1/s).
 *
 * Die eigene Position wird bewusst *nicht* verzoegert dargestellt: sie kommt
 * ungefiltert aus dem neuesten Snapshot. Die Glaettung buegelt nur die Stufen
 * der 30-Hz-Updates aus, ohne spuerbar Verzoegerung hinzuzufuegen.
 */
const CAMERA_FOLLOW_RATE = 26;

/** Ab dieser Distanz wird die Kamera gesetzt statt gezogen (Respawn). */
const CAMERA_TELEPORT = 3.0;

/**
 * Wie viele Bilder abgewartet werden, bevor ueber den Schattenwurf entschieden
 * wird. Die ersten Bilder sind durch Shader-Uebersetzung und Texturupload
 * verzerrt und taugen nicht als Massstab.
 */
const QUALITY_WARMUP_FRAMES = 30;

/** Wie viele Bilder danach gemessen werden. */
const QUALITY_SAMPLE_FRAMES = 60;

/**
 * Ab dieser Bildzeit wird der Schattenwurf abgeschaltet.
 *
 * 28 ms entsprechen gut 35 Bildern je Sekunde. Wer darunter liegt, hat mehr
 * davon, das Buero fluessig zu sehen als es beschattet zu sehen.
 */
const QUALITY_BUDGET_MS = 28;

/** Text des Pausenbildes im Normalfall. */
const RESUME_HINT = "Klicken, um weiterzuspielen.";

/**
 * Wartezeit vor dem zweiten Versuch, die Maus einzufangen.
 *
 * Browser sperren das Einfangen fuer etwa eine Sekunde, nachdem der Nutzer
 * es per Escape freigegeben hat. Wer sofort wieder klickt, faellt in diese
 * Frist - ein Versuch kurz danach greift dann von selbst.
 */
const LOCK_RETRY_MS = 1300;

const canvas = document.getElementById("viewport");
const joinOverlay = document.getElementById("join");
const joinForm = document.getElementById("join-form");
const joinStatus = document.getElementById("join-status");
const nameField = document.getElementById("name");
const resumeOverlay = document.getElementById("resume");
const resumeHint = document.getElementById("resume-hint");
const disconnectOverlay = document.getElementById("disconnected");
const disconnectReason = document.getElementById("disconnect-reason");

// Zuletzt benutzter Name, damit niemand ihn bei jedem Neuladen neu tippt.
nameField.value = localStorage.getItem("corpshoot.name") ?? "";

joinForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const button = joinForm.querySelector("button");
  button.disabled = true;
  joinStatus.className = "status";
  joinStatus.textContent = "Verbinde …";

  const name = nameField.value.trim();
  localStorage.setItem("corpshoot.name", name);

  const connection = new Connection();
  try {
    const welcome = await connection.connect(name);
    joinOverlay.classList.add("hidden");
    start(connection, welcome);
  } catch (error) {
    joinStatus.className = "status error";
    joinStatus.textContent = error.message;
    button.disabled = false;
  }
});

/** Startet das Spiel, nachdem Karte und Konfiguration vorliegen. */
function start(connection, welcome) {
  const { config, map, player_id: selfId } = welcome;

  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  // Schattenwurf gibt dem Buero erst Tiefe: ohne ihn schweben Schreibtische
  // und Stuehle ueber dem Teppich, statt darauf zu stehen. Eine 1024er
  // Schattenkarte reicht fuer ein Stockwerk und kostet auch auf einer
  // eingebauten Grafikeinheit kaum etwas.
  // `?grafik=einfach` schaltet den Schattenwurf von vornherein ab,
  // `?grafik=schoen` laesst ihn an, egal wie langsam es laeuft. Ohne Angabe
  // entscheidet die Messung weiter unten.
  const params = new URLSearchParams(location.search);
  const wunsch = params.get("grafik");
  const shadows = wunsch !== "einfach";
  renderer.shadowMap.enabled = shadows;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;

  const scene = buildScene(map, { shadows });
  const camera = new THREE.PerspectiveCamera(78, 1, 0.08, 200);
  // Yaw vor Pitch anwenden, sonst kippt der Horizont beim Umsehen.
  camera.rotation.order = "YXZ";
  // Die Kamera muss selbst Teil der Szene sein, sonst wird das an ihr
  // haengende Waffenmodell nicht gezeichnet.
  scene.add(camera);

  const players = new PlayerViews(scene, selfId, config.tick_rate);
  const effects = new Effects(scene);
  const input = new InputController(canvas, config.weapons);
  const viewmodel = new ViewModel(camera);
  const hud = new Hud(config, selfId);
  hud.show();

  // `?ton=aus` schaltet den Ton von vornherein ab, sonst gilt die gespeicherte
  // Lautstaerke. M schaltet stumm, Komma und Punkt regeln.
  const audio = new Audio(params.get("ton") === "aus" ? 0 : null);
  audio.onStatus = (text) => hud.notify(text);

  const cameraPos = new THREE.Vector3();
  const targetPos = new THREE.Vector3();
  let cameraPlaced = false;
  let lookInitialised = false;
  let local = null;

  function resize() {
    const { clientWidth: w, clientHeight: h } = document.documentElement;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  }
  window.addEventListener("resize", resize);
  resize();

  // Maus einfangen: beim Start und nach jedem Klick.
  //
  // Der Handler haengt bewusst *auch* am Pausenbild. Es liegt als Overlay
  // ueber dem Canvas und faengt jeden Klick ab - haenge er nur am Canvas,
  // bliebe ausgerechnet der Klick wirkungslos, zu dem das Bild auffordert.
  let retryTimer = null;
  // Nach einem Verbindungsverlust gibt es nichts mehr fortzusetzen. Ohne
  // diese Schranke wuerde ein Klick neben dem Verbindungsdialog die Maus
  // fuer ein totes Spiel einfangen.
  let running = true;

  const resumeGame = () => {
    if (!running) return;
    // Der Klick ist die Nutzergeste, auf die der Browser wartet, bevor er Ton
    // zulaesst - auch der Klick zurueck aus der Pause.
    audio.resume();
    // Ein laufender Wiederholversuch bleibt bewusst stehen. Ihn hier zu
    // loeschen hiesse: wer ungeduldig weiterklickt, setzt die Wartezeit
    // staendig zurueck - und der Versuch kaeme nie zum Zug.
    input.requestLock();
  };

  canvas.addEventListener("click", resumeGame);
  resumeOverlay.addEventListener("click", resumeGame);

  input.onLockChange = (locked) => {
    resumeOverlay.classList.toggle("hidden", locked || !running);
    if (locked) {
      clearTimeout(retryTimer);
      retryTimer = null;
      resumeHint.textContent = RESUME_HINT;
    }
  };

  input.onLockError = () => {
    if (!running || retryTimer !== null) return; // Beendet oder Versuch laeuft.
    resumeHint.textContent = "Der Browser gibt die Maus noch nicht frei \u2013 gleich noch einmal \u2026";
    retryTimer = setTimeout(() => {
      retryTimer = null;
      input.requestLock();
      // Klappt auch das nicht, muss der Nutzer selbst klicken.
      if (!input.locked) resumeHint.textContent = RESUME_HINT;
    }, LOCK_RETRY_MS);
  };

  // Der Klick auf "Beitreten" zaehlt als Nutzergeste, der Ton darf also sofort.
  audio.resume();
  input.requestLock();

  connection.onSnapshot = (snapshot) => {
    const byId = new Map(snapshot.players.map((p) => [p.id, p]));
    const self = byId.get(selfId);
    local = snapshot.local;

    // Beim Einstieg die Blickrichtung des Spawnpunkts uebernehmen. Ohne das
    // startet jeder mit Blick nach -Z, was meist eine Wand ist.
    if (self && !lookInitialised) {
      input.setLook(self.yaw, self.pitch);
      lookInitialised = true;
    }

    for (const event of snapshot.events) {
      if (event.t === "Shot") {
        effects.addShot(event.d);
        if (event.d.shooter === selfId) viewmodel.kick(event.d.weapon);
      } else if (event.t === "Hit") {
        effects.addHit(event.d);
      } else if (event.t === "Spawned" && event.d.id === selfId && self) {
        // Nach dem Wiedereinstieg dorthin blicken, wohin der Spawnpunkt zeigt.
        input.setLook(self.yaw, 0);
        cameraPlaced = false;
      }
    }

    if (self) viewmodel.setWeapon(self.weapon);
    audio.handleEvents(snapshot.events, selfId);
    audio.setReloading(snapshot.local?.reloading ?? false);
    hud.handleEvents(snapshot.events, byId);
    hud.update(self, snapshot.local);
  };

  connection.onClose = () => {
    disconnectReason.textContent =
      "Der Server ist nicht mehr erreichbar. Vermutlich ein Meeting.";
    disconnectOverlay.classList.remove("hidden");
    stop();
  };

  // Eingaben laufen mit der Tickrate des Servers, unabhaengig von der
  // Bildwiederholrate. Bei 144 Hz waere pro Bild zu senden reine Verschwendung,
  // bei 30 Hz Monitor wuerden Eingaben verschluckt.
  const inputTimer = setInterval(
    () => connection.sendInput(input.nextFrame()),
    1000 / config.tick_rate,
  );

  let previous = performance.now();
  let frame = 0;

  // Selbstmessung der Bildzeit. Ob ein Rechner den Schattenwurf traegt, laesst
  // sich nicht vorhersagen - also wird es gemessen statt geraten.
  const frameTimes = [];
  let qualityDecided = wunsch !== null;

  function judgeQuality(dt) {
    if (qualityDecided) return;
    frameTimes.push(dt * 1000);
    if (frameTimes.length < QUALITY_WARMUP_FRAMES + QUALITY_SAMPLE_FRAMES) return;

    qualityDecided = true;
    const messwerte = frameTimes.slice(QUALITY_WARMUP_FRAMES).sort((a, b) => a - b);
    const median = messwerte[Math.floor(messwerte.length / 2)];
    if (median <= QUALITY_BUDGET_MS) return;

    setShadowsEnabled(scene, renderer, false);
    console.info(
      `Schattenwurf abgeschaltet: ${median.toFixed(0)} ms je Bild. ` +
        "Mit ?grafik=schoen laesst er sich erzwingen.",
    );
  }

  function render(now) {
    frame = requestAnimationFrame(render);
    // Nach einem Tab-Wechsel kann `now` weit springen; ein gedeckelter
    // Zeitschritt verhindert, dass Effekte auf einen Schlag verschwinden.
    const dt = Math.min((now - previous) / 1000, 0.1);
    previous = now;

    judgeQuality(dt);
    players.update(connection.snapshots, now);
    effects.update(dt);
    viewmodel.update(dt, local?.reloading ?? false);
    hud.tick(now);
    hud.setPing(connection.ping);

    const latest = connection.latest;
    const self = latest?.players.find((p) => p.id === selfId);
    if (self) {
      targetPos.set(self.pos[0], self.pos[1] + config.eye_height, self.pos[2]);
      if (!cameraPlaced || cameraPos.distanceTo(targetPos) > CAMERA_TELEPORT) {
        cameraPos.copy(targetPos);
        cameraPlaced = true;
      } else {
        // Zeitschrittunabhaengige Glaettung: bei jeder Bildrate gleich schnell.
        cameraPos.lerp(targetPos, 1 - Math.exp(-CAMERA_FOLLOW_RATE * dt));
      }
      camera.position.copy(cameraPos);
      hud.setScoreboard(latest.players, input.scoreboardVisible);
    }

    camera.rotation.y = input.yaw;
    camera.rotation.x = input.pitch;

    // Der Hoerer sitzt an der Kamera, muss also nach ihrer Drehung gesetzt
    // werden. Die Schritte der Mitspieler kommen aus deren interpolierten
    // Positionen, die `players.update` weiter oben schon gesetzt hat.
    audio.update(dt, camera, self ? { x: self.pos[0], z: self.pos[2] } : null,
      local?.on_ground ?? false, players.avatars);

    renderer.render(scene, camera);
  }
  frame = requestAnimationFrame(render);

  function stop() {
    running = false;
    clearInterval(inputTimer);
    clearTimeout(retryTimer);
    cancelAnimationFrame(frame);
    // Erst die Maus freigeben, dann das Pausenbild wegnehmen: die Freigabe
    // loest `onLockChange` aus, das es sonst wieder einblenden wuerde.
    document.exitPointerLock?.();
    resumeOverlay.classList.add("hidden");
    players.dispose();
    effects.dispose();
    viewmodel.dispose();
    audio.dispose();
  }
}
