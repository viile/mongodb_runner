<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { useI18n } from 'vue-i18n';
import type { MongoConnection } from '../composables/useConnections';
import { listDatabases } from '../api/mongo';

const { t } = useI18n();

const props = defineProps<{
  modelValue: boolean;
  connection?: MongoConnection | null;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void;
  (e: 'save', payload: { id?: string; name: string; uri: string; defaultDatabase?: string }): void;
}>();

const visible = computed({
  get: () => props.modelValue,
  set: (v) => emit('update:modelValue', v),
});

const isEdit = computed(() => !!props.connection?.id);
const form = ref({
  id: '' as string | undefined,
  name: '',
  uri: 'mongodb://localhost:27017',
  defaultDatabase: '' as string | undefined,
});

const testing = ref(false);

watch(
  () => [props.modelValue, props.connection] as const,
  ([open, c]) => {
    if (!open) return;
    if (c) {
      form.value = {
        id: c.id,
        name: c.name,
        uri: c.uri,
        defaultDatabase: c.defaultDatabase || '',
      };
    } else {
      form.value = {
        id: undefined,
        name: '',
        uri: 'mongodb://localhost:27017',
        defaultDatabase: '',
      };
    }
  },
  { immediate: true }
);

async function testConnection() {
  if (!form.value.uri.trim()) {
    ElMessage.warning(t('connDialog.needUriValue'));
    return;
  }
  testing.value = true;
  try {
    const r = await listDatabases(form.value.uri.trim());
    if (r.ok) {
      const names = (r.databases || []).map((d) => d.name);
      ElMessage.success(t('connDialog.connectSuccess', { n: names.length }));
    } else {
      ElMessage.error(t('connDialog.connectFailed', { error: r.error || '' }));
    }
  } catch (e: any) {
    ElMessage.error(t('connDialog.connectFailed', { error: e?.message || String(e) }));
  } finally {
    testing.value = false;
  }
}

function save() {
  const name = form.value.name.trim();
  const uri = form.value.uri.trim();
  if (!name) {
    ElMessage.warning(t('connDialog.needName'));
    return;
  }
  if (!uri) {
    ElMessage.warning(t('connDialog.needUri'));
    return;
  }
  emit('save', {
    id: form.value.id,
    name,
    uri,
    defaultDatabase: form.value.defaultDatabase?.trim() || undefined,
  });
  visible.value = false;
}
</script>

<template>
  <el-dialog
    v-model="visible"
    :title="isEdit ? t('connDialog.editTitle') : t('connDialog.addTitle')"
    width="520px"
    align-center
    :close-on-click-modal="false"
  >
    <el-form label-position="top" :model="form">
      <el-form-item :label="t('connDialog.name')" required>
        <el-input v-model="form.name" :placeholder="t('connDialog.namePh')" />
      </el-form-item>
      <el-form-item :label="t('connDialog.uri')" required>
        <el-input
          v-model="form.uri"
          type="textarea"
          :rows="2"
          :placeholder="t('connDialog.uriPh')"
          class="mono-input"
        />
        <div class="hint">{{ t('connDialog.uriHint') }}</div>
      </el-form-item>
      <el-form-item :label="t('connDialog.defaultDb')">
        <el-input v-model="form.defaultDatabase" :placeholder="t('connDialog.defaultDbPh')" />
      </el-form-item>
    </el-form>
    <template #footer>
      <div class="footer">
        <el-button :loading="testing" @click="testConnection">{{ t('connDialog.test') }}</el-button>
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
.footer {
  display: flex;
  align-items: center;
  gap: 8px;
}
.grow {
  flex: 1;
}
:deep(.mono-input .el-textarea__inner) {
  font-family: var(--mono);
  font-size: 12px;
}
</style>
