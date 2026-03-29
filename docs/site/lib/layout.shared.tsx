import { i18n } from "@/lib/i18n";
import { defineI18nUI } from "fumadocs-ui/i18n";
import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";

export const i18nUI = defineI18nUI(i18n, {
  translations: {
    en: {
      displayName: "English",
    },
    ru: {
      displayName: "Русский",
      search: "Поиск в документации",
    },
  },
});

export function baseOptions(locale: string): BaseLayoutProps {
  return {
    nav: {
      title: (
        <span className="font-bold tracking-wide">
          <span className="text-fd-primary">gof</span>
          <span className="text-fd-muted-foreground ml-1.5 text-sm font-medium">
            docs
          </span>
        </span>
      ),
      url: `/${locale}/docs`,
    },
    githubUrl: "https://github.com/GofMan5/gof-lang",
  };
}
