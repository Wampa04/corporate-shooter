// Die Oberflaeche waehrend des Spiels.
//
// Alle angezeigten Zahlen stammen aus den Snapshots und aus der
// Serverkonfiguration. Der Client rechnet nichts nach - was hier steht, ist
// der Zustand, den der Server fuer verbindlich haelt.

/** Wie lange ein Eintrag in der Killfeed stehen bleibt (ms). */
const KILLFEED_LIFETIME = 7000;

/** Hoechstzahl gleichzeitiger Eintraege. */
const KILLFEED_MAX = 6;

const TEAM_CLASS = { Marketing: "marketing", Engineering: "engineering" };

const el = (id) => document.getElementById(id);

/**
 * Reihenfolge der Rangliste: Abschuesse, dann wenige Tode, dann Name.
 *
 * @template {{kills: number, deaths: number, name: string}} T
 * @param {T[]} players
 * @returns {T[]}
 */
export function rankPlayers(players) {
  return [...players].sort(
    (a, b) => b.kills - a.kills || a.deaths - b.deaths || a.name.localeCompare(b.name),
  );
}

/**
 * Fingerabdruck einer sortierten Rangliste: aendert er sich nicht, sieht die
 * Tabelle genauso aus wie beim letzten Mal.
 *
 * Die Rangliste wird bei gehaltener Tab-Taste in jedem Bild angefragt. Neu
 * gebaut wurde sie bisher auch jedes Mal - sechzig- bis hundertvierzigmal je
 * Sekunde dieselben Zeilen, obwohl sich Abschuesse nur selten aendern.
 *
 * @param {Array<{id: number, team: string, name: string, kills: number, deaths: number}>} ranked
 */
export function tableKey(ranked) {
  return ranked.map((p) => `${p.id}|${p.team}|${p.kills}|${p.deaths}|${p.name}`).join("\n");
}

/** Ein `<span>` mit Text - nie mit HTML, deshalb braucht es kein Escaping. */
function span(text, className) {
  const node = document.createElement("span");
  if (className) node.className = className;
  node.textContent = text;
  return node;
}

/** Spielername in Teamfarbe, oder "jemand", wenn er nicht mehr da ist. */
function playerTag(player) {
  if (!player) return span("jemand", "verb");
  return span(player.name, TEAM_CLASS[player.team]);
}

export class Hud {
  constructor(config, selfId) {
    this.config = config;
    this.selfId = selfId;
    this.weaponNames = new Map(config.weapons.map((w) => [w.id, w.name]));

    this.root = el("hud");
    this.healthFill = el("health-fill");
    this.healthValue = el("health-value");
    this.ammoBox = el("ammo");
    this.heatBox = el("heat");
    this.heatFill = el("heat-fill");
    this.heatLabel = el("heat-label");
    /** Waffenbeschreibung je Id - fuer die Frage, welche Anzeige gilt. */
    this.weaponById = new Map(config.weapons.map((w) => [w.id, w]));
    this.ammoCurrent = el("ammo-current");
    this.ammoMax = el("ammo-max");
    this.weaponName = el("weapon-name");
    this.reloading = el("reloading");
    this.dash = el("skill-dash");
    this.heal = el("skill-heal");
    this.dashBar = this.dash.querySelector("i");
    this.healBar = this.heal.querySelector("i");
    this.killfeed = el("killfeed");
    this.fps = el("fps");
    this.scoreMarketing = el("score-marketing");
    this.scoreEngineering = el("score-engineering");
    this.scoreLimit = el("score-limit");
    this.matchEnd = el("match-end");
    this.matchWinner = el("match-winner");
    this.matchSubtitle = el("match-subtitle");
    this.finalMarketing = el("final-marketing");
    this.finalEngineering = el("final-engineering");
    this.matchBody = el("match-body");
    this.matchTimer = el("match-timer");
    this.respawn = el("respawn");
    this.respawnTimer = el("respawn-timer");
    this.scoreboard = el("scoreboard");
    this.scoreboardBody = el("scoreboard-body");
    this.hitmarker = el("hitmarker");
    this.damageFlash = el("damage-flash");
    this.ping = el("ping");

    this._entries = [];
    this._damageTimer = null;
    /** Zuletzt gesetzter Wert je Knoten und Eigenschaft, siehe `_set`. */
    this._shown = new Map();
    /** Fingerabdruck der zuletzt gebauten Tabelle je `<tbody>`. */
    this._tableKeys = new WeakMap();
    this._fpsBuf = [];
  }

  /**
   * Setzt einen Anzeigewert, aber nur, wenn er sich geaendert hat.
   *
   * `update` laeuft mit jedem Snapshot, `setPing` mit jedem Bild. Meist
   * steht dort dasselbe wie zuvor; jedes Schreiben in `textContent` oder
   * `style` loest trotzdem Stil- und Layoutarbeit im Browser aus.
   */
  _set(node, prop, value) {
    let shown = this._shown.get(node);
    if (!shown) this._shown.set(node, (shown = {}));
    if (shown[prop] === value) return;
    shown[prop] = value;
    if (prop === "text") node.textContent = value;
    else node.style[prop] = value;
  }

  show() {
    this.root.classList.remove("hidden");
  }

  /** Uebernimmt eigenen Zustand und Munition aus dem Snapshot. */
  update(self, local) {
    if (self) {
      const ratio = Math.max(0, Math.min(1, self.health / this.config.max_health));
      this._set(this.healthFill, "width", `${ratio * 100}%`);
      this._set(
        this.healthFill,
        "backgroundColor",
        ratio > 0.5 ? "var(--ok)" : ratio > 0.25 ? "var(--warn)" : "var(--danger)",
      );
      this._set(this.healthValue, "text", String(self.health));
      this._set(this.weaponName, "text", this.weaponNames.get(self.weapon) ?? "–");
    }

    // Welche Anzeige gilt, haengt an der Waffe: Magazin zeigt Zahlen,
    // Ueberhitzung zeigt einen Balken, ein Schild zeigt gar nichts. Die
    // Entscheidung faellt an der Waffenbeschreibung und nicht daran, ob
    // `mag_size` zufaellig null ist - das waere dieselbe Aussage aus zweiter
    // Hand.
    const weapon = self ? this.weaponById.get(self.weapon) : null;
    const art = weapon?.ammo?.t ?? "Magazine";

    this.ammoBox.classList.toggle("hidden", art !== "Magazine");
    this.heatBox.classList.toggle("hidden", art !== "Heat");

    if (art === "Magazine") {
      this._set(this.ammoCurrent, "text", String(local.ammo));
      this._set(this.ammoMax, "text", String(local.mag_size));
      this.ammoBox.classList.toggle("empty", local.ammo === 0);
    } else if (art === "Heat") {
      const heiss = local.heat_lock > 0;
      this._set(this.heatFill, "width", `${Math.min(1, local.heat) * 100}%`);
      this.heatBox.classList.toggle("warm", !heiss && local.heat > 0.6);
      this.heatBox.classList.toggle("ueberhitzt", heiss);
      this._set(
        this.heatLabel,
        "text",
        heiss ? `Abkühlen ${local.heat_lock.toFixed(1)} s` : "Betriebstemperatur",
      );
    }

    this.reloading.classList.toggle("hidden", !local.reloading || art !== "Magazine");

    this._cooldown(
      this.dash,
      this.dashBar,
      local.dash_cooldown_remaining,
      this.config.dash_cooldown,
    );
    this._cooldown(
      this.heal,
      this.healBar,
      local.heal_cooldown_remaining,
      this.config.heal_cooldown,
    );

    const dead = self ? !self.alive : false;
    this.respawn.classList.toggle("hidden", !dead);
    if (dead) {
      this._set(this.respawnTimer, "text", local.respawn_remaining.toFixed(1));
    }
  }

  _cooldown(node, bar, remaining, total) {
    const ready = remaining <= 0;
    node.classList.toggle("ready", ready);
    // Der Balken laeuft von voll nach leer; `total` kann 0 sein, wenn eine
    // Faehigkeit ohne Cooldown konfiguriert ist.
    const fraction = total > 0 ? Math.max(0, Math.min(1, remaining / total)) : 0;
    this._set(bar, "transform", `scaleX(${fraction})`);
  }

  setPing(ms) {
    this._set(this.ping, "text", ms === null ? "–" : String(Math.round(ms)));
  }

  /**
   * Zeigt die Bildrate an.
   *
   * Nicht Kosmetik: ruckelt es, ist die erste Frage, ob es am Netz oder am
   * Zeichnen liegt. Ohne die Zahl laesst sich das von aussen nicht
   * unterscheiden - beides sieht gleich aus.
   *
   * Gezeigt wird der gleitende Mittelwert und, wenn er auffaellt, der
   * schlechteste Wert der letzten Sekunde: eine Bildrate von 120 mit einem
   * Ausreisser auf 20 fuehlt sich schlechter an als konstante 60.
   */
  setFrameTime(dt) {
    this._fpsBuf.push(dt);
    if (this._fpsBuf.length < 30) return;

    const mittel = this._fpsBuf.reduce((a, b) => a + b, 0) / this._fpsBuf.length;
    const schlimmster = Math.max(...this._fpsBuf);
    this._fpsBuf.length = 0;

    const fps = Math.round(1 / mittel);
    const min = Math.round(1 / schlimmster);
    // Nur zeigen, wenn der Ausreisser deutlich unter dem Mittel liegt.
    this.fps.textContent = min < fps * 0.7 ? `${fps} (min ${min})` : String(fps);
  }

  setScoreboard(players, visible) {
    this.scoreboard.classList.toggle("hidden", !visible);
    if (visible) this._fillTable(this.scoreboardBody, players);
  }

  /**
   * Baut die Ranglistenzeilen in eine beliebige Tabelle.
   *
   * Geteilt zwischen der Rangliste auf Tab und dem Abschlussbild: zwei Kopien
   * derselben Darstellung liefen sonst frueher oder spaeter auseinander, und
   * ausgerechnet der Endstand ist die Zahl, die am Ende zaehlt.
   */
  /**
   * @param {HTMLElement} tbody
   * @param {Array<{id: number, team: string, name: string, kills: number, deaths: number}>} players
   */
  _fillTable(tbody, players) {
    const ranked = rankPlayers(players);
    const key = tableKey(ranked);
    if (this._tableKeys.get(tbody) === key) return;
    this._tableKeys.set(tbody, key);

    const cell = (text, className) => {
      const td = document.createElement("td");
      if (className) td.className = className;
      td.textContent = text;
      return td;
    };
    tbody.replaceChildren(
      ...ranked.map((player) => {
        const row = document.createElement("tr");
        if (player.id === this.selfId) row.className = "self";
        row.append(
          cell(player.team, TEAM_CLASS[player.team]),
          cell(player.name),
          cell(String(player.kills), "num"),
          cell(String(player.deaths), "num"),
        );
        return row;
      }),
    );
  }

  /**
   * Uebernimmt den Rundenstand aus dem Snapshot.
   *
   * Der Client zaehlt nichts selbst - auch die Restzeit nicht. Sie steht in
   * jedem Snapshot, genau wie die Wartezeit beim Wiedereinstieg. Ein eigener
   * Zaehler im Browser liefe irgendwann anders als der Server, und dann stuende
   * auf dem Bildschirm eine Zahl, die nichts bedeutet.
   *
   * @param {object} stand `snapshot.match`
   * @param {Array}  players Spieler desselben Snapshots, fuer den Endstand
   */
  setMatchState(stand, players) {
    if (!stand) return;

    this._set(this.scoreMarketing, "text", String(stand.score_marketing));
    this._set(this.scoreEngineering, "text", String(stand.score_engineering));
    // Die Grenze steht in der Konfiguration, nicht im Snapshot: sie aendert
    // sich nie, und dreissigmal je Sekunde dieselbe Zahl zu schicken waere
    // Verschwendung.
    this._set(this.scoreLimit, "text", String(this.config.score_limit));

    const vorbei = stand.phase === "Over";
    this.matchEnd.classList.toggle("hidden", !vorbei);
    if (!vorbei) return;

    this.matchWinner.textContent = stand.winner ?? "Niemand";
    this.matchWinner.className = TEAM_CLASS[stand.winner] ?? "";
    this.matchSubtitle.textContent = stand.winner
      ? "hat das Quartal gewonnen"
      : "das Quartal endet ohne Ergebnis";
    this.finalMarketing.textContent = String(stand.score_marketing);
    this.finalEngineering.textContent = String(stand.score_engineering);
    // Aufgerundet: bei 0.4 Sekunden Rest steht "1", und die Anzeige springt
    // nicht auf 0, waehrend noch etwas kommt.
    this._set(this.matchTimer, "text", String(Math.ceil(stand.remaining)));
    this._fillTable(this.matchBody, players);
  }

  /**
   * Wertet die Ereignisse eines Ticks aus.
   *
   * @param {Array} events Ereignisliste des Snapshots
   * @param {Map<number, object>} byId Spielerzustaende dieses Snapshots
   */
  handleEvents(events, byId) {
    for (const event of events) {
      switch (event.t) {
        case "Death": {
          const victim = byId.get(event.d.victim);
          const killer = event.d.killer !== null ? byId.get(event.d.killer) : null;
          const weapon = event.d.weapon ? this.weaponNames.get(event.d.weapon) : null;
          this._addEntry(
            killer
              ? [
                  playerTag(killer),
                  " ",
                  span("hat", "verb"),
                  " ",
                  playerTag(victim),
                  " ",
                  span(`freigestellt${weapon ? ` (${weapon})` : ""}`, "verb"),
                ]
              : [playerTag(victim), " ", span("hat sich selbst wegrationalisiert", "verb")],
            killer?.team,
          );
          break;
        }
        case "Joined":
          this._addEntry(
            [span(event.d.name, TEAM_CLASS[event.d.team]), " ", span("wurde onboarded", "verb")],
            event.d.team,
          );
          break;
        case "Left":
          this._addEntry([span(`${event.d.name} hat innerlich gekündigt`, "verb")]);
          break;
        case "Hit":
          // Rueckmeldung nur fuer die eigenen Treffer und die eigenen Wunden.
          if (event.d.attacker === this.selfId) this._flashHitmarker();
          if (event.d.target === this.selfId) this._flashDamage();
          break;
        case "MatchOver": {
          const { winner, score_marketing, score_engineering } = event.d;
          this._addEntry(
            [
              span(winner, TEAM_CLASS[winner]),
              " ",
              span(`gewinnt das Quartal (${score_marketing}:${score_engineering})`, "verb"),
            ],
            winner,
          );
          break;
        }
        case "MatchStarted":
          this._addEntry([span("Neues Quartal, neue Ziele", "verb")]);
          break;
      }
    }
  }

  /**
   * Kurze Meldung an den Nutzer, im selben Band wie das Killfeed.
   *
   * Fuer Dinge, die der Client selbst entscheidet - etwa die Lautstaerke.
   */
  notify(text) {
    this._addEntry([span(text, "verb")]);
  }

  /**
   * Haengt einen Eintrag an die Killfeed.
   *
   * Nimmt Knoten und Text, kein HTML: Namen sind frei waehlbar, und was als
   * Textknoten im DOM landet, kann kein Markup werden - auch dann nicht, wenn
   * jemand beim Escapen etwas vergisst.
   *
   * @param {Array<Node|string>} parts
   * @param {string} [team]
   */
  _addEntry(parts, team) {
    const node = document.createElement("div");
    node.className = "kill-entry";
    if (TEAM_CLASS[team]) node.style.borderLeftColor = `var(--${TEAM_CLASS[team]})`;
    node.append(...parts);
    this.killfeed.append(node);
    this._entries.push({ node, until: performance.now() + KILLFEED_LIFETIME });

    while (this._entries.length > KILLFEED_MAX) {
      this._entries.shift().node.remove();
    }
  }

  /** Raeumt abgelaufene Killfeed-Eintraege ab. Wird pro Bild aufgerufen. */
  tick(now) {
    while (this._entries.length && this._entries[0].until <= now) {
      this._entries.shift().node.remove();
    }
  }

  _flashHitmarker() {
    this.hitmarker.classList.remove("on");
    // Erzwingt einen Neustart der Animation, auch bei Treffern kurz nacheinander.
    void this.hitmarker.offsetWidth;
    this.hitmarker.classList.add("on");
  }

  _flashDamage() {
    this.damageFlash.classList.add("on");
    clearTimeout(this._damageTimer);
    this._damageTimer = setTimeout(() => this.damageFlash.classList.remove("on"), 60);
  }
}
