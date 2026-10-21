#!/usr/bin/env node
/**
 * Kitron CLI entry (spec §37): `kitron <command> [args]`.
 *
 * Runs directly on Node 24+ (type stripping, no transpilation).
 */

import { main } from "./main.ts";

main(process.argv.slice(2)).catch((error: unknown) => {
  console.error(`kitron: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
