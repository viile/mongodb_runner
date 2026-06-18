<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import type { MongoConnection } from '../composables/useConnections';
import { listDatabases } from '../api/mongo';

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
    ElMessage.warning('URI 不能为空');
    return;
  }
  testing.value = true;
  try {
    const r = await listDatabases(form.value.uri.trim());
    if (r.ok) {
      const names = (r.databases || []).map((d) => d.name);
      ElMessage.success(`连接成功，发现 ${names.length} 个数据库`);
    } else {
      ElMessage.error(`连接失败: ${r.error || '未知错误'}`);
    }
  } catch (e: any) {
    ElMessage.error(`连接失败: ${e?.message || String(e)}`);
  } finally {
    testing.value = false;
  }
}

function save() {
  const name = form.value.name.trim();
  const uri = form.value.uri.trim();
  if (!name) {
    ElMessage.warning('请填写名称');
    return;
  }
  if (!uri) {
    ElMessage.warning('请填写 URI');
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
    :title="isEdit ? '编辑连接' : '新增连接'"
    width="520px"
    align-center
    :close-on-click-modal="false"
  >
    <el-form label-position="top" :model="form">
      <el-form-item label="名称" required>
        <el-input v-model="form.name" placeholder="比如：本地 / 生产只读" />
      </el-form-item>
      <el-form-item label="URI" required>
        <el-input
          v-model="form.uri"
          type="textarea"
          :rows="2"
          placeholder="mongodb://user:pass@host:27017/?authSource=admin"
          class="mono-input"
        />
        <div class="hint">支持标准 URI / SRV：<code>mongodb+srv://user:pass@cluster.example.net/</code></div>
      </el-form-item>
      <el-form-item label="默认数据库（可选）">
        <el-input v-model="form.defaultDatabase" placeholder="留空则连接后再选" />
      </el-form-item>
    </el-form>
    <template #footer>
      <div class="footer">
        <el-button :loading="testing" @click="testConnection">测试连接</el-button>
        <div class="grow" />
        <el-button @click="visible = false">取消</el-button>
        <el-button type="primary" @click="save">{{ isEdit ? '保存' : '新增' }}</el-button>
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
