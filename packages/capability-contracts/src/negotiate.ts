/**
 * Negotiation state machine (ADR-0018 decision 1: activation ownership).
 *
 * Every capability activation completes an independent
 * Hello -> Agreement -> active cycle; a previous Agreement is never reused
 * as a fact for a new generation.
 *
 *   proposed --receiveHello--> proposed
 *   proposed --issueAgreement--> agreed
 *   agreed   --activate--> active
 *   proposed|agreed --reject--> rejected  (terminal)
 *
 * Degrade path: `issueAgreement` with a non-empty `unavailable` list still
 * reaches agreed (partial grant); the session is flagged `degraded` and the
 * activation records exactly which capabilities were not granted and why.
 *
 * State errors surface as discriminated `{ ok: false, code }` results
 * (CONFLICT / INVALID_STATE / MALFORMED_MESSAGE) instead of exceptions;
 * repeated operations are idempotent where that is well-defined.
 */

import { validateEnvelope } from "./validate.ts";
import type {
  AgreementEnvelope,
  HelloEnvelope,
  LeaseConstraints,
  Participant,
  ProtocolCoordinate,
  UnavailableCapability,
} from "./types.ts";
import { PROTOCOL } from "./types.ts";

/** Phase of one activation negotiation. */
export type NegotiationPhase = "proposed" | "agreed" | "active" | "rejected";

/** Rejection reasons: protocol reasons, plus peer-side rejection. */
export type RejectionReason =
  | "unavailable"
  | "unsupported_version"
  | "policy_denied"
  | "provider_failed"
  | "peer_rejected";

/** Outcome codes for state-machine operations. */
export type NegotiationErrorCode = "CONFLICT" | "INVALID_STATE" | "MALFORMED_MESSAGE";

export type NegotiationResult<T> =
  | { ok: true; value: T }
  | { ok: false; code: NegotiationErrorCode; message: string };

/** Recorded activation (ADR-0018: broker registers one per negotiation). */
export interface Activation {
  activationId: string;
  granted: ProtocolCoordinate[];
  unavailable: UnavailableCapability[];
  leaseConstraints?: LeaseConstraints;
  createdAt: string;
  /** True when at least one requested capability was not granted. */
  degraded: boolean;
}

/** Observable timeline entry for audit/observability. */
export interface NegotiationEvent {
  type: "hello" | "agreement" | "activation" | "reject" | "degrade";
  at: string;
  detail: string;
}

/** Decision input for `issueAgreement`. */
export interface AgreementDecision {
  activationId: string;
  granted: ProtocolCoordinate[];
  unavailable: UnavailableCapability[];
  leaseConstraints?: LeaseConstraints;
  /** Envelope metadata; defaults keep the frame consistent with the Hello. */
  id?: string;
  generation?: number;
  timestamp?: string;
  participant?: Participant;
}

export function coordinatesEqual(a: ProtocolCoordinate, b: ProtocolCoordinate): boolean {
  return a.apiVersion === b.apiVersion && a.kind === b.kind;
}

function includesCoordinate(list: readonly ProtocolCoordinate[], target: ProtocolCoordinate): boolean {
  return list.some((c) => coordinatesEqual(c, target));
}

function nowIso(): string {
  return new Date().toISOString();
}

function newMessageId(): string {
  const rand =
    typeof globalThis.crypto !== "undefined" && typeof globalThis.crypto.randomUUID === "function"
      ? globalThis.crypto.randomUUID()
      : `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 14)}`;
  return `msg-${rand}`;
}

/**
 * One activation's negotiation session. Created `proposed`; drives
 * Hello -> Agreement -> active, with reject and degrade paths.
 */
export class NegotiationSession {
  readonly sessionId: string;
  phase: NegotiationPhase = "proposed";
  /** Last validated Hello (null until received). */
  hello: HelloEnvelope | null = null;
  /** Issued Agreement (null until issued). */
  agreement: AgreementEnvelope | null = null;
  /** Recorded activation (null until `activate()`). */
  activation: Activation | null = null;
  /** Rejection record (null unless rejected). */
  rejection: { reason: RejectionReason | string; message?: string; at: string } | null = null;
  /** True once an agreement contained at least one unavailable capability. */
  degraded = false;
  /** Append-only timeline. */
  readonly history: NegotiationEvent[] = [];
  private readonly now: () => string;

  constructor(sessionId: string, opts: { now?: () => string } = {}) {
    this.sessionId = sessionId;
    this.now = opts.now ?? nowIso;
  }

  /** Receive a validated Hello. Idempotent for the same Hello id; CONFLICT otherwise. */
  receiveHello(hello: HelloEnvelope): NegotiationResult<HelloEnvelope> {
    if (hello.kind !== "Hello") {
      return { ok: false, code: "MALFORMED_MESSAGE", message: `expected Hello, got ${hello.kind}` };
    }
    if (this.phase === "rejected") {
      return { ok: false, code: "INVALID_STATE", message: "session is rejected" };
    }
    if (this.phase !== "proposed") {
      return { ok: false, code: "CONFLICT", message: `cannot receive Hello in phase ${this.phase}` };
    }
    if (this.hello !== null) {
      if (this.hello.id === hello.id) return { ok: true, value: this.hello };
      return { ok: false, code: "CONFLICT", message: "Hello already received" };
    }
    this.hello = hello;
    this.history.push({ type: "hello", at: this.now(), detail: hello.id });
    return { ok: true, value: hello };
  }

  /**
   * Broker decision: grant a subset of the requested capabilities.
   * Enforces: granted ⊆ Hello.supports and granted ∩ unavailable = ∅.
   * A non-empty `unavailable` marks the session degraded (still agreed).
   */
  issueAgreement(decision: AgreementDecision): NegotiationResult<AgreementEnvelope> {
    if (this.phase === "rejected") {
      return { ok: false, code: "INVALID_STATE", message: "session is rejected" };
    }
    if (this.hello === null) {
      return { ok: false, code: "INVALID_STATE", message: "no Hello received yet" };
    }
    if (this.phase !== "proposed") {
      return { ok: false, code: "CONFLICT", message: `cannot issue Agreement in phase ${this.phase}` };
    }
    for (const c of decision.granted) {
      if (!includesCoordinate(this.hello.payload.supports, c)) {
        return {
          ok: false,
          code: "CONFLICT",
          message: `granted capability ${c.apiVersion}/${c.kind} is not in Hello.supports`,
        };
      }
    }
    for (const u of decision.unavailable) {
      if (includesCoordinate(decision.granted, u.coordinate)) {
        return {
          ok: false,
          code: "CONFLICT",
          message: `capability ${u.coordinate.apiVersion}/${u.coordinate.kind} is both granted and unavailable`,
        };
      }
    }
    const agreement: AgreementEnvelope = {
      protocol: PROTOCOL,
      id: decision.id ?? newMessageId(),
      kind: "Agreement",
      replyTo: this.hello.id,
      participant: decision.participant ?? this.hello.participant,
      timestamp: decision.timestamp ?? this.now(),
      generation: decision.generation ?? this.hello.generation,
      payload: {
        activationId: decision.activationId,
        granted: [...decision.granted],
        unavailable: [...decision.unavailable],
        ...(decision.leaseConstraints !== undefined ? { leaseConstraints: decision.leaseConstraints } : {}),
      },
    };
    const check = validateEnvelope(agreement);
    if (!check.ok) {
      return {
        ok: false,
        code: "MALFORMED_MESSAGE",
        message: `constructed Agreement failed frame validation: ${check.errors.map((e) => e.message).join("; ")}`,
      };
    }
    this.agreement = check.value as AgreementEnvelope;
    this.phase = "agreed";
    if (decision.unavailable.length > 0) {
      this.degraded = true;
      this.history.push({
        type: "degrade",
        at: this.now(),
        detail: `${decision.unavailable.length} capability(ies) unavailable`,
      });
    }
    this.history.push({ type: "agreement", at: this.now(), detail: agreement.id });
    return { ok: true, value: this.agreement };
  }

  /** Move agreed -> active and record the Activation. Idempotent when active. */
  activate(): NegotiationResult<Activation> {
    if (this.phase === "rejected") {
      return { ok: false, code: "INVALID_STATE", message: "session is rejected" };
    }
    if (this.phase === "active" && this.activation !== null) {
      return { ok: true, value: this.activation };
    }
    if (this.phase !== "agreed" || this.agreement === null) {
      return { ok: false, code: "INVALID_STATE", message: `cannot activate from phase ${this.phase}` };
    }
    const activation: Activation = {
      activationId: this.agreement.payload.activationId,
      granted: [...this.agreement.payload.granted],
      unavailable: [...this.agreement.payload.unavailable],
      ...(this.agreement.payload.leaseConstraints !== undefined
        ? { leaseConstraints: this.agreement.payload.leaseConstraints }
        : {}),
      createdAt: this.now(),
      degraded: this.degraded,
    };
    this.activation = activation;
    this.phase = "active";
    this.history.push({ type: "activation", at: activation.createdAt, detail: activation.activationId });
    return { ok: true, value: activation };
  }

  /** Reject the negotiation (terminal). Idempotent when already rejected. */
  reject(reason: RejectionReason | string, message?: string): NegotiationResult<{ reason: string; at: string }> {
    if (this.phase === "rejected") {
      return { ok: true, value: { reason: this.rejection!.reason, at: this.rejection!.at } };
    }
    if (this.phase === "active") {
      return { ok: false, code: "CONFLICT", message: "cannot reject an active session" };
    }
    const at = this.now();
    this.rejection = { reason, message, at };
    this.phase = "rejected";
    this.history.push({ type: "reject", at, detail: `${reason}${message ? `: ${message}` : ""}` });
    return { ok: true, value: { reason, at } };
  }

  /** All required Hello requirements; a peer should not be activated until these are satisfiable. */
  unsatisfiedRequirements(granted: readonly ProtocolCoordinate[] = this.agreement?.payload.granted ?? []): ProtocolCoordinate[] {
    const hello = this.hello;
    if (hello === null) return [];
    return hello.payload.requires
      .filter((r) => r.required)
      .map((r) => r.coordinate)
      .filter((c) => !includesCoordinate(granted, c));
  }
}
