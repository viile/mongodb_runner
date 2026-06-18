/**
 * i18n 入口。架构参考 curl_display/src/i18n/index.ts：
 *   - 自定义 messageCompiler，只识别 `{name}` 占位符；
 *     这样翻译文本里出现 JSON 字面量 `{"a":1}` 等花括号不会触发 vue-i18n 编译错误
 *   - LOCALE_OPTIONS 同时含 Element Plus 内置语言包，App.vue 通过 ElConfigProvider 注入
 *   - 切换语言时把 <html lang>/dir 也同步更新，方便 UA 控件与无障碍工具
 */

import { computed, ref } from 'vue';
import { createI18n } from 'vue-i18n';

// 应用文案
import zhCN from './locales/zh-CN';
import zhTW from './locales/zh-TW';
import enUS from './locales/en-US';
import jaJP from './locales/ja-JP';
import koKR from './locales/ko-KR';
import frFR from './locales/fr-FR';
import deDE from './locales/de-DE';
import esES from './locales/es-ES';
import ruRU from './locales/ru-RU';
import ptBR from './locales/pt-BR';

// Element Plus 内置 locale
import elZhCN from 'element-plus/es/locale/lang/zh-cn';
import elZhTW from 'element-plus/es/locale/lang/zh-tw';
import elEnUS from 'element-plus/es/locale/lang/en';
import elJaJP from 'element-plus/es/locale/lang/ja';
import elKoKR from 'element-plus/es/locale/lang/ko';
import elFrFR from 'element-plus/es/locale/lang/fr';
import elDeDE from 'element-plus/es/locale/lang/de';
import elEsES from 'element-plus/es/locale/lang/es';
import elRuRU from 'element-plus/es/locale/lang/ru';
import elPtBR from 'element-plus/es/locale/lang/pt-br';
import type { Language as ElLanguage } from 'element-plus/es/locale';

export type LocaleKey =
  | 'zh-CN'
  | 'zh-TW'
  | 'en-US'
  | 'ja-JP'
  | 'ko-KR'
  | 'fr-FR'
  | 'de-DE'
  | 'es-ES'
  | 'ru-RU'
  | 'pt-BR';

export interface LocaleOption {
  key: LocaleKey;
  /** 母语显示名（菜单主标题） */
  nativeName: string;
  /** 英文名（搜索 / 副标题） */
  englishName: string;
  /** 触发器短标签 */
  short: string;
  flag: string;
  rtl?: boolean;
  elementLocale: ElLanguage;
}

export const LOCALE_OPTIONS: LocaleOption[] = [
  { key: 'zh-CN', nativeName: '简体中文', englishName: 'Simplified Chinese', short: '简中', flag: '🇨🇳', elementLocale: elZhCN },
  { key: 'zh-TW', nativeName: '繁體中文', englishName: 'Traditional Chinese', short: '繁中', flag: '🇹🇼', elementLocale: elZhTW },
  { key: 'en-US', nativeName: 'English', englishName: 'English', short: 'EN', flag: '🇺🇸', elementLocale: elEnUS },
  { key: 'ja-JP', nativeName: '日本語', englishName: 'Japanese', short: '日', flag: '🇯🇵', elementLocale: elJaJP },
  { key: 'ko-KR', nativeName: '한국어', englishName: 'Korean', short: '한', flag: '🇰🇷', elementLocale: elKoKR },
  { key: 'fr-FR', nativeName: 'Français', englishName: 'French', short: 'FR', flag: '🇫🇷', elementLocale: elFrFR },
  { key: 'de-DE', nativeName: 'Deutsch', englishName: 'German', short: 'DE', flag: '🇩🇪', elementLocale: elDeDE },
  { key: 'es-ES', nativeName: 'Español', englishName: 'Spanish', short: 'ES', flag: '🇪🇸', elementLocale: elEsES },
  { key: 'pt-BR', nativeName: 'Português', englishName: 'Portuguese (Brazil)', short: 'PT', flag: '🇧🇷', elementLocale: elPtBR },
  { key: 'ru-RU', nativeName: 'Русский', englishName: 'Russian', short: 'RU', flag: '🇷🇺', elementLocale: elRuRU },
];

const STORAGE_KEY = 'mongodb-runner:locale';

/** 把任意 BCP-47 风格的语言码尽量匹配到我们的 LocaleKey */
function matchLocale(input: string | undefined | null): LocaleKey | null {
  if (!input) return null;
  const lower = input.toLowerCase().replace('_', '-');
  const exact = LOCALE_OPTIONS.find((o) => o.key.toLowerCase() === lower);
  if (exact) return exact.key;
  const primary = lower.split('-')[0];
  if (primary === 'zh') {
    if (/-tw|-hk|-mo|-hant/.test(lower)) return 'zh-TW';
    return 'zh-CN';
  }
  if (primary === 'pt') return 'pt-BR';
  const byPrimary = LOCALE_OPTIONS.find((o) => o.key.toLowerCase().startsWith(primary + '-'));
  return byPrimary?.key ?? null;
}

function detectInitialLocale(): LocaleKey {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    const matched = matchLocale(saved);
    if (matched) return matched;
  } catch {
    /* ignore */
  }
  const candidates: string[] = [];
  if (typeof navigator !== 'undefined') {
    if (Array.isArray(navigator.languages)) candidates.push(...navigator.languages);
    if (navigator.language) candidates.push(navigator.language);
  }
  for (const c of candidates) {
    const m = matchLocale(c);
    if (m) return m;
  }
  return 'en-US';
}

const initial = detectInitialLocale();

/**
 * 自定义 messageCompiler，替代 vue-i18n 内置的严格解析器。
 * - 仅把 `{name}`（name 为合法 JS 标识符）视为占位符
 * - 其它 `{...}`（如 JSON 字面量）按原文输出
 */
const PLACEHOLDER_RE = /\{([a-zA-Z_][a-zA-Z0-9_]*)\}/g;
function messageCompiler(source: unknown): (ctx: any) => string {
  if (typeof source !== 'string') {
    return () => (source == null ? '' : String(source));
  }
  return (ctx: any) =>
    source.replace(PLACEHOLDER_RE, (match: string, key: string) => {
      try {
        const named = ctx?.named?.(key);
        if (named !== undefined && named !== null) return String(named);
      } catch {
        /* ignore */
      }
      return match;
    });
}

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: initial,
  fallbackLocale: 'en-US',
  messageCompiler,
  messages: {
    'zh-CN': zhCN,
    'zh-TW': zhTW,
    'en-US': enUS,
    'ja-JP': jaJP,
    'ko-KR': koKR,
    'fr-FR': frFR,
    'de-DE': deDE,
    'es-ES': esES,
    'ru-RU': ruRU,
    'pt-BR': ptBR,
  },
});

export const currentLocale = ref<LocaleKey>(initial);

export const currentLocaleOption = computed<LocaleOption>(
  () => LOCALE_OPTIONS.find((o) => o.key === currentLocale.value) ?? LOCALE_OPTIONS[0]
);

export const currentElementLocale = computed<ElLanguage>(
  () => currentLocaleOption.value.elementLocale
);

export const isRTL = computed<boolean>(() => !!currentLocaleOption.value.rtl);

function applyDocAttrs(key: LocaleKey) {
  try {
    const opt = LOCALE_OPTIONS.find((o) => o.key === key);
    document.documentElement.lang = key;
    document.documentElement.dir = opt?.rtl ? 'rtl' : 'ltr';
  } catch {
    /* ignore */
  }
}

export function setLocale(key: LocaleKey) {
  if (!LOCALE_OPTIONS.some((o) => o.key === key)) return;
  currentLocale.value = key;
  (i18n.global.locale as unknown as { value: string }).value = key;
  try {
    localStorage.setItem(STORAGE_KEY, key);
  } catch {
    /* ignore */
  }
  applyDocAttrs(key);
}

applyDocAttrs(initial);
