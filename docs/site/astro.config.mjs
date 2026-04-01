import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
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
            { label: "TypeScript Interop", slug: "docs/guide/typescript-interop" },
            { label: "For Blocks & Traits", slug: "docs/guide/for-blocks" },
            { label: "JSX & React", slug: "docs/guide/jsx" },
            {
              label: "Type-Driven Features",
              slug: "docs/guide/type-driven-features",
            },
          ],
        },
        {
          label: "Reference",
          autogenerate: { directory: "docs/reference" },
        },
      ],
    }),
  ],
});
