// SPDX-License-Identifier: Apache-2.0 OR MIT

/**
 * TPT Bastion — TypeScript bindings.
 *
 * Author Bastion infrastructure in TypeScript. A {@link BastionDocument} mirrors
 * the stable Bastion IR; {@link toBcl} renders it to `.bcl` source and the
 * `compile` / `plan` / `apply` helpers drive the `bastion` CLI against that
 * source. This keeps the bindings targeting the *same* artifact (IR) as the DSL
 * and the Rust engine, so there is a single source of truth.
 *
 * Run with native TypeScript support (Node >= 22.6):
 *
 * ```sh
 * node --experimental-strip-types examples/aws_s3.ts
 * ```
 *
 * @module
 */

import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { IR_VERSION } from "./ir-version.js";

/** A compile-time IR value, mirroring `tpt_bastion_ir::Value`. */
export type BastionValue =
  | { kind: "string"; value: string }
  | { kind: "number"; value: number }
  | { kind: "bool"; value: boolean }
  | { kind: "null" }
  | { kind: "reference"; base: string; path: string[] }
  | { kind: "list"; value: BastionValue[] }
  | { kind: "map"; value: Record<string, BastionValue> };

export const str = (value: string): BastionValue => ({ kind: "string", value });
export const num = (value: number): BastionValue => ({ kind: "number", value });
export const bool = (value: boolean): BastionValue => ({ kind: "bool", value });
export const nil = (): BastionValue => ({ kind: "null" });
export const ref = (base: string, ...path: string[]): BastionValue => ({
  kind: "reference",
  base,
  path,
});
export const list = (...value: BastionValue[]): BastionValue => ({ kind: "list", value });
export const map = (value: Record<string, BastionValue>): BastionValue => ({
  kind: "map",
  value,
});

/** Top-level config block. */
export interface BastionConfig {
  name?: string;
  region?: string;
  extra?: Record<string, BastionValue>;
}

/** A declared input variable. */
export interface BastionVariable {
  name: string;
  typeName?: string;
  default?: BastionValue;
}

/** A managed resource. */
export interface BastionResource {
  typeName: string;
  name: string;
  attributes: Record<string, BastionValue>;
}

/** A module instantiation. */
export interface BastionModule {
  name: string;
  source?: string;
  attributes: Record<string, BastionValue>;
}

/** A named output. */
export interface BastionOutput {
  name: string;
  value: BastionValue;
}

/** A full Bastion document, mirroring `tpt_bastion_ir::Document`. */
export interface BastionDocument {
  config?: BastionConfig;
  variables?: BastionVariable[];
  resources?: BastionResource[];
  modules?: BastionModule[];
  outputs?: BastionOutput[];
}

function renderValue(v: BastionValue): string {
  switch (v.kind) {
    case "string": {
      const escaped = v.value
        .replace(/\\/g, "\\\\")
        .replace(/"/g, '\\"')
        .replace(/\n/g, "\\n")
        .replace(/\t/g, "\\t")
        .replace(/\r/g, "\\r");
      return `"${escaped}"`;
    }
    case "number":
      return String(v.value);
    case "bool":
      return v.value ? "true" : "false";
    case "null":
      return "null";
    case "reference":
      return [v.base, ...v.path].join(".");
    case "list":
      return `[ ${v.value.map(renderValue).join(", ")} ]`;
    case "map": {
      const entries = Object.entries(v.value).map(
        ([k, val]) => `${k} = ${renderValue(val)}`
      );
      return `{ ${entries.join(", ")} }`;
    }
  }
}

function renderBody(attrs: Record<string, BastionValue>): string {
  const lines: string[] = [];
  for (const [key, value] of Object.entries(attrs)) {
    if (value.kind === "map") {
      lines.push(`    ${key} {`);
      for (const [k, v] of Object.entries(value.value)) {
        lines.push(`        ${k} = ${renderValue(v)}`);
      }
      lines.push(`    }`);
    } else {
      lines.push(`    ${key} = ${renderValue(value)}`);
    }
  }
  return lines.join("\n");
}

/** Render a {@link BastionDocument} to `.bcl` source text. */
export function toBcl(doc: BastionDocument): string {
  const blocks: string[] = [];

  if (doc.config) {
    const body: Record<string, BastionValue> = {};
    if (doc.config.name) body.name = str(doc.config.name);
    if (doc.config.region) body.region = str(doc.config.region);
    if (doc.config.extra) Object.assign(body, doc.config.extra);
    blocks.push(`config {\n${renderBody(body)}\n}`);
  }

  for (const v of doc.variables ?? []) {
    const body: Record<string, BastionValue> = {};
    if (v.typeName) body.type = str(v.typeName);
    if (v.default) body.default = v.default;
    blocks.push(`variable ${v.name} {\n${renderBody(body)}\n}`);
  }

  for (const r of doc.resources ?? []) {
    blocks.push(
      `resource ${r.typeName} ${r.name} {\n${renderBody(r.attributes)}\n}`
    );
  }

  for (const m of doc.modules ?? []) {
    const body: Record<string, BastionValue> = {};
    if (m.source) body.source = str(m.source);
    Object.assign(body, m.attributes);
    blocks.push(`module ${m.name} {\n${renderBody(body)}\n}`);
  }

  for (const o of doc.outputs ?? []) {
    blocks.push(`output ${o.name} {\n    value = ${renderValue(o.value)}\n}`);
  }

  return blocks.join("\n\n") + "\n";
}

/** Serialize a document to IR JSON (the portable artifact). */
export function toIrJson(doc: BastionDocument): string {
  return JSON.stringify(toIr(doc), null, 2);
}

/** Convert a {@link BastionDocument} into the IR JSON shape. */
export function toIr(doc: BastionDocument): unknown {
  return {
    ir_version: IR_VERSION,
    config: doc.config
      ? {
          name: doc.config.name ?? null,
          region: doc.config.region ?? null,
          extra: doc.config.extra ?? {},
        }
      : null,
    variables: (doc.variables ?? []).map((v) => ({
      name: v.name,
      type_name: v.typeName ?? null,
      default: v.default ?? null,
    })),
    resources: (doc.resources ?? []).map((r) => ({
      id: `${r.typeName}.${r.name}`,
      type_name: r.typeName,
      name: r.name,
      attributes: r.attributes,
    })),
    modules: (doc.modules ?? []).map((m) => ({
      name: m.name,
      source: m.source ?? null,
      attributes: m.attributes,
    })),
    outputs: (doc.outputs ?? []).map((o) => ({ name: o.name, value: o.value })),
  };
}

export interface RunOptions {
  /** Working directory for the CLI invocation. */
  cwd?: string;
  /** Path/name of the `bastion` binary (default: `bastion`). */
  bastionBin?: string;
  /** State file path. */
  state?: string;
  /** Env var holding the state encryption passphrase. */
  passphraseEnv?: string;
  /** Apply without prompting. */
  autoApprove?: boolean;
  /** Keep the generated `.bcl` temp file (returns its path). */
  keepFile?: boolean;
}

export interface RunResult {
  stdout: string;
  stderr: string;
  status: number;
  /** Path to the generated `.bcl` file (when `keepFile` is set). */
  file?: string;
}

async function runBastion(args: string[], opts: RunOptions = {}): Promise<RunResult> {
  const bin = opts.bastionBin ?? "bastion";
  return new Promise((resolve, reject) => {
    const child = spawn(bin, args, { cwd: opts.cwd });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d.toString()));
    child.stderr.on("data", (d) => (stderr += d.toString()));
    child.on("error", reject);
    child.on("close", (status) =>
      resolve({ stdout, stderr, status: status ?? 0 })
    );
  });
}

async function withTempBcl(
  doc: BastionDocument,
  fn: (file: string) => Promise<RunResult>,
  opts: RunOptions
): Promise<RunResult> {
  const dir = mkdtempSync(join(tmpdir(), "bastion-"));
  const file = join(dir, "doc.bcl");
  writeFileSync(file, toBcl(doc));
  try {
    const result = await fn(file);
    if (opts.keepFile) result.file = file;
    return result;
  } finally {
    if (!opts.keepFile) rmSync(dir, { recursive: true, force: true });
  }
}

function stateArgs(opts: RunOptions): string[] {
  const a: string[] = [];
  if (opts.state) a.push("--state", opts.state);
  if (opts.passphraseEnv) a.push("--passphrase-env", opts.passphraseEnv);
  return a;
}

/** Compile a document to IR JSON via the CLI. */
export async function compile(
  doc: BastionDocument,
  opts: RunOptions = {}
): Promise<RunResult> {
  return withTempBcl(doc, (file) => runBastion(["compile", file], opts), opts);
}

/** Build an execution plan (dry-run). */
export async function plan(
  doc: BastionDocument,
  opts: RunOptions = {}
): Promise<RunResult> {
  return withTempBcl(
    doc,
    (file) => runBastion(["plan", file, ...stateArgs(opts)], opts),
    opts
  );
}

/** Apply a document, creating/updating infrastructure. */
export async function apply(
  doc: BastionDocument,
  opts: RunOptions = {}
): Promise<RunResult> {
  const auto = opts.autoApprove ? ["--auto-approve"] : [];
  return withTempBcl(
    doc,
    (file) => runBastion(["apply", file, ...auto, ...stateArgs(opts)], opts),
    opts
  );
}

export default {
  toBcl,
  toIr,
  toIrJson,
  compile,
  plan,
  apply,
  str,
  num,
  bool,
  nil,
  ref,
  list,
  map,
};
