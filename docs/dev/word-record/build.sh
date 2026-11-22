#!/bin/sh
# Rebuild the Word record's DOCX inputs (T103) from design/ and default/.
#
#   usage: docs/dev/word-record/build.sh [path/to/writer]
#
# The writer binary defaults to target/release/writer. The committed DOCX
# files were built with writer 0.3.1 (v0.3.1-16-gead21e88). design/ needs
# Source Sans 3 (Regular, Italic, SemiBold, Bold as SourceSans3-*.ttf) in
# $WORD_RECORD_FONTS, by default ~/Library/Fonts: build.sh copies the four
# files into a scratch copy of design/fonts, the directory its writer.toml
# names in [pdf] font_paths, so the font files are not committed.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
writer=${1:-$root/target/release/writer}
fonts=${WORD_RECORD_FONTS:-$HOME/Library/Fonts}

"$writer" --version
work=$(mktemp -d "${TMPDIR:-/tmp}/word-record.XXXXXX")
trap 'rm -rf "$work"' EXIT

for project in design default; do
    cp -R "$here/$project" "$work/$project"
    if [ "$project" = design ]; then
        mkdir -p "$work/design/fonts"
        for face in Regular Italic SemiBold Bold; do
            cp "$fonts/SourceSans3-$face.ttf" "$work/design/fonts/"
        done
    fi
    for source in "$work/$project"/w*.md; do
        name=$(basename "$source" .md)
        (cd "$work/$project" && "$writer" build "$name.md" --to docx)
        cp "$work/$project/dist/$name.docx" "$here/$name.docx"
    done
done

# Every Source Sans 3 line must come out at w:line="228" (1.35 / 1.424):
# 265 would mean the font was not found.
for docx in "$here"/w*.docx; do
    if unzip -p "$docx" word/styles.xml | grep -q 'w:line="265"'; then
        echo "error: $docx has w:line=\"265\": Source Sans 3 was not read" >&2
        exit 1
    fi
done
