/**
 * Lease mapping tests: LeaseConstraints <-> CapabilityLease, both
 * directions validated against the embedded schemas.
 *
 * Note: approvalRequired is broker-side policy and has no home in the wire
 * lease (capability-lease.schema.json forbids extra properties); the
 * mapping only translates maxSeconds <-> expiresAt.
 */

import { describe, expect, it } from "vitest";

import {
  constraintsToLease,
  leaseToConstraints,
  roundTripSeconds,
  unavailableList,
  validateConstraintsShape,
} from "../src/lease.ts";
import { validateLease } from "../src/validate.ts";
import type { CapabilityLease } from "../src/types.ts";

const TERMINAL = { apiVersion: "terminal.dsh-desktop.local/v1alpha1", kind: "Terminal" };
const CREATED_AT = "2026-08-31T09:30:00.000Z";
const EXPIRES_AT = "2026-08-31T10:30:00.000Z"; // +3600s

const base: Omit<CapabilityLease, "expiresAt"> = {
  leaseId: "lease-8f3a9c2e01",
  participantId: "dsh-shell-host-7f3a9c2e",
  activationId: "act-7f3a9c2e",
  capability: TERMINAL,
  owner: "agent",
  generation: 1,
  scope: { sessionId: "pty-4f2k9d8a", domains: ["example.com"] },
};

describe("constraintsToLease", () => {
  it("maps maxSeconds to expiresAt relative to createdAt", () => {
    const r = constraintsToLease({ maxSeconds: 3600 }, base, { createdAt: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value.expiresAt).toBe(EXPIRES_AT);
      expect(validateLease(r.value).ok).toBe(true);
    }
  });

  it("omits expiresAt without maxSeconds", () => {
    const r = constraintsToLease({}, base, { createdAt: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value.expiresAt).toBeUndefined();
  });

  it("omits expiresAt when constraints are absent entirely", () => {
    const r = constraintsToLease(undefined, base, { createdAt: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value.expiresAt).toBeUndefined();
      expect(validateLease(r.value).ok).toBe(true);
    }
  });

  it("preserves approvalRequired only as broker-side input (never on the wire lease)", () => {
    const r = constraintsToLease({ maxSeconds: 3600, approvalRequired: true }, base, { createdAt: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect("approvalRequired" in r.value).toBe(false);
      expect(validateLease(r.value).ok).toBe(true);
    }
  });

  it("rejects non-integer or out-of-range maxSeconds", () => {
    expect(constraintsToLease({ maxSeconds: 0 }, base, { createdAt: CREATED_AT }).ok).toBe(false);
    expect(constraintsToLease({ maxSeconds: 1.5 }, base, { createdAt: CREATED_AT }).ok).toBe(false);
  });
});

describe("leaseToConstraints", () => {
  it("maps expiresAt to remaining seconds (ceil, min 1)", () => {
    const lease: CapabilityLease = { ...base, expiresAt: EXPIRES_AT };
    const r = leaseToConstraints(lease, { now: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ maxSeconds: 3600 });
  });

  it("returns empty constraints without expiresAt", () => {
    const r = leaseToConstraints({ ...base }, { now: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({});
  });

  it("rounds partial remaining time up, never below 1 second", () => {
    const lease: CapabilityLease = {
      ...base,
      expiresAt: "2026-08-31T09:30:00.500Z", // 500ms remain
    };
    const r = leaseToConstraints(lease, { now: CREATED_AT });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value.maxSeconds).toBe(1);
  });

  it("carries approvalRequired only when the caller supplies it", () => {
    const lease: CapabilityLease = { ...base, expiresAt: EXPIRES_AT };
    const r = leaseToConstraints(lease, { now: CREATED_AT, approvalRequired: true });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ maxSeconds: 3600, approvalRequired: true });
  });

  it("rejects an unparseable expiresAt", () => {
    const lease: CapabilityLease = { ...base, expiresAt: "whenever" };
    const r = leaseToConstraints(lease, { now: CREATED_AT });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("MALFORMED_LEASE");
  });
});

describe("round trip", () => {
  it("constraints -> lease -> constraints reproduces maxSeconds", () => {
    const mapped = constraintsToLease({ maxSeconds: 3600 }, base, { createdAt: CREATED_AT });
    expect(mapped.ok).toBe(true);
    if (!mapped.ok) return;
    const back = leaseToConstraints(mapped.value, { now: CREATED_AT });
    expect(back.ok).toBe(true);
    if (back.ok) expect(back.value.maxSeconds).toBe(3600);
  });

  it("roundTripSeconds derives the original duration at the same clock", () => {
    const mapped = constraintsToLease({ maxSeconds: 7200 }, base, { createdAt: CREATED_AT });
    expect(mapped.ok).toBe(true);
    if (!mapped.ok) return;
    expect(roundTripSeconds(mapped.value, { now: CREATED_AT })).toBe(7200);
    expect(roundTripSeconds({ ...base }, { now: CREATED_AT })).toBeUndefined();
  });
});

describe("helpers", () => {
  it("unavailableList builds valid unavailable entries", () => {
    const list = unavailableList([
      { coordinate: TERMINAL, reason: "policy_denied" },
      { coordinate: { apiVersion: "browser.dsh-desktop.local/v1alpha1", kind: "Browser" }, reason: "provider_failed" },
    ]);
    expect(list).toEqual([
      { coordinate: TERMINAL, reason: "policy_denied" },
      { coordinate: { apiVersion: "browser.dsh-desktop.local/v1alpha1", kind: "Browser" }, reason: "provider_failed" },
    ]);
  });

  it("validateConstraintsShape accepts a valid shape", () => {
    const r = validateConstraintsShape({ maxSeconds: 3600, approvalRequired: true });
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.value).toEqual({ maxSeconds: 3600, approvalRequired: true });
  });

  it("validateConstraintsShape rejects unknown fields", () => {
    expect(validateConstraintsShape({ maxSeconds: 3600, bogus: 1 }).ok).toBe(false);
  });

  it("validateConstraintsShape rejects out-of-range maxSeconds", () => {
    expect(validateConstraintsShape({ maxSeconds: 0 }).ok).toBe(false);
    expect(validateConstraintsShape({ maxSeconds: 1.5 }).ok).toBe(false);
  });

  it("validateConstraintsShape rejects a non-boolean approvalRequired", () => {
    expect(validateConstraintsShape({ approvalRequired: "yes" }).ok).toBe(false);
  });
});
