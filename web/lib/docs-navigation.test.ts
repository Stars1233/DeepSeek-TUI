import { describe, expect, it } from "vitest";
import { DOC_TOPICS } from "./docs-map";
import { docsTopicIsCurrent } from "./docs-navigation";

function topic(id: string) {
  const value = DOC_TOPICS.find((candidate) => candidate.id === id);
  if (!value) throw new Error(`missing test topic: ${id}`);
  return value;
}

describe("docsTopicIsCurrent", () => {
  it("marks a dedicated docs page current in either website locale", () => {
    expect(docsTopicIsCurrent(topic("modes"), "en", "/en/docs/modes")).toBe(true);
    expect(docsTopicIsCurrent(topic("tools"), "zh", "/zh/docs/tools/")).toBe(true);
  });

  it("does not mark a different page or the docs hub current", () => {
    expect(docsTopicIsCurrent(topic("modes"), "en", "/en/docs/tools")).toBe(false);
    expect(docsTopicIsCurrent(topic("modes"), "en", "/en/docs")).toBe(false);
  });

  it("never marks source-document links as local pages", () => {
    expect(docsTopicIsCurrent(topic("runtime-api"), "en", "/en/docs/runtime-api")).toBe(false);
  });
});
