import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://milkyskies.github.io",
  base: "/floe",
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
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/milkyskies/floe",
        },
      ],
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "guide/introduction" },
            { label: "Installation", slug: "guide/installation" },
            { label: "First Program", slug: "guide/first-program" },
            { label: "Types", slug: "guide/types" },
            { label: "Functions & Const", slug: "guide/functions" },
            { label: "Pipes", slug: "guide/pipes" },
            { label: "Pattern Matching", slug: "guide/pattern-matching" },
            { label: "Error Handling", slug: "guide/error-handling" },
            { label: "For Blocks", slug: "guide/for-blocks" },
            { label: "Traits", slug: "guide/traits" },
            { label: "JSX", slug: "guide/jsx" },
            { label: "Testing", slug: "guide/testing" },
            { label: "Migrating from TypeScript", slug: "guide/from-typescript" },
            { label: "Comparison", slug: "guide/comparison" },
          ],
        },
        {
          label: "Reference",
          autogenerate: { directory: "reference" },
        },
        {
          label: "Tooling",
          autogenerate: { directory: "tooling" },
        },
      ],
    }),
  ],
});
