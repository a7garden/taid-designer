---
name: taid-workbench
description: Use Taid's local CLI to inspect, import, prepare and deliver game assets with preserved originals and explicit review gates.
---

# Taid workbench

Use `taid --help` and subcommand `--help` for the installed CLI. This workflow uses the same processing, metadata and approval engine as the desktop. It does not generate artwork, build atlases or edit Unity `.meta` files.

Successful commands write one JSON object to stdout: `status`, `summary`, `data`, `artifacts`, `nextActions`. Failures write the same envelope shape to stderr and exit 1; stdout is empty. Help and version are plain text. Asset IDs come from `data.assets[].id`; image fields contain local paths, not embedded pixels.

## Read

1. Resolve the intended project. Do not guess among recent projects.
2. Read its notes, profiles, source names, hashes, frame metadata, reference flags, issues and approval state.
3. Establish which source files the user owns or is authorized to use and which images are reference-only.

```sh
taid project recent
taid project open "$PROJECT"
taid --project "$PROJECT" inspect
taid --project "$PROJECT" profile list
taid settings get
```

`project open` selects the project for the next desktop launch. Project commands do not require `--project`; asset commands do, so an agent cannot accidentally edit a previously selected project. Unavailable recent paths remain listed.

To create a project, use `taid project create --parent "$PARENT" --name "$NAME"`; use the returned `data.root` for later commands. To change Codex defaults, use `taid settings set --model "$MODEL_ID" --effort medium`. Either settings flag may be supplied alone; the other field is retained. Model settings are not a generation or review request.

The GUI offers direct import modes: inspect only, prepare to the saved profile (default), cutout then prepare, or prepare then AI review. PNG, drag-and-drop and Ctrl/Cmd+V share that pipeline. Local preparation never sends images to Codex; only the explicit AI mode or manual review does. The installed, logged-in Codex CLI is required for review; Taid uses that account's Codex allowance, not a bypass of ChatGPT chat quota. GUI model settings discover models/efforts through the CLI and apply saved choices to the next review. Model settings are not permission to send company data. The analysis connection is advisory and tool-disabled; it cannot generate images, artistically fix files, pack Unity atlases or edit `.meta` files.

## Plan

Choose explicit asset IDs, a profile and the smallest requested operations. `generic` preserves the imported canvas and does not impose Stage 03 rules. Apply Stage 03 profiles only to Stage 03 work. Preserve intentional semitransparency: manual `process --alpha` hardens it, and `prepare` also hardens it when the selected saved profile specifies binary alpha. Inspect that policy before a requested profile preparation.

A trailing `_NN` or `-NN` filename suffix with at least two digits supplies the sequence and numeric frame index. Other stems remain separate sequences at frame 0. Inspect the inferred values; duplicate frames are QC errors, not permission to invent timing, movement or alignment. Union trim requires the whole sequence and preserves relative offsets. Scale is an integer from 1 to 8.

Record provenance in project or asset notes: source/owner, reference or production use, relevant authorization, generation tool/model and prompt or run identifier when known. Distinguish unknown provenance from verified facts. Never invent license terms or accept a license on the user's behalf. Do not upload local art or attach images to a model without the user's explicit opt-in. Taid's existing Codex review is advisory; it is not artistic approval or a grant of source rights.

If a separate image-generation tool is used, follow that tool's policies and the user's authorized scope. Keep generated candidates as new files, preserve their provenance, import them for inspection and leave them unapproved. Reference images are not production inputs merely because they can be opened.

## Execute

```sh
taid --project "$PROJECT" import "$INPUT" --profile generic
taid --project "$PROJECT" asset update "$ASSET_ID" --file "$PATCH_JSON"
taid --project "$PROJECT" profile put --file "$PROFILE_JSON"
taid --project "$PROJECT" notes --text "$NOTES"
taid --project "$PROJECT" process --ids "$ASSET_IDS" --snap --trim --scale 2
```

Current-image operations create new unapproved variants and preserve the parent:

```sh
taid --project "$PROJECT" prepare --ids "$ASSET_IDS" --plan
taid --project "$PROJECT" prepare --ids "$ASSET_IDS"
taid --project "$PROJECT" cutout --ids "$ASSET_IDS"
taid --project "$PROJECT" palette inspect --asset "$ASSET_ID"
taid --project "$PROJECT" palette import --file "$PALETTE_FILE" --profile "$PROFILE_ID"
taid --project "$PROJECT" palette map --ids "$ASSET_IDS"
taid --project "$PROJECT" tile --ids "$ASSET_IDS" --width 32 --height 32 --scale 4 --mode pad --anchor bottom-center
```

`prepare --plan` reads actual current pixels and returns per-asset steps, warnings, output dimensions, changed count and blocking reasons without writing files. `prepare` executes needed grid padding/snap, profile-required alpha hardening, approved-palette mapping and full-canvas centered nearest fit. It never trims, cuts out, calls AI, changes approval or artistically removes orphan dots. Changed inputs create new unapproved variants; conforming files do not create duplicates. A batch with invalid rules, references, changed files or invalid output is rejected atomically. These operations can lose colors, semitransparency and small details; inspect the plan and before/after images.

New variants record `parentId` and `parentHash`. For an honest immediate-input comparison, use the parent's current image only if its hash matches, otherwise its preserved original only if that hash matches. An old asset without those fields or an unavailable matching parent is not evidence of the exact previous processing input.

Cutout explicitly downloads the pinned 4.4 MiB (4,574,861-byte) Apache-2.0 U2Netp weights on first use, then runs local CPU inference; images are not uploaded. It retains source RGB, canvas and existing transparency. A 320×320 salient-object model can miss fine lines, hair, effects and pixel edges. Inspect the mask; do not describe segmentation as pixel-art correction or approval. Model/license references: https://github.com/xuebinqin/U-2-Net and https://github.com/xuebinqin/U-2-Net/blob/master/LICENSE .

`palette inspect` reads the current image's visible RGB histogram without edits; fully transparent pixels are excluded, semitransparent pixels count by RGB. No saved palette means no palette-conformance verdict. `palette import` accepts `.hex` or GIMP `.gpl` and immediately replaces the named saved profile's palette (up to 256 distinct RGB colors), invalidating affected approvals. Obtain the actual game palette; never infer approval from a color count or adopt every source color as approved. `palette map` uses nearest RGB squared distance, preserves alpha and creates variants. It does not understand artistic color roles.

Tile width/height are logical dots, not physical PNG pixels: `32×32 --scale 4` produces 128×128px. Input must align to its saved profile's pixel size with uniform dot blocks; establish the correct grid and explicitly snap first if requested. `--scale` is 1–64; `--mode` is `pad` (no cropping, reject oversize) or `fit` (integer enlargement or nearest-dot reduction, which can lose detail); `--anchor` is `center` or `bottom-center`. Tile uses the whole current canvas, transforms the pivot and creates a dedicated profile retaining palette/alpha policy. It does not automatically recenter visible content. Frame timing, layer and reference status carry forward. Chain operations using the returned new asset IDs, not the unchanged parents.

Unlike these variant operations, `process` always starts from that asset's preserved original and uses scale 1–8. Never promise that it preserves the preceding processed output.

Replace the variables with paths and IDs from the inspected project. `--ids` takes a comma-separated list. Omit every `process` flag the user did not request; `--palette` requires a profile palette and `--alpha` explicitly hardens transparency.

`profile put` reads one complete camelCase profile object. `asset update` reads a camelCase patch containing only the requested fields: `name`, `profileId`, `sequence`, `frameIndex`, `durationMs`, `pivotX`, `pivotY`, `layer`, `notes`, `reference`. Use `{"reference":true}` for reference-only material. Original `sourceName` and hashes are retained by the engine and cannot be rewritten by the metadata patch. Pivot units are physical pixels from the top-left, x right and y down.

Use `taid --project "$PROJECT" reset --ids "$ASSET_IDS"` only when reverting the selected processing was requested. It restores originals and clears approvals.

## Inspect and hand off

After every mutation, inspect the returned workspace or run `inspect`. Check source provenance, dimensions, alpha, palette, grid, sequence completeness, pivots, crop offsets and issue severity. A successful command is not evidence that the art looks right. View the local images when visual review is part of the task.

Never grant design or QC approval unless the user directly requested that approval for those assets. A request to process, fix QC, export, or “make it work” does not by itself authorize artistic approval. Report blocked delivery instead of approving to bypass the export gate. When authorized and the reviews have actually occurred:

```sh
taid --project "$PROJECT" approve --ids "$ASSET_IDS" --design-reviewed --qc-reviewed
taid --project "$PROJECT" export --ids "$ASSET_IDS" --out "$DELIVERY_PARENT"
```

`--design-reviewed` is required; omit `--qc-reviewed` when only design approval was requested. Export requires both current approvals, no QC errors and non-reference assets. Changed metadata, notes, rules or pixels may invalidate prior review. The engine creates a new delivery folder atomically and returns its path; inspect its manifest, CSV and handoff notes. Atlas packing and Unity importer work remain downstream.

The desktop keeps its loaded snapshot until the user reloads it. If another desktop window or CLI changes the manifest, stale writes are rejected before processing or metadata changes. Do not retry blindly: inspect the new state, use the desktop reload control when applicable, then reapply only the still-intended edit. A busy-lock error means another writer holds the project; wait for that operation to finish, then inspect before trying again. Stop if the project, provenance or approval authorization remains unclear.
