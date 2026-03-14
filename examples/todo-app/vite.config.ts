import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";
import floe from "vite-plugin-floe";
import path from "node:path";

export default defineConfig({
  plugins: [
    floe({
      compiler: path.resolve(__dirname, "../../target/debug/floe"),
    }),
    tailwindcss(),
  ],
  resolve: {
    extensions: [".fl", ".ts", ".tsx", ".js", ".jsx"],
  },
  esbuild: {
    include: /\.(tsx?|jsx?|fl)$/,
    loader: "tsx",
  },
});
