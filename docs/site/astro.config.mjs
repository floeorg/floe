import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://milkyskies.github.io",
  base: "/zenscript",
  integrations: [
    starlight({
      title: "ZenScript",
      description:
        "A Gleam-inspired language that compiles to TypeScript + React",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/milkyskies/zenscript",
        },
      ],
      sidebar: [
        {
          label: "Getting Started",
          items: [
            { label: "Introduction", id: "guide/introduction" },
            { label: "Installation", id: "guide/installation" },
            { label: "Your First Program", id: "guide/first-program" },
          ],
        },
        {
          label: "Core Concepts",
          items: [
            { label: "Functions & Const", id: "guide/functions" },
            { label: "Pipes", id: "guide/pipes" },
            { label: "Pattern Matching", id: "guide/pattern-matching" },
            { label: "Types", id: "guide/types" },
            { label: "Error Handling", id: "guide/error-handling" },
            { label: "JSX & React", id: "guide/jsx" },
          ],
        },
        {
          label: "Migration",
          items: [
            { label: "From TypeScript", id: "guide/from-typescript" },
            { label: "Comparison", id: "guide/comparison" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Syntax", id: "reference/syntax" },
            { label: "Types", id: "reference/types" },
            { label: "Operators", id: "reference/operators" },
          ],
        },
        {
          label: "Tooling",
          items: [
            { label: "CLI (zsc)", id: "tooling/cli" },
            { label: "Vite Plugin", id: "tooling/vite" },
            { label: "VS Code Extension", id: "tooling/vscode" },
            { label: "Configuration", id: "tooling/configuration" },
          ],
        },
      ],
    }),
  ],
});
