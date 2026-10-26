#!/usr/bin/env bash
# Compiles BibTeX generated from the test fixtures with several styles; fails on any warning.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

BIB_OUT="$work/refs.bib" cargo test --quiet --manifest-path "$root/Cargo.toml" \
  write_fixture_bib -- --ignored >/dev/null

styles=(plain abbrv unsrt alpha ieeetr plainnat IEEEtran ACM-Reference-Format)
status=0
cd "$work"
for style in "${styles[@]}"; do
  if ! kpsewhich "$style.bst" >/dev/null; then
    echo "skip  $style (not installed)"
    continue
  fi
  # acmart always loads hyperref; the ACM bbl relies on it.
  extra=""
  [ "$style" = ACM-Reference-Format ] && extra='\usepackage{hyperref}'
  cat >"$style.tex" <<EOF
\documentclass{article}
\usepackage{url}
\usepackage[numbers]{natbib}
$extra
\begin{document}
\nocite{*}
\bibliographystyle{$style}
\bibliography{refs}
\end{document}
EOF
  pdflatex -interaction=nonstopmode "$style" >/dev/null || true
  bibtex "$style" >/dev/null || true
  pdflatex -interaction=nonstopmode "$style" >/dev/null || true
  pdflatex -interaction=nonstopmode "$style" >/dev/null || true
  if grep -E "^Warning--|error message" "$style.blg" || grep -E "Warning|^!" "$style.log"; then
    echo "FAIL  $style"
    status=1
  else
    echo "ok    $style"
  fi
done
[ "$status" -eq 0 ] && cp refs.bib "$root/target/fixtures.bib"
exit $status
