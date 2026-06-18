<script setup lang="ts">
import { useTheme, type ThemeMode } from '../composables/useTheme';

const { mode, setMode } = useTheme();

const options: { value: ThemeMode; label: string; icon: string }[] = [
  { value: 'light', label: '浅色', icon: '☀️' },
  { value: 'dark', label: '深色', icon: '🌙' },
  { value: 'auto', label: '跟随系统', icon: '💻' },
];

function cycle() {
  const cur = mode.value;
  const idx = options.findIndex((o) => o.value === cur);
  const next = options[(idx + 1) % options.length].value;
  setMode(next);
}

function currentLabel() {
  return options.find((o) => o.value === mode.value) || options[0];
}
</script>

<template>
  <button class="hdr-btn" :title="`主题：${currentLabel().label}`" @click="cycle">
    <span>{{ currentLabel().icon }}</span>
    <span>{{ currentLabel().label }}</span>
  </button>
</template>
