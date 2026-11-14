<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { api, ago, fmtId } from '../api.js'

const devices = ref([])
const groups = ref([])
const error = ref('')
const notice = ref('')
const loading = ref(true)
const q = ref('')
const groupFilter = ref('all')
const unmanagedOnly = ref(false)
const selId = ref(null)
const form = reactive({ alias: '', owner_name: '', note: '', group_id: null })
const saving = ref(false)
const reveal = reactive({ open: false, code: '', password: '', error: '' })
const confirm = reactive({ open: false, title: '', text: '', action: null })

async function load() {
  error.value = ''
  try {
    ;[devices.value, groups.value] = await Promise.all([api.get('/devices'), api.get('/groups')])
  } catch (e) {
    error.value = e.message
  } finally {
    loading.value = false
  }
}

const list = computed(() => {
  const k = q.value.trim().toLowerCase()
  return devices.value.filter((d) => {
    if (groupFilter.value === 'none' && d.group_id !== null) return false
    if (typeof groupFilter.value === 'number' && d.group_id !== groupFilter.value) return false
    if (unmanagedOnly.value && d.managed) return false
    if (k && !`${d.id} ${d.alias} ${d.hostname} ${d.owner_name}`.toLowerCase().includes(k)) return false
    return true
  })
})
const sel = computed(() => devices.value.find((d) => d.id === selId.value) || null)
const name = (d) => d.alias || d.hostname || d.id

function open(d) {
  selId.value = d.id
  Object.assign(form, { alias: d.alias, owner_name: d.owner_name, note: d.note, group_id: d.group_id })
  notice.value = ''
}

async function save() {
  saving.value = true
  error.value = ''
  try {
    await api.patch(`/devices/${encodeURIComponent(sel.value.id)}`, { ...form })
    notice.value = '已保存'
    await load()
  } catch (e) {
    error.value = e.message
  } finally {
    saving.value = false
  }
}

function ask(title, text, action) {
  Object.assign(confirm, { open: true, title, text, action })
}
async function runConfirm() {
  const a = confirm.action
  confirm.open = false
  try { await a() } catch (e) { error.value = e.message }
}

const rotate = () => ask('轮换托管密码', `设备在下一次心跳（约 15 秒内）生成新密码并上报，旧密码立即失效。正在进行的连接不受影响。`, async () => {
  await api.post(`/devices/${encodeURIComponent(sel.value.id)}/rotate`)
  notice.value = '已下达轮换指令'
  await load()
})
const disconnect = () => ask('断开当前所有连接', `设备上正在进行的远程连接会在下一次心跳时被断开。`, async () => {
  await api.post(`/devices/${encodeURIComponent(sel.value.id)}/disconnect`)
  notice.value = '已下达断开指令'
})
const remove = () => ask('移除设备', `从设备库移除 ${name(sel.value)}。设备若仍在运行，下一次上报时会重新登记为“未分组”。`, async () => {
  await api.del(`/devices/${encodeURIComponent(sel.value.id)}`)
  selId.value = null
  await load()
})

async function doReveal() {
  reveal.error = ''
  try {
    const r = await api.post(`/devices/${encodeURIComponent(sel.value.id)}/reveal`, { code: reveal.code.trim() })
    reveal.password = r.password
  } catch (e) {
    reveal.error = e.message
  }
}
function closeReveal() {
  Object.assign(reveal, { open: false, code: '', password: '', error: '' })
}

onMounted(load)
</script>

<template>
  <div class="stack">
    <div class="row"><h1>设备</h1><span class="muted">{{ list.length }} 台</span><div class="grow"></div><button class="btn sm" @click="load">刷新</button></div>
    <div class="row">
      <label class="sr-only" for="dq">搜索设备</label>
      <input id="dq" v-model="q" class="input search" placeholder="搜索名称、ID、使用人">
      <button class="chip" :class="{ on: groupFilter === 'all' }" @click="groupFilter = 'all'">全部</button>
      <button v-for="g in groups" :key="g.id" class="chip" :class="{ on: groupFilter === g.id }" @click="groupFilter = g.id">{{ g.name }}</button>
      <button class="chip" :class="{ on: groupFilter === 'none' }" @click="groupFilter = 'none'">未分组</button>
      <button class="chip" :class="{ on: unmanagedOnly }" :aria-pressed="unmanagedOnly" @click="unmanagedOnly = !unmanagedOnly">仅看未托管</button>
    </div>
    <div v-if="error" class="banner danger" role="alert">{{ error }}</div>

    <div class="layout">
      <div class="table-wrap grow">
        <table>
          <thead><tr><th>设备</th><th>ID</th><th>系统</th><th>分组</th><th>使用人</th><th>托管密码</th><th>最后在线</th><th></th></tr></thead>
          <tbody>
            <tr v-for="d in list" :key="d.id" :class="{ sel: d.id === selId }">
              <td><span class="dot" :class="{ on: d.online }" :aria-label="d.online ? '在线' : '离线'"></span><strong>{{ name(d) }}</strong></td>
              <td>{{ fmtId(d.id) }}</td>
              <td class="muted">{{ (d.os.split(' / ')[1] || d.os).split(' - ')[0] }}</td>
              <td :class="d.group_id === null ? 'warn' : ''">{{ d.group_name || '未分组' }}</td>
              <td>{{ d.owner_name }}</td>
              <td class="small" :class="d.managed ? 'ok' : 'warn'">{{ d.rotate_pending ? '等待轮换…' : d.managed ? '已托管 · ' + ago(d.secret_at) : '未托管' }}</td>
              <td class="small muted">{{ d.online ? '在线' : ago(d.last_online_at) }}</td>
              <td><button class="btn sm" @click="open(d)">管理</button></td>
            </tr>
          </tbody>
        </table>
        <div v-if="loading" class="empty">正在加载…</div>
        <div v-else-if="!list.length" class="empty">{{ devices.length ? '没有符合条件的设备' : '还没有设备上线。安装 MyDesk 客户端后会自动出现在这里。' }}</div>
      </div>

      <aside v-if="sel" class="card stack side" aria-label="设备详情">
        <div class="row">
          <div class="grow"><strong style="font-size: 17px">{{ name(sel) }}</strong><div class="small muted">ID {{ fmtId(sel.id) }} · 客户端 {{ sel.version || '未知' }}</div></div>
          <button class="btn sm" aria-label="关闭详情" @click="selId = null">关闭</button>
        </div>
        <label class="field"><span>显示名称</span><input v-model="form.alias" class="input" :placeholder="sel.hostname" maxlength="64"></label>
        <label class="field"><span>使用人</span><input v-model="form.owner_name" class="input" maxlength="64"></label>
        <label class="field"><span>分组（决定哪些运维可以看到并连接）</span>
          <select v-model="form.group_id" class="input">
            <option :value="null">未分组</option>
            <option v-for="g in groups" :key="g.id" :value="g.id">{{ g.name }}</option>
          </select>
        </label>
        <label class="field"><span>备注</span><input v-model="form.note" class="input" placeholder="例如：工位 3F-12" maxlength="200"></label>
        <button class="btn primary" :disabled="saving" @click="save">{{ saving ? '正在保存…' : '保存' }}</button>
        <div v-if="notice" class="banner ok" role="status">{{ notice }}</div>

        <div class="box stack">
          <strong>托管密码</strong>
          <span class="small" :class="sel.managed ? 'ok' : 'warn'">{{ sel.managed ? '已托管 · 更新于 ' + ago(sel.secret_at) : '未托管（官方客户端或尚未上报）' }}</span>
          <span class="small muted">密码由设备生成并加密保存，只下发给有该分组权限的运维；每次下发与查看都记入审计。</span>
          <div class="row">
            <button class="btn sm" :disabled="!sel.managed" @click="rotate">要求设备轮换密码</button>
            <button class="btn sm" :disabled="!sel.managed" @click="reveal.open = true">查看密码（需两步验证）</button>
          </div>
        </div>
        <div class="box danger-box stack">
          <strong class="danger">危险操作</strong>
          <div class="row">
            <button class="btn sm danger" @click="disconnect">断开当前所有连接</button>
            <button class="btn sm danger" @click="remove">移除设备</button>
          </div>
        </div>
      </aside>
    </div>

    <div v-if="reveal.open" class="modal-mask" @click.self="closeReveal">
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="rv-title">
        <h2 id="rv-title">查看托管密码</h2>
        <template v-if="!reveal.password">
          <div class="small">输入你的两步验证动态码。本次查看会记入审计。</div>
          <input v-model="reveal.code" class="input" inputmode="numeric" maxlength="6" placeholder="000000" aria-label="动态码" @keyup.enter="doReveal">
          <div v-if="reveal.error" class="banner danger" role="alert">{{ reveal.error }}</div>
          <div class="row"><div class="grow"></div><button class="btn" @click="closeReveal">取消</button><button class="btn primary" :disabled="reveal.code.trim().length !== 6" @click="doReveal">查看</button></div>
        </template>
        <template v-else>
          <div class="mono pw">{{ reveal.password }}</div>
          <div class="banner warn">请勿通过聊天工具明文传播。用完后建议对该设备执行“轮换密码”。</div>
          <div class="row"><div class="grow"></div><button class="btn primary" @click="closeReveal">关闭</button></div>
        </template>
      </div>
    </div>

    <div v-if="confirm.open" class="modal-mask" @click.self="confirm.open = false">
      <div class="modal" role="alertdialog" aria-modal="true" aria-labelledby="cf-title">
        <h2 id="cf-title">{{ confirm.title }}</h2>
        <div>{{ confirm.text }}</div>
        <div class="row"><div class="grow"></div><button class="btn" @click="confirm.open = false">取消</button><button class="btn primary" @click="runConfirm">确定</button></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.search { flex: 1 1 240px; max-width: 360px; }
.layout { display: flex; flex-wrap: wrap; gap: 16px; align-items: flex-start; }
.layout > .table-wrap { flex: 999 1 560px; min-width: 0; }
.side { flex: 1 1 320px; max-width: 380px; }
.box { padding: 14px; border-radius: 10px; background: var(--inset); gap: 8px; }
.danger-box { background: transparent; border: 1px solid var(--danger); }
.pw { font-size: 22px; padding: 12px; background: var(--inset); border-radius: 8px; text-align: center; user-select: all; }
</style>
