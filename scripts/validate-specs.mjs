/**
 * Minimal JSON Schema (draft 2020-12) validator for this repository's
 * normative specs and fixtures.
 *
 * Rules:
 *  - every schema file under specs/ must parse and compile;
 *  - every fixture file under specs/.../fixtures/ must match exactly one
 *    schema whose basename is a prefix of the fixture basename (e.g.
 *    managed-runtime-report.safe-stop.valid.json matches
 *    managed-runtime-report.schema.json), and must validate when the
 *    filename contains ".valid." and must fail when it contains ".invalid.";
 *  - unsupported keywords abort loudly (never silently pass).
 *
 * Usage: node scripts/validate-specs.mjs [repo-root]
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = process.argv[2] ?? ".";
const SUPPORTED = new Set([
  "type", "enum", "const", "properties", "required", "additionalProperties",
  "pattern", "minLength", "maxLength", "minimum", "maximum", "exclusiveMinimum",
  "exclusiveMaximum", "minItems", "maxItems", "items", "oneOf", "allOf", "anyOf",
  "if", "then", "else", "title", "description", "$id", "$schema", "$defs", "$ref", "uniqueItems",
]);

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (name.endsWith(".json")) out.push(full);
  }
  return out;
}

function checkKeywords(schema, path, errors) {
  for (const key of Object.keys(schema)) {
    if (!SUPPORTED.has(key)) {
      errors.push(`unsupported schema keyword "${key}" at ${path}`);
    }
  }
}

function resolve(schema, defs, registry, schemaDir) {
  if (schema && typeof schema === "object" && typeof schema.$ref === "string") {
    const ref = schema.$ref;
    if (ref.startsWith("#/$defs/")) {
      const name = ref.slice("#/$defs/".length);
      if (!defs || !(name in defs)) throw new Error(`missing \$defs entry "${name}"`);
      return defs[name];
    }
    if (ref.startsWith("./")) {
      const target = join(schemaDir, ref.slice(2));
      const doc = registry.get(target);
      if (!doc) throw new Error(`unresolved file \$ref "${ref}"`);
      return doc;
    }
    throw new Error(`unsupported \$ref "${ref}" (only local \$defs and file refs are supported)`);
  }
  return schema;
}

function validate(instance, schema, path, errors, defs, registry, schemaDir) {
  schema = resolve(schema, defs, registry, schemaDir);
  if (schema === true) return;
  if (schema === false) {
    errors.push(`schema false at ${path}`);
    return;
  }
  checkKeywords(schema, path, errors);
  const { type, enum: enumVals, const: constVal } = schema;
  if (constVal !== undefined) {
    if (JSON.stringify(instance) !== JSON.stringify(constVal)) {
      errors.push(`${path}: expected const ${JSON.stringify(constVal)}, got ${JSON.stringify(instance)}`);
    }
    return;
  }
  if (enumVals !== undefined) {
    if (!enumVals.some((v) => JSON.stringify(v) === JSON.stringify(instance))) {
      errors.push(`${path}: value ${JSON.stringify(instance)} not in enum`);
    }
  }
  if (type !== undefined) {
    const types = Array.isArray(type) ? type : [type];
    let actual = Array.isArray(instance) ? "array" : instance === null ? "null" : typeof instance;
    if (actual === "number" && types.includes("integer") && Number.isInteger(instance)) {
      actual = "integer";
    }
    if (!types.includes(actual)) {
      errors.push(`${path}: expected type ${types.join("/")}, got ${actual}`);
      return;
    }
  }
  if (typeof instance === "string") {
    if (schema.minLength !== undefined && instance.length < schema.minLength) {
      errors.push(`${path}: shorter than minLength ${schema.minLength}`);
    }
    if (schema.maxLength !== undefined && instance.length > schema.maxLength) {
      errors.push(`${path}: longer than maxLength ${schema.maxLength}`);
    }
    if (schema.pattern !== undefined && !new RegExp(schema.pattern).test(instance)) {
      errors.push(`${path}: "${instance}" does not match ${schema.pattern}`);
    }
  }
  if (typeof instance === "number") {
    if (schema.minimum !== undefined && instance < schema.minimum) {
      errors.push(`${path}: ${instance} < minimum ${schema.minimum}`);
    }
    if (schema.maximum !== undefined && instance > schema.maximum) {
      errors.push(`${path}: ${instance} > maximum ${schema.maximum}`);
    }
    if (schema.exclusiveMinimum !== undefined && instance <= schema.exclusiveMinimum) {
      errors.push(`${path}: ${instance} <= exclusiveMinimum ${schema.exclusiveMinimum}`);
    }
    if (schema.exclusiveMaximum !== undefined && instance >= schema.exclusiveMaximum) {
      errors.push(`${path}: ${instance} >= exclusiveMaximum ${schema.exclusiveMaximum}`);
    }
  }
  if (Array.isArray(instance)) {
    if (schema.minItems !== undefined && instance.length < schema.minItems) {
      errors.push(`${path}: fewer than minItems ${schema.minItems}`);
    }
    if (schema.maxItems !== undefined && instance.length > schema.maxItems) {
      errors.push(`${path}: more than maxItems ${schema.maxItems}`);
    }
    if (schema.items !== undefined) {
      instance.forEach((item, i) => validate(item, schema.items, `${path}[${i}]`, errors, defs, registry, schemaDir));
    }
    if (schema.uniqueItems === true) {
      const seen = new Set();
      for (const item of instance) {
        const key = typeof item === "object" && item !== null ? JSON.stringify(item) : String(item);
        if (seen.has(key)) errors.push(`${path}: items must be unique`);
        seen.add(key);
      }
    }
  }
  if (instance !== null && typeof instance === "object" && !Array.isArray(instance)) {
    const props = schema.properties ?? {};
    for (const [key, value] of Object.entries(props)) {
      if (key in instance) validate(instance[key], value, `${path}.${key}`, errors, defs, registry, schemaDir);
    }
    for (const key of schema.required ?? []) {
      if (!(key in instance)) errors.push(`${path}: missing required "${key}"`);
    }
    if (schema.additionalProperties === false) {
      for (const key of Object.keys(instance)) {
        if (!(key in props)) errors.push(`${path}: unexpected property "${key}"`);
      }
    }
  }
  for (const sub of schema.allOf ?? []) validate(instance, sub, `${path} (allOf)`, errors, defs, registry, schemaDir);
  for (const sub of schema.anyOf ?? []) {
    const local = [];
    validate(instance, sub, `${path} (anyOf)`, local, defs, registry, schemaDir);
    if (local.length === 0) break;
    if (sub === schema.anyOf[schema.anyOf.length - 1]) errors.push(...local);
  }
  if (schema.oneOf !== undefined) {
    let passed = 0;
    for (const sub of schema.oneOf) {
      const local = [];
      validate(instance, sub, `${path} (oneOf)`, local, defs, registry, schemaDir);
      if (local.length === 0) passed += 1;
    }
    if (passed !== 1) errors.push(`${path}: oneOf matched ${passed} branches (expected exactly 1)`);
  }
  if (schema.if !== undefined) {
    const branchErrors = [];
    validate(instance, schema.if, `${path} (if)`, branchErrors, defs, registry, schemaDir);
    const matched = branchErrors.length === 0;
    if (matched && schema.then !== undefined) validate(instance, schema.then, `${path} (then)`, errors, defs, registry, schemaDir);
    if (!matched && schema.else !== undefined) validate(instance, schema.else, `${path} (else)`, errors, defs, registry, schemaDir);
  }
}

function main() {
  const files = walk(join(root, "specs"));
  const schemas = files.filter((f) => f.endsWith(".schema.json"));
  const fixtures = files.filter((f) => f.includes("fixtures"));
  const schemaDocs = new Map();
  let failures = 0;

  for (const file of schemas) {
    try {
      const doc = JSON.parse(readFileSync(file, "utf8"));
      schemaDocs.set(file, doc);
      checkKeywords(doc, file, []);
    } catch (e) {
      failures++;
      console.log(`FAIL  schema parse ${relative(root, file)}: ${e.message}`);
    }
  }

  for (const file of fixtures) {
    const base = file.split("\\").pop().split("/").pop();
    const match = [...schemaDocs.keys()].find((s) => {
      const sb = s.split("\\").pop().split("/").pop().replace(/\.schema\.json$/, "");
      return base.startsWith(sb + ".");
    });
    if (!match) {
      failures++;
      console.log(`FAIL  ${relative(root, file)}: no matching schema`);
      continue;
    }
    let doc;
    try {
      doc = JSON.parse(readFileSync(file, "utf8"));
    } catch (e) {
      failures++;
      console.log(`FAIL  fixture parse ${relative(root, file)}: ${e.message}`);
      continue;
    }
    const errors = [];
    try {
      const schemaDoc = schemaDocs.get(match);
      validate(doc, schemaDoc, base, errors, schemaDoc.$defs, schemaDocs, match.slice(0, match.lastIndexOf("\\") !== -1 ? match.lastIndexOf("\\") : match.lastIndexOf("/")));
    } catch (e) {
      failures++;
      console.log(`FAIL  ${relative(root, file)}: ${e.message}`);
      continue;
    }
    const expectValid = base.includes(".valid.");
    const expectInvalid = base.includes(".invalid.");
    if (expectValid && errors.length === 0) {
      console.log(`PASS  ${relative(root, file)} (valid)`);
    } else if (expectInvalid && errors.length > 0) {
      console.log(`PASS  ${relative(root, file)} (rejected: ${errors.slice(0, 3).join("; ")})`);
    } else if (expectValid && errors.length > 0) {
      failures++;
      console.log(`FAIL  ${relative(root, file)} expected valid:
      ${errors.slice(0, 5).join("\n      ")}`);
    } else if (expectInvalid && errors.length === 0) {
      failures++;
      console.log(`FAIL  ${relative(root, file)} expected invalid but validated`);
    } else {
      failures++;
      console.log(`FAIL  ${relative(root, file)}: cannot classify (name lacks .valid./.invalid.)`);
    }
  }

  console.log(`\n${schemas.length} schemas, ${fixtures.length} fixtures, ${failures === 0 ? "ALL PASS" : failures + " FAILURES"}`);
  process.exit(failures === 0 ? 0 : 1);
}

main();
