<script setup>
// 登录：账号密码 → 两步验证（首次登录先扫码绑定验证器）
import { ref } from 'vue'
import { api } from '../api.js'

const emit = defineEmits(['done'])
const step = ref('password')
const username = ref('')
const password = ref('')
const code = ref('')
const pending = ref('')
const enroll = ref(null)
const error = ref('')
const busy = ref(false)

async function submitPassword() {
  error.value = ''
  busy.value = true
  try {
    const r = await api.post('/login', { username: username.value.trim(), password: password.value })
    if (r.ok) return emit('done')
    pending.value = r.pending
    enroll.value = r.step === 'enroll' ? { secret: r.secret, qr: r.qr_svg } : null
    step.value = 'tfa'
  } catch (e) {
    error.value = e.message
  } finally {
    busy.value = false
  }
}

async function submitCode() {
  error.value = ''
  busy.value = true
  try {
    await api.post('/login/tfa', { pending: pending.value, code: code.value.trim() })
    emit('done')
  } catch (e) {
    error.value = e.message
    if (e.status === 401 && /过期/.test(e.message)) back()
  } finally {
    busy.value = false
  }
}

function back() {
  step.value = 'password'
  code.value = ''
  enroll.value = null
}
</script>

<template>
  <div class="wrap">
    <div class="box stack">
      <div class="row">
        <svg width="40" height="40" viewBox="0 0 36 36" aria-hidden="true"><rect x="1" y="1" width="34" height="34" rx="9" fill="var(--accent)" /><rect x="9" y="10" width="18" height="12" rx="2" fill="none" stroke="var(--on-accent)" stroke-width="2" /><path d="M14 27h8M18 22v5" stroke="var(--on-accent)" stroke-width="2" stroke-linecap="round" /></svg>
        <div><h1 style="font-size: 22px">MyDesk 管理后台</h1><div class="muted small">仅限管理员登录</div></div>
      </div>

      <form v-if="step === 'password'" class="card stack" @submit.prevent="submitPassword">
        <label class="field"><span>用户名</span><input v-model="username" class="input" autocomplete="username" required autofocus></label>
        <label class="field"><span>密码</span><input v-model="password" class="input" type="password" autocomplete="current-password" required></label>
        <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
        <button class="btn primary" :disabled="busy">{{ busy ? '正在验证…' : '下一步' }}</button>
      </form>

      <form v-else class="card stack" @submit.prevent="submitCode">
        <strong>两步验证</strong>
        <template v-if="enroll">
          <div class="small">首次登录需绑定验证器：用 Google Authenticator、Microsoft Authenticator 或“腾讯身份验证器”扫描下方二维码，然后输入显示的 6 位动态码。</div>
          <div class="qr" v-html="enroll.qr"></div>
          <div class="small muted">无法扫码时手动输入密钥：<span class="mono">{{ enroll.secret }}</span></div>
        </template>
        <div v-else class="small">打开手机上的验证器，输入 6 位动态码。</div>
        <label class="field"><span>动态码</span>
          <input v-model="code" class="input code" inputmode="numeric" autocomplete="one-time-code" maxlength="6" placeholder="000000" required autofocus>
        </label>
        <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
        <button class="btn primary" :disabled="busy || code.trim().length !== 6">{{ busy ? '正在验证…' : '登录' }}</button>
        <button type="button" class="btn" @click="back">返回修改账号</button>
      </form>

      <div class="banner warn">连续 5 次错误将锁定 15 分钟。忘记密码可在服务器执行 mydesk-server --reset-password admin。</div>
    </div>
  </div>
</template>

<style scoped>
.wrap { min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 16px; }
.box { width: min(420px, 100%); }
.code { height: 48px; font-size: 22px; letter-spacing: 8px; text-align: center; }
.qr { align-self: center; width: 200px; background: #fff; border-radius: 8px; padding: 6px; }
.qr :deep(svg) { width: 100%; height: auto; display: block; }
</style>
