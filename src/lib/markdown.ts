// Minimal Markdown to HTML for what a model writes. Everything is HTML-escaped first, so text
// coming from mails, web pages or the clipboard can never inject markup or scripts.

const escapeHtml = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

function inline(text: string): string {
  return text
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\((https:\/\/[^)\s]+)\)/g, '<a href="$2">$1</a>')
}

export function markdown(source: string): string {
  const lines = escapeHtml(source.replace(/\r\n/g, '\n')).split('\n')
  const out: string[] = []
  let list: 'ul' | 'ol' | null = null
  let code: string[] | null = null

  const closeList = () => {
    if (list) out.push(`</${list}>`)
    list = null
  }

  for (const line of lines) {
    if (line.startsWith('```')) {
      if (code === null) {
        closeList()
        code = []
      } else {
        out.push(`<pre><code>${code.join('\n')}</code></pre>`)
        code = null
      }
      continue
    }
    if (code !== null) {
      code.push(line)
      continue
    }
    const heading = line.match(/^(#{1,4})\s+(.*)$/)
    const bullet = line.match(/^\s*[-*•]\s+(.*)$/)
    const numbered = line.match(/^\s*\d+[.)]\s+(.*)$/)
    const item = bullet ?? numbered
    if (heading) {
      closeList()
      out.push(`<h4>${inline(heading[2])}</h4>`)
    } else if (item) {
      const tag = bullet ? 'ul' : 'ol'
      if (list !== tag) {
        closeList()
        out.push(`<${tag}>`)
        list = tag
      }
      out.push(`<li>${inline(item[1])}</li>`)
    } else {
      closeList()
      if (line.trim()) out.push(`<p>${inline(line)}</p>`)
    }
  }
  if (code !== null) out.push(`<pre><code>${code.join('\n')}</code></pre>`)
  closeList()
  return out.join('')
}

export interface Section {
  heading: string
  body: string
}

/** Splits a text on its `## headings`. What comes before the first heading is dropped. */
export function sections(source: string): Section[] {
  const found: Section[] = []
  for (const line of source.replace(/\r\n/g, '\n').split('\n')) {
    const heading = line.match(/^#{1,4}\s+(.*)$/)
    if (heading) found.push({ heading: heading[1].trim(), body: '' })
    else if (found.length > 0) found[found.length - 1].body += `${line}\n`
  }
  return found
}

/** The items of the bullet or numbered list in a text, as written. */
export function bullets(body: string): string[] {
  return body
    .split('\n')
    .map(line => line.match(/^\s*(?:[-*•]|\d+[.)])\s+(.*)$/)?.[1].trim())
    .filter((item): item is string => Boolean(item))
}
