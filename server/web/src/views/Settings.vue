<script setup>
import { onMounted, ref } from 'vue'
import { api } from '../api.js'

const s = ref(null)
const error = ref('')
const notice = ref('')
const saving = ref(false)
const copied = ref(false)

async function load() {
  try { s.value = await api.get('/settings') } catch (e) { error.value = e.message }
}

async function save() {
  error.value = ''
  notice.value = ''
  saving.value = true
  try {
    await api.put('/settings', {
      require_admin_tfa: s.value.require_admin_tfa,
      log_credential_release: s.value.log_credential_release,
      min_client_version: s.value.min_client_version,
    })
    notice.value = '已保存'
  } catch (e) {
    error.value = e.message
  } finally {
    saving.value = false
  }
}

async function copyKey() {
  try { await navigator.clipboard.writeText(s.value.public_key); copied.value = true } catch { /* 剪贴板不可用时用户可手动选择 */ }
}

onMounted(load)
</script>

<template>
  <div class="stack wrap">
    <h1>系统设置</h1>
    <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
    <template v-if="s">
      <section class="card stack">
        <h2>服务器</h2>
        <div class="kv">
          <span class="muted">域名</span><span>{{ s.domain || '未配置（未开启自动 HTTPS）' }}</span>
          <span class="muted">服务器公钥</span><span class="row"><span class="mono grow">{{ s.public_key || '尚未生成' }}</span><button v-if="s.public_key" class="btn sm" @click="copyKey">{{ copied ? '已复制' : '复制' }}</button></span>
          <span class="muted">后台路径</span><span class="mono">/{{ s.admin_path }}/</span>
          <span class="muted">后台来源限制</span><span>{{ s.admin_allow.length ? s.admin_allow.join('，') : '不限制（可用 --admin-allow 限定公司出口 IP）' }}</span>
          <span class="muted">服务端版本</span><span>{{ s.server_version }}</span>
        </div>
        <div class="small muted">打包客户端时，把公钥填入客户端仓库变量 MYDESK_SERVER_KEY。</div>
      </section>
      <section class="card stack">
        <h2>安全</h2>
        <label class="toggle"><span class="grow"><strong>管理员必须开启两步验证</strong><br><span class="small muted">未绑定验证器的管理员登录后台时会被要求先绑定</span></span><input v-model="s.require_admin_tfa" type="checkbox" role="switch"></label>
        <label class="toggle"><span class="grow"><strong>记录每次托管密码下发</strong><br><span class="small muted">运维拉取设备库时记录下发的设备数量与来源 IP</span></span><input v-model="s.log_credential_release" type="checkbox" role="switch"></label>
        <div class="small muted">登录失败锁定（同一账号连续 5 次错误锁定 15 分钟）始终开启。</div>
      </section>
      <section class="card stack">
        <h2>客户端版本</h2>
        <label class="field"><span>最低允许版本（低于此版本的 MyDesk 客户端将被强制升级；留空不限制）</span>
          <input v-model="s.min_client_version" class="input" placeholder="例如 1.5.3" style="max-width: 200px"></label>
      </section>
      <div class="row"><button class="btn primary" :disabled="saving" @click="save">{{ saving ? '正在保存…' : '保存设置' }}</button><span v-if="notice" class="ok small" role="status">{{ notice }}</span></div>
    </template>
  </div>
</template>

<style scoped>
.wrap { max-width: 820px; }
.kv { display: grid; grid-template-columns: 120px minmax(0, 1fr); gap: 10px 12px; align-items: center; }
.toggle { display: flex; align-items: center; gap: 12px; cursor: pointer; }
.toggle input { width: 22px; height: 22px; accent-color: var(--accent); }
@media (max-width: 560px) { .kv { grid-template-columns: 1fr; } }
</style>
