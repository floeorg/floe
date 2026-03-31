import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import starlightLlmsTxt from "starlight-llms-txt";
import floeGrammar from "../../editors/vscode/syntaxes/floe.tmLanguage.json";

const floeLang = {
  ...floeGrammar,
  aliases: ["floe", "fl"],
};

export default defineConfig({
  site: "https://floeorg.github.io",
  base: "/floe",
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
            { label: "Introduction", slug: "guide/introduction" },
            { label: "Installation", slug: "guide/installation" },
            { label: "Your First Project", slug: "guide/first-program" },
            { label: "Language Tour", slug: "guide/tour" },
            { label: "Using Floe with LLMs", slug: "guide/llm-setup" },
            {
              label: "Migrating from TypeScript",
              slug: "guide/from-typescript",
            },
          ],
        },
        {
          label: "Language Guide",
          items: [
            { label: "Functions & Const", slug: "guide/functions" },
            { label: "Types", slug: "guide/types" },
            { label: "Pipes", slug: "guide/pipes" },
            { label: "Pattern Matching", slug: "guide/pattern-matching" },
            { label: "Error Handling", slug: "guide/error-handling" },
            { label: "TypeScript Interop", slug: "guide/typescript-interop" },
            { label: "For Blocks & Traits", slug: "guide/for-blocks" },
            { label: "JSX & React", slug: "guide/jsx" },
            {
              label: "Type-Driven Features",
              slug: "guide/type-driven-features",
            },
          ],
        },
        {
          label: "Reference",
          autogenerate: { directory: "reference" },
        },
      ],
    }),
  ],
});
