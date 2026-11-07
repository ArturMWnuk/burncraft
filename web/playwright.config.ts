import { defineConfig } from "@playwright/test";

// Testy E2E frontendu serwowanego przez `burncraft-cli serve` (ten sam kod co w oknie Tauri).
// Wymaga: `cargo build -p bc-cli` i `npm run build`. Lokalnie można użyć zainstalowanej
// przeglądarki: PW_CHANNEL=msedge (albo chrome); w CI — Chromium z `npx playwright install`.
declare const process: { env: Record<string, string | undefined>; platform: string };

const PORT = Number(process.env.BC_E2E_PORT ?? 7991);
const exe = process.platform === "win32" ? "burncraft-cli.exe" : "burncraft-cli";
const cli = process.env.BURNCRAFT_CLI ?? `../target/debug/${exe}`;

export default defineConfig({
  testDir: "e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 30_000,
  reporter: [["list"], ["./e2e/req-reporter.ts"]],
  use: {
    baseURL: `http://127.0.0.1:${PORT}/`,
    channel: process.env.PW_CHANNEL?.trim() || undefined,
    viewport: { width: 1280, height: 800 },
    // CI: maszyny bez GPU albo z wirtualnym GPU (macOS arm: Metal oddawał przezroczyste piksele)
    // — jednolity, programowy WebGL2 przez ANGLE/SwiftShader na wszystkich systemach
    launchOptions: {
      args: process.env.CI
        ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"]
        : ["--enable-unsafe-swiftshader"],
    },
  },
  webServer: {
    command: `"${cli}" serve --port ${PORT} --web-dir dist --no-audit-log --autosave-minutes 0 --token-file test-results/control.token --prefs-dir test-results/prefs`,
    url: `http://127.0.0.1:${PORT}/`,
    reuseExistingServer: false,
    stdout: "ignore",
    stderr: "pipe",
    timeout: 60_000,
    // urządzenia fikcyjne USB (K40 bez sterownika, galvo) dla testów kreatora USB
    env: { BURNCRAFT_FAKE_USB: "1" },
  },
});
