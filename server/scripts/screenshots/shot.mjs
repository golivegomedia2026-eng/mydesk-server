// 管理后台真实界面截图：浅色全部页面 + 设备详情 + 深色 + 手机窄屏，并检查横向溢出。
// 用法: node shot.mjs <工作目录（seed.sh 写入 admin.pw/admin.totp）> <输出目录>
// 环境变量 CHROME 指定浏览器可执行文件；FONT 指定中文字体文件（系统已有中文字体时可不设）。
import { chromium } from 'playwright-core'
import crypto from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

const demo = process.argv[2]
const out = process.argv[3]
const BASE = 'http://127.0.0.1:21114/mydeskadmin/'
const pw = fs.readFileSync(path.join(demo, 'admin.pw'), 'utf8').trim()
const secret = fs.readFileSync(path.join(demo, 'admin.totp'), 'utf8').trim()
const font = process.env.FONT ? fs.readFileSync(process.env.FONT) : null
const exe = process.env.CHROME || '/usr/bin/google-chrome'

function totp(b32) {
  const alpha = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
  let bits = ''
  for (const c of b32) bits += alpha.indexOf(c).toString(2).padStart(5, '0')
  const key = Buffer.from(bits.match(/.{8}/g).map((b) => parseInt(b, 2)))
  const ctr = Buffer.alloc(8)
  ctr.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)))
  const h = crypto.createHmac('sha1', key).update(ctr).digest()
  const o = h[19] & 15
  return String((h.readUInt32BE(o) & 0x7fffffff) % 1000000).padStart(6, '0')
}

async function prep(ctx) {
  if (!font) return
  // 同源提供中文字体（后台 CSP 只允许同源资源）
  await ctx.route('**/__shotfont.ttf', (r) => r.fulfill({ body: font, contentType: 'font/ttf' }))
}
async function addFont(page) {
  if (!font) return
  await page.addStyleTag({ content: "@font-face{font-family:'Noto Sans SC';src:url('/mydeskadmin/__shotfont.ttf')} @font-face{font-family:'Noto Serif SC';src:url('/mydeskadmin/__shotfont.ttf');font-weight:700}" })
  await page.evaluate(() => document.fonts.ready)
}

async function overflow(page, label) {
  const bad = await page.evaluate(() => {
    const vw = document.documentElement.clientWidth
    const r = []
    if (document.documentElement.scrollWidth > vw + 1) r.push(`页面宽 ${document.documentElement.scrollWidth} > ${vw}`)
    for (const el of document.querySelectorAll('body *')) {
      if (el.closest('.table-wrap')) continue // 表格在可横向滚动的容器里
      const b = el.getBoundingClientRect()
      if (b.right > vw + 1 && b.width > 0) r.push(`${el.tagName.toLowerCase()}.${el.className} right=${Math.round(b.right)}`)
    }
    return r.slice(0, 5)
  })
  console.log(bad.length ? `溢出 ${label}: ${bad.join(' | ')}` : `无溢出 ${label}`)
}

const browser = await chromium.launch({ executablePath: exe })
fs.mkdirSync(out, { recursive: true })
const shot = async (page, name) => { await page.waitForTimeout(400); await page.screenshot({ path: path.join(out, name + '.png') }); console.log('截图', name) }

async function login(ctx) {
  const page = await ctx.newPage()
  await page.goto(BASE)
  await addFont(page)
  return page
}

// 浅色桌面
const light = await browser.newContext({ viewport: { width: 1360, height: 880 }, colorScheme: 'light', locale: 'zh-CN' })
await prep(light)
let page = await login(light)
await page.fill('input[autocomplete=username]', 'admin')
await page.fill('input[autocomplete=current-password]', pw)
await shot(page, 'admin-login')
await page.click('button:has-text("下一步")')
await page.waitForSelector('input[autocomplete=one-time-code]')
await page.fill('input[autocomplete=one-time-code]', totp(secret))
await shot(page, 'admin-login-tfa')
await page.click('button:has-text("登录")')
await page.waitForSelector('nav.nav')
await addFont(page)
for (const [label, name] of [['概览', 'admin-overview'], ['设备', 'admin-devices'], ['分组', 'admin-groups'], ['用户与权限', 'admin-users'], ['审计日志', 'admin-audit'], ['系统设置', 'admin-settings']]) {
  await page.click(`nav.nav > button:has-text("${label}")`)
  await page.waitForTimeout(600)
  await overflow(page, `${name}@1360`)
  await shot(page, name)
  if (name === 'admin-devices') {
    await page.click('tr:has-text("PC-财务-01") button:has-text("管理")')
    await shot(page, 'admin-device-detail')
  }
}
const state = await light.storageState()

// 深色桌面
const dark = await browser.newContext({ viewport: { width: 1360, height: 880 }, colorScheme: 'dark', locale: 'zh-CN', storageState: state })
await prep(dark)
page = await login(dark)
await page.waitForSelector('nav.nav')
await page.click('nav.nav > button:has-text("设备")')
await page.waitForTimeout(600)
await shot(page, 'admin-devices-dark')

// 手机窄屏
const phone = await browser.newContext({ viewport: { width: 390, height: 844 }, colorScheme: 'light', locale: 'zh-CN', storageState: state, deviceScaleFactor: 2 })
await prep(phone)
page = await login(phone)
await page.waitForSelector('nav.nav')
await page.waitForTimeout(600)
await overflow(page, 'admin-overview@390')
await shot(page, 'admin-overview-phone')

await browser.close()
