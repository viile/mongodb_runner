<script setup lang="ts">
/**
 * 主题切换器：浅色 / 深色 / 跟随系统 三档下拉菜单。
 *
 * - trigger 上展示当前模式的图标
 * - auto 模式下额外显示一个小 badge（L / D）说明此刻实际生效的颜色，
 *   避免用户在「跟随系统」时不清楚现在到底是亮还是暗
 * - 选中项右侧打勾
 */
import { computed } from 'vue';
import { Sunny, Moon, Monitor, ArrowDown, Check } from '@element-plus/icons-vue';
import type { Component } from 'vue';
import { useI18n } from 'vue-i18n';
import { useTheme, type ThemeMode } from '../composables/useTheme';

const { t } = useI18n();
const { mode, effective, setMode } = useTheme();

interface ThemeOption {
  key: ThemeMode;
  labelKey: 'theme.light' | 'theme.dark' | 'theme.auto';
  icon: Component;
}

const OPTIONS: ThemeOption[] = [
  { key: 'light', labelKey: 'theme.light', icon: Sunny },
  { key: 'dark', labelKey: 'theme.dark', icon: Moon },
  { key: 'auto', labelKey: 'theme.auto', icon: Monitor },
];

const current = computed(() => OPTIONS.find((o) => o.key === mode.value) ?? OPTIONS[0]);

const triggerTitle = computed(() => `${t('theme.title')}: ${t(current.value.labelKey)}`);
const badgeTitle = computed(() =>
  effective.value === 'dark' ? t('theme.effectiveDark') : t('theme.effectiveLight')
);

function handleSelect(cmd: string | number | object) {
  setMode(cmd as ThemeMode);
}
</script>

<template>
  <el-dropdown trigger="click" placement="bottom-end" @command="handleSelect">
    <button class="hdr-btn" type="button" :title="triggerTitle">
      <el-icon class="hdr-btn-icon">
        <component :is="current.icon" />
      </el-icon>
      <span class="hdr-btn-label">{{ t(current.labelKey) }}</span>
      <span v-if="mode === 'auto'" class="hdr-btn-badge" :title="badgeTitle">
        {{ effective === 'dark' ? 'D' : 'L' }}
      </span>
      <el-icon class="hdr-btn-caret"><ArrowDown /></el-icon>
    </button>
    <template #dropdown>
      <el-dropdown-menu>
        <el-dropdown-item v-for="opt in OPTIONS" :key="opt.key" :command="opt.key">
          <el-icon class="opt-icon" :size="14">
            <component :is="opt.icon" />
          </el-icon>
          <span class="opt-label">{{ t(opt.labelKey) }}</span>
          <el-icon v-if="opt.key === mode" class="opt-check" :size="14"><Check /></el-icon>
        </el-dropdown-item>
      </el-dropdown-menu>
    </template>
  </el-dropdown>
</template>

<style scoped>
.opt-icon {
  margin-right: 8px;
}
.opt-label {
  margin-right: 8px;
  min-width: 60px;
}
.opt-check {
  margin-left: auto;
  color: var(--accent-2);
}
</style>
