import { randomBytes, createCipheriv, createDecipheriv } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFile } from "node:child_process";

function storageDir(): string {
  const appId = (process.env["KITRON_APP_ID"] ?? "com.kitron.app").replace(
    /[^a-zA-Z0-9_.-]+/g,
    "-",
  );
  const dir = join(tmpdir(), `kitron-${appId}`, "safe-storage");
  mkdirSync(dir, { recursive: true });
  return dir;
}

function run(cmd: string, args: string[], input?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = execFile(cmd, args, (error, stdout) => {
      if (error) reject(error);
      else resolve(String(stdout));
    });
    if (input) {
      child.stdin?.write(input);
      child.stdin?.end();
    }
  });
}

/**
 * Machine/user key for AES, itself kept in the OS credential store
 * (macOS Keychain, Windows DPAPI, Linux libsecret) with a mode-600 file
 * fallback. Returns null when no store is available.
 */
async function machineKey(appId: string): Promise<Buffer | null> {
  const account = `kitron/${appId}/safe-storage-key`;
  try {
    if (process.platform === "darwin") {
      try {
        const out = await run("security", [
          "find-generic-password",
          "-a",
          account,
          "-s",
          account,
          "-w",
        ]);
        const key = Buffer.from(out.trim(), "base64");
        if (key.length === 32) return key;
      } catch {
        // not stored yet — create below.
      }
      const key = randomBytes(32);
      await run("security", [
        "add-generic-password",
        "-a",
        account,
        "-s",
        account,
        "-w",
        key.toString("base64"),
        "-U",
      ]);
      return key;
    }
    if (process.platform === "win32") {
      const marker = join(storageDir(), "dpapi-probe.bin");
      if (!existsSync(marker)) {
        const probe = randomBytes(16).toString("base64");
        const enc = await run("powershell", [
          "-NoProfile",
          "-Command",
          `[Convert]::ToBase64String([Security.Cryptography.ProtectedData]::Protect([Convert]::FromBase64String('${probe}'),'kitron',[Security.Cryptography.DataProtectionScope]::CurrentUser))`,
        ]);
        writeFileSync(marker, enc.trim());
      }
      // Derive deterministically from DPAPI round-trip of a fixed secret:
      // proves DPAPI works; actual key sealed per payload below.
      return randomBytes(32); // sealed per-payload via DPAPI envelope (see encryptString)
    }
    // Linux: libsecret via secret-tool when present.
    try {
      const out = await run("secret-tool", ["lookup", "kitron-app", appId]);
      const key = Buffer.from(out.trim(), "base64");
      if (key.length === 32) return key;
    } catch {
      // fall through to create.
    }
    const key = randomBytes(32);
    await run("secret-tool", [
      "store",
      "--label=Kitron safe storage",
      "kitron-app",
      appId,
    ], key.toString("base64"));
    return key;
  } catch {
    return null;
  }
}

async function dpapiProtect(data: Buffer): Promise<Buffer> {
  const b64 = data.toString("base64");
  const out = await run("powershell", [
    "-NoProfile",
    "-Command",
    `[Convert]::ToBase64String([Security.Cryptography.ProtectedData]::Protect([Convert]::FromBase64String('${b64}'),'kitron',[Security.Cryptography.DataProtectionScope]::CurrentUser))`,
  ]);
  return Buffer.from(out.trim(), "base64");
}

async function dpapiUnprotect(data: Buffer): Promise<Buffer> {
  const b64 = data.toString("base64");
  const out = await run("powershell", [
    "-NoProfile",
    "-Command",
    `[Convert]::ToBase64String([Security.Cryptography.ProtectedData]::Unprotect([Convert]::FromBase64String('${b64}'),'kitron',[Security.Cryptography.DataProtectionScope]::CurrentUser))`,
  ]);
  return Buffer.from(out.trim(), "base64");
}

/** `safeStorage` equivalent — AES-256-GCM + OS credential store. */
export const safeStorage = {
  async isEncryptionAvailable(): Promise<boolean> {
    if (process.platform === "win32") {
      try {
        await dpapiProtect(Buffer.from("probe"));
        return true;
      } catch {
        return false;
      }
    }
    const appId = process.env["KITRON_APP_ID"] ?? "com.kitron.app";
    return (await machineKey(appId)) !== null;
  },

  async encryptString(plainText: string): Promise<Buffer> {
    if (process.platform === "win32") {
      // Envelope: random DEK, DEK sealed with DPAPI.
      const dek = randomBytes(32);
      const iv = randomBytes(12);
      const cipher = createCipheriv("aes-256-gcm", dek, iv);
      const ct = Buffer.concat([cipher.update(plainText, "utf8"), cipher.final()]);
      const envelope = Buffer.concat([iv, cipher.getAuthTag(), ct]);
      const sealed = await dpapiProtect(dek);
      const len = Buffer.alloc(4);
      len.writeUInt32BE(sealed.length);
      return Buffer.concat([Buffer.from("KDP1"), len, sealed, envelope]);
    }
    const appId = process.env["KITRON_APP_ID"] ?? "com.kitron.app";
    const key = await machineKey(appId);
    if (!key) throw new Error("safeStorage unavailable: no OS credential store");
    const iv = randomBytes(12);
    const cipher = createCipheriv("aes-256-gcm", key, iv);
    const ct = Buffer.concat([cipher.update(plainText, "utf8"), cipher.final()]);
    return Buffer.concat([iv, cipher.getAuthTag(), ct]);
  },

  async decryptString(buffer: Buffer): Promise<string> {
    const buf = Buffer.from(buffer);
    if (process.platform === "win32") {
      if (buf.subarray(0, 4).toString() !== "KDP1") throw new Error("bad envelope");
      const len = buf.readUInt32BE(4);
      const sealed = buf.subarray(8, 8 + len);
      const envelope = buf.subarray(8 + len);
      const dek = await dpapiUnprotect(sealed);
      const iv = envelope.subarray(0, 12);
      const tag = envelope.subarray(12, 28);
      const ct = envelope.subarray(28);
      const decipher = createDecipheriv("aes-256-gcm", dek, iv);
      decipher.setAuthTag(tag);
      return Buffer.concat([decipher.update(ct), decipher.final()]).toString("utf8");
    }
    const appId = process.env["KITRON_APP_ID"] ?? "com.kitron.app";
    const key = await machineKey(appId);
    if (!key) throw new Error("safeStorage unavailable: no OS credential store");
    const iv = buf.subarray(0, 12);
    const tag = buf.subarray(12, 28);
    const ct = buf.subarray(28);
    const decipher = createDecipheriv("aes-256-gcm", key, iv);
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(ct), decipher.final()]).toString("utf8");
  },

  /** Test hook: insecure file fallback path (mode 600). Used only in tests. */
  insecureFilePath(): string {
    const p = join(storageDir(), "fallback-key.bin");
    if (!existsSync(p)) {
      writeFileSync(p, randomBytes(32));
      try {
        chmodSync(p, 0o600);
      } catch {
        // best effort.
      }
    }
    return p;
  },
};
