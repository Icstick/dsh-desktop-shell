/**
 * Wire/shape types for the interop protocol.
 *
 * Field-for-field mirror of the normative schemas:
 *   specs/protocol/envelope.schema.json
 *   specs/protocol/protocol-coordinate.schema.json
 *   specs/protocol/capability-lease.schema.json
 *
 * No extra fields are permitted at runtime: shape enforcement lives in
 * `validate.ts` (frame-level) and `semantics.ts` (cross-message rules).
 * These types describe the *validated* shape — prefer narrowing values
 * through `validateEnvelope` before treating them as `Envelope`.
 */

/** Wire protocol identifier (envelope.schema.json `protocol` const). */
export const PROTOCOL = "interop.dsh-desktop.local/v1alpha1" as const;

/** Envelope kind discriminator (envelope.schema.json `kind` enum). */
export type EnvelopeKind = "Hello" | "Agreement" | "Invocation" | "Result" | "Event";

/** Reason a requested capability was not granted (agreementPayload.unavailable[].reason enum). */
export type UnavailableReason =
  | "unavailable"
  | "unsupported_version"
  | "policy_denied"
  | "provider_failed";

/** Structured error codes (envelope `error.code` enum). */
export type ErrorCode =
  | "UNAVAILABLE"
  | "UNAUTHORIZED"
  | "UNSUPPORTED_VERSION"
  | "NOT_PROCESS_OWNER"
  | "USER_GESTURE_REQUIRED"
  | "USER_DENIED"
  | "STALE_GENERATION"
  | "MALFORMED_MESSAGE"
  | "CONFLICT"
  | "TIMEOUT"
  | "SAFE_STOP";

/**
 * Capability coordinate (protocol-coordinate.schema.json).
 * `apiVersion` matches `^[a-z0-9.-]+/v[0-9]+(alpha[0-9]+|beta[0-9]+)?$`,
 * `kind` matches `^[A-Z][A-Za-z0-9]+$` (enforced by validate.ts).
 */
export interface ProtocolCoordinate {
  apiVersion: string;
  kind: string;
}

/** One entry of `Hello.payload.requires` (envelope.schema.json `requirement`). */
export interface Requirement {
  coordinate: ProtocolCoordinate;
  required: boolean;
}

/** `Hello` payload (envelope.schema.json `helloPayload`). */
export interface HelloPayload {
  /** Peer instance identifier, 8..128 chars. */
  instanceId: string;
  /** Capabilities the peer can provide; unique array. */
  supports: ProtocolCoordinate[];
  /** Capabilities the peer requires of us; unique array. */
  requires: Requirement[];
}

/** One entry of `Agreement.payload.unavailable` (envelope.schema.json `unavailableCapability`). */
export interface UnavailableCapability {
  coordinate: ProtocolCoordinate;
  reason: UnavailableReason;
}

/** Lease constraints offered inside an Agreement (envelope.schema.json `agreementPayload.leaseConstraints`). */
export interface LeaseConstraints {
  /** Lease duration in seconds (integer >= 1). */
  maxSeconds?: number;
  /** Whether mutation under this lease requires an explicit human approval. */
  approvalRequired?: boolean;
}

/** `Agreement` payload (envelope.schema.json `agreementPayload`). */
export interface AgreementPayload {
  activationId: string;
  /** Capabilities granted to the peer; unique array. */
  granted: ProtocolCoordinate[];
  /** Requested capabilities that could not be granted; unique array. */
  unavailable: UnavailableCapability[];
  leaseConstraints?: LeaseConstraints;
}

/** Envelope sender identity (envelope.schema.json `participant`). */
export interface Participant {
  /** Component identifier, minLength 1. */
  component: string;
  /** Facet identifier, minLength 1. */
  facet: string;
  /** Activation this participant is acting under, 1..128 chars. */
  activationId?: string;
}

/** Structured protocol error (envelope `error`). */
export interface ProtocolError {
  code: ErrorCode;
  /** 0..512 chars. */
  message: string;
  retryable: boolean;
  /** Id of the message this error correlates with, 8..128 chars. */
  correlationId: string;
}

/** Fields shared by every envelope (envelope.schema.json root `required`). */
export interface EnvelopeBase {
  protocol: typeof PROTOCOL;
  /** Message id, 8..128 chars, unique per session (replay rejected). */
  id: string;
  participant: Participant;
  /** RFC 3339 date-time. */
  timestamp: string;
  /** Non-negative integer; must be monotonic per participant stream. */
  generation: number;
}

/** `Hello` — negotiation opener; must NOT carry replyTo/capability/method/error. */
export interface HelloEnvelope extends EnvelopeBase {
  kind: "Hello";
  payload: HelloPayload;
}

/** `Agreement` — reply to a Hello; must NOT carry capability/method/error. */
export interface AgreementEnvelope extends EnvelopeBase {
  kind: "Agreement";
  replyTo: string;
  payload: AgreementPayload;
}

/** `Invocation` — a capability call; must NOT carry replyTo/error. */
export interface InvocationEnvelope extends EnvelopeBase {
  kind: "Invocation";
  capability: ProtocolCoordinate;
  /** Matches `^[a-z][a-z0-9._-]+$`; per-capability method names. */
  method: string;
  /** Opaque call payload; frame-level validation does not refine it. */
  payload: Record<string, unknown>;
}

/** `Result` — success branch: exactly one of payload/error (schema oneOf). */
export interface ResultSuccessEnvelope extends EnvelopeBase {
  kind: "Result";
  replyTo: string;
  capability: ProtocolCoordinate;
  method: string;
  payload: Record<string, unknown>;
}

/** `Result` — error branch: exactly one of payload/error (schema oneOf). */
export interface ResultErrorEnvelope extends EnvelopeBase {
  kind: "Result";
  replyTo: string;
  capability: ProtocolCoordinate;
  method: string;
  error: ProtocolError;
}

/** `Result` discriminated on the oneOf: success xor error. */
export type ResultEnvelope = ResultSuccessEnvelope | ResultErrorEnvelope;

/** `Event` — asynchronous capability event; must NOT carry error. */
export interface EventEnvelope extends EnvelopeBase {
  kind: "Event";
  capability: ProtocolCoordinate;
  method: string;
  payload: Record<string, unknown>;
}

/** Any validated envelope, discriminated on `kind`. */
export type Envelope =
  | HelloEnvelope
  | AgreementEnvelope
  | InvocationEnvelope
  | ResultEnvelope
  | EventEnvelope;

/**
 * Capability lease (capability-lease.schema.json).
 * `scope` must have at least one property and no unknown properties.
 */
export interface LeaseScope {
  sessionId?: string;
  workspace?: string;
  /** Unique array. */
  domains?: string[];
  /** Unique array. */
  resources?: string[];
}

/** Broker-issued lease for one granted capability. */
export interface CapabilityLease {
  /** 8+ chars. */
  leaseId: string;
  participantId: string;
  activationId: string;
  capability: ProtocolCoordinate;
  owner: string;
  /** Non-negative integer. */
  generation: number;
  scope: LeaseScope;
  /** RFC 3339 date-time. */
  expiresAt?: string;
}
