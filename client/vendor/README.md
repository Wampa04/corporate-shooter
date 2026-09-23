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

Danach die Versionsnummer in dieser Datei anpassen und die Pruefsummen neu
schreiben:

    (cd client/vendor && sha256sum three.module.min.js three.core.min.js \
        three-LICENSE.txt > SHA256SUMS)

`SHA256SUMS` haelt fest, was eingecheckt ist; die CI prueft sie mit
`sha256sum -c`. Eine versehentlich veraenderte oder beschaedigte Datei faellt
damit auf, statt still ausgeliefert zu werden. Beim Aktualisieren lohnt der
Abgleich gegen das npm-Paket, bevor die neuen Summen eingecheckt werden.

## predict.wasm

Die Bewegungsvorhersage: `crates/predict`, übersetzt nach
`wasm32-unknown-unknown`. Enthält dieselbe Funktion, die auch der Server
rechnet — es gibt keine zweite, in JavaScript gepflegte Fassung der Bewegung.

Neu bauen:

```sh
cargo build -p predict --target wasm32-unknown-unknown --profile wasm
cp target/wasm32-unknown-unknown/wasm/predict.wasm client/vendor/
```

Eingecheckt ist sie, damit `cargo run -p server` ohne WASM-Werkzeugkette
funktioniert. Das Docker-Image baut sie frisch.

`scripts/gleichlauf.sh` prüft, dass Rust und WebAssembly dieselbe Bewegung
rechnen - für das frisch übersetzte *und* für das eingecheckte Modul. Wer
Protokoll oder Bewegung ändert und die Datei nicht neu baut, bekommt in der CI
einen roten Lauf mit der Anleitung dazu. Früher prüfte die CI nur das frische
Modul, und das eingecheckte blieb über vier Protokolländerungen liegen.
