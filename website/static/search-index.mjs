// Ferravex full-text retrieval; Qikbar provides fuzzy title matching and the UI.
import { fuzzyMatch, normalize } from './vendor/qikbar/utils.js';

export function prepareIndex(rows) {
  return rows.filter(row => typeof row.title === 'string' && /^\/(?!\/)/.test(row.url)).map(row => ({
    ...row,
    titleNorm: normalize(row.title),
    contextNorm: normalize(row.subtitle || ''),
    textNorm: normalize(`${row.title} ${row.subtitle || ''} ${row.text || ''}`),
  }));
}

export function searchIndex(rows, query, {section, limit = 8, titleOnly = false} = {}) {
  const term = normalize(query.trim()).slice(0, 160);
  if (!term) return [];
  const words = term.split(/\s+/);
  const scored = [];
  for (const row of rows) {
    if (section && row.section !== section) continue;
    const match = fuzzyMatch(term, row.title, row.titleNorm);
    const fuzzy = match && match.score >= 0.45 ? match : null;
    const fullText = words.every(word => row.textNorm.includes(word));
    // Subsequence matching is valuable for names, but noisy across long prose.
    if ((!fuzzy && !fullText) || (titleOnly && !fuzzy)) continue;
    let score = fuzzy ? 50 + fuzzy.score * 50 : 0;
    if (row.titleNorm === term) score += 100;
    else if (row.titleNorm.startsWith(term)) score += 35;
    else if (row.titleNorm.includes(term)) score += 25;
    score += words.reduce((sum, word) => sum + (row.titleNorm.includes(word) ? 12 : 0) + (row.contextNorm.includes(word) ? 3 : 0), 0);
    if (row.textNorm.includes(term)) score += 4;
    if (!row.url.includes('#')) score += 1;
    if (row.section === 'Examples' && fuzzy) score += 20;
    if (row.section === 'API reference' && !fuzzy) score -= 6;
    scored.push({row, score});
  }
  scored.sort((a, b) => b.score - a.score || a.row.title.localeCompare(b.row.title));
  return scored.slice(0, limit).map(({row}) => ({
    id: row.id, title: row.title, href: row.url, subtitle: snippet(row, words), icon: row.icon,
    accessories: [{text: row.section === 'API reference' ? 'Rust API' : row.section === 'Documentation' ? 'Docs' : row.section}],
  }));
}

function snippet(row, words) {
  // Context remains predictable for named matches; prose matches show the actual passage.
  if (words.some(word => row.titleNorm.includes(word))) return row.subtitle;
  const text = (row.text || '').replace(/\s+/g, ' ');
  const lower = normalize(text);
  const positions = words.map(word => lower.indexOf(word)).filter(index => index >= 0);
  const start = Math.max(0, (positions.length ? Math.min(...positions) : 0) - 36);
  const excerpt = text.slice(start, start + 110);
  return `${start ? '…' : ''}${excerpt}${start + 110 < text.length ? '…' : ''}` || row.subtitle;
}
