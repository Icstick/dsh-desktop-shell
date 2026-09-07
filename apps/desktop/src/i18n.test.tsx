import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it } from "vitest";

import {
  defaultLang,
  I18nProvider,
  LANG_STORAGE_KEY,
  loadLang,
  persistLang,
  resolveLang,
  translate,
  translations,
  useI18n,
  type Lang,
} from "./i18n";

function wrapper({ children }: { children: ReactNode }) {
  return <I18nProvider>{children}</I18nProvider>;
}

describe("i18n", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("resolves keys from the default zh locale", () => {
    expect(translate("zh", "rail.browser")).toBe("浏览器");
    expect(translate("zh", "rail.timer")).toBe("计时器（M3）");
    expect(translate("zh", "common.close")).toBe("关闭");
    expect(translate("zh", "browser.urlLabel")).toBe("浏览器 URL");
  });

  it("resolves keys from the en locale", () => {
    expect(translate("en", "rail.timer")).toBe("Timer (M3)");
    expect(translate("en", "surface.settings")).toBe("Environment Settings");
  });

  it("interpolates {name} placeholders", () => {
    expect(
      translate("zh", "usage.inOut", { input: "1200", output: "300" }),
    ).toBe("1200 进 · 300 出");
    expect(
      translate("zh", "runtime.confirmStop.action", { generation: "1" }),
    ).toBe("确认停止代次 1");
    expect(
      translate("zh", "runtime.verifiedEndpoint", {
        endpoint: "http://127.0.0.1:4317",
      }),
    ).toBe("已确认的实例访问地址：http://127.0.0.1:4317");
  });

  it("falls back to the zh table when a key is missing from the active locale", () => {
    const original = translations.en["rail.dsh"];
    delete translations.en["rail.dsh"];
    try {
      expect(translate("en", "rail.dsh")).toBe("DSH");
    } finally {
      translations.en["rail.dsh"] = original;
    }
  });

  it("returns the key itself for completely missing keys (visible placeholder)", () => {
    expect(translate("zh", "no.such.key")).toBe("no.such.key");
    expect(translate("en", "no.such.key")).toBe("no.such.key");
  });

  it("parses persisted and raw language values", () => {
    expect(resolveLang("en")).toBe("en");
    expect(resolveLang("zh")).toBe("zh");
    expect(resolveLang("fr")).toBe("zh");
    expect(resolveLang(null)).toBe("zh");
  });

  it("persists and reloads the language choice", () => {
    expect(loadLang()).toBe("zh");
    persistLang("en");
    expect(window.localStorage.getItem(LANG_STORAGE_KEY)).toBe("en");
    expect(loadLang()).toBe("en");
    persistLang("zh");
    expect(loadLang()).toBe("zh");
  });

  it("ignores garbage in storage and defaults to zh", () => {
    window.localStorage.setItem(LANG_STORAGE_KEY, "de");
    expect(loadLang()).toBe("zh");
  });

  it("provides a switching context that re-renders t() and persists", () => {
    const { result } = renderHook(() => useI18n(), { wrapper });

    expect(result.current.lang).toBe("zh");
    expect(result.current.t("rail.timer")).toBe("计时器（M3）");

    act(() => result.current.setLang("en"));

    expect(result.current.lang).toBe("en");
    expect(result.current.t("rail.timer")).toBe("Timer (M3)");
    expect(window.localStorage.getItem(LANG_STORAGE_KEY)).toBe("en");
  });

  it("reads a persisted language on provider mount", () => {
    persistLang("en");
    const { result } = renderHook(() => useI18n(), { wrapper });
    expect(result.current.lang).toBe("en");
  });

  it("falls back to the default locale outside a provider", () => {
    const { result } = renderHook(() => useI18n());
    expect(result.current.lang).toBe(defaultLang);
    expect(result.current.t("rail.browser")).toBe("浏览器");
    expect(() => result.current.setLang("en" as Lang)).not.toThrow();
    expect(result.current.lang).toBe("zh");
  });
});
