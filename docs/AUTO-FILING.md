# Local auto-filing

Papiro analyses every document **on your own server**. No language model, no external service: just rules and the full-text index it already has. The code is in [`src/suggest.rs`](../src/suggest.rs).

## What it does

**Values found by rules** (they only fill empty fields):

| Value | How it's detected |
|---|---|
| Expiry date | The latest date right after words like "vigencia", "vence", "válido hasta", "expires", "valid until"… A bare year ("VIGENCIA 2029") means December 31 |
| Amount | The largest money-looking number (with `$` or decimals) after "total" (not "subtotal") |
| CURP and RFC | Mexican personal and tax IDs, by their format with the embedded date validated; only when the document contains exactly one |

**Suggestions by similarity** (folder, document type, correspondent and tags):

1. Take the document's most distinctive words: frequent in it, rare in the archive.
2. Use the full-text index to find the already-filed documents that share most of them.
3. Each neighbour "votes" for what it has, weighted by how similar it is. A neighbour that doesn't have that field yet doesn't vote against.
4. High confidence (≥ 0.70) with at least two neighbours agreeing: applied automatically and marked as such. Medium (≥ 0.45): shown as a suggestion to accept or dismiss. Tags are never applied automatically.

There's nothing to train: every document you file or correct counts for the next one. Other users' private documents are never used as neighbours.

## Measured accuracy

Synthetic archive of 56 documents (8 types, 9 correspondents), leave-one-out: remove the field, ask for an analysis, compare (`bench/tools/suggest_eval.py`, result in [`docs/bench/suggest-eval.json`](bench/suggest-eval.json)).

| | Applied automatically | Suggested | No proposal |
|---|---|---|---|
| Correspondent | 7, all 7 correct | 40, all 40 correct | 9 |
| Type | 5, all 5 correct | 14, 11 correct | 37 |

About 12 ms per analysis. Proposals with confidence ≥ 0.55 were all correct; between 0.35 and 0.45, 7 of 9 were wrong, which is why the suggestion threshold sits at 0.45.

## Limits

- **Those numbers come from a synthetic archive** with template text. Real scans with imperfect OCR will score lower; the thresholds live at the top of `src/suggest.rs`.
- **It needs examples.** With no similar documents filed yet it proposes nothing; a new folder starts being suggested once it holds two or three documents.
- **Types are harder than correspondents** when the text doesn't say what the document is.
- **The rules are literal.** An expiry phrased differently, a total without `$` or decimals, or European number format (`1.234,56`) won't be detected. Both fields can be edited by hand.
- It doesn't write titles or summaries and doesn't understand natural-language questions. For that there's the optional LLM integration (off by default; it sends text to the provider you configure).
