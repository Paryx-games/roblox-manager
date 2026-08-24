import fs from 'node:fs'
import path from 'node:path'

const rootDir = path.resolve(process.cwd(), '..')
const changelogPath = path.join(rootDir, 'CHANGELOG.md')
const outputPath = path.resolve(process.cwd(), 'src/generated/changelog.ts')

const changelog = fs.readFileSync(changelogPath, 'utf8')

const releases = []
const releasePattern = /^## v?(\d+\.\d+\.\d+)\s*$/gm
const matches = [...changelog.matchAll(releasePattern)]

for (let i = 0; i < matches.length; i++) {
  const match = matches[i]
  const version = match[1]
  const start = match.index + match[0].length
  const end = matches[i + 1]?.index ?? changelog.length
  const section = changelog.slice(start, end)

  const items = []
  const lines = section.split(/\r?\n/)
  let currentSection = null

  for (const line of lines) {
    const heading = line.match(/^### (.+)$/)
    if (heading) {
      currentSection = heading[1].trim().toLowerCase()
      continue
    }

    const item = line.match(/^- (?:\*\*([^*]+)\*\*\.?\s*)?(.+)$/)
    if (!item || !currentSection) continue

    if (currentSection !== 'added' && currentSection !== 'changed') {
      continue
    }

    const text = item[1]
      ? `${item[1]}. ${item[2]}`
      : item[2]

    items.push(text.replace(/\*\*/g, '').trim())
  }

  if (items.length > 0) {
    releases.push({
      version,
      items,
    })
  }
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true })

const output = `export const releases = ${JSON.stringify(releases, null, 2)} as const\n`

fs.writeFileSync(outputPath, output)