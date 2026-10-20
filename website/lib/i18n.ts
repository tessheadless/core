// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Languages of the website. English is served at `/`, the others under
// `/{locale}/` (`/fr/features`). Interface strings are in locales/{locale}/translation.json
// (i18next); the content comes from Tess entries of the same `locale`.

import i18next, { type i18n } from "i18next";
import { initReactI18next } from "react-i18next";
import en from "../locales/en/translation.json";
import es from "../locales/es/translation.json";
import fr from "../locales/fr/translation.json";
import zh from "../locales/zh/translation.json";
import { isExternal, isOtherApp } from "./meta";

export const LOCALES = ["en", "fr", "zh", "es"] as const;
export type Locale = (typeof LOCALES)[number];
export const DEFAULT_LOCALE: Locale = "en";

export const LOCALE_INFO: Record<Locale, { name: string; htmlLang: string; intl: string }> = {
  en: { name: "English", htmlLang: "en", intl: "en-US" },
  fr: { name: "Français", htmlLang: "fr", intl: "fr-FR" },
  zh: { name: "简体中文", htmlLang: "zh-Hans", intl: "zh-CN" },
  es: { name: "Español", htmlLang: "es", intl: "es-ES" },
};

const resources = {
  en: { translation: en },
  fr: { translation: fr },
  zh: { translation: zh },
  es: { translation: es },
};

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && (LOCALES as readonly string[]).includes(value);
}

/** `/fr/features` → `{ locale: "fr", path: "/features" }`; `/features` → English. */
export function extractLocale(pathname: string): { locale: Locale; path: string } {
  const [, first, ...rest] = pathname.split("/");
  if (first !== DEFAULT_LOCALE && isLocale(first)) return { locale: first, path: `/${rest.join("/")}` };
  return { locale: DEFAULT_LOCALE, path: pathname };
}

/**
 * Internal website link in `locale`: `/features` → `/fr/features`. Other sites,
 * the docs (English only) and anchors are left as they are.
 */
export function localizeUrl(url: string, locale: Locale): string {
  if (locale === DEFAULT_LOCALE || !url.startsWith("/") || url.startsWith("//")) return url;
  if (isExternal(url) || isOtherApp(url) || extractLocale(url).locale !== DEFAULT_LOCALE) return url;
  return url === "/" ? `/${locale}/` : `/${locale}${url}`;
}

/** An i18next instance for one language; the resources are bundled, so it is ready at once. */
export function createI18n(locale: Locale): i18n {
  const instance = i18next.createInstance();
  void instance.use(initReactI18next).init({
    resources,
    lng: locale,
    fallbackLng: DEFAULT_LOCALE,
    initAsync: false,
    interpolation: { escapeValue: false },
  });
  return instance;
}

const instances = new Map<Locale, i18n>();

/** Shared instance per language (render and data hooks). */
export function i18nFor(locale: Locale): i18n {
  let instance = instances.get(locale);
  if (!instance) {
    instance = createI18n(locale);
    instances.set(locale, instance);
  }
  return instance;
}
