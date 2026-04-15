/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import path from "path";

export default defineConfig({
  plugins: [react()],
  base: "/prism/",
  resolve: {
    alias: {
      "@evilmartians/agent-prism-types": path.resolve(
        __dirname,
        "src/vendor/types/index.ts",
      ),
      "@evilmartians/agent-prism-data": path.resolve(
        __dirname,
        "src/vendor/data/index.ts",
      ),
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: "assets/prism.js",
        chunkFileNames: "assets/prism-[name].js",
        assetFileNames: "assets/prism.[ext]",
      },
    },
  },
});
