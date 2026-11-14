// 管理接口客户端：相对路径（后台路径可配置），修改类请求带防伪请求头。
import { reactive } from 'vue'

export const session = reactive({ user: null, checked: false })

export class ApiError extends Error {
  constructor(status, message) {
    super(message)
    this.status = status
  }
}

async function request(method, path, body) {
  const opts = { method, headers: { 'X-MyDesk': '1' }, credentials: 'same-origin' }
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json'
    opts.body = JSON.stringify(body)
  }
  let resp
  try {
    resp = await fetch('api' + path, opts)
  } catch {
    throw new ApiError(0, '无法连接服务器，请检查网络')
  }
  const text = await resp.text()
  let data = null
  try { data = text ? JSON.parse(text) : null } catch { data = null }
  if (!resp.ok) {
    if (resp.status === 401 && path !== '/login' && path !== '/login/tfa') session.user = null
    throw new ApiError(resp.status, (data && data.error) || `请求失败（${resp.status}）`)
  }
  return data
}

export const api = {
  get: (p) => request('GET', p),
  post: (p, b = {}) => request('POST', p, b),
  patch: (p, b) => request('PATCH', p, b),
  put: (p, b) => request('PUT', p, b),
  del: (p) => request('DELETE', p),
}

/** Unix 秒 → 相对时间。 */
export function ago(ts) {
  if (!ts) return '从未'
  const d = Math.floor(Date.now() / 1000) - ts
  if (d < 60) return '刚刚'
  if (d < 3600) return `${Math.floor(d / 60)} 分钟前`
  if (d < 86400) return `${Math.floor(d / 3600)} 小时前`
  if (d < 86400 * 30) return `${Math.floor(d / 86400)} 天前`
  return new Date(ts * 1000).toLocaleDateString('zh-CN')
}

/** Unix 秒 → 月-日 时:分。 */
export function when(ts) {
  if (!ts) return ''
  const t = new Date(ts * 1000)
  const p = (n) => String(n).padStart(2, '0')
  return `${p(t.getMonth() + 1)}-${p(t.getDate())} ${p(t.getHours())}:${p(t.getMinutes())}`
}

/** RustDesk ID 分组显示：123456789 → 123 456 789。 */
export function fmtId(id) {
  return /^\d{9,}$/.test(id) ? id.replace(/(\d{3})(?=\d)/g, '$1 ') : id
}
