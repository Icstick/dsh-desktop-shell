/**
 * LeaseConstraints <-> CapabilityLease mapping.
 *
 * `LeaseConstraints` rides inside Agreement.payload (envelope schema);
 * `CapabilityLease` is the broker-issued lease (capability-lease schema).
 * The two shapes overlap on duration only:
 *
 *   constraints.maxSeconds  ->  lease.expiresAt = createdAt + maxSeconds
 *   lease.expiresAt         ->  constraints.maxSeconds = ceil(remaining seconds)
 *
 * `approvalRequired` is broker-side authorization policy and has no home in
 * the wire lease (capability-lease.schema.json forbids extra properties), so
 * the mapping deliberately does not carry it; brokers keep it in their own
 * grant record.
 *
 * Both directions are validated against the embedded schemas; produced
 * leases are checked with `validateLease` and produced constraints by
 * `validateEnvelope`-compatible shape rules via the embedded agreement
 * payload schema.
 */

import { validateAgainstSchema, validateLease } from "./validate.ts";
import { schemaRegistry } from "./schema.ts";
import type {
  CapabilityLease,
  LeaseConstraints,
  ProtocolCoordinate,
  UnavailableCapability,
} from "./types.ts";

export interface LeaseBase {
  leaseId: string;
  participantId: string;
  activationId: string;
  capability: ProtocolCoordinate;
  owner: string;
  generation: number;
  scope: CapabilityLease["scope"];
}

export type LeaseMappingResult<T> =
  | { ok: true; value: T }
  | { ok: false; code: "INVALID_INPUT" | "MALFORMED_LEASE"; message: string };

/**
 * Map Agreement lease constraints onto a broker lease.
 * maxSeconds -> expiresAt (createdAt + maxSeconds*1000ms). Without
 * maxSeconds the lease carries no expiresAt (no expiry enforcement).
 */
export function constraintsToLease(
  constraints: LeaseConstraints | undefined,
  base: LeaseBase,
  opts: { createdAt?: string } = {},
): LeaseMappingResult<CapabilityLease> {
  const createdAt = opts.createdAt ?? new Date().toISOString();
  const lease: CapabilityLease = {
    leaseId: base.leaseId,
    participantId: base.participantId,
    activationId: base.activationId,
    capability: { ...base.capability },
    owner: base.owner,
    generation: base.generation,
    scope: { ...base.scope },
  };
  if (constraints?.maxSeconds !== undefined) {
    if (!Number.isInteger(constraints.maxSeconds) || constraints.maxSeconds < 1) {
      return { ok: false, code: "INVALID_INPUT", message: `maxSeconds must be an integer >= 1, got ${constraints.maxSeconds}` };
    }
    lease.expiresAt = new Date(Date.parse(createdAt) + constraints.maxSeconds * 1000).toISOString();
  }
  const check = validateLease(lease);
  if (!check.ok) {
    return {
      ok: false,
      code: "MALFORMED_LEASE",
      message: `mapped lease failed capability-lease schema: ${check.errors.map((e) => e.message).join("; ")}`,
    };
  }
  return { ok: true, value: lease };
}

/**
 * Map a broker lease back to Agreement lease constraints.
 * expiresAt -> maxSeconds = ceil(remaining seconds, min 1). Without
 * expiresAt the result is an empty constraints object.
 * `approvalRequired` is not representable in the wire lease; the caller
 * supplies it via `approvalRequired` (default false).
 */
export function leaseToConstraints(
  lease: CapabilityLease,
  opts: { now?: string; approvalRequired?: boolean } = {},
): LeaseMappingResult<LeaseConstraints> {
  const now = Date.parse(opts.now ?? new Date().toISOString());
  const constraints: LeaseConstraints = {};
  if (lease.expiresAt !== undefined) {
    const expires = Date.parse(lease.expiresAt);
    if (Number.isNaN(expires)) {
      return { ok: false, code: "MALFORMED_LEASE", message: `lease.expiresAt "${lease.expiresAt}" is not a date-time` };
    }
    const remainingMs = expires - now;
    const maxSeconds = Math.max(1, Math.ceil(remainingMs / 1000));
    constraints.maxSeconds = maxSeconds;
  }
  if (opts.approvalRequired === true) {
    constraints.approvalRequired = true;
  }
  return { ok: true, value: constraints };
}

/**
 * Round-trip helper: derive the constraints that would reproduce a lease's
 * expiry exactly (test seam for the maxSeconds <-> expiresAt mapping).
 */
export function roundTripSeconds(lease: CapabilityLease, opts: { now?: string } = {}): number | undefined {
  const now = Date.parse(opts.now ?? new Date().toISOString());
  if (lease.expiresAt === undefined) return undefined;
  return Math.max(1, Math.ceil((Date.parse(lease.expiresAt) - now) / 1000));
}

/**
 * Build the agreement payload's unavailable list for a set of declined
 * capabilities (helper for degrade-path agreements).
 */
export function unavailableList(
  items: ReadonlyArray<{ coordinate: ProtocolCoordinate; reason: UnavailableCapability["reason"] }>,
): UnavailableCapability[] {
  return items.map((i) => ({ coordinate: { ...i.coordinate }, reason: i.reason }));
}

/**
 * Validate a candidate leaseConstraints object against the embedded
 * agreement payload schema (same checker as validate.ts).
 */
export function validateConstraintsShape(constraints: unknown): LeaseMappingResult<LeaseConstraints> {
  const envelope = schemaRegistry.get("envelope.schema.json");
  if (!envelope) return { ok: false, code: "MALFORMED_LEASE", message: "embedded envelope schema missing" };
  const defs = envelope.$defs as Record<string, unknown> | undefined;
  const agreementPayload = defs?.agreementPayload as { properties?: Record<string, unknown> } | undefined;
  const constraintsSchema = agreementPayload?.properties?.leaseConstraints;
  if (constraintsSchema === undefined) {
    return { ok: false, code: "MALFORMED_LEASE", message: "leaseConstraints schema not found in embedded envelope schema" };
  }
  const errors = validateAgainstSchema(constraints, constraintsSchema, defs, schemaRegistry);
  if (errors.length > 0) {
    return { ok: false, code: "MALFORMED_LEASE", message: errors.map((e) => e.message).join("; ") };
  }
  return { ok: true, value: constraints as LeaseConstraints };
}
