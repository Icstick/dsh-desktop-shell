/**
 * Frame validation tests.
 *
 * The core binding: validateEnvelope must agree with the JSON Schema
 * verdict on every fixture under specs/protocol/fixtures (22 files,
 * M5-A artifacts). Three independent checks converge:
 *
 *   1. embedded schemas (src/schema.ts) deep-equal the live spec files;
 *   2. the checker run against the *disk* schemas reproduces the expected
 *      valid/invalid verdict for all 22 fixtures (port fidelity);
 *   3. validateEnvelope (embedded schemas) reproduces the same verdict.
 *
 * If the spec files change without the library being updated, at least one
 * of these three tests fails.
 */

import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";

import { validateAgainstSchema, validateEnvelope, validateLease } from "../src/validate.ts";
import { schemaRegistry } from "../src/schema.ts";
import type { Envelope } from "../src/types.ts";

const SPECS_PROTOCOL = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "specs", "protocol");
const FIXTURES = join(SPECS_PROTOCOL, "fixtures");

function readJson(rel: string): Record<string, unknown> {
  return JSON.parse(readFileSync(join(SPECS_PROTOCOL, rel), "utf8")) as Record<string, unknown>;
}

function readFixtures(): Array<{ name: string; expectedValid: boolean; doc: unknown }> {
  return readdirSync(FIXTURES)
    .filter((f) => f.endsWith(".json"))
    .sort()
    .map((name) => ({
      name,
      expectedValid: name.includes(".valid."),
      doc: JSON.parse(readFileSync(join(FIXTURES, name), "utf8")),
    }));
}

const fixtures = readFixtures();
const diskEnvelope = readJson("envelope.schema.json");
const diskCoordinate = readJson("protocol-coordinate.schema.json");
const diskLease = readJson("capability-lease.schema.json");

const diskRegistry = new Map<string, Record<string, unknown>>([
  ["protocol-coordinate.schema.json", diskCoordinate],
  ["envelope.schema.json", diskEnvelope],
  ["capability-lease.schema.json", diskLease],
]);

describe("embedded schemas match specs (anti-drift)", () => {
  it("envelope.schema.json is embedded verbatim", () => {
    expect(schemaRegistry.get("envelope.schema.json")).toEqual(diskEnvelope);
  });
  it("protocol-coordinate.schema.json is embedded verbatim", () => {
    expect(schemaRegistry.get("protocol-coordinate.schema.json")).toEqual(diskCoordinate);
  });
  it("capability-lease.schema.json is embedded verbatim", () => {
    expect(schemaRegistry.get("capability-lease.schema.json")).toEqual(diskLease);
  });
});

describe("checker port fidelity (disk schemas, all fixtures)", () => {
  it.each(fixtures.map((f) => [f.name, f.expectedValid] as const))(
    "%s -> %s",
    (name, expectedValid) => {
      const doc = fixtures.find((f) => f.name === name)!.doc;
      const errors = validateAgainstSchema(doc, diskEnvelope, diskEnvelope.$defs as Record<string, unknown> | undefined, diskRegistry);
      expect(errors.length === 0).toBe(expectedValid);
    },
  );
});

describe("validateEnvelope over all 22 protocol fixtures", () => {
  it.each(fixtures.map((f) => [f.name, f.expectedValid] as const))(
    "%s -> %s",
    (name, expectedValid) => {
      const doc = fixtures.find((f) => f.name === name)!.doc;
      const result = validateEnvelope(doc);
      expect(result.ok).toBe(expectedValid);
    },
  );
});

describe("validateEnvelope frame-level rules beyond fixtures", () => {
  const validHello = () =>
    readJson("fixtures/envelope.hello.valid.json") as unknown as Record<string, unknown>;

  it("rejects a non-object input", () => {
    expect(validateEnvelope(undefined).ok).toBe(false);
    expect(validateEnvelope(null).ok).toBe(false);
    expect(validateEnvelope("hello").ok).toBe(false);
    expect(validateEnvelope(42).ok).toBe(false);
    expect(validateEnvelope([]).ok).toBe(false);
  });

  it("rejects an unknown top-level property (additionalProperties false)", () => {
    const doc = { ...validHello(), extra: 1 };
    const r = validateEnvelope(doc);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.errors.some((e) => e.message.includes("unexpected property"))).toBe(true);
  });

  it("rejects id shorter than 8 chars", () => {
    const doc = { ...validHello(), id: "short" };
    expect(validateEnvelope(doc).ok).toBe(false);
  });

  it("rejects a non-integer generation", () => {
    const doc = { ...validHello(), generation: 1.5 };
    expect(validateEnvelope(doc).ok).toBe(false);
  });

  it("rejects an unparseable timestamp", () => {
    const doc = { ...validHello(), timestamp: "not-a-date" };
    expect(validateEnvelope(doc).ok).toBe(false);
  });

  it("rejects a payload that is not an object", () => {
    const doc = { ...validHello(), payload: ["not", "an", "object"] };
    expect(validateEnvelope(doc).ok).toBe(false);
  });

  it("rejects participant missing facet", () => {
    const doc = { ...validHello(), participant: { component: "dsh-desktop-shell" } };
    expect(validateEnvelope(doc).ok).toBe(false);
  });

  it("rejects duplicate supports entries (uniqueItems)", () => {
    const doc = structuredClone(validHello()) as Record<string, Record<string, unknown>>;
    (doc.payload as { supports: unknown[] }).supports.push((doc.payload as { supports: unknown[] }).supports[0]);
    expect(validateEnvelope(doc).ok).toBe(false);
  });

  it("accepts an empty agreement payload (granted/unavailable may be empty)", () => {
    const agreement = readJson("fixtures/envelope.agreement.degraded.valid.json");
    const r = validateEnvelope(agreement);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value.kind).toBe("Agreement");
  });

  it("narrows ok results to discriminated envelopes", () => {
    const r = validateEnvelope(validHello());
    expect(r.ok).toBe(true);
    if (r.ok) {
      const envelope: Envelope = r.value;
      expect(envelope.kind).toBe("Hello");
    }
  });
});

describe("validateLease against capability-lease schema", () => {
  const baseLease = {
    leaseId: "lease-8f3a9c2e01",
    participantId: "dsh-shell-host-7f3a9c2e",
    activationId: "act-7f3a9c2e",
    capability: { apiVersion: "terminal.dsh-desktop.local/v1alpha1", kind: "Terminal" },
    owner: "agent",
    generation: 1,
    scope: { sessionId: "pty-4f2k9d8a" },
  };

  it("accepts a minimal valid lease", () => {
    const r = validateLease(baseLease);
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value.leaseId).toBe("lease-8f3a9c2e01");
  });

  it("accepts expiresAt as date-time", () => {
    expect(validateLease({ ...baseLease, expiresAt: "2026-08-31T10:30:00.000Z" }).ok).toBe(true);
  });

  it("rejects an unknown lease property (additionalProperties false)", () => {
    const r = validateLease({ ...baseLease, approvalRequired: true });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.errors.some((e) => e.message.includes("unexpected property"))).toBe(true);
  });

  it("rejects a missing scope property", () => {
    const { scope: _scope, ...noScope } = baseLease;
    expect(validateLease(noScope).ok).toBe(false);
  });

  it("rejects an empty scope (minProperties 1)", () => {
    expect(validateLease({ ...baseLease, scope: {} }).ok).toBe(false);
  });

  it("rejects a short leaseId", () => {
    expect(validateLease({ ...baseLease, leaseId: "short" }).ok).toBe(false);
  });

  it("rejects a negative generation", () => {
    expect(validateLease({ ...baseLease, generation: -1 }).ok).toBe(false);
  });

  it("rejects a bad coordinate kind in capability", () => {
    expect(
      validateLease({
        ...baseLease,
        capability: { apiVersion: "terminal.dsh-desktop.local/v1alpha1", kind: "terminal" },
      }).ok,
    ).toBe(false);
  });
});
