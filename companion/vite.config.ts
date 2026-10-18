import { defineConfig } from "vite-plus";
import react from "@vitejs/plugin-react";
import { companionBridge } from "./server/bridge";

export default defineConfig(({ mode }) => ({
  plugins: [react(), companionBridge()],
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    cors: false,
    fs: {
      allow: [import.meta.dirname],
      deny: [
        "**/.env*",
        "**/.git/**",
        "**/*.{crt,pem,key}",
        "**/*.toml",
        ...(mode === "test" ? [] : ["**/server/**", "**/tests/**", "**/scripts/**"]),
      ],
    },
  },
  preview: { host: "127.0.0.1", port: 4173, strictPort: true, cors: false },
  test: { include: ["tests/**/*.test.{ts,tsx}"], environment: "node", testTimeout: 15_000 },
  fmt: {},
  lint: {
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: { "vite-plus/prefer-vite-plus-imports": "error" },
    options: { typeAware: true, typeCheck: true },
  },
}));
