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
    this.killfeed = el("killfeed");
    this.fps = el("fps");
    this.teamscore = el("teamscore");
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
  }

  show() {
    this.root.classList.remove("hidden");
  }

  /** Uebernimmt eigenen Zustand und Munition aus dem Snapshot. */
  update(self, local) {
    if (self) {
      const ratio = Math.max(0, Math.min(1, self.health / this.config.max_health));
      this.healthFill.style.width = `${ratio * 100}%`;
      this.healthFill.style.backgroundColor =
        ratio > 0.5 ? "var(--ok)" : ratio > 0.25 ? "#e0a33c" : "var(--danger)";
      this.healthValue.textContent = String(self.health);
      this.weaponName.textContent = this.weaponNames.get(self.weapon) ?? "–";
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
      this.ammoCurrent.textContent = String(local.ammo);
      this.ammoMax.textContent = String(local.mag_size);
      this.ammoBox.classList.toggle("empty", local.ammo === 0);
    } else if (art === "Heat") {
      const heiss = local.heat_lock > 0;
      this.heatFill.style.width = `${Math.min(1, local.heat) * 100}%`;
      this.heatBox.classList.toggle("warm", !heiss && local.heat > 0.6);
      this.heatBox.classList.toggle("ueberhitzt", heiss);
      this.heatLabel.textContent = heiss
        ? `Abkühlen ${local.heat_lock.toFixed(1)} s`
        : "Betriebstemperatur";
    }

    this.reloading.classList.toggle("hidden", !local.reloading || art !== "Magazine");

    this._cooldown(this.dash, local.dash_cooldown_remaining, this.config.dash_cooldown);
    this._cooldown(this.heal, local.heal_cooldown_remaining, this.config.heal_cooldown);

    const dead = self ? !self.alive : false;
    this.respawn.classList.toggle("hidden", !dead);
    if (dead) {
      this.respawnTimer.textContent = local.respawn_remaining.toFixed(1);
    }
  }

  _cooldown(node, remaining, total) {
    const ready = remaining <= 0;
    node.classList.toggle("ready", ready);
    // Der Balken laeuft von voll nach leer; `total` kann 0 sein, wenn eine
    // Faehigkeit ohne Cooldown konfiguriert ist.
    const fraction = total > 0 ? Math.max(0, Math.min(1, remaining / total)) : 0;
    node.querySelector("i").style.transform = `scaleX(${fraction})`;
  }

  setPing(ms) {
    this.ping.textContent = ms === null ? "–" : Math.round(ms);
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
    this._fpsBuf ??= [];
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
  _fillTable(tbody, players) {
    const sorted = [...players].sort(
      (a, b) => b.kills - a.kills || a.deaths - b.deaths || a.name.localeCompare(b.name),
    );
    tbody.replaceChildren(
      ...sorted.map((player) => {
        const row = document.createElement("tr");
        if (player.id === this.selfId) row.className = "self";
        row.innerHTML =
          `<td class="${TEAM_CLASS[player.team] ?? ""}">${escapeHtml(player.team)}</td>` +
          `<td>${escapeHtml(player.name)}</td>` +
          `<td class="num">${player.kills}</td>` +
          `<td class="num">${player.deaths}</td>`;
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

    this.scoreMarketing.textContent = String(stand.score_marketing);
    this.scoreEngineering.textContent = String(stand.score_engineering);
    // Die Grenze steht in der Konfiguration, nicht im Snapshot: sie aendert
    // sich nie, und dreissigmal je Sekunde dieselbe Zahl zu schicken waere
    // Verschwendung.
    this.scoreLimit.textContent = String(this.config.score_limit);

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
    this.matchTimer.textContent = String(Math.ceil(stand.remaining));
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
              ? `${tag(killer)} <span class="verb">hat</span> ${tag(victim)} ` +
                `<span class="verb">freigestellt${weapon ? ` (${escapeHtml(weapon)})` : ""}</span>`
              : `${tag(victim)} <span class="verb">hat sich selbst wegrationalisiert</span>`,
            killer?.team,
          );
          break;
        }
        case "Joined":
          this._addEntry(
            `<span class="${TEAM_CLASS[event.d.team] ?? ""}">${escapeHtml(event.d.name)}</span> ` +
              `<span class="verb">wurde onboarded</span>`,
            event.d.team,
          );
          break;
        case "Left":
          this._addEntry(
            `<span class="verb">${escapeHtml(event.d.name)} hat innerlich gekündigt</span>`,
          );
          break;
        case "Hit":
          // Rueckmeldung nur fuer die eigenen Treffer und die eigenen Wunden.
          if (event.d.attacker === this.selfId) this._flashHitmarker();
          if (event.d.target === this.selfId) this._flashDamage();
          break;
        case "MatchOver": {
          const { winner, score_marketing, score_engineering } = event.d;
          this._addEntry(
            `<span class="${TEAM_CLASS[winner] ?? ""}">${escapeHtml(winner)}</span> ` +
              `<span class="verb">gewinnt das Quartal ` +
              `(${score_marketing}:${score_engineering})</span>`,
            winner,
          );
          break;
        }
        case "MatchStarted":
          this._addEntry(`<span class="verb">Neues Quartal, neue Ziele</span>`);
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
    this._addEntry(`<span class="verb">${escapeHtml(text)}</span>`);
  }

  _addEntry(html, team) {
    const node = document.createElement("div");
    node.className = "kill-entry";
    if (team) node.style.borderLeftColor = `var(--${TEAM_CLASS[team]})`;
    node.innerHTML = html;
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
    this._damageTimer = setTimeout(
      () => this.damageFlash.classList.remove("on"),
      60,
    );
  }
}

/** Namen sind frei waehlbar und landen im DOM - also escapen. */
function escapeHtml(text) {
  return String(text).replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
  );
}

function tag(player) {
  if (!player) return `<span class="verb">jemand</span>`;
  return `<span class="${TEAM_CLASS[player.team] ?? ""}">${escapeHtml(player.name)}</span>`;
}
