import { defineConfig } from "vite";

// `npm run dev`: Vite na :5173, kanał /rpc przekazywany do `burncraft-cli serve` (7984).
// W trybie dev token podaje się w adresie: http://localhost:5173/?token=...
export default defineConfig({
  base: "./",
  build: { outDir: "dist", emptyOutDir: true, target: "es2022" },
  server: {
    proxy: { "/rpc": { target: "ws://127.0.0.1:7984", ws: true } },
  },
});
