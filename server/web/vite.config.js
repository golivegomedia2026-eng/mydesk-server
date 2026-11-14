import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// base 用相对路径：后台路径可配置（默认 /mydeskadmin/），资源需相对页面加载
export default defineConfig({
  base: './',
  plugins: [vue()],
  build: { outDir: 'dist', emptyOutDir: true, assetsInlineLimit: 0 },
  server: {
    proxy: { '/mydeskadmin/api': 'http://127.0.0.1:21114' },
  },
})
