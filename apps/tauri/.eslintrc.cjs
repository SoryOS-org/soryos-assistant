/** ESLint configuration for panel webview JS. */
module.exports = {
  env: {
    browser: true,
    es2022: true,
  },
  extends: ["eslint:recommended"],
  parserOptions: {
    ecmaVersion: "latest",
    sourceType: "module",
  },
  ignorePatterns: ["**/node_modules/**", "**/target/**", "src-tauri/**"],
  rules: {
    "no-console": "off",
  },
};
