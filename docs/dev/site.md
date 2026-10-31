# The project website

<https://jblaschke.github.io/writer_rs/> is built by `scripts/site.py` and
deployed to GitHub Pages by `.github/workflows/site.yml`.

## What is on it

- **The landing page**, `docs/site/index.html` with `docs/site/site.css`: a
  Python `string.Template` that `site.py` fills with the example PDFs' first
  pages, the start of the basic example's source and its five outputs, the
  latest release's download table, the example cards and the guide's pages.
- **The user guide**, every page of `docs/guide/`, rendered by `writer` with
  the site theme in `docs/site/theme/` (the default theme plus a header and
  footer). Links between guide pages stay on the site; links to other
  repository files point at GitHub at the built revision.
- **The examples** `basic`, `notebook` and `typst`, each as a web page plus a
  PDF, a Word file and zips of the LaTeX and Typst exports (no Typst export
  for the Typst example, whose source is already Typst). Ghostscript draws
  each PDF's first page for the landing page.
- The MIT license, `writer --licenses` output and `site-manifest.json`.

Nothing loads fonts or scripts from elsewhere; every page works offline.
URLs are relative, so the site works under any path. `site.py` validates
every local link and fragment before it writes the output directory.

Templates link with `writer-site:` URLs: a page key (`home`, `guide`,
`guide-<page>`, `example-<name>`, optionally with `#fragment`),
`artifact/<route>` for a file the build writes, `source`, `repository`,
`license` or `embedded-notices`.

## When it deploys

On every push to `main`, on "Run workflow" (Actions → Site), and after the
Release workflow publishes a release: its last step starts a Site run on
`main`, because GitHub starts no workflow for a release that a workflow's own
token published. Pull requests that touch the site's inputs only build it;
download the run's `github-pages` artifact to look at the result.

The download table comes from the GitHub API's `releases/latest`, which skips
drafts and pre-releases. Before the first release the page says that no
release is published yet.

One-time setup: Settings → Pages → Build and deployment → Source: "GitHub
Actions". Until then the Configure Pages step fails with a message that says
so.

## Building it locally

With a release build and Ghostscript (`brew install ghostscript`,
`apt-get install ghostscript`):

```sh
cargo build --release
python3 -I scripts/site.py --binary target/release/writer --source . \
    --out target/site --repo-url https://github.com/JBlaschke/writer_rs \
    --revision "$(git rev-parse HEAD)"
python3 -m http.server --directory target/site
```

Add `--release-json FILE` with a saved `gh api
repos/JBlaschke/writer_rs/releases/latest` document to see the download
table. The output directory must not exist yet. The pure tests run with
`python3 -I -m unittest discover -s scripts/tests -p test_site.py`.
