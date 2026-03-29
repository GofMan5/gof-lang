import { RootProvider } from "fumadocs-ui/provider/next";
import { i18nUI } from "@/lib/layout.shared";
import type { ReactNode } from "react";
import "@/app/global.css";

export default async function RootLayout({
  params,
  children,
}: {
  params: Promise<{ lang: string }>;
  children: ReactNode;
}) {
  const { lang } = await params;

  return (
    <html lang={lang} className="dark" suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <RootProvider
          theme={{
            defaultTheme: "dark",
            forcedTheme: "dark",
          }}
          i18n={i18nUI.provider(lang)}
        >
          {children}
        </RootProvider>
      </body>
    </html>
  );
}
