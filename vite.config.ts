import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const DEV_SERVER_PORT = 1520;
const HOT_RELOAD_PORT = 1521;

export default defineConfig({
  root: fileURLToPath(new URL("./window", import.meta.url)),
  plugins: [react()],
  clearScreen: false,
  server: {
    port: DEV_SERVER_PORT,
    strictPort: true,
    hmr: { port: HOT_RELOAD_PORT },
    watch: { ignored: ["**/crates/**", "**/target/**"] },
  },
  build: {
    outDir: fileURLToPath(new URL("./dist", import.meta.url)),
    emptyOutDir: true,
    target: "safari16",
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
