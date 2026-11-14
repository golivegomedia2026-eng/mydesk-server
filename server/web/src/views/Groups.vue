<script setup>
import { onMounted, reactive, ref } from 'vue'
import { api } from '../api.js'

const groups = ref([])
const error = ref('')
const loading = ref(true)
const edit = reactive({ open: false, id: null, name: '', note: '', error: '' })
const del = reactive({ open: false, g: null })

async function load() {
  error.value = ''
  try { groups.value = await api.get('/groups') } catch (e) { error.value = e.message } finally { loading.value = false }
}

function openEdit(g) {
  Object.assign(edit, { open: true, id: g?.id ?? null, name: g?.name ?? '', note: g?.note ?? '', error: '' })
}

async function save() {
  edit.error = ''
  try {
    const body = { name: edit.name.trim(), note: edit.note.trim() }
    if (edit.id === null) await api.post('/groups', body)
    else await api.patch(`/groups/${edit.id}`, body)
    edit.open = false
    await load()
  } catch (e) {
    edit.error = e.message
  }
}

async function remove() {
  try { await api.del(`/groups/${del.g.id}`); del.open = false; await load() } catch (e) { error.value = e.message; del.open = false }
}

onMounted(load)
</script>

<template>
  <div class="stack">
    <div class="row"><h1>分组</h1><div class="grow"></div><button class="btn primary" @click="openEdit(null)">新建分组</button></div>
    <div v-if="error" class="banner danger" role="alert">{{ error }}</div>
    <div v-if="loading" class="empty">正在加载…</div>
    <div v-else-if="!groups.length" class="empty">还没有分组。新建分组后，把设备分进去，再给运维授权该分组。</div>
    <div class="grid">
      <div v-for="g in groups" :key="g.id" class="card stack g">
        <div class="row"><strong class="grow" style="font-size: 16px">{{ g.name }}</strong><span class="muted small">{{ g.devices }} 台 · {{ g.online }} 在线</span></div>
        <div v-if="g.note" class="small">{{ g.note }}</div>
        <div class="small">可访问的运维：<span :class="g.operators.length ? '' : 'warn'">{{ g.operators.join('、') || '暂无（只有管理员可见）' }}</span></div>
        <div class="small muted">部署自动归组：preset-device-group-name = <span class="mono">{{ g.name }}</span></div>
        <div class="row"><button class="btn sm" @click="openEdit(g)">编辑</button><button class="btn sm danger" @click="Object.assign(del, { open: true, g })">删除</button></div>
      </div>
    </div>
    <div class="banner info">新上线、未指定分组的设备进入“未分组”，只有管理员可见。给设备分组后，被授权该分组的运维才能看到并免密连接。运维的分组权限在“用户与权限”中设置。</div>

    <div v-if="edit.open" class="modal-mask" @click.self="edit.open = false">
      <form class="modal" role="dialog" aria-modal="true" aria-labelledby="ge-title" @submit.prevent="save">
        <h2 id="ge-title">{{ edit.id === null ? '新建分组' : '编辑分组' }}</h2>
        <label class="field"><span>名称</span><input v-model="edit.name" class="input" maxlength="32" required autofocus></label>
        <label class="field"><span>说明（可选）</span><input v-model="edit.note" class="input" maxlength="200"></label>
        <div v-if="edit.error" class="banner danger" role="alert">{{ edit.error }}</div>
        <div class="row"><div class="grow"></div><button type="button" class="btn" @click="edit.open = false">取消</button><button class="btn primary">保存</button></div>
      </form>
    </div>
    <div v-if="del.open" class="modal-mask" @click.self="del.open = false">
      <div class="modal" role="alertdialog" aria-modal="true" aria-labelledby="gd-title">
        <h2 id="gd-title">删除分组“{{ del.g.name }}”</h2>
        <div>组内 {{ del.g.devices }} 台设备会转为“未分组”，运维将无法再看到它们。</div>
        <div class="row"><div class="grow"></div><button class="btn" @click="del.open = false">取消</button><button class="btn danger" @click="remove">删除</button></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
.g { gap: 8px; }
</style>
