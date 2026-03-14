import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  integrations: [
    starlight({
      title: "ZenScript",
      description:
        "A Gleam-inspired language that compiles to TypeScript + React",
      social: {
        github: "https://github.com/milkyskies/zenscript",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", slug: "guide/introduction" },
            { label: "Installation", slug: "guide/installation" },
            { label: "Your First Program", slug: "guide/first-program" },
          ],
        },
        {
          label: "Core Concepts",
          items: [
            { label: "Functions & Const", slug: "guide/functions" },
            { label: "Pipes", slug: "guide/pipes" },
            { label: "Pattern Matching", slug: "guide/pattern-matching" },
            { label: "Types", slug: "guide/types" },
            { label: "Error Handling", slug: "guide/error-handling" },
            { label: "JSX & React", slug: "guide/jsx" },
          ],
        },
        {
          label: "Migration",
          items: [
            { label: "From TypeScript", slug: "guide/from-typescript" },
            { label: "Comparison", slug: "guide/comparison" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Syntax", slug: "reference/syntax" },
            { label: "Types", slug: "reference/types" },
            { label: "Operators", slug: "reference/operators" },
          ],
        },
        {
          label: "Tooling",
          items: [
            { label: "CLI (zsc)", slug: "tooling/cli" },
            { label: "Vite Plugin", slug: "tooling/vite" },
            { label: "VS Code Extension", slug: "tooling/vscode" },
            { label: "Configuration", slug: "tooling/configuration" },
          ],
        },
      ],
    }),
  ],
});
