"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { docsTopicIsCurrent } from "@/lib/docs-navigation";
import { getTopicsByCategory, REPO_DOCS_BASE, type DocTopic } from "@/lib/docs-map";

const CATEGORY_LABELS: Record<string, { en: string; zh: string }> = {
  "getting-started": { en: "Getting started", zh: "入门" },
  "core-concepts": { en: "Core concepts", zh: "核心概念" },
  reference: { en: "Reference", zh: "参考" },
  extending: { en: "Extending", zh: "扩展" },
  operations: { en: "Operations", zh: "运维" },
};

function topicHref(topic: DocTopic, locale: string): string {
  if (topic.hasPage) return `/${locale}/docs/${topic.slug}`;
  const source = Array.isArray(topic.repoSource) ? topic.repoSource[0] : topic.repoSource;
  return `${REPO_DOCS_BASE}/${source}`;
}

export function DocsSidebar({ locale }: { locale: string }) {
  const isZh = locale === "zh";
  const pathname = usePathname();
  const byCategory = getTopicsByCategory();

  return (
    <aside className="docs-sidebar min-w-0">
      <div className="lg:sticky lg:top-24">
        <div className="docs-sidebar-heading">
          <Link href={`/${locale}/docs`}>
            <span>{isZh ? "文档目录" : "Documentation"}</span>
            <span aria-hidden="true">→</span>
          </Link>
        </div>
        <nav aria-label={isZh ? "文档目录" : "Documentation index"}>
          {[...byCategory.entries()].map(([category, topics]) => (
            <div key={category} className="docs-sidebar-group">
              <div className="docs-sidebar-category">
                {isZh
                  ? CATEGORY_LABELS[category]?.zh ?? category
                  : CATEGORY_LABELS[category]?.en ?? category}
              </div>
              <ul>
                {topics.map((topic) => {
                  const isCurrent = docsTopicIsCurrent(topic, locale, pathname);
                  return (
                    <li key={topic.id}>
                      <Link
                        href={topicHref(topic, locale)}
                        target={topic.hasPage ? undefined : "_blank"}
                        rel={topic.hasPage ? undefined : "noreferrer"}
                        aria-current={isCurrent ? "page" : undefined}
                        className={
                          isCurrent
                            ? "docs-sidebar-link docs-sidebar-link-current"
                            : "docs-sidebar-link"
                        }
                      >
                        <span>{isZh ? topic.label.zh : topic.label.en}</span>
                        {!topic.hasPage && <span aria-hidden="true">↗</span>}
                      </Link>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </nav>
      </div>
    </aside>
  );
}
