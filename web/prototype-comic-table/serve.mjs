import http from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname);
const PORT = Number(process.env.PORT) || 4173;
const HOST = '0.0.0.0';

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.webp': 'image/webp',
};

function getLanIPv4Addresses() {
  const nets = os.networkInterfaces();
  const addresses = [];
  for (const entries of Object.values(nets)) {
    if (!entries) continue;
    for (const info of entries) {
      if (info.family === 'IPv4' && !info.internal) {
        addresses.push(info.address);
      }
    }
  }
  return addresses;
}

const server = http.createServer(async (req, res) => {
  try {
    const reqUrl = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);
    let relPath = decodeURIComponent(reqUrl.pathname);
    if (relPath === '/') relPath = '/index.html';

    const targetPath = path.resolve(ROOT_DIR, `.${relPath}`);
    if (targetPath !== ROOT_DIR && !targetPath.startsWith(`${ROOT_DIR}${path.sep}`)) {
      res.writeHead(403, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end('Forbidden');
      return;
    }

    const stat = await fs.stat(targetPath).catch(() => null);
    if (!stat || !stat.isFile()) {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end('Not Found');
      return;
    }

    const ext = path.extname(targetPath).toLowerCase();
    const contentType = MIME_TYPES[ext] || 'application/octet-stream';
    const data = await fs.readFile(targetPath);

    res.writeHead(200, {
      'Content-Type': contentType,
      'Cache-Control': 'no-store',
    });
    res.end(data);
  } catch (err) {
    res.writeHead(500, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end(`Server Error: ${err instanceof Error ? err.message : String(err)}`);
  }
});

server.listen(PORT, HOST, () => {
  const lanIps = getLanIPv4Addresses();
  console.log('\n[PROTOTYPE] 2D 漫画风推倒胡牌桌原型已启动');
  console.log(`  Mac 本地浏览:`);
  console.log(`    Variant A (皮城舞台):     http://localhost:${PORT}/?variant=A`);
  console.log(`    Variant B (祖安漫画分镜): http://localhost:${PORT}/?variant=B`);
  console.log(`    Variant C (雀馆暖桌):     http://localhost:${PORT}/?variant=C`);
  if (lanIps.length > 0) {
    console.log(`  同局域网 iPhone / 另一台 Mac 访问:`);
    for (const ip of lanIps) {
      console.log(`    http://${ip}:${PORT}/?variant=A`);
    }
  }
  console.log('\n  按 Ctrl+C 停止原型服务器\n');
});
