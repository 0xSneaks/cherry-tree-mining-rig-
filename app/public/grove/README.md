# Grove art

Cosmetic miner stages for THE CHERRY TREE.

| file | stage | seeds |
|---|---|---|
| `stage-0-seed.svg` | Bare soil | 0 |
| `stage-1-sprout.svg` | Sapling | 1+ |
| `stage-2-young.svg` | Young tree | 100+ |
| `stage-3-bloom.svg` | Cherry tree | 1K+ |
| `stage-4-producing.svg` | Producing | 10K+ |

Served from `/grove/...` (this folder is `app/public/grove`).
Swap any file in place — `RigVisual` reads these paths.

The SVGs are the production assets. Photoreal stills can replace them later with the same filenames (`.svg` → `.png` requires a one-line path change in `RigVisual.tsx`).
