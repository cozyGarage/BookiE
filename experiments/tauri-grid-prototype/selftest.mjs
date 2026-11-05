import { spawn } from 'node:child_process';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const binary = process.argv[2] ?? path.join(here, 'src-tauri/target/debug/bookie-tauri-grid-prototype');
const child = spawn('dbus-run-session', [
  '--', 'xvfb-run', '--auto-servernum', '-s', '-screen 0 1600x1200x24', binary,
], {
  env: { ...process.env, GDK_BACKEND: 'x11', BOOKIE_PROTOTYPE_SELFTEST: '1', BOOKIE_PROTOTYPE_AUTOSCROLL: '0' },
  detached: true,
  stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
let errors = '';
let finished = false;
let failure;
const deadline = setTimeout(() => finish(new Error('Timed out waiting for the self-test reports')), 30_000);

function finish(error) {
  if (finished) return;
  finished = true;
  failure = error;
  clearTimeout(deadline);
  try { process.kill(-child.pid, 'SIGTERM'); } catch { child.kill('SIGTERM'); }
}

child.stdout.on('data', (chunk) => {
  output += chunk;
  if (!output.includes('dialog_open=false')) return;
  try {
    assert.match(output, /selected_rows_drawn=3\b/);
    assert.match(output, /edit_invalid=true .*edit_applied=2026-02-03 editors_left=0\b/);
    assert.match(output, /cursor_at=3\/2 space_selected=1 .*page_after=200\/200 dialog_open=false/);
    finish();
  } catch (error) { finish(error); }
});
child.stderr.on('data', (chunk) => { errors = (errors + chunk).slice(-16_384); });
child.on('error', (error) => { finish(error); });
child.on('close', (code) => {
  clearTimeout(deadline);
  const reports = output.split('\n').filter((line) => line.startsWith('PROTOTYPE_REPORT'));
  console.log(reports.join('\n'));
  if (!finished || failure) {
    console.error(failure?.message ?? `Application exited before completing the self-test (${code})`);
    console.error(errors);
    process.exitCode = 1;
    return;
  }
  console.log('Tauri selection, edit and navigation self-test passed');
});
