// Minimal static file server for browser acceptance (#152).
//
// Serves one directory (default: the built `site/`) exactly like a static host
// such as GitHub Pages: no backend routes, no directory listings, no files from
// outside the root. Usage: node tests/browser/serve-static.mjs [root] [port]
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";

const root = path.resolve(process.argv[2] ?? "site");
const port = Number(process.argv[3] ?? process.env.PORT ?? 4173);

const types = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".wasm": "application/wasm",
};

createServer(async (request, response) => {
  try {
    if (request.method !== "GET" && request.method !== "HEAD") {
      response.writeHead(405).end();
      return;
    }
    const { pathname } = new URL(request.url, "http://localhost");
    let file = path.join(root, path.normalize(decodeURIComponent(pathname)));
    if (file !== root && !file.startsWith(root + path.sep)) {
      response.writeHead(403).end();
      return;
    }
    let info = await stat(file).catch(() => null);
    if (info?.isDirectory()) {
      file = path.join(file, "index.html");
      info = await stat(file).catch(() => null);
    }
    if (!info?.isFile()) {
      response.writeHead(404, { "content-type": "text/plain" }).end("Not found");
      return;
    }
    response.writeHead(200, {
      "content-type": types[path.extname(file)] ?? "application/octet-stream",
      "content-length": info.size,
      "cache-control": "no-store",
    });
    if (request.method === "HEAD") {
      response.end();
      return;
    }
    createReadStream(file).pipe(response);
  } catch (error) {
    response.writeHead(500).end(String(error));
  }
}).listen(port, "127.0.0.1", () => {
  console.log(`serving ${root} on http://127.0.0.1:${port}`);
});
