# Mitgelieferte Fremdbibliotheken

## three.js 0.185.1

`three.module.min.js` und `three.core.min.js` stammen unveraendert aus dem
npm-Paket `three@0.185.1` (`build/`-Verzeichnis). Lizenz: MIT, siehe
`three-LICENSE.txt`.

Die Dateien liegen hier im Repository statt von einem CDN geladen zu werden,
weil der Server im LAN laufen soll - moeglicherweise ohne Internetzugang.
Es gibt bewusst keinen npm-Build-Schritt: der Client besteht aus ES-Modulen,
die der Browser direkt laedt.

### Aktualisieren

    curl -sSO https://registry.npmjs.org/three/-/three-<version>.tgz
    tar xzf three-<version>.tgz \
        package/build/three.module.min.js \
        package/build/three.core.min.js \
        package/LICENSE
    cp package/build/three.*.min.js client/vendor/
    cp package/LICENSE client/vendor/three-LICENSE.txt

Danach die Versionsnummer in dieser Datei anpassen.

## predict.wasm

Die Bewegungsvorhersage: `crates/predict`, übersetzt nach
`wasm32-unknown-unknown`. Enthält dieselbe Funktion, die auch der Server
rechnet — es gibt keine zweite, in JavaScript gepflegte Fassung der Bewegung.

Neu bauen:

```sh
cargo build -p predict --target wasm32-unknown-unknown --profile wasm
cp target/wasm32-unknown-unknown/wasm/predict.wasm client/vendor/
```

`scripts/gleichlauf.sh` prüft, dass Rust und WebAssembly dieselbe Bewegung
rechnen. Docker und CI bauen die Datei ohnehin frisch; eingecheckt ist sie,
damit `cargo run -p server` ohne WASM-Werkzeugkette funktioniert.
