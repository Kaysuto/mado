import { expect, test } from 'vitest'
import { bullets, markdown, sections } from './markdown'

test('html is escaped, so nothing a model repeats can inject markup', () => {
  expect(markdown('<script>alert(1)</script>')).toBe('<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>')
  expect(markdown('```\n<b>x</b>\n```')).toBe('<pre><code>&lt;b&gt;x&lt;/b&gt;</code></pre>')
})

test('basic formatting', () => {
  expect(markdown('**bold** and `code`')).toBe('<p><strong>bold</strong> and <code>code</code></p>')
  expect(markdown('- a\n- b')).toBe('<ul><li>a</li><li>b</li></ul>')
  expect(markdown('1. a\n2. b')).toBe('<ol><li>a</li><li>b</li></ol>')
  expect(markdown('## Title')).toBe('<h4>Title</h4>')
})

test('only https links become links, and quotes cannot break out of them', () => {
  expect(markdown('[site](https://ex.com/a)')).toBe('<p><a href="https://ex.com/a">site</a></p>')
  expect(markdown('[x](javascript:alert(1))')).toBe('<p>[x](javascript:alert(1))</p>')
  expect(markdown('[x](https://a.com/"onmouseover=1)')).not.toMatch(/"\s*onmouseover/)
})

test('a text is split on its headings, and a list gives its items', () => {
  const text = 'Intro\n## Today\nBusy day.\n\n## To handle\n- Reply to **Tomás**\n2. Review #4\n\n## Tip\nStart early.\n'
  const parts = sections(text)
  expect(parts.map(p => p.heading)).toEqual(['Today', 'To handle', 'Tip'])
  expect(parts[0].body.trim()).toBe('Busy day.')
  expect(bullets(parts[1].body)).toEqual(['Reply to **Tomás**', 'Review #4'])
  expect(bullets(parts[2].body)).toEqual([])
})
