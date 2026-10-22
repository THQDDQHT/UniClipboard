#!/usr/bin/env node
// Build matching daemon and GUI targets without upstream signing credentials.
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('../', import.meta.url))
const args = process.argv.slice(2)
const daemonArgs = []
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--target') {
    if (!args[i + 1]) throw new Error('--target requires a target triple')
    daemonArgs.push('--target', args[++i])
  } else if (args[i].startsWith('--target=')) {
    daemonArgs.push(args[i])
  }
}
execFileSync(process.execPath, ['scripts/prepare-daemon-sidecar.mjs', ...daemonArgs], {
  cwd: repoRoot,
  stdio: 'inherit',
})
execFileSync(
  process.platform === 'win32' ? 'bun.exe' : 'bun',
  [
    'run',
    'tauri',
    'build',
    '--config',
    'src-tauri/tauri.local.conf.json',
    '--features',
    'manual-updates',
    ...args,
    '--',
    '--locked',
  ],
  { cwd: repoRoot, stdio: 'inherit' }
)
