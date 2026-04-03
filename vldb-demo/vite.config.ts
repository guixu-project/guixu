/*
 * Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
 * SPDX-License-Identifier: Apache-2.0
 */

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  base: '/vldb-demo/',
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: 'https://guixu.org',
        changeOrigin: true,
        secure: true,
      },
      '/api/reviews': {
        target: 'https://guixu.org',
        changeOrigin: true,
        secure: true,
      },
    },
  },
})
