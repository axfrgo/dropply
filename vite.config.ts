import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        "**/.publication/**",
        "**/.public-release/**",
        "**/dist/**",
        "**/src-tauri/target/**",
        "**/relay-server/target/**",
      ],
    },
  },
  clearScreen: false,
});
