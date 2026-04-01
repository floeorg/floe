import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import sitemap from "@astrojs/sitemap";
import starlightLlmsTxt from "starlight-llms-txt";
import floeGrammar from "../../editors/vscode/syntaxes/floe.tmLanguage.json";

const floeLang = {
  ...floeGrammar,
  aliases: ["floe", "fl"],
};

export default defineConfig({
  site: "https://floe-lang.dev",
  markdown: {
    shikiConfig: {
      langs: [floeLang],
    },
  },
  vite: {
    ssr: {
      noExternal: ["zod"],
    },
  },
  integrations: [
    sitemap(),
    starlight({
      title: "Floe",
      logo: {
        src: "./src/assets/logo.svg",
        alt: "Floe",
      },
      favicon: "/logo.svg",
      description:
        "A strict, functional language that compiles to TypeScript. Use any TypeScript or React library as-is.",
      plugins: [starlightLlmsTxt()],
      routeMiddleware: "./src/routeData.ts",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/floeorg/floe",
        },
      ],
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "docs/guide/introduction" },
            { label: "Installation", slug: "docs/guide/installation" },
            { label: "Your First Project", slug: "docs/guide/first-program" },
            { label: "Language Tour", slug: "docs/guide/tour" },
            { label: "Using Floe with LLMs", slug: "docs/guide/llm-setup" },
            {
              label: "Migrating from TypeScript",
              slug: "docs/guide/from-typescript",
            },
          ],
        },
        {
          label: "Language Guide",
          items: [
            { label: "Functions & Const", slug: "docs/guide/functions" },
            { label: "Types", slug: "docs/guide/types" },
            { label: "Pipes", slug: "docs/guide/pipes" },
            { label: "Pattern Matching", slug: "docs/guide/pattern-matching" },
            { label: "Error Handling", slug: "docs/guide/error-handling" },
            {
              label: "Callback Flattening & Guards",
              slug: "docs/guide/use",
            },
            { label: "TypeScript Interop", slug: "docs/guide/typescript-interop" },
            { label: "For Blocks", slug: "docs/guide/for-blocks" },
            { label: "Traits", slug: "docs/guide/traits" },
            { label: "JSX & React", slug: "docs/guide/jsx" },
            {
              label: "Type-Driven Features",
              slug: "docs/guide/type-driven-features",
            },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "CLI", slug: "docs/reference/cli" },
            { label: "Configuration", slug: "docs/reference/configuration" },
            { label: "Operators", slug: "docs/reference/operators" },
            { label: "Syntax", slug: "docs/reference/syntax" },
            { label: "Types", slug: "docs/reference/types" },
            {
              label: "Standard Library",
              autogenerate: { directory: "docs/reference/stdlib" },
            },
            { label: "Vite", slug: "docs/reference/vite" },
            { label: "VS Code", slug: "docs/reference/vscode" },
            { label: "Neovim", slug: "docs/reference/neovim" },
          ],
        },
      ],
    }),
  ],
});
