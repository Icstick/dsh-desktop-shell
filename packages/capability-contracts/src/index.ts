/**
 * capability-contracts — DSH-neutral protocol wire/shape layer.
 *
 * Public surface:
 *   types        envelope/coordinate/lease type mirror (specs/protocol)
 *   validate     frame-level envelope + lease validation (embedded schemas)
 *   negotiate    negotiation state machine (ADR-0018 activation ownership)
 *   semantics    cross-message sequence rules (reply/correlation/grant/generation)
 *   lease        LeaseConstraints <-> CapabilityLease mapping
 *
 * Normative source: specs/protocol/*.schema.json (bound by validate.test.ts).
 */

export * from "./types.ts";
export * from "./validate.ts";
export * from "./negotiate.ts";
export * from "./semantics.ts";
export * from "./lease.ts";
export { schemaRegistry } from "./schema.ts";
