/**
 * Frame-level envelope validation.
 *
 * `validateEnvelope` runs the embedded normative schemas
 * (src/schema.ts) with a JSON Schema (draft 2020-12) checker whose semantics
 * are a faithful port of scripts/validate-specs.mjs — the same engine that
 * gates the repo fixtures. The fixture cross-check in test/validate.test.ts
 * pins both to each other, so the library verdict and the schema verdict can
 * never drift apart.
 *
 * Payloads are opaque: the envelope schema only constrains payload to be an
 * object. Per-method payload refinement is intentionally out of scope for
 * the frame layer (see README).
 */

import { schemaRegistry } from "./schema.ts";
import type { CapabilityLease, Envelope } from "./types.ts";

/** One validation finding, with a JSON-pointer-ish path. */
export interface ValidationIssue {
  path: string;
  message: string;
}

/** Discriminated result: `ok: true` narrows to the validated envelope. */
export type ValidationResult =
  | { ok: true; value: Envelope }
  | { ok: false; errors: ValidationIssue[] };

/** Discriminated result for lease-shaped inputs. */
export type LeaseValidationResult =
  | { ok: true; value: CapabilityLease }
  | { ok: false; errors: ValidationIssue[] };

type JsonSchema = Record<string, unknown>;

/**
 * Keyword set supported by this checker — mirrors scripts/validate-specs.mjs,
 * plus minProperties/maxProperties: capability-lease.schema.json relies on
 * minProperties (scope) while the gate script tolerates it silently (it only
 * flags unsupported keywords that a validated fixture actually exercises,
 * and no lease fixtures exist). Supporting it here is a deliberate,
 * documented extension.
 */
const SUPPORTED = new Set([
  "type", "enum", "const", "properties", "required", "additionalProperties",
  "pattern", "minLength", "maxLength", "minimum", "maximum", "exclusiveMinimum",
  "exclusiveMaximum", "minProperties", "maxProperties",
  "minItems", "maxItems", "items", "oneOf", "allOf", "anyOf",
  "if", "then", "else", "not", "title", "description", "$id", "$schema", "$defs", "$ref", "uniqueItems", "format",
]);

function checkKeywords(schema: JsonSchema, at: string, errors: ValidationIssue[]): void {
  for (const key of Object.keys(schema)) {
    if (!SUPPORTED.has(key)) {
      errors.push({ path: at, message: `unsupported schema keyword "${key}"` });
    }
  }
}

function isJsonSchema(value: unknown): value is JsonSchema {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Resolve `$ref`: local \$defs or `./file.schema.json` registry lookup. */
function resolve(
  schema: unknown,
  defs: JsonSchema | undefined,
  registry: ReadonlyMap<string, JsonSchema>,
  at: string,
  errors: ValidationIssue[],
): unknown {
  if (isJsonSchema(schema) && typeof schema.$ref === "string") {
    const ref = schema.$ref;
    if (ref.startsWith("#/$defs/")) {
      const name = ref.slice("#/$defs/".length);
      if (!defs || !(name in defs)) {
        errors.push({ path: at, message: `missing \$defs entry "${name}"` });
        return undefined;
      }
      return defs[name];
    }
    if (ref.startsWith("./")) {
      const target = registry.get(ref.slice(2));
      if (!target) {
        errors.push({ path: at, message: `unresolved file \$ref "${ref}"` });
        return undefined;
      }
      return target;
    }
    errors.push({ path: at, message: `unsupported \$ref "${ref}" (only local \$defs and file refs)` });
    return undefined;
  }
  return schema;
}

function validate(
  instance: unknown,
  rawSchema: unknown,
  at: string,
  errors: ValidationIssue[],
  defs: JsonSchema | undefined,
  registry: ReadonlyMap<string, JsonSchema>,
): void {
  const schema = resolve(rawSchema, defs, registry, at, errors);
  if (schema === undefined) return;
  if (schema === true) return;
  if (schema === false) {
    errors.push({ path: at, message: "schema false" });
    return;
  }
  if (!isJsonSchema(schema)) {
    errors.push({ path: at, message: "schema node is not an object" });
    return;
  }
  checkKeywords(schema, at, errors);

  const constVal = schema.const;
  if (constVal !== undefined) {
    if (JSON.stringify(instance) !== JSON.stringify(constVal)) {
      errors.push({ path: at, message: `expected const ${JSON.stringify(constVal)}, got ${JSON.stringify(instance)}` });
    }
    return;
  }

  const enumVals = schema.enum;
  if (Array.isArray(enumVals)) {
    if (!enumVals.some((v) => JSON.stringify(v) === JSON.stringify(instance))) {
      errors.push({ path: at, message: `value ${JSON.stringify(instance)} not in enum` });
    }
  }

  const type = schema.type;
  if (type !== undefined) {
    const types = Array.isArray(type) ? (type as unknown[]) : [type];
    const actual =
      Array.isArray(instance) ? "array" :
      instance === null ? "null" :
      typeof instance;
    const normalized =
      actual === "number" && types.includes("integer") && Number.isInteger(instance) ? "integer" : actual;
    if (!types.includes(normalized)) {
      errors.push({ path: at, message: `expected type ${types.join("/")}, got ${actual}` });
      return;
    }
  }

  if (typeof instance === "string") {
    if (typeof schema.minLength === "number" && instance.length < schema.minLength) {
      errors.push({ path: at, message: `shorter than minLength ${schema.minLength}` });
    }
    if (typeof schema.maxLength === "number" && instance.length > schema.maxLength) {
      errors.push({ path: at, message: `longer than maxLength ${schema.maxLength}` });
    }
    if (typeof schema.pattern === "string" && !new RegExp(schema.pattern).test(instance)) {
      errors.push({ path: at, message: `"${instance}" does not match ${schema.pattern}` });
    }
    if (schema.format === "date-time" && Number.isNaN(Date.parse(instance))) {
      errors.push({ path: at, message: `"${instance}" is not a valid date-time` });
    }
  }

  if (typeof instance === "number") {
    if (typeof schema.minimum === "number" && instance < schema.minimum) {
      errors.push({ path: at, message: `${instance} < minimum ${schema.minimum}` });
    }
    if (typeof schema.maximum === "number" && instance > schema.maximum) {
      errors.push({ path: at, message: `${instance} > maximum ${schema.maximum}` });
    }
    if (typeof schema.exclusiveMinimum === "number" && instance <= schema.exclusiveMinimum) {
      errors.push({ path: at, message: `${instance} <= exclusiveMinimum ${schema.exclusiveMinimum}` });
    }
    if (typeof schema.exclusiveMaximum === "number" && instance >= schema.exclusiveMaximum) {
      errors.push({ path: at, message: `${instance} >= exclusiveMaximum ${schema.exclusiveMaximum}` });
    }
  }

  if (Array.isArray(instance)) {
    if (typeof schema.minItems === "number" && instance.length < schema.minItems) {
      errors.push({ path: at, message: `fewer than minItems ${schema.minItems}` });
    }
    if (typeof schema.maxItems === "number" && instance.length > schema.maxItems) {
      errors.push({ path: at, message: `more than maxItems ${schema.maxItems}` });
    }
    if (schema.items !== undefined) {
      instance.forEach((item, i) => validate(item, schema.items, `${at}[${i}]`, errors, defs, registry));
    }
    if (schema.uniqueItems === true) {
      const seen = new Set<string>();
      for (const item of instance) {
        const key =
          typeof item === "object" && item !== null ? JSON.stringify(item) : String(item);
        if (seen.has(key)) errors.push({ path: at, message: "items must be unique" });
        seen.add(key);
      }
    }
  }

  if (instance !== null && typeof instance === "object" && !Array.isArray(instance)) {
    const props = isJsonSchema(schema.properties) ? schema.properties : {};
    for (const [key, value] of Object.entries(props)) {
      if (key in (instance as Record<string, unknown>)) {
        validate((instance as Record<string, unknown>)[key], value, `${at}.${key}`, errors, defs, registry);
      }
    }
    for (const key of Array.isArray(schema.required) ? (schema.required as unknown[]) : []) {
      if (typeof key === "string" && !(key in (instance as Record<string, unknown>))) {
        errors.push({ path: at, message: `missing required "${key}"` });
      }
    }
    if (typeof schema.minProperties === "number" && Object.keys(instance as Record<string, unknown>).length < schema.minProperties) {
      errors.push({ path: at, message: `fewer than minProperties ${schema.minProperties}` });
    }
    if (typeof schema.maxProperties === "number" && Object.keys(instance as Record<string, unknown>).length > schema.maxProperties) {
      errors.push({ path: at, message: `more than maxProperties ${schema.maxProperties}` });
    }
    if (schema.additionalProperties === false) {
      for (const key of Object.keys(instance as Record<string, unknown>)) {
        if (!(key in props)) errors.push({ path: at, message: `unexpected property "${key}"` });
      }
    }
  }

  for (const sub of Array.isArray(schema.allOf) ? (schema.allOf as unknown[]) : []) {
    validate(instance, sub, `${at} (allOf)`, errors, defs, registry);
  }
  for (const sub of Array.isArray(schema.anyOf) ? (schema.anyOf as unknown[]) : []) {
    const local: ValidationIssue[] = [];
    validate(instance, sub, `${at} (anyOf)`, local, defs, registry);
    if (local.length === 0) break;
    const subs = schema.anyOf as unknown[];
    if (sub === subs[subs.length - 1]) errors.push(...local);
  }
  if (Array.isArray(schema.oneOf)) {
    let passed = 0;
    for (const sub of schema.oneOf as unknown[]) {
      const local: ValidationIssue[] = [];
      validate(instance, sub, `${at} (oneOf)`, local, defs, registry);
      if (local.length === 0) passed += 1;
    }
    if (passed !== 1) errors.push({ path: at, message: `oneOf matched ${passed} branches (expected exactly 1)` });
  }
  if (schema.not !== undefined) {
    const local: ValidationIssue[] = [];
    validate(instance, schema.not, `${at} (not)`, local, defs, registry);
    if (local.length === 0) errors.push({ path: at, message: "instance matches forbidden subschema (not)" });
  }
  if (schema.if !== undefined) {
    const branchErrors: ValidationIssue[] = [];
    validate(instance, schema.if, `${at} (if)`, branchErrors, defs, registry);
    const matched = branchErrors.length === 0;
    if (matched && schema.then !== undefined) validate(instance, schema.then, `${at} (then)`, errors, defs, registry);
    if (!matched && schema.else !== undefined) validate(instance, schema.else, `${at} (else)`, errors, defs, registry);
  }
}

/** Compile the embedded envelope schema (with its \$defs + file refs). */
function envelopeValidator(): (input: unknown) => ValidationIssue[] {
  const doc = schemaRegistry.get("envelope.schema.json");
  if (!doc) throw new Error("embedded envelope schema missing");
  return (input) => {
    const errors: ValidationIssue[] = [];
    validate(input, doc, "envelope", errors, doc.$defs as JsonSchema | undefined, schemaRegistry as ReadonlyMap<string, JsonSchema>);
    return errors;
  };
}

/**
 * Low-level entry: run the embedded checker against an arbitrary schema node.
 * Used by lease.ts for leaseConstraints shape checks; not part of the
 * public surface for frame validation.
 */
export function validateAgainstSchema(
  instance: unknown,
  schema: unknown,
  defs: JsonSchema | undefined,
  registry: ReadonlyMap<string, JsonSchema> = schemaRegistry as ReadonlyMap<string, JsonSchema>,
): ValidationIssue[] {
  const errors: ValidationIssue[] = [];
  validate(instance, schema, "value", errors, defs, registry);
  return errors;
}

const checkEnvelope = envelopeValidator();

/** Validate one envelope frame against specs/protocol/envelope.schema.json. */
export function validateEnvelope(input: unknown): ValidationResult {
  const errors = checkEnvelope(input);
  return errors.length === 0
    ? { ok: true, value: input as Envelope }
    : { ok: false, errors };
}

const leaseDoc = schemaRegistry.get("capability-lease.schema.json");

/** Validate a capability lease against specs/protocol/capability-lease.schema.json. */
export function validateLease(input: unknown): LeaseValidationResult {
  if (!leaseDoc) throw new Error("embedded lease schema missing");
  const errors: ValidationIssue[] = [];
  validate(input, leaseDoc, "lease", errors, leaseDoc.$defs as JsonSchema | undefined, schemaRegistry as ReadonlyMap<string, JsonSchema>);
  return errors.length === 0
    ? { ok: true, value: input as CapabilityLease }
    : { ok: false, errors };
}
