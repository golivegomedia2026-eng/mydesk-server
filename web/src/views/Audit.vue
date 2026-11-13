<script setup>
import { computed, onMounted, ref, watch } from 'vue'
import { api, when } from '../api.js'

const tabs = [
  { key: 'conn', label: '连接', cols: ['控制端', '设备', '事件'] },
  { key: 'file', label: '文件传输', cols: ['控制端', '设备', '方向'] },
  { key: 'alarm', label: '告警', cols: ['设备', '设备 ID', '类型'] },
  { key: 'admin', label: '后台操作', cols: ['操作人', '动作', '对象'] },
]
const CONN_TYPE = ['远程控制', '文件传输', '端口转发', '摄像头', '终端']
const CONN_ACTION = { new: '发起连接', auth: '认证成功', close: '断开', note: '会话备注' }
const ALARM = { 0: 'IP 白名单拒绝', 1: '累计密码错误过多', 2: '1 分钟内密码错误过多', 6: 'IPv6 前缀尝试过多', 7: '终端登录退避', 8: '终端登录并发', 9: '会话越权', 10: 'ID 白名单拒绝' }
const ADMIN_ACTION = {
  'login': '客户端登录', 'admin.login': '登录后台', 'admin.login_failed': '后台登录失败', 'admin.tfa_enrolled': '绑定两步验证',
  'credential.release': '下发托管密码', 'credential.reveal': '查看托管密码', 'credential.reveal_denied': '查看密码被拒',
  'device.update': '修改设备', 'device.rotate': '要求轮换密码', 'device.disconnect': '断开设备连接', 'device.delete': '移除设备',
  'device.uuid_mismatch': '设备 ID 冒用', 'group.create': '新建分组', 'group.update': '修改分组', 'group.delete': '删除分组',
  'user.create': '新建账号', 'user.update': '修改账号', 'user.reset_password': '重置密码', 'user.reset_tfa': '重置两步验证',
  'settings.update': '修改设置', 'audit.export': '导出审计',
}

const tab = ref('conn')
const page = ref(1)
const size = 50
const total = ref(0)
const rows = ref([])
const error = ref('')
const loading = ref(false)
const cols = computed(() => tabs.find((t) => t.key === tab.value).cols)
const pages = computed(() => Math.max(1, Math.ceil(total.value / size)))

function label(r) {
  if (tab.value === 'conn') {
    const [a, t] = r.label.split(':')
    return a === 'auth' ? `${CONN_ACTION.auth} · ${CONN_TYPE[Number(t)] || ''}` : CONN_ACTION[a] || a
  }
  if (tab.value === 'file') return r.label === '0' ? '设备 → 控制端（下载）' : '控制端 → 设备（上传）'
  if (tab.value === 'alarm') return ALARM[r.label] || `类型 ${r.label}`
  return r.label
}
const target = (r) => (tab.value === 'admin' ? ADMIN_ACTION[r.target] || r.target : r.target)
const danger = (r) => tab.value === 'alarm' || /mismatch|denied|failed/.test(r.target)

async function load() {
  error.value = ''
  loading.value = true
  try {
    const r = await api.get(`/audit?kind=${tab.value}&page=${page.value}&size=${size}`)
    total.value = r.total
    rows.value = r.rows
  } catch (e) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

watch(tab, () => { page.value = 1; load() })
watch(page, load)
onMounted(load)
</script>

<template>
  <div class="stack">
    <div class="row"><h1>审计日志</h1><div class="grow"></div><a class="btn sm exp" :href="`api/audit.csv?kind=${tab}`" download>导出 CSV（最近 1000 条）</a></div>
    <div class="tabs" role="tablist">
      <button v-for="t in tabs" :key="t.key" role="tab" :aria-selected="tab === t.key" :class="{ on: tab === t.key }" @click="tab = t.key">{{ t.label }}</button>
    </div>
    <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
    <div class="table-wrap">
      <table>
        <thead><tr><th>时间</th><th>{{ cols[0] }}</th><th>{{ cols[1] }}</th><th>{{ cols[2] }}</th><th>详情</th></tr></thead>
        <tbody>
          <tr v-for="r in rows" :key="r.id">
            <td class="small muted nowrap">{{ when(r.at) }}</td>
            <td>{{ r.actor || '—' }}</td>
            <td>{{ target(r) }}</td>
            <td :class="danger(r) ? 'danger' : ''">{{ label(r) }}</td>
            <td class="small muted">{{ r.detail }}</td>
          </tr>
        </tbody>
      </table>
      <div v-if="loading" class="empty">正在加载…</div>
      <div v-else-if="!rows.length" class="empty">暂无记录</div>
    </div>
    <div class="row">
      <span class="muted small">共 {{ total }} 条</span><div class="grow"></div>
      <button class="btn sm" :disabled="page <= 1" @click="page--">上一页</button>
      <span class="small">{{ page }} / {{ pages }}</span>
      <button class="btn sm" :disabled="page >= pages" @click="page++">下一页</button>
    </div>
  </div>
</template>

<style scoped>
.tabs { display: flex; flex-wrap: wrap; gap: 4px; padding: 4px; border-radius: 10px; background: var(--inset); align-self: flex-start; }
.tabs button { height: 36px; padding: 0 16px; border-radius: 8px; border: 0; background: transparent; color: var(--text-2); cursor: pointer; }
.tabs button.on { background: var(--surface); color: var(--accent); font-weight: 700; }
.nowrap { white-space: nowrap; }
.exp { display: inline-flex; align-items: center; text-decoration: none; }
</style>
