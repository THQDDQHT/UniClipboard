#!/usr/bin/env node
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const target = process.env.BUILD_TARGET
if (!target || !/^[a-z0-9_-]+$/.test(target)) throw new Error('Invalid BUILD_TARGET')
const windows = target.includes('windows')
const bundleDir = join('target', target, 'release', 'bundle', windows ? 'nsis' : 'dmg')
const installers = readdirSync(bundleDir)
  .filter(name => (windows ? name.endsWith('-setup.exe') : name.endsWith('.dmg')))
  .sort()
if (installers.length === 0) throw new Error(`No installers in ${bundleDir}`)
mkdirSync('dist-installers', { recursive: true })
const checksums = []
for (const name of installers) {
  const source = join(bundleDir, name)
  copyFileSync(source, join('dist-installers', name))
  checksums.push(`${createHash('sha256').update(readFileSync(source)).digest('hex')}  ${name}`)
}
writeFileSync('dist-installers/SHA256SUMS.txt', checksums.join('\n') + '\n')
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
writeFileSync(
  'dist-installers/SOURCE.txt',
  `base=v0.19.3\nrevision=${revision}\ntarget=${target}\nupdater=manual-only\n`
)
