"use client";

import { create } from "@orama/orama";
import { useDocsSearch } from "fumadocs-core/search/client";
import {
  SearchDialog,
  SearchDialogClose,
  SearchDialogContent,
  SearchDialogFooter,
  SearchDialogHeader,
  SearchDialogIcon,
  SearchDialogInput,
  SearchDialogList,
  SearchDialogOverlay,
  TagsList,
  TagsListItem,
} from "fumadocs-ui/components/dialog/search";
import type { DefaultSearchDialogProps } from "fumadocs-ui/components/dialog/search-default";
import { useI18n } from "fumadocs-ui/contexts/i18n";
import { useEffect, useMemo, useState } from "react";

function withBasePath(path: string) {
  if (/^https?:\/\//.test(path)) return path;

  const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
  if (!basePath || path.startsWith(basePath)) return path;

  return `${basePath}${path.startsWith("/") ? path : `/${path}`}`;
}

function initOrama(locale?: string) {
  return create({
    schema: {
      _: "string",
    },
    language: locale === "ru" ? "russian" : "english",
  });
}

export function StaticSearchDialog({
  defaultTag,
  tags = [],
  api = "/api/search",
  delayMs,
  allowClear = false,
  links = [],
  footer,
  ...props
}: DefaultSearchDialogProps) {
  const { locale } = useI18n();
  const [tag, setTag] = useState(defaultTag);
  const { search, setSearch, query } = useDocsSearch({
    type: "static",
    from: withBasePath(api),
    initOrama,
    locale,
    tag,
    delayMs,
  });

  const defaultItems = useMemo(() => {
    if (links.length === 0) return null;

    return links.map(([name, link]) => ({
      type: "page" as const,
      id: name,
      content: name,
      url: link,
    }));
  }, [links]);

  useEffect(() => {
    setTag(defaultTag);
  }, [defaultTag]);

  return (
    <SearchDialog
      search={search}
      onSearchChange={setSearch}
      isLoading={query.isLoading}
      {...props}
    >
      <SearchDialogOverlay />
      <SearchDialogContent>
        <SearchDialogHeader>
          <SearchDialogIcon />
          <SearchDialogInput />
          <SearchDialogClose />
        </SearchDialogHeader>
        <SearchDialogList items={query.data !== "empty" ? query.data : defaultItems} />
      </SearchDialogContent>
      <SearchDialogFooter>
        {tags.length > 0 ? (
          <TagsList tag={tag} onTagChange={setTag} allowClear={allowClear}>
            {tags.map((entry) => (
              <TagsListItem key={entry.value} value={entry.value}>
                {entry.name}
              </TagsListItem>
            ))}
          </TagsList>
        ) : null}
        {footer}
      </SearchDialogFooter>
    </SearchDialog>
  );
}