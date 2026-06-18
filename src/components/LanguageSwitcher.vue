<script setup lang="ts">
/**
 * 语言切换器（参考 curl_display/src/components/LanguageSwitcher.vue）：
 *   - 触发器：国旗 + 短标签 + caret
 *   - 弹出层：搜索框 + 母语名/英文名双行列表 + 当前选中打勾
 */
import { computed, nextTick, ref, watch } from 'vue';
import { ArrowDown, Search, Check } from '@element-plus/icons-vue';
import { useI18n } from 'vue-i18n';
import {
  LOCALE_OPTIONS,
  currentLocale,
  currentLocaleOption,
  setLocale,
  type LocaleKey,
} from '../i18n';

const { t } = useI18n();

const popoverVisible = ref(false);
const keyword = ref('');
const searchInput = ref<HTMLInputElement | null>(null);

const filtered = computed(() => {
  const q = keyword.value.trim().toLowerCase();
  if (!q) return LOCALE_OPTIONS;
  return LOCALE_OPTIONS.filter(
    (o) =>
      o.nativeName.toLowerCase().includes(q) ||
      o.englishName.toLowerCase().includes(q) ||
      o.key.toLowerCase().includes(q) ||
      o.short.toLowerCase().includes(q)
  );
});

function handlePick(key: LocaleKey) {
  setLocale(key);
  popoverVisible.value = false;
  keyword.value = '';
}

watch(popoverVisible, (v) => {
  if (v) {
    nextTick(() => searchInput.value?.focus());
  } else {
    keyword.value = '';
  }
});
</script>

<template>
  <el-popover
    v-model:visible="popoverVisible"
    trigger="click"
    placement="bottom-end"
    :width="280"
    popper-class="lang-popover"
    :show-arrow="false"
  >
    <template #reference>
      <button class="hdr-btn" type="button" :title="currentLocaleOption.englishName">
        <span class="lang-flag">{{ currentLocaleOption.flag }}</span>
        <span class="hdr-btn-label">{{ currentLocaleOption.short }}</span>
        <el-icon class="hdr-btn-caret"><ArrowDown /></el-icon>
      </button>
    </template>

    <div class="lang-panel" dir="ltr">
      <div class="search-wrap">
        <el-icon class="search-icon"><Search /></el-icon>
        <input
          ref="searchInput"
          v-model="keyword"
          class="search-input"
          type="text"
          :placeholder="t('lang.searchPh')"
          spellcheck="false"
          autocorrect="off"
          autocapitalize="off"
        />
      </div>

      <div class="lang-list">
        <button
          v-for="opt in filtered"
          :key="opt.key"
          type="button"
          class="lang-item"
          :class="{ active: opt.key === currentLocale }"
          @click="handlePick(opt.key)"
        >
          <span class="item-flag">{{ opt.flag }}</span>
          <span class="item-text">
            <span class="item-native">{{ opt.nativeName }}</span>
            <span class="item-en">{{ opt.englishName }}</span>
          </span>
          <el-icon v-if="opt.key === currentLocale" class="item-check"><Check /></el-icon>
        </button>
        <div v-if="!filtered.length" class="empty-tip">{{ t('lang.noMatch') }}</div>
      </div>

      <div class="lang-footer">
        <span>{{ filtered.length }} / {{ LOCALE_OPTIONS.length }}</span>
      </div>
    </div>
  </el-popover>
</template>

<style scoped>
.lang-flag {
  font-size: 14px;
  line-height: 1;
  flex-shrink: 0;
}
.lang-panel {
  display: flex;
  flex-direction: column;
  max-height: 460px;
}
.search-wrap {
  position: relative;
  margin-bottom: 8px;
}
.search-icon {
  position: absolute;
  left: 8px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-dim);
  font-size: 14px;
  pointer-events: none;
}
.search-input {
  width: 100%;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text);
  font-size: 13px;
  padding: 7px 10px 7px 30px;
  outline: none;
  transition: border-color 0.15s;
}
.search-input:focus {
  border-color: var(--accent);
}
.search-input::placeholder {
  color: var(--text-mute);
}
.lang-list {
  flex: 1;
  overflow-y: auto;
  margin: 0 -4px;
  padding: 0 4px;
}
.lang-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  background: transparent;
  border: none;
  padding: 8px 8px;
  border-radius: 6px;
  cursor: pointer;
  color: var(--text);
  text-align: left;
  font-size: 13px;
  transition: background 0.12s;
}
.lang-item:hover {
  background: var(--hover-strong);
}
.lang-item.active {
  background: var(--active);
}
.item-flag {
  font-size: 18px;
  line-height: 1;
  flex-shrink: 0;
}
.item-text {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}
.item-native {
  color: var(--text);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-en {
  color: var(--text-dim);
  font-size: 11px;
  margin-top: 1px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-check {
  color: var(--accent-2);
  font-size: 14px;
  flex-shrink: 0;
}
.empty-tip {
  padding: 16px;
  text-align: center;
  color: var(--text-dim);
  font-size: 12px;
}
.lang-footer {
  padding-top: 8px;
  margin-top: 4px;
  border-top: 1px solid var(--border);
  color: var(--text-dim);
  font-size: 11px;
  text-align: right;
  font-family: var(--mono);
}
</style>

<style>
.lang-popover.el-popper {
  background: var(--panel-2) !important;
  border: 1px solid var(--border) !important;
  padding: 10px !important;
  border-radius: 10px !important;
  box-shadow: var(--shadow-strong) !important;
}
</style>
