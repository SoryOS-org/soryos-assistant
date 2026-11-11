/** Stylelint configuration for the Tauri webviews. */
module.exports = {
  extends: ["stylelint-config-standard"],
  ignoreFiles: ["**/node_modules/**", "**/target/**", "src-tauri/**"],
  rules: {
    "no-empty-source": null,
    "selector-pseudo-class-no-unknown": [
      true,
      {
        ignorePseudoClasses: ["global"],
      },
    ],
  },
};
