# tokens

Generates `apps/apple/Vaultday/Tokens.swift` (SwiftUI colors, spacing, radius, sizes, type styles and shadows, light and dark) from the design system export at `design/tokens.json`. The JSON is validated with zod; invalid input (bad hex, missing or cyclic alias, Swift-hostile token name) fails with a message instead of writing a partial file.

```bash
pnpm --filter @vaultday/tokens generate   # rewrite Tokens.swift
pnpm --filter @vaultday/tokens check      # exit 1 if the committed Tokens.swift is stale
pnpm --filter @vaultday/tokens test
```

Never edit `Tokens.swift` by hand; change `design/tokens.json` and regenerate. `--input` and `--output` override the paths (relative to the repository root).
