<script setup>
import { onMounted, reactive, ref } from 'vue'
import { api, ago } from '../api.js'

const users = ref([])
const groups = ref([])
const error = ref('')
const loading = ref(true)
const edit = reactive({ open: false, id: null, username: '', display_name: '', role: 'operator', status: 1, group_ids: [], error: '' })
const shown = reactive({ open: false, title: '', password: '' })
const confirm = reactive({ open: false, title: '', text: '', action: null })

async function load() {
  error.value = ''
  try { [users.value, groups.value] = await Promise.all([api.get('/users'), api.get('/groups')]) } catch (e) { error.value = e.message } finally { loading.value = false }
}

const groupNames = (ids) => ids.map((id) => groups.value.find((g) => g.id === id)?.name).filter(Boolean).join('、')

function openEdit(u) {
  Object.assign(edit, u
    ? { open: true, id: u.id, username: u.username, display_name: u.display_name, role: u.role, status: u.status, group_ids: [...u.group_ids], error: '' }
    : { open: true, id: null, username: '', display_name: '', role: 'operator', status: 1, group_ids: [], error: '' })
}

async function save() {
  edit.error = ''
  try {
    if (edit.id === null) {
      const r = await api.post('/users', { username: edit.username.trim(), display_name: edit.display_name.trim(), role: edit.role, group_ids: edit.group_ids })
      Object.assign(shown, { open: true, title: `账号 ${edit.username.trim()} 已创建`, password: r.password })
    } else {
      await api.patch(`/users/${edit.id}`, { display_name: edit.display_name.trim(), role: edit.role, status: edit.status, group_ids: edit.group_ids })
    }
    edit.open = false
    await load()
  } catch (e) {
    edit.error = e.message
  }
}

function ask(title, text, action) { Object.assign(confirm, { open: true, title, text, action }) }
async function runConfirm() {
  const a = confirm.action
  confirm.open = false
  try { await a() } catch (e) { error.value = e.message }
}

const resetPassword = (u) => ask(`重置 ${u.username} 的密码`, '将生成新的随机密码，该账号已登录的客户端和后台会话全部失效。', async () => {
  const r = await api.post(`/users/${u.id}/reset-password`)
  Object.assign(shown, { open: true, title: `${u.username} 的新密码`, password: r.password })
})
const resetTfa = (u) => ask(`重置 ${u.username} 的两步验证`, '该账号下次登录后台时需要重新绑定验证器（手机丢失时使用）。', async () => {
  await api.post(`/users/${u.id}/reset-tfa`)
  await load()
})

onMounted(load)
</script>

<template>
  <div class="stack">
    <div class="row"><h1>用户与权限</h1><div class="grow"></div><button class="btn primary" @click="openEdit(null)">新建运维账号</button></div>
    <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
    <div class="table-wrap">
      <table>
        <thead><tr><th>用户名</th><th>姓名</th><th>角色</th><th>可访问分组</th><th>两步验证</th><th>状态</th><th>最后登录</th><th></th></tr></thead>
        <tbody>
          <tr v-for="u in users" :key="u.id">
            <td><strong>{{ u.username }}</strong></td>
            <td>{{ u.display_name }}</td>
            <td :class="u.role === 'admin' ? 'ok' : ''">{{ u.role === 'admin' ? '管理员' : '运维' }}</td>
            <td class="small">{{ u.role === 'admin' ? '全部（含未分组）' : (groupNames(u.group_ids) || '无') }}</td>
            <td class="small" :class="u.tfa ? 'ok' : 'warn'">{{ u.tfa ? '已开启' : '未开启' }}</td>
            <td class="small" :class="u.status === 1 ? 'ok' : 'muted'">{{ u.status === 1 ? '启用' : '停用' }}</td>
            <td class="small muted">{{ ago(u.last_login_at) }}</td>
            <td><div class="row nowrap">
              <button class="btn sm" @click="openEdit(u)">编辑</button>
              <button class="btn sm" @click="resetPassword(u)">重置密码</button>
              <button v-if="u.tfa" class="btn sm" @click="resetTfa(u)">重置两步验证</button>
            </div></td>
          </tr>
        </tbody>
      </table>
      <div v-if="loading" class="empty">正在加载…</div>
    </div>
    <div class="banner warn">停用账号或移除分组权限后，该运维的客户端会在下一次同步地址簿时失去对应设备与托管密码；停用会立即吊销其全部登录。</div>

    <div v-if="edit.open" class="modal-mask" @click.self="edit.open = false">
      <form class="modal" role="dialog" aria-modal="true" aria-labelledby="ue-title" @submit.prevent="save">
        <h2 id="ue-title">{{ edit.id === null ? '新建账号' : '编辑 ' + edit.username }}</h2>
        <label v-if="edit.id === null" class="field"><span>用户名（字母、数字、. _ -）</span><input v-model="edit.username" class="input" maxlength="32" pattern="[A-Za-z0-9._\-]+" required autofocus></label>
        <label class="field"><span>姓名</span><input v-model="edit.display_name" class="input" maxlength="32"></label>
        <label class="field"><span>角色</span>
          <select v-model="edit.role" class="input"><option value="operator">运维（只能看到被授权的分组）</option><option value="admin">管理员（全部设备 + 后台）</option></select>
        </label>
        <fieldset v-if="edit.role === 'operator'" class="field fs">
          <legend class="small muted">可访问的分组</legend>
          <label v-for="g in groups" :key="g.id" class="row small"><input v-model="edit.group_ids" type="checkbox" :value="g.id"> {{ g.name }}</label>
          <span v-if="!groups.length" class="small muted">还没有分组，请先到“分组”中新建。</span>
        </fieldset>
        <label v-if="edit.id !== null" class="row small"><input type="checkbox" :checked="edit.status === 1" @change="edit.status = $event.target.checked ? 1 : 0"> 启用</label>
        <div v-if="edit.error" class="banner danger" role="alert">{{ edit.error }}</div>
        <div class="row"><div class="grow"></div><button type="button" class="btn" @click="edit.open = false">取消</button><button class="btn primary">保存</button></div>
      </form>
    </div>

    <div v-if="shown.open" class="modal-mask">
      <div class="modal" role="dialog" aria-modal="true" aria-labelledby="pw-title">
        <h2 id="pw-title">{{ shown.title }}</h2>
        <div class="mono pw">{{ shown.password }}</div>
        <div class="banner warn">密码只显示这一次。请通过安全渠道交给本人，并提醒其登录后尽快修改。</div>
        <div class="row"><div class="grow"></div><button class="btn primary" @click="shown.open = false">我已记下</button></div>
      </div>
    </div>

    <div v-if="confirm.open" class="modal-mask" @click.self="confirm.open = false">
      <div class="modal" role="alertdialog" aria-modal="true" aria-labelledby="uc-title">
        <h2 id="uc-title">{{ confirm.title }}</h2>
        <div>{{ confirm.text }}</div>
        <div class="row"><div class="grow"></div><button class="btn" @click="confirm.open = false">取消</button><button class="btn primary" @click="runConfirm">确定</button></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.nowrap { flex-wrap: nowrap; justify-content: flex-end; }
.fs { border: 1px solid var(--line); border-radius: 8px; padding: 10px; gap: 6px; }
.pw { font-size: 20px; padding: 12px; background: var(--inset); border-radius: 8px; text-align: center; user-select: all; }
</style>
