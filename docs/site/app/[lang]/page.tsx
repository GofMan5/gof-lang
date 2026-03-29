import { i18n } from "@/lib/i18n";
import { redirect } from "next/navigation";

export default async function LangPage({
  params,
}: {
  params: Promise<{ lang: string }>;
}) {
  const { lang } = await params;
  redirect(`/${lang}/docs`);
}

export function generateStaticParams() {
  return i18n.languages.map((lang) => ({ lang }));
}
