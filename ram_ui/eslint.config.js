import tsParser from "@typescript-eslint/parser";
import tsPlugin from "@typescript-eslint/eslint-plugin";

export default [
  {
    ignores: ["dist/**", "node_modules/**"]
  },
  {
    files: ["frontend/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        ecmaFeatures: { jsx: true }
      }
    },
    plugins: { "@typescript-eslint": tsPlugin },
    rules: {
      "no-unused-vars": "off",
      "@typescript-eslint/no-unused-vars": "error",
      "no-restricted-syntax": ["error", {
        selector: "JSXOpeningElement[name.name=/^[a-z]/] > JSXAttribute[name.name='title']",
        message: "Use data-tip and the shared custom tooltip instead of a native title tooltip."
      }]
    }
  }
];
