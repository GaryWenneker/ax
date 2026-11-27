export interface CssRule {
  /** Selectors of the rule, split on top-level commas. */
  selectors: string[];
  /** Declarations as lower-case property → value, last one wins. */
  declarations: Map<string, string>;
}

function splitTopLevel(text: string, separator: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === '(' || ch === '[') depth += 1;
    else if (ch === ')' || ch === ']') depth -= 1;
    else if (ch === separator && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((part) => part.trim()).filter(Boolean);
}

/**
 * Style rules of a stylesheet in source order, including rules nested in @media and other
 * at-rule blocks. Each rule keeps its own declarations, so a check reads one block at a time.
 */
export function cssRules(css: string): CssRule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, '');
  const rules: CssRule[] = [];
  const walk = (from: number, to: number) => {
    let i = from;
    while (i < to) {
      const open = text.indexOf('{', i);
      if (open < 0 || open >= to) return;
      const prelude = text.slice(i, open).replace(/^[\s\S]*;/, '').trim();
      let depth = 1;
      let close = open + 1;
      while (close < to && depth > 0) {
        if (text[close] === '{') depth += 1;
        else if (text[close] === '}') depth -= 1;
        close += 1;
      }
      const body = text.slice(open + 1, close - 1);
      if (prelude.startsWith('@')) {
        if (body.includes('{')) walk(open + 1, close - 1);
      } else {
        const declarations = new Map<string, string>();
        for (const decl of splitTopLevel(body, ';')) {
          const colon = decl.indexOf(':');
          if (colon > 0) declarations.set(decl.slice(0, colon).trim().toLowerCase(), decl.slice(colon + 1).trim());
        }
        rules.push({ selectors: splitTopLevel(prelude, ','), declarations });
      }
      i = close;
    }
  };
  walk(0, text.length);
  return rules;
}

const SELECTED_NODE = '.page-item--selected.page-item--graph .page-item-node::';

/**
 * True when the portal layout switches off the selected-row orbit glow on both pseudo-elements:
 * at least one portal rule sets `animation: none` on each, and no portal rule for them sets any
 * other animation.
 */
export function portalOrbitGlowOff(css: string): boolean {
  const rules = cssRules(css);
  return ['before', 'after'].every((pseudo) => {
    const animated = rules.filter((rule) =>
      rule.selectors.some((sel) => sel.includes('html[data-ax-layout="portal"]') && sel.endsWith(`${SELECTED_NODE}${pseudo}`))
      && (rule.declarations.has('animation') || rule.declarations.has('animation-name')));
    return animated.length > 0 && animated.every((rule) =>
      (rule.declarations.get('animation') ?? rule.declarations.get('animation-name')) === 'none');
  });
}
