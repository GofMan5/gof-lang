import { RootProvider } from "fumadocs-ui/provider/next";
import { StaticSearchDialog } from "@/components/search-dialog";
import { i18nUI } from "@/lib/layout.shared";
import type { ReactNode } from "react";

export default async function RootLayout({
  params,
  children,
}: {
  params: Promise<{ lang: string }>;
  children: ReactNode;
}) {
  const { lang } = await params;

  return (
    <RootProvider
      theme={{
        defaultTheme: "dark",
        forcedTheme: "dark",
      }}
      search={{
        SearchDialog: StaticSearchDialog,
      }}
      i18n={i18nUI.provider(lang)}
    >
      {children}
    </RootProvider>
  );
}
