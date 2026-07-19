import eslint from "@eslint/js";
import { defineConfig } from "eslint/config";
import astro from "eslint-plugin-astro";
import tseslint from "typescript-eslint";

export default defineConfig(
  {
    ignores: [
      ".astro/",
      ".dependencies/",
      "dist/",
      "node_modules/",
      "playwright-report/",
      "test-results/",
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...astro.configs.recommended,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-magic-numbers": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/no-unsafe-assignment": "error",
    },
  },
  {
    files: ["**/*.astro", "tests/**/*.ts"],
    rules: { "@typescript-eslint/no-magic-numbers": "off" },
  },
);
