/**
 * Cross-message semantics tests: replyTo chaining, error correlation,
 * negotiation-time capability discipline, generation monotonicity and
 * id replay rejection — positive and negative cases for every rule.
 */

import { describe, expect, it } from "vitest";

import { SemanticValidator, validateSequence } from "../src/semantics.ts";
import type { Envelope, InvocationEnvelope } from "../src/types.ts";

const TERMINAL = { apiVersion: "terminal.dsh-desktop.local/v1alpha1", kind: "Terminal" };
const BROWSER = { apiVersion: "browser.dsh-desktop.local/v1alpha1", kind: "Browser" };

function hello(id = "msg-hk8sj2k3l4m5n6p7"): Envelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id,
    kind: "Hello",
    participant: { component: "dsh-desktop-shell", facet: "agent" },
    timestamp: "2026-08-31T09:30:00.000Z",
    generation: 0,
    payload: {
      instanceId: "dsh-shell-host-7f3a9c2e",
      supports: [TERMINAL, BROWSER],
      requires: [],
    },
  };
}

function agreement(id = "msg-agreement-001", replyTo = "msg-hk8sj2k3l4m5n6p7", granted = [TERMINAL, BROWSER]): Envelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id,
    kind: "Agreement",
    participant: { component: "dsh-desktop-shell", facet: "broker", activationId: "act-7f3a9c2e" },
    timestamp: "2026-08-31T09:30:01.000Z",
    generation: 1,
    replyTo,
    payload: { activationId: "act-7f3a9c2e", granted, unavailable: [] },
  };
}

function invocation(id = "msg-invocation-001", overrides: Partial<InvocationEnvelope> = {}): Envelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id,
    kind: "Invocation",
    participant: { component: "dsh-desktop-shell", facet: "agent", activationId: "act-7f3a9c2e" },
    timestamp: "2026-08-31T09:30:02.000Z",
    generation: 1,
    capability: TERMINAL,
    method: "write",
    payload: { schemaVersion: 1, sessionId: "pty-4f2k9d8a", data: "ls\n" },
    ...overrides,
  };
}

function resultSuccess(id = "msg-result-001", replyTo = "msg-invocation-001"): Envelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id,
    kind: "Result",
    participant: { component: "terminal-dsh-desktop-local", facet: "pty-4f2k9d8a" },
    timestamp: "2026-08-31T09:30:02.100Z",
    generation: 1,
    replyTo,
    capability: TERMINAL,
    method: "write",
    payload: { bytesWritten: 8 },
  };
}

function resultError(id = "msg-result-err-01", replyTo = "msg-invocation-001", correlationId = "msg-invocation-001"): Envelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id,
    kind: "Result",
    participant: { component: "terminal-dsh-desktop-local", facet: "pty-4f2k9d8a" },
    timestamp: "2026-08-31T09:30:02.200Z",
    generation: 1,
    replyTo,
    capability: TERMINAL,
    method: "write",
    error: { code: "TIMEOUT", message: "no ack", retryable: true, correlationId },
  };
}

function event(id = "msg-event-001"): Envelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id,
    kind: "Event",
    participant: { component: "terminal-dsh-desktop-local", facet: "pty-4f2k9d8a" },
    timestamp: "2026-08-31T09:30:03.000Z",
    generation: 1,
    capability: TERMINAL,
    method: "output",
    payload: { seq: 1, data: "total 12\n" },
  };
}

const happyPath = [hello(), agreement(), invocation(), resultSuccess(), event()];

describe("happy path", () => {
  it("accepts Hello -> Agreement -> Invocation -> Result -> Event", () => {
    const r = validateSequence(happyPath);
    expect(r.ok).toBe(true);
    expect(r.issues).toEqual([]);
  });

  it("accepts an error Result whose correlationId matches the Invocation", () => {
    const seq = [hello(), agreement(), invocation(), resultError()];
    expect(validateSequence(seq).ok).toBe(true);
  });

  it("supports incremental push with cumulative results", () => {
    const v = new SemanticValidator();
    expect(v.push(hello()).ok).toBe(true);
    expect(v.push(agreement()).ok).toBe(true);
    const r = v.push(invocation());
    expect(r.ok).toBe(true);
    expect(v.push(resultSuccess()).ok).toBe(true);
  });
});

describe("replyTo rules", () => {
  it("rejects a dangling replyTo (no earlier message with that id)", () => {
    const seq = [hello(), { ...agreement(), replyTo: "msg-ghost-id" }];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "reply-dangling")).toBe(true);
  });

  it("rejects a Result that does not reference an Invocation", () => {
    const seq = [hello(), resultSuccess("msg-r1", hello().id)];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "result-target")).toBe(true);
  });

  it("rejects an Agreement that does not reference a Hello", () => {
    // The invocation exists in the sequence, so the Agreement's replyTo is
    // resolvable but points at the wrong kind.
    const seq = [hello(), agreement(), invocation(), { ...agreement("msg-agreement-002", "msg-invocation-001") }];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "agreement-target")).toBe(true);
  });

  it("rejects a replyTo pointing at a later message", () => {
    // Result is pushed first: its target invocation only arrives later.
    const seq = [hello(), agreement(), resultSuccess("msg-r2", "msg-invocation-later")];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "reply-dangling")).toBe(true);
  });
});

describe("correlation rules", () => {
  it("rejects Result.error.correlationId that does not match the referenced Invocation", () => {
    const seq = [hello(), agreement(), invocation(), resultError("msg-r3", "msg-invocation-001", "msg-other-id")];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "correlation-match")).toBe(true);
  });
});

describe("negotiation-time capability discipline", () => {
  it("rejects Agreement.granted outside Hello.supports", () => {
    const seq = [hello(), agreement("msg-agreement-003", "msg-hk8sj2k3l4m5n6p7", [
      { apiVersion: "usage.dsh-desktop.local/v1alpha1", kind: "Usage" },
    ])];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "grant-within-supports")).toBe(true);
  });

  it("rejects a capability both granted and unavailable", () => {
    const both = {
      ...agreement("msg-agreement-004"),
      payload: {
        activationId: "act-7f3a9c2e",
        granted: [TERMINAL],
        unavailable: [{ coordinate: TERMINAL, reason: "unavailable" }],
      },
    } as Envelope;
    const r = validateSequence([hello(), both]);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "grant-unavailable-disjoint")).toBe(true);
  });

  it("rejects an Invocation whose capability was not granted", () => {
    const seq = [
      hello(),
      agreement("msg-agreement-005", "msg-hk8sj2k3l4m5n6p7", [BROWSER]),
      invocation("msg-invocation-002", { capability: TERMINAL }),
    ];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "invocation-granted")).toBe(true);
  });

  it("rejects an Invocation with no prior Agreement at all", () => {
    const r = validateSequence([hello(), invocation("msg-invocation-003")]);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "invocation-granted")).toBe(true);
  });

  it("matches Agreement by participant.activationId", () => {
    // An invocation for a different activation must not borrow the grant.
    const seq = [
      hello(),
      agreement(),
      invocation("msg-invocation-004", {
        participant: { component: "dsh-desktop-shell", facet: "agent", activationId: "act-other-01" },
      }),
    ];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "invocation-granted")).toBe(true);
  });
});

describe("generation and replay rules", () => {
  it("rejects a stale (decreasing) generation on a participant stream", () => {
    // Two invocations on the same activation stream: 1 then 0 is stale.
    const seq = [
      hello(),
      agreement(),
      invocation("msg-invocation-005", { generation: 1 }),
      invocation("msg-invocation-006", { generation: 0 }),
    ];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "generation-monotonic")).toBe(true);
  });

  it("accepts equal generations (non-decreasing)", () => {
    const seq = [
      hello(),
      agreement(),
      invocation("msg-invocation-007", { generation: 1 }),
      invocation("msg-invocation-008", { generation: 1 }),
    ];
    expect(validateSequence(seq).ok).toBe(true);
  });

  it("tracks generations per participant stream, not globally", () => {
    // Peer stream runs 0 -> 1 (hello, agreement); agent activation stream and
    // provider stream each start at 1 independently.
    const seq = [hello(), agreement(), invocation("msg-invocation-009"), resultSuccess("msg-result-004", "msg-invocation-009")];
    expect(validateSequence(seq).ok).toBe(true);
  });

  it("rejects id replay", () => {
    const seq = [hello(), hello("msg-hk8sj2k3l4m5n6p7")];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    expect(r.issues.some((i) => i.rule === "id-replay")).toBe(true);
  });

  it("reports multiple independent issues at once", () => {
    const seq = [
      hello(),
      { ...agreement(), replyTo: "msg-ghost-id" }, // dangling replyTo
      invocation("msg-invocation-001", { generation: 1 }),
      invocation("msg-invocation-001", { generation: 0 }), // id replay + stale generation
    ];
    const r = validateSequence(seq);
    expect(r.ok).toBe(false);
    const rules = new Set(r.issues.map((i) => i.rule));
    expect(rules.has("reply-dangling")).toBe(true);
    expect(rules.has("generation-monotonic")).toBe(true);
    expect(rules.has("id-replay")).toBe(true);
  });
});
