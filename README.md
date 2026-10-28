# easy-ref

A small reference manager written in Rust (Leptos + WebAssembly). It runs as a static site on GitHub Pages and can sync its library across devices through a secret GitHub gist.

## Features

- Paste DOIs, doi.org / ACM / Springer links or arXiv IDs (several at once) to create entries; metadata comes from Crossref and DataCite
- Paste BibTeX copied from Google Scholar, ACM, IEEE Xplore, DBLP or arXiv, or import whole `.bib` files; LaTeX accents are decoded, and `{...}` case protection and `and others` are kept
- Paste other web links to create online entries (`@online` with `url` and `urldate`); GitHub repositories are filled from the GitHub API, other pages get the link, site and access date and open for the title and author. `@online` / `@electronic` BibTeX and Zotero's `@misc` web pages are imported as online entries
- Duplicates (same DOI, same link, or same title and first author) are not added twice
- A library view with every entry, plus nested collections; an entry can belong to several collections (drag and drop or batch add), and a collection shows and exports the entries of its subcollections
- Deleted entries go to a trash, from which they can be restored or deleted permanently
- Table columns for title, authors, year, venue, type, citation key and date added, sortable by clicking the header
- Export a collection or a selection as BibTeX, copied or downloaded as `.bib`
- Citation keys are first author's last name + year + venue abbreviation, e.g. `He2016CVPR`, `Ren2017TPAMI`, `Vaswani2017arXiv`; a clashing key gets the first title word (`He2016CVPRIdentity`), then `a`, `b`, …
- Export checks for missing fields and duplicate keys, escapes special characters and accents, and protects acronyms in titles, so common styles (plain, IEEEtran, ACM, …) compile without warnings
- Chinese and English UI; light, dark or system theme
- Installable as an app (Settings → Install app, or the browser's install option) and usable offline; DOI lookup and sync need a connection

## Development

Requires Nix with flakes enabled.

```sh
direnv allow        # or: nix develop
trunk serve --open  # http://localhost:8080
cargo test
```

## Deploying to GitHub Pages

Pushing to `main` runs `.github/workflows/pages.yml`, which tests, builds and publishes the site. Before the first run, set **Settings → Pages → Build and deployment → Source** to **GitHub Actions**.

The site is served at `https://<user>.github.io/<repo>/`.

## Sync

The library can be synced through a secret gist in your GitHub account:

1. Create a GitHub token with the `gist` scope ([direct link](https://github.com/settings/tokens/new?scopes=gist&description=easy-ref))
2. In easy-ref, open **Settings → Sync**, paste the token and click **Save and sync**

The first sync creates a secret gist containing `easy-ref-library.json`. Other devices that use a token from the same account find that gist automatically. Each sync pulls, merges and pushes: the newer edit of an entry wins, deletions propagate to other devices, and the same DOI added on two devices becomes one entry. The token is stored only in the browser's localStorage.

## Limitations

- IEEE Xplore `ieeexplore.ieee.org/document/…` links contain no DOI; copy the DOI from the page instead
- Papers without a DOI (NeurIPS, ICML, ICLR, …) can be added by arXiv ID or created manually
- If two devices change the members of the same collection at the same time, the later change wins

## Credits

Icons are based on [Lucide](https://lucide.dev) (ISC license).
