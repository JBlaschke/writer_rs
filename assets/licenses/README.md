writer embeds the following third-party files and data. writer --licenses prints
this overview and the notice files named here. Paths below are relative to
assets/.

KaTeX 0.19.1 JavaScript and CSS are MIT licensed; see vendor/katex/LICENSE.
Its 20 KaTeX web fonts are under SIL OFL 1.1, with copyright notices and
Reserved Font Names in licenses/katex-fonts-OFL.txt. The npm package contains
only the code's MIT notice, so the fonts' notice is supplied separately.

morphdom 2.7.8 is MIT licensed; see vendor/morphdom/LICENSE.

Latin Modern Math 1.959 is under the GUST Font License. See
vendor/latin-modern-math/GUST-FONT-LICENSE.txt, README-Latin-Modern-Math.txt
and MANIFEST-Latin-Modern-Math.txt in the same directory.

The complete @preview/mitex:0.2.7 package, including mitex.wasm, is shipped
as published by Typst Universe under Apache-2.0. Its notice is
vendor/typst-packages/preview/mitex/0.2.7/LICENSE. The WASM is compiled from
the mitex Rust workspace and the crates it links.

typst-assets 0.15.1 supplies fonts and data through typst and typst-kit.
vendor/crates/typst-assets/LICENSE and vendor/crates/typst-assets/NOTICE
contain its complete notices. Libertinus Serif uses SIL OFL 1.1; New Computer
Modern and New Computer Modern Math use the GUST Font License, except
NewCM10-Regular.otf, which uses GPL-3.0-or-later with the Font Exception and
Distribution Exception. DejaVu Sans Mono uses the Bitstream Vera and Arev
licenses, with DejaVu changes in the public domain. The PDF base-14 fonts
use BSD-3-Clause. ICC profiles use CC0-1.0; the ICU segmentation data is
covered by the crate license. HTML and MathML tables include CC-BY-4.0,
BSD-3-Clause and W3C Document License material. All fonts are unmodified;
the NewCM10-Regular Distribution Exception permits this distribution, and
its Font Exception covers documents embedding the font.

two-face 0.5.2+bat-0.26.1 supplies writer's syntax definitions and themes;
licenses/two-face.md contains its generated acknowledgements. typst links
two-face 0.4.5 for its own syntax definitions; licenses/two-face-0.4.md
contains that version's generated acknowledgements.

hayagriva 0.10.1 embeds CSL styles and locales, primarily under Creative
Commons Attribution-ShareAlike 3.0. licenses/hayagriva-styles.md names all
80 styles, their authors, contributors and rights, and records the count of
63 locales.

codex 0.3.0 supplies symbol and emoji name tables under Apache-2.0. Its
crate LICENSE is included with the linked Rust crates in the release archive.

typst-library 0.15.1 embeds its translation files under the crate's license.
Its crate LICENSE is included with the linked Rust crates in the release
archive; the files remain in typst's source rather than this assets tree.

hypher embeds hyphenation tries from 48 pattern sets under LPPL, MPL, MIT
and BSD-3-Clause licenses. Their notices are in the patterns/ directory at
https://github.com/typst/hypher; its crate LICENSE is included with the
linked Rust crates in the release archive.

The Unicode-to-LaTeX table in math/unicode-latex.toml is derived from
tylax 0.3.8, whose maps derive from tex2typst. Both use Apache-2.0; see
vendor/crates/tylax/LICENSE. VERSIONS.toml in vendor/ records source pins.

Rust crates: THIRD-PARTY-LICENSES/ in the release archive
