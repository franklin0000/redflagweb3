import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { nodePolyfills } from 'vite-plugin-node-polyfills'

export default defineConfig({
  base: process.env.IPFS_BUILD ? './' : '/',
  plugins: [
    react(),
    nodePolyfills({
      include: ['buffer', 'process', 'stream', 'util', 'crypto'],
      globals: {
        Buffer: true,
        global: true,
        process: true,
      },
    }),
  ],
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: {
      '/status': 'http://localhost:8545',
      '/network-info': 'http://localhost:8545',
      '/network/stats': 'http://localhost:8545',
      '/balance': 'http://localhost:8545',
      '/account': 'http://localhost:8545',
      '/history': 'http://localhost:8545',
      '/tx': 'http://localhost:8545',
      '/mempool': 'http://localhost:8545',
      '/round-ek': 'http://localhost:8545',
      '/dag': 'http://localhost:8545',
      '/wallet': 'http://localhost:8545',
      '/explorer': 'http://localhost:8545',
      '/ws': {
        target: 'ws://localhost:8545',
        ws: true
      }
    }
  }
})
