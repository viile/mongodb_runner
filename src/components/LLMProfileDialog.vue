<script setup lang="ts">
/**
 * 新增 / 编辑一个 LLM API profile。
 *
 * - 模板选择会自动填 base URL + 默认 model；用户可继续改
 * - "测试连接" 调用 llm_status 把当前表单当 profile 发给 Rust，看是否能解析出可用配置
 *   （不实际发请求，避免误用 Key；想真测的话用 ChatPanel 发一句话即可）
 */

import { computed, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { useI18n } from 'vue-i18n';
import {
  PROVIDER_TEMPLATES,
  type LLMProfile,
  type LLMProviderKind,
  type ProviderTemplate,
} from '../composables/useLLMProfiles';
import { getLLMStatus } from '../api/llm';

const props = defineProps<{
  modelValue: boolean;
  profile?: LLMProfile | null;
  /** 系统检测出的 cursor-agent 路径，用作 binPath 默认值 */
  detectedCursorBin?: string | null;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void;
  (e: 'save', payload: Omit<LLMProfile, 'id' | 'createdAt' | 'updatedAt'> & { id?: string }): void;
}>();

const { t } = useI18n();

const visible = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});

const isEdit = computed(() => !!props.profile?.id);

interface FormState {
  id?: string;
  name: string;
  providerKind: LLMProviderKind;
  templateKey: string;
  baseUrl: string;
  model: string;
  apiKey: string;
  binPath: string;
  cursorModel: string;
  timeout: number;
}

function emptyForm(): FormState {
  return {
    id: undefined,
    name: '',
    providerKind: 'openai',
    templateKey: 'openai',
    baseUrl: 'https://api.openai.com',
    model: 'gpt-4o-mini',
    apiKey: '',
    binPath: '',
    cursorModel: '',
    timeout: 60,
  };
}

const form = ref<FormState>(emptyForm());

const currentTemplate = computed<ProviderTemplate | null>(
  () => PROVIDER_TEMPLATES.find((p) => p.key === form.value.templateKey) ?? null
);

function modelSuggestions(query: string, cb: (arr: { value: string }[]) => void) {
  const list = currentTemplate.value?.suggestedModels ?? [];
  const q = query.trim().toLowerCase();
  const filtered = q ? list.filter((m) => m.toLowerCase().includes(q)) : list;
  cb(filtered.map((value) => ({ value })));
}

watch(
  () => [props.modelValue, props.profile] as const,
  ([open, p]) => {
    if (!open) return;
    if (p) {
      // 反推 templateKey：按 baseUrl 优先匹配，匹配不到归到 custom
      let templateKey = 'custom';
      if (p.providerKind === 'cursor') {
        templateKey = 'cursor';
      } else if (p.baseUrl) {
        const norm = p.baseUrl.replace(/\/+$/, '');
        const hit = PROVIDER_TEMPLATES.find(
          (t) => t.baseUrl && norm.startsWith(t.baseUrl.replace(/\/+$/, ''))
        );
        if (hit) templateKey = hit.key;
      }
      form.value = {
        id: p.id,
        name: p.name,
        providerKind: p.providerKind,
        templateKey,
        baseUrl: p.baseUrl ?? '',
        model: p.model ?? '',
        apiKey: p.apiKey ?? '',
        binPath: p.binPath ?? '',
        cursorModel: p.cursorModel ?? '',
        timeout: p.timeout ?? (p.providerKind === 'cursor' ? 120 : 60),
      };
    } else {
      const f = emptyForm();
      if (props.detectedCursorBin) f.binPath = props.detectedCursorBin;
      form.value = f;
    }
  },
  { immediate: true }
);

function applyTemplate(key: string) {
  const tpl = PROVIDER_TEMPLATES.find((p) => p.key === key);
  if (!tpl) return;
  form.value.templateKey = key;
  form.value.providerKind = tpl.providerKind;
  if (tpl.providerKind === 'openai') {
    if (tpl.baseUrl) form.value.baseUrl = tpl.baseUrl;
    else if (key === 'custom') form.value.baseUrl = form.value.baseUrl || '';
    if (tpl.defaultModel) form.value.model = tpl.defaultModel;
    if (form.value.timeout < 1) form.value.timeout = 60;
  } else {
    if (form.value.timeout < 1) form.value.timeout = 120;
    if (!form.value.binPath && props.detectedCursorBin) {
      form.value.binPath = props.detectedCursorBin;
    }
  }
  // 名称缺省补一个友好默认
  if (!form.value.name.trim()) {
    form.value.name = tpl.label;
  }
}

const testing = ref(false);
async function testConnection() {
  testing.value = true;
  try {
    const partial: LLMProfile = {
      id: form.value.id ?? 'preview',
      name: form.value.name || '(preview)',
      providerKind: form.value.providerKind,
      baseUrl: form.value.baseUrl.trim() || undefined,
      model: form.value.model.trim() || undefined,
      apiKey: form.value.apiKey.trim() || undefined,
      binPath: form.value.binPath.trim() || undefined,
      cursorModel: form.value.cursorModel.trim() || undefined,
      timeout: form.value.timeout || undefined,
      createdAt: 0,
      updatedAt: 0,
    };
    const r = await getLLMStatus(partial);
    if (r.available) {
      const a = r.active;
      const desc =
        a?.providerKind === 'cursor'
          ? `cursor-agent (${a.binPath})`
          : `${a?.providerKind ?? 'openai'} · ${a?.model ?? form.value.model}`;
      ElMessage.success(t('llm.testOk', { desc }));
    } else {
      ElMessage.error(r.error || t('llm.testBad'));
    }
  } catch (e: any) {
    ElMessage.error(e?.message || String(e));
  } finally {
    testing.value = false;
  }
}

function save() {
  const name = form.value.name.trim();
  if (!name) {
    ElMessage.warning(t('llm.needName'));
    return;
  }
  if (form.value.providerKind === 'openai') {
    if (!form.value.apiKey.trim()) {
      ElMessage.warning(t('llm.needApiKey'));
      return;
    }
  }
  emit('save', {
    id: form.value.id,
    name,
    providerKind: form.value.providerKind,
    baseUrl: form.value.baseUrl.trim() || undefined,
    model: form.value.model.trim() || undefined,
    apiKey: form.value.apiKey.trim() || undefined,
    binPath: form.value.binPath.trim() || undefined,
    cursorModel: form.value.cursorModel.trim() || undefined,
    timeout: Math.max(1, Number(form.value.timeout) || 60),
  });
  visible.value = false;
}
</script>

<template>
  <el-dialog
    v-model="visible"
    :title="isEdit ? t('llm.editTitle') : t('llm.addTitle')"
    width="560px"
    align-center
    :close-on-click-modal="false"
  >
    <el-form label-position="top" :model="form">
      <el-form-item :label="t('llm.template')">
        <el-select
          :model-value="form.templateKey"
          style="width: 100%"
          @update:model-value="applyTemplate"
        >
          <el-option
            v-for="tpl in PROVIDER_TEMPLATES"
            :key="tpl.key"
            :value="tpl.key"
            :label="tpl.label"
          />
        </el-select>
        <div v-if="currentTemplate?.docsUrl" class="hint">
          <a :href="currentTemplate.docsUrl" target="_blank" rel="noopener noreferrer">
            {{ t('llm.getKey') }} ↗
          </a>
        </div>
      </el-form-item>

      <el-form-item :label="t('llm.name')" required>
        <el-input v-model="form.name" :placeholder="t('llm.namePh')" />
      </el-form-item>

      <!-- OpenAI 兼容字段 -->
      <template v-if="form.providerKind === 'openai'">
        <el-form-item :label="t('llm.baseUrl')">
          <el-input
            v-model="form.baseUrl"
            placeholder="https://api.openai.com"
            class="mono-input"
          />
          <div class="hint">{{ t('llm.baseUrlHint') }}</div>
        </el-form-item>
        <el-form-item :label="t('llm.model')">
          <el-autocomplete
            v-model="form.model"
            :fetch-suggestions="modelSuggestions"
            placeholder="gpt-4o-mini"
            class="mono-input"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item :label="t('llm.apiKey')" required>
          <el-input
            v-model="form.apiKey"
            type="password"
            show-password
            placeholder="sk-..."
            class="mono-input"
            autocomplete="off"
          />
        </el-form-item>
      </template>

      <!-- cursor-agent 字段 -->
      <template v-else>
        <el-form-item :label="t('llm.binPath')">
          <el-input
            v-model="form.binPath"
            :placeholder="detectedCursorBin || '/usr/local/bin/cursor-agent'"
            class="mono-input"
          />
          <div class="hint">{{ t('llm.binPathHint') }}</div>
        </el-form-item>
        <el-form-item :label="t('llm.cursorModel')">
          <el-input v-model="form.cursorModel" :placeholder="t('llm.cursorModelPh')" class="mono-input" />
        </el-form-item>
      </template>

      <el-form-item :label="t('llm.timeout')">
        <el-input-number v-model="form.timeout" :min="1" :max="600" :step="10" />
        <span class="hint inline">{{ t('llm.timeoutUnit') }}</span>
      </el-form-item>
    </el-form>

    <template #footer>
      <div class="footer">
        <el-button :loading="testing" @click="testConnection">{{ t('llm.test') }}</el-button>
        <div class="grow" />
        <el-button @click="visible = false">{{ t('connDialog.cancel') }}</el-button>
        <el-button type="primary" @click="save">{{ isEdit ? t('connDialog.save') : t('connDialog.add') }}</el-button>
      </div>
    </template>
  </el-dialog>
</template>

<style scoped>
.hint {
  font-size: 11px;
  color: var(--text-mute);
  margin-top: 4px;
}
.hint a {
  color: var(--accent);
  text-decoration: none;
}
.hint a:hover {
  text-decoration: underline;
}
.hint.inline {
  margin-left: 8px;
}
.footer {
  display: flex;
  align-items: center;
  gap: 8px;
}
.grow {
  flex: 1;
}
:deep(.mono-input .el-input__inner),
:deep(.mono-input .el-textarea__inner) {
  font-family: var(--mono);
  font-size: 12px;
}
</style>
