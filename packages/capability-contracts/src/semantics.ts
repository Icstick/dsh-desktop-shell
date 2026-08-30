/**
 * Cross-message semantic validation.
 *
 * Frame validation (validate.ts) checks each envelope in isolation; this
 * module checks a *sequence* of messages against the protocol's ordering
 * and correlation rules:
 *
 *   1. reply-dangling      replyTo must reference an earlier message id
 *   2. result-target       a Result's replyTo must reference an Invocation
 *   3. correlation-match   Result.error.correlationId === referenced Invocation.id
 *   4. agreement-target    an Agreement's replyTo must reference a Hello
 *   5. grant-within-supports  Agreement.granted ⊆ Hello.supports
 *   6. grant-unavailable-disjoint  Agreement.granted ∩ unavailable = ∅
 *   7. invocation-granted  Invocation.capability ∈ granted of its Agreement
 *   8. generation-monotonic  generation non-decreasing per participant stream
 *   9. id-replay           message ids must be unique within the sequence
 *
 * Rules 5-7 implement the negotiation-time capability discipline
 * (ADR-0018); rules 8-9 reject stale generations and id replay before the
 * broker dispatch gate sees them.
 */

import type {
  AgreementEnvelope,
  Envelope,
  HelloEnvelope,
  InvocationEnvelope,
  Participant,
  ProtocolCoordinate,
} from "./types.ts";
import { coordinatesEqual } from "./negotiate.ts";

/** One semantic finding; `messageId` is the offending message when known. */
export interface SemanticIssue {
  rule:
    | "reply-dangling"
    | "result-target"
    | "correlation-match"
    | "agreement-target"
    | "grant-within-supports"
    | "grant-unavailable-disjoint"
    | "invocation-granted"
    | "generation-monotonic"
    | "id-replay";
  messageId?: string;
  message: string;
}

export interface SemanticResult {
  ok: boolean;
  issues: SemanticIssue[];
}

function participantKey(p: Participant): string {
  return [p.component, p.facet, p.activationId ?? ""].join("|");
}

function includesCoordinate(list: readonly ProtocolCoordinate[], target: ProtocolCoordinate): boolean {
  return list.some((c) => coordinatesEqual(c, target));
}

/**
 * Incremental sequence validator. `push` each message in arrival order;
 * issues accumulate and are re-reported on every call (the result reflects
 * the whole sequence seen so far).
 */
export class SemanticValidator {
  private readonly seenIds = new Set<string>();
  private readonly byId = new Map<string, Envelope>();
  private readonly maxGeneration = new Map<string, number>();
  private readonly agreements: AgreementEnvelope[] = [];
  private readonly issues: SemanticIssue[] = [];

  /** Validate a full, ordered sequence in one call. */
  static validateSequence(messages: readonly Envelope[]): SemanticResult {
    const v = new SemanticValidator();
    for (const m of messages) v.push(m);
    return v.result();
  }

  /** Validate one message against the previously seen sequence. */
  push(msg: Envelope): SemanticResult {
    this.checkIdReplay(msg);
    this.checkGeneration(msg);
    this.checkReply(msg);
    this.checkAgreement(msg);
    this.checkInvocation(msg);
    this.checkResult(msg);
    this.seenIds.add(msg.id);
    this.byId.set(msg.id, msg);
    if (msg.kind === "Agreement") this.agreements.push(msg);
    return this.result();
  }

  /** Current result for the whole sequence so far. */
  result(): SemanticResult {
    return { ok: this.issues.length === 0, issues: [...this.issues] };
  }

  /** Ids seen so far (for tests and diagnostics). */
  get ids(): ReadonlySet<string> {
    return this.seenIds;
  }

  private issue(rule: SemanticIssue["rule"], messageId: string | undefined, message: string): void {
    this.issues.push({ rule, messageId, message });
  }

  private checkIdReplay(msg: Envelope): void {
    if (this.seenIds.has(msg.id)) {
      this.issue("id-replay", msg.id, `message id "${msg.id}" already used in this sequence`);
    }
  }

  private checkGeneration(msg: Envelope): void {
    const key = participantKey(msg.participant);
    const max = this.maxGeneration.get(key);
    if (max !== undefined && msg.generation < max) {
      this.issue(
        "generation-monotonic",
        msg.id,
        `stale generation ${msg.generation} for stream "${key}" (max seen ${max})`,
      );
    } else if (max === undefined || msg.generation > max) {
      this.maxGeneration.set(key, msg.generation);
    }
  }

  private checkReply(msg: Envelope): void {
    if (msg.kind !== "Hello" && !("replyTo" in msg)) return;
    if (msg.kind === "Hello") return; // Hello must not carry replyTo (frame rule)
    const replyTo = (msg as { replyTo: string }).replyTo;
    if (!this.byId.has(replyTo)) {
      this.issue("reply-dangling", msg.id, `replyTo "${replyTo}" does not reference an earlier message`);
      return;
    }
    const target = this.byId.get(replyTo)!;
    if (msg.kind === "Agreement" && target.kind !== "Hello") {
      this.issue("agreement-target", msg.id, `Agreement replyTo must reference a Hello, got ${target.kind}`);
    }
    if (msg.kind === "Result" && target.kind !== "Invocation") {
      this.issue("result-target", msg.id, `Result replyTo must reference an Invocation, got ${target.kind}`);
    }
  }

  private checkAgreement(msg: Envelope): void {
    if (msg.kind !== "Agreement") return;
    const target = this.byId.get(msg.replyTo);
    if (target === undefined || target.kind !== "Hello") return; // reported by reply rules
    const hello = target as HelloEnvelope;
    for (const c of msg.payload.granted) {
      if (!includesCoordinate(hello.payload.supports, c)) {
        this.issue(
          "grant-within-supports",
          msg.id,
          `granted ${c.apiVersion}/${c.kind} is not in Hello.supports`,
        );
      }
    }
    for (const u of msg.payload.unavailable) {
      if (includesCoordinate(msg.payload.granted, u.coordinate)) {
        this.issue(
          "grant-unavailable-disjoint",
          msg.id,
          `${u.coordinate.apiVersion}/${u.coordinate.kind} is both granted and unavailable`,
        );
      }
    }
  }

  private checkInvocation(msg: Envelope): void {
    if (msg.kind !== "Invocation") return;
    const activationId = msg.participant.activationId;
    const agreement = this.agreements
      .filter((a) => activationId === undefined || a.payload.activationId === activationId)
      .at(-1);
    if (agreement === undefined) {
      this.issue(
        "invocation-granted",
        msg.id,
        `no prior Agreement${activationId === undefined ? "" : ` for activation "${activationId}"`} grants ${msg.capability.apiVersion}/${msg.capability.kind}`,
      );
      return;
    }
    if (!includesCoordinate(agreement.payload.granted, msg.capability)) {
      this.issue(
        "invocation-granted",
        msg.id,
        `capability ${msg.capability.apiVersion}/${msg.capability.kind} not granted by Agreement ${agreement.id}`,
      );
    }
  }

  private checkResult(msg: Envelope): void {
    if (msg.kind !== "Result" || !("error" in msg)) return;
    const target = this.byId.get(msg.replyTo);
    if (target === undefined || target.kind !== "Invocation") return; // reported by reply rules
    const invocation = target as InvocationEnvelope;
    if (msg.error.correlationId !== invocation.id) {
      this.issue(
        "correlation-match",
        msg.id,
        `error.correlationId "${msg.error.correlationId}" does not match referenced Invocation id "${invocation.id}"`,
      );
    }
  }
}

/** Convenience: validate an ordered message sequence in one call. */
export function validateSequence(messages: readonly Envelope[]): SemanticResult {
  return SemanticValidator.validateSequence(messages);
}

/** Convenience: validate a single message against prior history. */
export function validateMessage(prior: readonly Envelope[], msg: Envelope): SemanticResult {
  const v = new SemanticValidator();
  for (const m of prior) v.push(m);
  return v.push(msg);
}
