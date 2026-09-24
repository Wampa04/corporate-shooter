// ESLint fuer den Browser-Client und die Pruefskripte.
//
// Bewusst schmal: die empfohlenen Regeln fangen echte Fehler (unbenutzte
// Variablen, Tippfehler bei Namen, unerreichbarer Code); Stilfragen regelt
// Prettier. Wer hier eine Regel ergaenzt, sollte einen Fehler vor Augen haben,
// den sie gefunden haette.
import js from "@eslint/js";
import globals from "globals";

export default [
  {
    ignores: ["client/vendor/**", "target/**", "node_modules/**"],
  },
  js.configs.recommended,
  {
    files: ["client/js/**/*.js"],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: "module",
      globals: globals.browser,
    },
  },
  {
    files: ["scripts/**/*.mjs", "crates/**/*.mjs", "*.js"],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: "module",
      globals: globals.node,
    },
  },
  {
    rules: {
      // `==` vergleicht mit Typumwandlung; bei Werten aus JSON ist das eine
      // Einladung fuer `"0" == 0`.
      eqeqeq: ["error", "always"],
      "no-unused-vars": ["error", { args: "after-used", argsIgnorePattern: "^_" }],
      "prefer-const": "error",
    },
  },
];
