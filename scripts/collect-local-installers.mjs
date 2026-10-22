#!/usr/bin/env node
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'

const target = process.env.BUILD_TARGET
if (!target || !/^[a-z0-9_-]+$/.test(target)) throw new Error('Invalid BUILD_TARGET')
const windows = target.includes('windows')
const bundleDir = join('target', target, 'release', 'bundle', windows ? 'nsis' : 'dmg')
const installers = readdirSync(bundleDir)
  .filter(name => (windows ? name.endsWith('-setup.exe') : name.endsWith('.dmg')))
  .sort()
if (installers.length === 0) throw new Error(`No installers in ${bundleDir}`)
const artifacts = installers.map(name => join(bundleDir, name))
if (windows) {
  const portableDir = join('target', target, 'release', 'bundle', 'portable')
  const portable = readdirSync(portableDir)
    .filter(name => name.endsWith('-portable.zip'))
    .sort()
  if (portable.length === 0) throw new Error(`No portable archives in ${portableDir}`)
  artifacts.push(...portable.map(name => join(portableDir, name)))
}
mkdirSync('dist-installers', { recursive: true })
const checksums = []
for (const source of artifacts) {
  const name = basename(source)
  copyFileSync(source, join('dist-installers', name))
  checksums.push(`${createHash('sha256').update(readFileSync(source)).digest('hex')}  ${name}`)
}
writeFileSync('dist-installers/SHA256SUMS.txt', checksums.join('\n') + '\n')
const revision = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
writeFileSync(
  'dist-installers/SOURCE.txt',
  `base=v0.19.3\nrevision=${revision}\ntarget=${target}\nupdater=manual-only\n`
)
