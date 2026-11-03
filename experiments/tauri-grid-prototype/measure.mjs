import { spawn } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const options = Object.fromEntries(
  process.argv.slice(2).map((argument) => {
    const [key, value = 'true'] = argument.replace(/^--/, '').split('=');
    return [key, value];
  }),
);
const rows = Number(options.rows ?? 150000);
const steps = Number(options.steps ?? 120);
const runs = Number(options.runs ?? 3);
const width = Number(options.width ?? 1100);
const height = Number(options.height ?? 800);
const binary = options.binary ?? path.join(here, 'src-tauri/target/release/bookie-tauri-grid-prototype');
const timeoutMs = Number(options.timeout ?? 240000);
const classes = { app: /^bookie-tauri-gr/, webview: /^WebKitWebProces/, network: /^WebKitNetworkPr/, xvfb: /^Xvfb/ };

function read(file) {
  try {
    return readFileSync(file, 'utf8');
  } catch {
    return '';
  }
}

function processTable() {
  return readdirSync('/proc')
    .filter((name) => /^\d+$/.test(name))
    .map((pid) => {
      const stat = read(`/proc/${pid}/stat`);
      const close = stat.lastIndexOf(')');
      if (close < 0) return null;
      return { pid: Number(pid), parent: Number(stat.slice(close + 2).split(' ')[1]), name: read(`/proc/${pid}/comm`).trim() };
    })
    .filter(Boolean);
}

function descendants(root) {
  const table = processTable();
  const found = new Set([root]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const entry of table) {
      if (!found.has(entry.pid) && found.has(entry.parent)) {
        found.add(entry.pid);
        grew = true;
      }
    }
  }
  return table.filter((entry) => found.has(entry.pid));
}

function kib(text, key) {
  const match = new RegExp(`^${key}:\\s+(\\d+)`, 'm').exec(text);
  return match ? Number(match[1]) : 0;
}

function sample(root) {
  const totals = Object.fromEntries(Object.keys(classes).map((name) => [name, { rss_kib: 0, pss_kib: 0 }]));
  for (const entry of descendants(root)) {
    const kind = Object.keys(classes).find((name) => classes[name].test(entry.name));
    if (!kind) continue;
    totals[kind].rss_kib += kib(read(`/proc/${entry.pid}/status`), 'VmRSS');
    totals[kind].pss_kib += kib(read(`/proc/${entry.pid}/smaps_rollup`), 'Pss');
  }
  return totals;
}

function peak(into, current) {
  for (const [kind, value] of Object.entries(current)) {
    into[kind].rss_kib = Math.max(into[kind].rss_kib, value.rss_kib);
    into[kind].pss_kib = Math.max(into[kind].pss_kib, value.pss_kib);
  }
}

function runOnce(run) {
  return new Promise((resolve) => {
    const env = {
      ...process.env,
      BOOKIE_PROTOTYPE_ROWS: String(rows),
      BOOKIE_PROTOTYPE_AUTOSCROLL: String(steps),
      BOOKIE_PROTOTYPE_WIDTH: String(width),
      BOOKIE_PROTOTYPE_HEIGHT: String(height),
    };
    const hasDisplay = Boolean(process.env.DISPLAY || process.env.WAYLAND_DISPLAY);
    if (!hasDisplay) {
      Object.assign(env, {
        WEBKIT_DISABLE_COMPOSITING_MODE: '1',
        WEBKIT_DISABLE_DMABUF_RENDERER: '1',
        LIBGL_ALWAYS_SOFTWARE: '1',
        GDK_BACKEND: 'x11',
      });
    }
    const launcher = hasDisplay ? [binary] : ['xvfb-run', '-a', '-s', '-screen 0 1600x1200x24', binary];
    const child = spawn(launcher[0], launcher.slice(1), { env, detached: true, stdio: ['ignore', 'pipe', 'inherit'] });
    const peaks = Object.fromEntries(Object.keys(classes).map((name) => [name, { rss_kib: 0, pss_kib: 0 }]));
    let before = null;
    let done = null;
    let finished = false;
    const timer = setInterval(() => {
      const current = sample(child.pid);
      peak(peaks, current);
      if (before === null && current.webview.rss_kib > 0) before = current;
      if (done) finish(current);
    }, 400);
    const deadline = setTimeout(() => finish(sample(child.pid), 'timeout'), timeoutMs);
    child.stdout.on('data', (chunk) => {
      const match = /PROTOTYPE_SCROLL_DONE steps=(\d+) elapsed_ms=(\d+)/.exec(String(chunk));
      if (match) done = { steps: Number(match[1]), elapsed_ms: Number(match[2]) };
    });
    function finish(after, failure) {
      if (finished) return;
      finished = true;
      clearInterval(timer);
      clearTimeout(deadline);
      try {
        process.kill(-child.pid, 'SIGTERM');
      } catch {
        child.kill('SIGTERM');
      }
      resolve({ run, rows, steps, width, height, failure: failure ?? null, scroll: done, before_scroll: before, after_scroll: after, peak: peaks });
    }
    child.on('exit', () => finish(sample(child.pid), done ? null : 'exited'));
  });
}

for (let run = 1; run <= runs; run += 1) {
  const result = await runOnce(run);
  process.stdout.write(`${JSON.stringify(result)}\n`);
}
