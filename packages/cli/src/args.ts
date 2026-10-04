export interface ParsedArgs {
  command: string | null;
  positional: string[];
  flags: Record<string, string | boolean>;
}

export function parseArgs(argv: string[]): ParsedArgs {
  const [command, ...rest] = argv;
  const positional: string[] = [];
  const flags: Record<string, string | boolean> = {};
  let i = 0;
  while (i < rest.length) {
    const tok = rest[i]!;
    if (tok.startsWith("--")) {
      const eq = tok.indexOf("=");
      if (eq >= 0) {
        flags[tok.slice(2, eq)] = tok.slice(eq + 1);
      } else if (i + 1 < rest.length && !rest[i + 1]!.startsWith("-")) {
        flags[tok.slice(2)] = rest[i + 1]!;
        i += 1;
      } else {
        flags[tok.slice(2)] = true;
      }
    } else if (tok.startsWith("-") && tok.length === 2) {
      const name = tok.slice(1);
      if (i + 1 < rest.length && !rest[i + 1]!.startsWith("-")) {
        flags[name] = rest[i + 1]!;
        i += 1;
      } else {
        flags[name] = true;
      }
    } else {
      positional.push(tok);
    }
    i += 1;
  }
  return { command: command ?? null, positional, flags };
}

export function flag<T extends string | boolean>(
  flags: Record<string, string | boolean>,
  ...names: string[]
): string | boolean | undefined {
  for (const n of names) {
    if (n in flags) return flags[n] as T;
  }
  return undefined;
}
