<script setup lang="ts">
/**
 * 「LLM API」侧边面板：
 *
 *   1. 顶部：当前 active profile（可一键切换 / 取消）
 *   2. 我的 profile 列表（用户在 UI 里添加的）
 *   3. 「+ 新增」按钮 → LLMProfileDialog
 *   4. 「系统检测」一栏：env / dotenv / cursor-agent，每条都能「导入为 profile」
 *
 * 这里不直接调 LLM，只管理「用哪份配置」。真正的对话/生成走 ChatPanel。
 */

import { computed, onMounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useI18n } from 'vue-i18n';
import { detectLocalLLM, type LocalDetect } from '../api/llm';
import { useLLMProfiles, type LLMProfile } from '../composables/useLLMProfiles';
import LLMProfileDialog from './LLMProfileDialog.vue';

const { t } = useI18n();
const llm = useLLMProfiles();

const dialogVisible = ref(false);
const editing = ref<LLMProfile | null>(null);

const detect = ref<LocalDetect | null>(null);
const detecting = ref(false);

async function refreshDetect() {
  detecting.value = true;
  try {
    detect.value = await detectLocalLLM();
  } catch (e: any) {
    ElMessage.error(e?.message || String(e));
  } finally {
    detecting.value = false;
  }
}

onMounted(refreshDetect);

const detectedCursorBin = computed(() => detect.value?.cursor?.binPath ?? null);

function openAdd() {
  editing.value = null;
  dialogVisible.value = true;
}

function openEdit(p: LLMProfile) {
  editing.value = p;
  dialogVisible.value = true;
}

function onSave(payload: Omit<LLMProfile, 'id' | 'createdAt' | 'updatedAt'> & { id?: string }) {
  if (payload.id) {
    llm.update(payload.id, payload);
    ElMessage.success(t('sidebar.saved'));
  } else {
    const created = llm.add(payload);
    if (!llm.active.value) llm.setActive(created.id);
    ElMessage.success(t('sidebar.addedActivated'));
  }
}

async function removeProfile(p: LLMProfile) {
  try {
    await ElMessageBox.confirm(
      t('llm.confirmDelete', { name: p.name }),
      t('sidebar.confirmTitle'),
      { type: 'warning' }
    );
    llm.remove(p.id);
    ElMessage.success(t('sidebar.deleted'));
  } catch {
    /* user cancelled */
  }
}

function activate(p: LLMProfile) {
  llm.setActive(p.id);
  ElMessage.success(t('llm.activated', { name: p.name }));
}

function deactivate() {
  llm.setActive(null);
  ElMessage.success(t('llm.deactivated'));
}

/* ---------------- 从系统检测一键导入 ---------------- */

function importDetectedOpenAI() {
  const d = detect.value?.openai;
  if (!d) return;
  // env 里的 apiKey 已经被脱敏（只回了 sk-…abcd），用户得自己在弹窗里补全
  editing.value = null;
  dialogVisible.value = true;
  // 通过下一帧的 watch 让 Dialog 拉到初值；这里我们手工构造一个 profile-like 然后让 Dialog 当作编辑
  // 但为了简单：直接 push 一条新 profile，apiKey 留空（用户必须重新粘贴），让用户在 dialog 里完成
  // —— 实际更友好：直接给 Dialog 传 prefill。这里我们用 editing 的方式：
  const prefill: LLMProfile = {
    id: '',
    name: t('llm.importedFromSystem'),
    providerKind: 'openai',
    baseUrl: d.baseUrl,
    model: d.model,
    apiKey: '',
    timeout: 60,
    createdAt: 0,
    updatedAt: 0,
  };
  editing.value = prefill;
}

function importDetectedCursor() {
  const d = detect.value?.cursor;
  if (!d) return;
  const prefill: LLMProfile = {
    id: '',
    name: 'cursor-agent',
    providerKind: 'cursor',
    binPath: d.binPath,
    timeout: 120,
    createdAt: 0,
    updatedAt: 0,
  };
  editing.value = prefill;
  dialogVisible.value = true;
}

const cursorBadge = computed(() => {
  const c = detect.value?.cursor;
  if (!c) return null;
  if (c.loggedIn === true) return { label: t('llm.cursorLoggedIn'), tone: 'ok' as const };
  if (c.loggedIn === false) return { label: t('llm.cursorNotLoggedIn'), tone: 'warn' as const };
  return { label: t('llm.cursorUnknown'), tone: 'mute' as const };
});
</script>

<template>
  <div class="content">
    <!-- active -->
    <div class="section-head">
      <span class="section-title">{{ t('llm.activeTitle') }}</span>
      <button class="mini-btn" @click="refreshDetect" :disabled="detecting">
        {{ detecting ? t('sidebar.loading') : t('sidebar.refresh') }}
      </button>
    </div>

    <div v-if="llm.active.value" class="active-card">
      <div class="active-line">
        <span class="dot ok" />
        <span class="active-name">{{ llm.active.value.name }}</span>
        <span class="prov-badge">{{ llm.active.value.providerKind }}</span>
      </div>
      <div class="active-meta mono" :title="llm.active.value.model || llm.active.value.binPath">
        {{
          llm.active.value.providerKind === 'cursor'
            ? llm.active.value.binPath || '(cursor-agent)'
            : `${llm.active.value.model || '?'}  @  ${llm.active.value.baseUrl || '?'}`
        }}
      </div>
      <div class="active-actions">
        <button class="link" @click="openEdit(llm.active.value!)">{{ t('sidebar.edit') }}</button>
        <button class="link" @click="deactivate">{{ t('llm.deactivate') }}</button>
      </div>
    </div>
    <div v-else class="active-card inactive">
      <div class="active-line">
        <span class="dot off" />
        <span class="active-name muted">{{ t('llm.noActive') }}</span>
      </div>
      <div class="active-meta muted">{{ t('llm.noActiveHint') }}</div>
    </div>

    <!-- 我的 profiles -->
    <div class="section-head with-margin">
      <span class="section-title">{{ t('llm.myProfiles') }}</span>
      <button class="mini-btn" @click="openAdd">{{ t('sidebar.add') }}</button>
    </div>

    <div v-if="llm.profiles.value.length === 0" class="empty">
      <p>{{ t('llm.empty') }}</p>
      <el-button type="primary" size="small" @click="openAdd">{{ t('llm.addFirst') }}</el-button>
    </div>

    <ul v-else class="prof-list">
      <li
        v-for="p in llm.profiles.value"
        :key="p.id"
        :class="['prof', { active: p.id === llm.activeId.value }]"
        @click="activate(p)"
      >
        <div class="prof-head">
          <span class="dot" :class="{ ok: p.id === llm.activeId.value }" />
          <span class="prof-name">{{ p.name }}</span>
          <span class="prov-badge">{{ p.providerKind }}</span>
        </div>
        <div class="prof-meta mono" :title="p.model || p.binPath || ''">
          {{
            p.providerKind === 'cursor'
              ? p.binPath || '(cursor-agent)'
              : `${p.model || '?'}  @  ${p.baseUrl || '?'}`
          }}
        </div>
        <div class="prof-actions" @click.stop>
          <button class="link" @click="openEdit(p)">{{ t('sidebar.edit') }}</button>
          <button class="link danger" @click="removeProfile(p)">{{ t('sidebar.remove') }}</button>
        </div>
      </li>
    </ul>

    <!-- 系统检测 -->
    <div class="section-head with-margin">
      <span class="section-title">{{ t('llm.detectedTitle') }}</span>
    </div>
    <p class="hint">{{ t('llm.detectedHint') }}</p>

    <!-- OpenAI 兼容（env/file） -->
    <div v-if="detect?.openai" class="detect-card">
      <div class="detect-head">
        <span class="detect-tag">OpenAI compatible</span>
        <button class="link" @click="importDetectedOpenAI">{{ t('llm.importAsProfile') }}</button>
      </div>
      <div class="detect-row mono">
        <span class="kv-k">base</span>
        <span class="kv-v">{{ detect.openai.baseUrl }}</span>
      </div>
      <div class="detect-row mono">
        <span class="kv-k">model</span>
        <span class="kv-v">{{ detect.openai.model }}</span>
      </div>
      <div class="detect-row mono">
        <span class="kv-k">key</span>
        <span class="kv-v">{{ detect.openai.apiKeyMasked }}</span>
      </div>
      <div class="detect-row">
        <span class="kv-k">from</span>
        <span class="kv-v src">{{ detect.openai.source }}</span>
      </div>
    </div>

    <!-- cursor-agent -->
    <div v-if="detect?.cursor" class="detect-card">
      <div class="detect-head">
        <span class="detect-tag">cursor-agent</span>
        <span v-if="cursorBadge" :class="['status-badge', cursorBadge.tone]">{{ cursorBadge.label }}</span>
        <button class="link" @click="importDetectedCursor">{{ t('llm.importAsProfile') }}</button>
      </div>
      <div class="detect-row mono">
        <span class="kv-k">bin</span>
        <span class="kv-v">{{ detect.cursor.binPath }}</span>
      </div>
      <div class="detect-row">
        <span class="kv-k">from</span>
        <span class="kv-v src">{{ detect.cursor.source }}</span>
      </div>
    </div>

    <!-- env 文件清单 -->
    <details v-if="detect?.envFiles?.length" class="env-files">
      <summary>{{ t('llm.envFilesTitle') }}</summary>
      <ul>
        <li v-for="f in detect.envFiles" :key="f.path" class="ef">
          <span :class="['ef-dot', f.exists ? 'on' : 'off']" />
          <span class="mono ef-path" :title="f.path">{{ f.path }}</span>
          <span v-if="f.exists && f.keys.length" class="ef-keys">{{ f.keys.length }} keys</span>
          <span v-else-if="f.exists" class="ef-keys muted">(empty)</span>
          <span v-else class="ef-keys muted">(missing)</span>
        </li>
      </ul>
    </details>

    <LLMProfileDialog
      v-model="dialogVisible"
      :profile="editing"
      :detected-cursor-bin="detectedCursorBin"
      @save="onSave"
    />
  </div>
</template>

<style scoped>
.content {
  flex: 1;
  overflow: auto;
  padding: 8px 10px 16px;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 2px;
}
.section-head.with-margin {
  margin-top: 14px;
}
.section-title {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  font-weight: 700;
  color: var(--text-mute);
}
.mini-btn {
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 2px 8px;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
  transition: all 0.15s;
}
.mini-btn:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
  border-color: var(--border-strong);
}
.mini-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* active card */
.active-card {
  margin-top: 4px;
  padding: 8px 10px;
  border: 1px solid var(--active-border);
  border-radius: 6px;
  background: var(--active);
}
.active-card.inactive {
  border-color: var(--border);
  background: var(--panel-2);
}
.active-line {
  display: flex;
  align-items: center;
  gap: 6px;
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-faint);
  flex-shrink: 0;
}
.dot.ok {
  background: var(--accent);
  box-shadow: 0 0 6px var(--accent);
}
.dot.off {
  background: var(--text-faint);
}
.active-name {
  font-weight: 700;
  font-size: 13px;
  color: var(--text);
}
.active-name.muted {
  font-weight: 500;
  color: var(--text-mute);
}
.prov-badge {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.4px;
  padding: 1px 5px;
  border-radius: 3px;
  background: var(--panel);
  color: var(--text-dim);
  border: 1px solid var(--border);
  text-transform: uppercase;
}
.active-meta {
  margin-top: 4px;
  font-size: 11px;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.active-meta.muted {
  color: var(--text-mute);
}
.active-actions {
  margin-top: 6px;
  display: flex;
  gap: 10px;
}

.empty {
  text-align: center;
  color: var(--text-mute);
  font-size: 12px;
  padding: 16px 12px;
}

/* profiles list */
.prof-list {
  list-style: none;
  margin: 4px 0 0;
  padding: 0;
}
.prof {
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-bottom: 6px;
  cursor: pointer;
  background: var(--panel);
  transition: all 0.15s;
}
.prof:hover {
  background: var(--hover);
  border-color: var(--border-strong);
}
.prof.active {
  background: var(--active);
  border-color: var(--active-border);
}
.prof-head {
  display: flex;
  align-items: center;
  gap: 6px;
}
.prof-name {
  font-weight: 600;
  font-size: 13px;
  color: var(--text);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.prof-meta {
  font-size: 10.5px;
  color: var(--text-mute);
  margin-top: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.prof-actions {
  margin-top: 6px;
  display: flex;
  gap: 8px;
}

.link {
  background: transparent;
  border: 0;
  padding: 0;
  font-size: 11px;
  color: var(--text-dim);
  cursor: pointer;
}
.link:hover {
  color: var(--accent);
  text-decoration: underline;
}
.link.danger:hover {
  color: var(--danger);
}

/* detected */
.hint {
  margin: 4px 2px 8px;
  font-size: 11px;
  color: var(--text-mute);
  line-height: 1.5;
}
.detect-card {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 8px;
  margin-bottom: 8px;
  background: var(--panel);
}
.detect-head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}
.detect-tag {
  font-weight: 700;
  font-size: 11.5px;
  color: var(--text);
  flex: 1;
}
.status-badge {
  font-size: 10px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 9px;
}
.status-badge.ok {
  background: var(--active);
  color: var(--accent);
}
.status-badge.warn {
  background: rgba(194, 112, 10, 0.15);
  color: var(--warn);
}
.status-badge.mute {
  background: var(--kbd-bg);
  color: var(--text-mute);
}
.detect-row {
  display: flex;
  gap: 6px;
  font-size: 11px;
  padding: 1px 0;
}
.kv-k {
  width: 38px;
  color: var(--text-mute);
  flex-shrink: 0;
}
.kv-v {
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.kv-v.src {
  font-family: var(--mono);
  font-size: 10.5px;
}

/* env files */
.env-files {
  margin-top: 6px;
  font-size: 11px;
}
.env-files summary {
  cursor: pointer;
  color: var(--text-dim);
  padding: 4px 2px;
}
.env-files ul {
  list-style: none;
  padding: 0;
  margin: 4px 0 0;
}
.ef {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
}
.ef-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-faint);
  flex-shrink: 0;
}
.ef-dot.on {
  background: var(--accent);
}
.ef-path {
  flex: 1;
  font-size: 10.5px;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ef-keys {
  font-size: 10px;
  color: var(--accent);
}
.ef-keys.muted {
  color: var(--text-mute);
}
.muted {
  color: var(--text-mute);
}
</style>
