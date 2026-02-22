import express from 'express';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const app = express();

// The server is started from the `site-checker/` folder (package.json lives here),
// but the data directory is at the repository root: `../data/position`.
//
// Allow overriding locations via env vars so the server keeps working even if
// the folder layout changes again.
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Static files (index.html, app.js, styles.css) live next to this server file.
const STATIC_ROOT = process.env.STATIC_ROOT
  ? path.resolve(process.env.STATIC_ROOT)
  : __dirname;

// Data lives at repo root by default.
const DATA_ROOT = process.env.DATA_ROOT
  ? path.resolve(process.env.DATA_ROOT)
  : path.resolve(__dirname, '..');

const DATA_DIR = path.join(DATA_ROOT, 'data', 'position');

app.use(express.json({ limit: '1mb' }));
app.use(express.static(STATIC_ROOT));

function sessionToFile(session) {
  const ss = String(session).padStart(2, '0');
  return path.join(DATA_DIR, `${ss}.json`);
}

app.get('/api/position/:session', async (req, res) => {
  const session = Number.parseInt(req.params.session, 10);
  if (!Number.isFinite(session) || session < 1 || session > 99) {
    res.status(400).json({ error: 'session must be 1..99' });
    return;
  }

  const filePath = sessionToFile(session);

  try {
    const content = await fs.readFile(filePath, 'utf8');
    const json = JSON.parse(content);
    res.json(json);
  } catch (e) {
    if (e && (/** @type {any} */ (e)).code === 'ENOENT') {
      // No file yet = empty list
      res.json([]);
      return;
    }
    res.status(500).json({ error: 'failed to read session file' });
  }
});

app.post('/api/position/:session', async (req, res) => {
  const session = Number.parseInt(req.params.session, 10);
  if (!Number.isFinite(session) || session < 1 || session > 99) {
    res.status(400).json({ error: 'session must be 1..99' });
    return;
  }

  const body = req.body;
  if (!Array.isArray(body)) {
    res.status(400).json({ error: 'body must be an array of points' });
    return;
  }

  const filePath = sessionToFile(session);

  try {
    await fs.mkdir(DATA_DIR, { recursive: true });
    await fs.writeFile(filePath, JSON.stringify(body, null, 2) + '\n', 'utf8');
    res.json({ ok: true });
  } catch {
    res.status(500).json({ error: 'failed to write session file' });
  }
});

const port = Number(process.env.PORT || 5173);
app.listen(port, () => {
  console.log(`Server running on http://localhost:${port}`);
});
