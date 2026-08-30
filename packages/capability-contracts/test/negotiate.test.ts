/**
 * Negotiation state machine tests (ADR-0018 decision 1: activation
 * ownership — every activation negotiates Hello -> Agreement -> active;
 * previous Agreements are never reused).
 */

import { describe, expect, it } from "vitest";

import { NegotiationSession } from "../src/negotiate.ts";
import { validateEnvelope } from "../src/validate.ts";
import type { HelloEnvelope } from "../src/types.ts";

const TERMINAL = { apiVersion: "terminal.dsh-desktop.local/v1alpha1", kind: "Terminal" };
const BROWSER = { apiVersion: "browser.dsh-desktop.local/v1alpha1", kind: "Browser" };
const RUNTIME = { apiVersion: "runtime.dsh-desktop.local/v1alpha1", kind: "Runtime" };

const FIXED_NOW = "2026-08-31T09:30:00.000Z";
const now = () => FIXED_NOW;

function makeHello(overrides: Partial<HelloEnvelope> = {}): HelloEnvelope {
  return {
    protocol: "interop.dsh-desktop.local/v1alpha1",
    id: "msg-hk8sj2k3l4m5n6p7",
    kind: "Hello",
    participant: { component: "dsh-desktop-shell", facet: "agent" },
    timestamp: FIXED_NOW,
    generation: 0,
    payload: {
      instanceId: "dsh-shell-host-7f3a9c2e",
      supports: [TERMINAL, BROWSER],
      requires: [
        { coordinate: RUNTIME, required: true },
        { coordinate: { apiVersion: "notification.dsh-desktop.local/v1alpha1", kind: "Notification" }, required: false },
      ],
    },
    ...overrides,
  };
}

describe("happy path: proposed -> agreed -> active", () => {
  it("receives Hello and issues a full Agreement", () => {
    const s = new NegotiationSession("sess-1", { now });
    expect(s.phase).toBe("proposed");

    const hello = s.receiveHello(makeHello());
    expect(hello.ok).toBe(true);
    expect(s.phase).toBe("proposed");
    expect(s.hello?.id).toBe("msg-hk8sj2k3l4m5n6p7");

    const agreement = s.issueAgreement({
      activationId: "act-7f3a9c2e",
      granted: [TERMINAL, BROWSER],
      unavailable: [],
    });
    expect(agreement.ok).toBe(true);
    if (agreement.ok) {
      expect(s.phase).toBe("agreed");
      expect(agreement.value.kind).toBe("Agreement");
      expect(agreement.value.replyTo).toBe("msg-hk8sj2k3l4m5n6p7");
      expect(agreement.value.payload.activationId).toBe("act-7f3a9c2e");
      expect(agreement.value.payload.granted).toEqual([TERMINAL, BROWSER]);
      expect(agreement.value.generation).toBe(0); // defaults to hello generation
      // the constructed agreement must itself pass frame validation
      expect(validateEnvelope(agreement.value).ok).toBe(true);
    }

    const activation = s.activate();
    expect(activation.ok).toBe(true);
    if (activation.ok) {
      expect(s.phase).toBe("active");
      expect(activation.value.activationId).toBe("act-7f3a9c2e");
      expect(activation.value.granted).toEqual([TERMINAL, BROWSER]);
      expect(activation.value.degraded).toBe(false);
      expect(activation.value.createdAt).toBe(FIXED_NOW);
    }
  });

  it("supports explicit envelope metadata on the Agreement", () => {
    const s = new NegotiationSession("sess-2", { now });
    s.receiveHello(makeHello());
    const agreement = s.issueAgreement({
      activationId: "act-7f3a9c2e",
      granted: [TERMINAL],
      unavailable: [],
      id: "msg-agreement-0001",
      generation: 1,
      timestamp: "2026-08-31T09:30:01.000Z",
      participant: { component: "dsh-desktop-shell", facet: "broker", activationId: "act-7f3a9c2e" },
    });
    expect(agreement.ok).toBe(true);
    if (agreement.ok) {
      expect(agreement.value.id).toBe("msg-agreement-0001");
      expect(agreement.value.generation).toBe(1);
      expect(agreement.value.participant).toEqual({
        component: "dsh-desktop-shell",
        facet: "broker",
        activationId: "act-7f3a9c2e",
      });
    }
  });

  it("carries leaseConstraints into agreement and activation", () => {
    const s = new NegotiationSession("sess-3", { now });
    s.receiveHello(makeHello());
    const agreement = s.issueAgreement({
      activationId: "act-7f3a9c2e",
      granted: [TERMINAL],
      unavailable: [],
      leaseConstraints: { maxSeconds: 3600, approvalRequired: true },
    });
    expect(agreement.ok).toBe(true);
    if (agreement.ok) {
      expect(agreement.value.payload.leaseConstraints).toEqual({ maxSeconds: 3600, approvalRequired: true });
      expect(validateEnvelope(agreement.value).ok).toBe(true);
    }
    const activation = s.activate();
    expect(activation.ok).toBe(true);
    if (activation.ok) expect(activation.value.leaseConstraints).toEqual({ maxSeconds: 3600, approvalRequired: true });
  });

  it("activate is idempotent once active", () => {
    const s = new NegotiationSession("sess-4", { now });
    s.receiveHello(makeHello());
    s.issueAgreement({ activationId: "act-7f3a9c2e", granted: [TERMINAL], unavailable: [] });
    const first = s.activate();
    const second = s.activate();
    expect(first.ok && second.ok).toBe(true);
    if (first.ok && second.ok) expect(second.value).toEqual(first.value);
    expect(s.history.filter((e) => e.type === "activation")).toHaveLength(1);
  });
});

describe("degrade path", () => {
  it("agrees with partial grants and flags the session degraded", () => {
    const s = new NegotiationSession("sess-degrade", { now });
    s.receiveHello(makeHello());
    const agreement = s.issueAgreement({
      activationId: "act-degraded-01",
      granted: [],
      unavailable: [
        { coordinate: TERMINAL, reason: "unavailable" },
        { coordinate: BROWSER, reason: "unsupported_version" },
      ],
    });
    expect(agreement.ok).toBe(true);
    if (agreement.ok) {
      expect(s.degraded).toBe(true);
      expect(agreement.value.payload.granted).toEqual([]);
      expect(agreement.value.payload.unavailable).toHaveLength(2);
      expect(validateEnvelope(agreement.value).ok).toBe(true);
    }
    const activation = s.activate();
    expect(activation.ok).toBe(true);
    if (activation.ok) {
      expect(activation.value.degraded).toBe(true);
      expect(activation.value.unavailable[0]?.reason).toBe("unavailable");
      expect(activation.value.unavailable[1]?.reason).toBe("unsupported_version");
    }
    expect(s.history.map((e) => e.type)).toEqual(["hello", "degrade", "agreement", "activation"]);
  });
});

describe("reject path", () => {
  it("rejects from proposed and is terminal", () => {
    const s = new NegotiationSession("sess-reject", { now });
    const r = s.reject("policy_denied", "no grant for this peer");
    expect(r.ok).toBe(true);
    expect(s.phase).toBe("rejected");
    expect(s.rejection?.reason).toBe("policy_denied");

    // nothing can proceed after rejection
    expect(s.receiveHello(makeHello()).ok).toBe(false);
    expect(s.issueAgreement({ activationId: "act-x", granted: [], unavailable: [] }).ok).toBe(false);
    expect(s.activate().ok).toBe(false);
  });

  it("rejects from agreed", () => {
    const s = new NegotiationSession("sess-reject2", { now });
    s.receiveHello(makeHello());
    s.issueAgreement({ activationId: "act-7f3a9c2e", granted: [], unavailable: [{ coordinate: TERMINAL, reason: "provider_failed" }] });
    const r = s.reject("provider_failed");
    expect(r.ok).toBe(true);
    expect(s.phase).toBe("rejected");
  });

  it("reject is idempotent and cannot reject an active session", () => {
    const s = new NegotiationSession("sess-reject3", { now });
    s.reject("unavailable");
    expect(s.reject("unavailable").ok).toBe(true);

    const a = new NegotiationSession("sess-active", { now });
    a.receiveHello(makeHello());
    a.issueAgreement({ activationId: "act-7f3a9c2e", granted: [TERMINAL], unavailable: [] });
    a.activate();
    const r = a.reject("policy_denied");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("CONFLICT");
  });
});

describe("state discipline (idempotent-or-CONFLICT)", () => {
  it("rejects a non-Hello envelope", () => {
    const s = new NegotiationSession("sess-x");
    const r = s.receiveHello({ ...makeHello(), kind: "Invocation" } as unknown as HelloEnvelope);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("MALFORMED_MESSAGE");
  });

  it("CONFLICT on a second Hello with a different id", () => {
    const s = new NegotiationSession("sess-x", { now });
    s.receiveHello(makeHello());
    const r = s.receiveHello(makeHello({ id: "msg-different-id" }));
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("CONFLICT");
  });

  it("is idempotent for the same Hello id", () => {
    const s = new NegotiationSession("sess-x", { now });
    s.receiveHello(makeHello());
    const r = s.receiveHello(makeHello());
    expect(r.ok).toBe(true);
  });

  it("cannot issue an Agreement before a Hello", () => {
    const s = new NegotiationSession("sess-x");
    const r = s.issueAgreement({ activationId: "act-x", granted: [], unavailable: [] });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("INVALID_STATE");
  });

  it("CONFLICT on issuing a second Agreement", () => {
    const s = new NegotiationSession("sess-x", { now });
    s.receiveHello(makeHello());
    s.issueAgreement({ activationId: "act-7f3a9c2e", granted: [TERMINAL], unavailable: [] });
    const r = s.issueAgreement({ activationId: "act-7f3a9c2e", granted: [BROWSER], unavailable: [] });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("CONFLICT");
  });

  it("cannot activate from proposed", () => {
    const s = new NegotiationSession("sess-x");
    const r = s.activate();
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("INVALID_STATE");
  });
});

describe("negotiation-time discipline (ADR-0018 decision 1)", () => {
  it("refuses to grant a capability that is not in Hello.supports", () => {
    const s = new NegotiationSession("sess-x", { now });
    s.receiveHello(makeHello());
    const r = s.issueAgreement({
      activationId: "act-7f3a9c2e",
      granted: [{ apiVersion: "usage.dsh-desktop.local/v1alpha1", kind: "Usage" }],
      unavailable: [],
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("CONFLICT");
  });

  it("refuses to grant and mark unavailable the same capability", () => {
    const s = new NegotiationSession("sess-x", { now });
    s.receiveHello(makeHello());
    const r = s.issueAgreement({
      activationId: "act-7f3a9c2e",
      granted: [TERMINAL],
      unavailable: [{ coordinate: TERMINAL, reason: "unavailable" }],
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.code).toBe("CONFLICT");
  });

  it("reports unsatisfied required requirements", () => {
    const s = new NegotiationSession("sess-x", { now });
    s.receiveHello(makeHello());
    s.issueAgreement({ activationId: "act-7f3a9c2e", granted: [TERMINAL], unavailable: [] });
    expect(s.unsatisfiedRequirements()).toEqual([RUNTIME]);
    const withRuntime = new NegotiationSession("sess-y", { now });
    withRuntime.receiveHello(
      makeHello({
        payload: {
          instanceId: "dsh-shell-host-7f3a9c2e",
          supports: [TERMINAL, BROWSER, RUNTIME],
          requires: [
            { coordinate: RUNTIME, required: true },
            { coordinate: { apiVersion: "notification.dsh-desktop.local/v1alpha1", kind: "Notification" }, required: false },
          ],
        },
      }),
    );
    withRuntime.issueAgreement({ activationId: "act-7f3a9c2e", granted: [TERMINAL, RUNTIME], unavailable: [] });
    expect(withRuntime.unsatisfiedRequirements()).toEqual([]);
  });

  it("a new session starts fresh (no Agreement reuse across activations)", () => {
    const first = new NegotiationSession("act-1", { now });
    first.receiveHello(makeHello());
    first.issueAgreement({ activationId: "act-1", granted: [TERMINAL], unavailable: [] });
    first.activate();

    const second = new NegotiationSession("act-2", { now });
    expect(second.phase).toBe("proposed");
    expect(second.agreement).toBeNull();
    expect(second.activation).toBeNull();
    second.receiveHello(makeHello({ id: "msg-second-hello" }));
    second.issueAgreement({ activationId: "act-2", granted: [BROWSER], unavailable: [] });
    expect(second.agreement?.payload.activationId).toBe("act-2");
    expect(second.agreement?.replyTo).toBe("msg-second-hello");
  });
});
