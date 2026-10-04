import { createServer } from "node:net";

/** Find a free TCP port (Vite dev server auto-port — §24). */
export async function findFreePort(preferred?: number | null, host = "127.0.0.1"): Promise<number> {
  if (preferred) {
    if (await isFree(preferred, host)) return preferred;
  }
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.on("error", reject);
    srv.listen(0, host, () => {
      const addr = srv.address();
      const port = typeof addr === "object" && addr ? addr.port : 0;
      srv.close(() => resolve(port));
    });
  });
}

function isFree(port: number, host: string): Promise<boolean> {
  return new Promise((resolve) => {
    const srv = createServer();
    srv.on("error", () => resolve(false));
    srv.listen(port, host, () => srv.close(() => resolve(true)));
  });
}
