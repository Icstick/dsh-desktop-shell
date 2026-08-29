import { describe, expect, it } from "vitest";

import {
  convertEnvironmentDraft,
  environmentToDraft,
  initialEnvironmentDraft,
} from "./environment-draft";

describe("convertEnvironmentDraft", () => {
  const validDraft = {
    ...initialEnvironmentDraft,
    dshHome: "C:/Users/example/.dsh",
  };

  it("normalizes auto port without shell parsing", () => {
    const result = convertEnvironmentDraft({
      ...validDraft,
      extraArguments: "--verbose\nvalue with spaces",
    });
    expect(result.issues).toEqual([]);
    expect(result.environment?.endpoint).toEqual({ host: "127.0.0.1", port: "auto" });
    expect(result.environment?.harness.args).toEqual(["--verbose", "value with spaces"]);
  });

  it.each(["--host", "--host=0.0.0.0", "--port", "--no-open", "--trusted-host=evil.test"])(
    "rejects the reserved argument %s",
    (argument) => {
      const result = convertEnvironmentDraft({ ...validDraft, extraArguments: argument });
      expect(result.environment).toBeNull();
      expect(result.issues).toContainEqual(expect.objectContaining({ code: "UNAUTHORIZED" }));
    },
  );

  it("rejects a non-loopback policy port", () => {
    const result = convertEnvironmentDraft({ ...validDraft, port: "80" });
    expect(result.environment).toBeNull();
    expect(result.issues).toContainEqual(expect.objectContaining({ field: "endpoint.port" }));
  });

  it("restores a persisted environment without losing literal arguments", () => {
    const converted = convertEnvironmentDraft({
      ...validDraft,
      extraArguments: "--verbose\nvalue with spaces",
      port: "4317",
    });
    expect(converted.environment).not.toBeNull();
    expect(environmentToDraft(converted.environment!)).toMatchObject({
      extraArguments: "--verbose\nvalue with spaces",
      port: "4317",
    });
  });
});
