<script setup>
// 外壳：登录态判断 + 左侧导航 + 视图切换（hash 记住当前页）
import { computed, onMounted, ref } from 'vue'
import { api, session } from './api.js'
import Login from './views/Login.vue'
import Overview from './views/Overview.vue'
import Devices from './views/Devices.vue'
import Groups from './views/Groups.vue'
import Users from './views/Users.vue'
import Audit from './views/Audit.vue'
import Settings from './views/Settings.vue'

const views = [
  { key: 'overview', label: '概览', comp: Overview },
  { key: 'devices', label: '设备', comp: Devices },
  { key: 'groups', label: '分组', comp: Groups },
  { key: 'users', label: '用户与权限', comp: Users },
  { key: 'audit', label: '审计日志', comp: Audit },
  { key: 'settings', label: '系统设置', comp: Settings },
]
const current = ref(views.some((v) => v.key === location.hash.slice(1)) ? location.hash.slice(1) : 'overview')
const view = computed(() => views.find((v) => v.key === current.value).comp)

function go(key) {
  current.value = key
  history.replaceState(null, '', '#' + key)
}

async function loadMe() {
  try {
    session.user = await api.get('/me')
  } catch {
    session.user = null
  } finally {
    session.checked = true
  }
}

async function logout() {
  try { await api.post('/logout') } catch { /* 已失效也视为退出 */ }
  session.user = null
}

onMounted(loadMe)
</script>

<template>
  <div v-if="!session.checked" class="empty">正在加载…</div>
  <Login v-else-if="!session.user" @done="loadMe" />
  <div v-else class="shell">
    <nav class="nav" aria-label="主导航">
      <div class="brand">
        <svg width="30" height="30" viewBox="0 0 36 36" aria-hidden="true"><rect x="1" y="1" width="34" height="34" rx="9" fill="var(--accent)" /><rect x="9" y="10" width="18" height="12" rx="2" fill="none" stroke="var(--on-accent)" stroke-width="2" /><path d="M14 27h8M18 22v5" stroke="var(--on-accent)" stroke-width="2" stroke-linecap="round" /></svg>
        <div><strong class="serif">MyDesk</strong><div class="muted small">管理后台</div></div>
      </div>
      <button v-for="v in views" :key="v.key" :class="{ on: current === v.key }" :aria-current="current === v.key ? 'page' : undefined" @click="go(v.key)">{{ v.label }}</button>
      <div class="grow"></div>
      <div class="foot small muted">
        <div class="me">{{ session.user.display_name || session.user.username }} · 管理员</div>
        <div v-if="!session.user.tfa" class="warn">未开启两步验证</div>
        <div>mydesk-server {{ session.user.version }}</div>
        <button class="link" @click="logout">退出登录</button>
      </div>
    </nav>
    <main class="main">
      <component :is="view" @go="go" />
    </main>
  </div>
</template>

<style scoped>
.shell { min-height: 100vh; display: flex; flex-wrap: wrap; }
.nav {
  flex: 1 1 220px; max-width: 240px; padding: 20px 14px; background: var(--surface);
  border-right: 1px solid var(--line); display: flex; flex-direction: column; gap: 4px;
}
.brand { display: flex; align-items: center; gap: 10px; padding: 0 8px 18px; }
.serif { font-family: 'Noto Serif SC', serif; font-size: 16px; }
.nav > button {
  height: 42px; padding: 0 12px; border: 0; border-radius: 8px; background: transparent;
  color: var(--text-2); text-align: left; cursor: pointer;
}
.nav > button.on { background: var(--accent-soft); color: var(--accent); font-weight: 700; }
.foot { border-top: 1px solid var(--line); padding: 12px 8px 0; display: flex; flex-direction: column; gap: 4px; }
.me { color: var(--text-2); font-size: 13px; }
.link { border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; text-align: left; font-size: 13px; }
.main { flex: 999 1 640px; min-width: 0; padding: 24px 28px 40px; }
@media (max-width: 720px) {
  .nav { max-width: none; flex-basis: 100%; flex-direction: row; flex-wrap: wrap; border-right: 0; border-bottom: 1px solid var(--line); }
  .brand { width: 100%; padding-bottom: 8px; }
  .foot { width: 100%; }
  .main { padding: 16px; }
}
</style>
