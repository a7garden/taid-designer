#!/bin/bash
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then echo 'This installer requires macOS.' >&2; exit 1; fi
root="$(cd "$(dirname "$0")/.." && pwd)"
source_app="$root/src-tauri/target/release/bundle/macos/Taid.app"
destination="$HOME/Applications/Taid.app"
cli_link="$HOME/.local/bin/taid"
skill_link="$HOME/.agents/skills/taid-workbench"
skill_source="$destination/Contents/Resources/skills/taid-workbench"
legacy_skill_source="$root/skills/taid-workbench"
if [[ ! -x "$source_app/Contents/MacOS/taid-designer" || ! -x "$source_app/Contents/MacOS/taid" ]]; then
  echo 'Build the app and CLI first: npm run desktop:build -- --bundles app,dmg --no-sign --no-binary-patching -- --locked' >&2; exit 1
fi
for resource in THIRD_PARTY_NOTICES.md CUTOUT_NOTICE.txt skills/taid-workbench/SKILL.md; do
  if [[ ! -f "$source_app/Contents/Resources/$resource" ]]; then
    echo "Missing bundled resource: $resource. Rebuild the macOS app before installing." >&2; exit 1
  fi
done
if [[ -e "$cli_link" || -L "$cli_link" ]]; then
  if [[ ! -L "$cli_link" || "$(readlink "$cli_link")" != "$destination/Contents/MacOS/taid" ]]; then echo "Refusing to replace unrelated CLI: $cli_link" >&2; exit 1; fi
fi
if [[ -e "$skill_link" || -L "$skill_link" ]]; then
  if [[ ! -L "$skill_link" ]]; then echo "Refusing to replace unrelated skill: $skill_link" >&2; exit 1; fi
  existing_skill_source="$(readlink "$skill_link")"
  if [[ "$existing_skill_source" != "$skill_source" && "$existing_skill_source" != "$legacy_skill_source" ]]; then
    echo "Refusing to replace unrelated skill: $skill_link" >&2; exit 1
  fi
fi
if [[ -e "$destination" ]]; then
  bundle_id="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$destination/Contents/Info.plist")"
  if [[ "$bundle_id" != 'com.a7garden.taid-designer' ]]; then echo 'Another app already owns ~/Applications/Taid.app.' >&2; exit 1; fi
  if pgrep -x taid-designer >/dev/null; then echo 'Quit Taid before replacing the installed application.' >&2; exit 1; fi
fi
mkdir -p "$HOME/Applications" "$HOME/.local/bin" "$HOME/.agents/skills"
staging="$(mktemp -d "$HOME/Applications/.taid-install.XXXXXX")"
trap 'rm -rf "$staging"' EXIT
/usr/bin/ditto "$source_app" "$staging/Taid.app"
backup=''
if [[ -e "$destination" ]]; then
  mkdir -p "$HOME/Library/Caches/Taid/previous-apps"
  backup="$(mktemp -d "$HOME/Library/Caches/Taid/previous-apps/install.XXXXXX")/Taid.app"
  mv "$destination" "$backup"
fi
if ! mv "$staging/Taid.app" "$destination"; then
  if [[ -n "$backup" ]]; then mv "$backup" "$destination"; fi
  exit 1
fi
if [[ ! -L "$cli_link" ]]; then ln -s "$destination/Contents/MacOS/taid" "$cli_link"; fi
# Migrate the former checkout-linked skill to the copy shipped inside the app.
if [[ -L "$skill_link" && "$(readlink "$skill_link")" != "$skill_source" ]]; then rm "$skill_link"; fi
if [[ ! -L "$skill_link" ]]; then ln -s "$skill_source" "$skill_link"; fi
printf 'App: %s\nCLI: %s\nCodex skill: %s\n' "$destination" "$cli_link" "$skill_link"
if [[ -n "$backup" ]]; then printf 'Previous app retained: %s\n' "$backup"; fi
