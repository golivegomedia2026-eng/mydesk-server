<script setup>
import { computed, onMounted, ref } from 'vue'
import { api, ago, when } from '../api.js'

const emit = defineEmits(['go'])
const data = ref(null)
const help = ref([])
const error = ref('')

const KIND = ['远程控制', '文件传输', '端口转发', '摄像头', '终端']

async function load() {
  error.value = ''
  try {
    ;[data.value, help.value] = await Promise.all([api.get('/overview'), api.get('/help')])
  } catch (e) {
    error.value = e.message
  }
}

async function done(h) {
  try { await api.post(`/help/${h.id}/done`); await load() } catch (e) { error.value = e.message }
}

const alerts = computed(() => {
  const s = data.value?.stats
  if (!s) return []
  const a = []
  if (s.alarms_24h) a.push({ level: '高', cls: 'danger', title: `24 小时内 ${s.alarms_24h} 条安全告警`, detail: '密码猜测、白名单拒绝等，详见审计日志 · 告警', go: 'audit' })
  if (s.uuid_conflicts_24h) a.push({ level: '高', cls: 'danger', title: `${s.uuid_conflicts_24h} 次设备 ID 冒用上报`, detail: '机器码与登记不符，已拒绝覆盖托管密码', go: 'audit' })
  if (s.ungrouped) a.push({ level: '提醒', cls: 'warn', title: `${s.ungrouped} 台设备未分组`, detail: '分组前只有管理员可见', go: 'devices' })
  if (s.unmanaged) a.push({ level: '提醒', cls: 'warn', title: `${s.unmanaged} 台设备未托管密码`, detail: '可能安装的是官方客户端，运维需手动输入密码', go: 'devices' })
  return a
})

const recent = computed(() =>
  (data.value?.recent || []).filter((r) => r.label.startsWith('auth')).map((r) => ({
    ...r, kind: KIND[Number(r.label.split(':')[1])] || '连接',
  })),
)

onMounted(load)
</script>

<template>
  <div class="stack">
    <div class="row"><h1>概览</h1><div class="grow"></div><button class="btn sm" @click="load">刷新</button></div>
    <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
    <div v-if="!data && !error" class="empty">正在加载…</div>
    <template v-if="data">
      <div class="stats">
        <div class="card"><div class="muted small">设备总数</div><div class="num">{{ data.stats.devices }}</div><div class="muted small">{{ data.stats.ungrouped }} 台未分组</div></div>
        <div class="card"><div class="muted small">当前在线</div><div class="num ok">{{ data.stats.online }}</div><div class="muted small">离线 {{ data.stats.devices - data.stats.online }} 台</div></div>
        <div class="card"><div class="muted small">待处理求助</div><div class="num warn">{{ data.stats.open_help }}</div><div class="muted small">员工在客户端发起</div></div>
        <div class="card"><div class="muted small">今日连接</div><div class="num">{{ data.stats.conns_today }}</div><div class="muted small">认证成功的会话</div></div>
      </div>
      <div class="cols">
        <section class="card stack">
          <h2>需要关注</h2>
          <div v-if="!alerts.length" class="muted">一切正常</div>
          <button v-for="a in alerts" :key="a.title" class="alert" :class="a.cls" @click="emit('go', a.go)">
            <span class="tag">{{ a.level }}</span>
            <span><strong>{{ a.title }}</strong><br><span class="small">{{ a.detail }}</span></span>
          </button>
          <template v-if="help.length">
            <h2>员工求助</h2>
            <div v-for="h in help" :key="h.id" class="row help">
              <div class="grow"><strong>{{ h.hostname || h.device_id }}</strong> <span class="muted small">{{ ago(h.created_at) }}</span><br><span class="small">{{ h.message || '（未填写问题描述）' }}</span></div>
              <button class="btn sm" @click="done(h)">标记已处理</button>
            </div>
          </template>
        </section>
        <section class="card stack">
          <div class="row"><h2>最近连接</h2><div class="grow"></div><button class="link" @click="emit('go', 'audit')">查看全部审计 ›</button></div>
          <div v-if="!recent.length" class="muted">暂无连接记录</div>
          <div v-for="r in recent" :key="r.id" class="rec">
            <span class="muted">{{ when(r.at) }}</span><span>{{ r.actor || '—' }}</span><span class="muted">{{ r.target }}</span><span class="muted">{{ r.kind }}</span>
          </div>
        </section>
      </div>
    </template>
  </div>
</template>

<style scoped>
.stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
.num { font-size: 30px; font-weight: 700; font-variant-numeric: tabular-nums; }
.cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 16px; align-items: start; }
.alert { display: flex; gap: 10px; text-align: left; padding: 12px; border-radius: 10px; border: 0; cursor: pointer; color: var(--text); }
.alert.danger { background: var(--danger-bg); } .alert.warn { background: var(--warn-bg); }
.tag { flex-shrink: 0; padding: 0 8px; border-radius: 6px; border: 1px solid currentColor; font-size: 12px; height: 22px; line-height: 20px; }
.alert.danger .tag { color: var(--danger); } .alert.warn .tag { color: var(--warn); }
.rec { display: grid; grid-template-columns: 96px minmax(0, 1fr) minmax(0, 1fr) 72px; gap: 8px; padding: 8px 0; border-bottom: 1px solid var(--line); font-size: 13px; }
.rec span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.help { padding: 8px 0; border-bottom: 1px solid var(--line); }
.link { border: 0; background: none; color: var(--accent); cursor: pointer; font-size: 13px; }
</style>
