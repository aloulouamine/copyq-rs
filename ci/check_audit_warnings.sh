#!/usr/bin/env bash
# Filtre RustSec : correspondance exacte nom/version avec l'union des graphes macOS.
set -euo pipefail

audit_report=$1
macos_packages=$2
output_dir=${3:-.}

jq -r '.warnings[][] | "\(.package.name) v\(.package.version) \(.kind) \(.advisory.id // "-")"' "$audit_report" \
  | sort -u > "$output_dir/audit-warnings.log"
awk 'NR == FNR { macos[$1 " " $2] = 1; next } ($1 " " $2) in macos' \
  "$macos_packages" "$output_dir/audit-warnings.log" > "$output_dir/audit-macos-warnings.log"
if [ -s "$output_dir/audit-macos-warnings.log" ]; then
  echo "::error::Avertissements RustSec sur des paquets des graphes macOS :"
  cat "$output_dir/audit-macos-warnings.log"
  exit 1
fi
