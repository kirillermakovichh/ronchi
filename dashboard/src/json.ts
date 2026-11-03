// JSON integer facts can exceed JavaScript's exact number range. Preserve their
// decimal representation for display. This module does not evaluate facts.
export function parseEvidence(text: string): unknown {
  let output = "",
    start = 0,
    index = 0;
  while (index < text.length) {
    if (text[index] === '"') {
      index++;
      while (index < text.length) {
        if (text[index] === "\\") {
          index += 2;
          continue;
        }
        if (text[index++] === '"') break;
      }
      continue;
    }
    if (text[index] === "-" || /[0-9]/.test(text[index])) {
      const match = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;
      match.lastIndex = index;
      const token = match.exec(text)?.[0];
      if (token) {
        if (
          /^-?\d+$/.test(token) &&
          (BigInt(token) > BigInt(Number.MAX_SAFE_INTEGER) ||
            BigInt(token) < BigInt(Number.MIN_SAFE_INTEGER))
        ) {
          output += text.slice(start, index) + JSON.stringify(token);
          start = index + token.length;
        }
        index += token.length;
        continue;
      }
    }
    index++;
  }
  return JSON.parse(output + text.slice(start));
}
