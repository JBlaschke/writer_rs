#!/usr/bin/env bash
# Re-download and trim the vendored assets (docs/spec/vendored-assets.md).
#
# Usage:
#   scripts/vendor.sh
# Run from anywhere; writes into <repo>/assets/vendor/.
#
# What it does:
#   1. downloads the pinned archives and Latin Modern Math files into a
#      temporary directory,
#   2. verifies each download's SHA-256 against the value hard-coded below,
#   3. copies exactly the files listed in docs/spec/vendored-assets.md §2
#      (nothing else) into a staging directory and writes VERSIONS.toml there,
#   4. only if every step succeeded, replaces assets/vendor/ with the staging
#      directory (a failure leaves the existing assets/vendor/ untouched).
#
# The output depends only on the pinned downloads, so rerunning produces no git
# diff.
# Requirements: bash, curl, tar, awk, and sha256sum or shasum.
#
# Upgrading (vendored-assets.md §9): edit the versions, URLs, hashes and
# PINNED_DATE below, run the script, review `git diff`, then run the required
# tests from that procedure.

set -euo pipefail

KATEX_VERSION="0.19.1"
KATEX_URL="https://registry.npmjs.org/katex/-/katex-${KATEX_VERSION}.tgz"
KATEX_SHA256="24da5a599eb2fcede2e270b99a6ff48bca816beecc95feeb84e8cccbd1bb34fd"

MORPHDOM_VERSION="2.7.8"
MORPHDOM_URL="https://registry.npmjs.org/morphdom/-/morphdom-${MORPHDOM_VERSION}.tgz"
MORPHDOM_SHA256="06bfcd3edc6dcb0fcaa4e6f2aab28656830fd29816bfc96553c6d221bfab4bf3"

LMM_VERSION="1.959"
LMM_COMMIT="9182b146cb64ac1a5e9cb92a09cd79612ca95d1c"
LMM_BASE="https://raw.githubusercontent.com/fred-wang/MathFonts/${LMM_COMMIT}/LatinModern"
LMM_FONT_SHA256="811ec18a1f3dca9e118c4f26bd3d39a677bcc73e009b83c10f940c950b170c13"
LMM_LICENSE_SHA256="2bd69affc3da00715116f713f57eab9707e96daf3562ad0215987b15b9c16f73"
LMM_README_SHA256="e6fdd5fb44b656146fa988d191f58901c09a347d3f4fbaa3b45b24a6ad7ec30b"
LMM_MANIFEST_SHA256="1daa729f7922c42ba5ce8da7dfe4ecdc9e5c878ad86217fd6f32c4671505b007"

MITEX_VERSION="0.2.7"
MITEX_URL="https://packages.typst.org/preview/mitex-${MITEX_VERSION}.tar.gz"
MITEX_SHA256="0159e214845e49cbdc332d9d572da112dae5ad248072e0a7680d38c8307c2e15"

TYPST_ASSETS_VERSION="0.15.1"
TYPST_ASSETS_URL="https://static.crates.io/crates/typst-assets/typst-assets-${TYPST_ASSETS_VERSION}.crate"
TYPST_ASSETS_SHA256="bcee505dac6702dd1c5e65aa2e94a6179d19ee09e2e5637d7313db91765dc4e0"

TYLAX_VERSION="0.3.8"
TYLAX_URL="https://static.crates.io/crates/tylax/tylax-${TYLAX_VERSION}.crate"
TYLAX_SHA256="145d65de1d49c044ae64c664edf034f4956c17dbddaa0cf2fad3afea85400672"

# Date the pins above were verified. Fixed (not "today") for idempotent reruns.
PINNED_DATE="2026-10-25"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${ROOT}/assets/vendor"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/writer-vendor.XXXXXX")"
trap 'rm -rf "${WORK}"' EXIT
# Everything is assembled here first and moved to ${DEST} at the very end.
OUT="${WORK}/out"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# fetch NAME URL SHA256: download, verify and extract into $WORK/NAME/.
# npm uses package/, crates use <name>-<version>/, and mitex has no top directory.
fetch() {
    local name="$1"
    local url="$2"
    local expected="$3"
    local tarball="${WORK}/${name}.tgz"
    echo "downloading ${url}" >&2
    curl --fail --silent --show-error --location --output "${tarball}" "${url}"
    local actual
    actual="$(sha256_of "${tarball}")"
    if [[ "${actual}" != "${expected}" ]]; then
        echo "error: SHA-256 mismatch for ${url}" >&2
        echo "  expected ${expected}" >&2
        echo "  actual   ${actual}" >&2
        exit 1
    fi
    mkdir -p "${WORK}/${name}"
    tar -xzf "${tarball}" -C "${WORK}/${name}"
}

# fetch_file NAME URL SHA256: download and verify one raw file at $WORK/NAME.
fetch_file() {
    local name="$1"
    local url="$2"
    local expected="$3"
    local file="${WORK}/${name}"
    mkdir -p "$(dirname "${file}")"
    echo "downloading ${url}" >&2
    curl --fail --silent --show-error --location --output "${file}" "${url}"
    local actual
    actual="$(sha256_of "${file}")"
    if [[ "${actual}" != "${expected}" ]]; then
        echo "error: SHA-256 mismatch for ${url}" >&2
        echo "  expected ${expected}" >&2
        echo "  actual   ${actual}" >&2
        exit 1
    fi
}

# copy SRC_ROOT DEST_ROOT REL...: copy relative paths; fail if one is missing.
copy() {
    local src_root="$1"
    local dest_root="$2"
    shift 2
    local rel
    for rel in "$@"; do
        rel="${rel#./}"
        if [[ ! -f "${src_root}/${rel}" ]]; then
            echo "error: expected file missing from package: ${rel}" >&2
            exit 1
        fi
        mkdir -p "$(dirname "${dest_root}/${rel}")"
        cp "${src_root}/${rel}" "${dest_root}/${rel}"
        chmod 0644 "${dest_root}/${rel}"
    done
}

# copy_found SRC_ROOT DEST_ROOT FIND-ARGS...: run `find FIND-ARGS -type f`
# inside SRC_ROOT and copy every file it prints (paths stay relative).
# Fails if nothing matches.
copy_found() {
    local src_root="$1"
    local dest_root="$2"
    shift 2
    local found
    local rel
    found="$(cd "${src_root}" && find "$@" -type f | LC_ALL=C sort)"
    if [[ -z "${found}" ]]; then
        echo "error: no files match: find $*" >&2
        exit 1
    fi
    while IFS= read -r rel; do
        copy "${src_root}" "${dest_root}" "${rel}"
    done <<<"${found}"
}

vendor_katex() {
    local src="${WORK}/katex/package"
    local dest="${OUT}/katex"
    copy "${src}" "${dest}" LICENSE
    copy "${src}/dist" "${dest}" katex.min.js katex.min.css
    copy_found "${src}/dist" "${dest}" fonts -maxdepth 1 -name '*.woff2'
}

vendor_morphdom() {
    local src="${WORK}/morphdom/package"
    local dest="${OUT}/morphdom"
    copy "${src}" "${dest}" LICENSE
    copy "${src}/dist" "${dest}" morphdom-umd.min.js
}

write_versions() {
    cat >"${OUT}/VERSIONS.toml" <<EOF
# Generated by scripts/vendor.sh; do not edit by hand.
# See docs/spec/vendored-assets.md for what is copied and why.

[[package]]
name = "katex"
version = "${KATEX_VERSION}"
license = "MIT AND OFL-1.1"
role = "web"
date = "${PINNED_DATE}"
path = "katex"
[[package.source]]
url = "${KATEX_URL}"
sha256 = "${KATEX_SHA256}"

[[package]]
name = "morphdom"
version = "${MORPHDOM_VERSION}"
license = "MIT"
role = "web"
date = "${PINNED_DATE}"
path = "morphdom"
[[package.source]]
url = "${MORPHDOM_URL}"
sha256 = "${MORPHDOM_SHA256}"

[[package]]
name = "latin-modern-math"
version = "${LMM_VERSION}"
license = "GUST Font License"
role = "font"
date = "${PINNED_DATE}"
path = "latin-modern-math"
[[package.source]]
url = "${LMM_BASE}/latinmodern-math.woff2"
sha256 = "${LMM_FONT_SHA256}"
[[package.source]]
url = "${LMM_BASE}/GUST-FONT-LICENSE.txt"
sha256 = "${LMM_LICENSE_SHA256}"
[[package.source]]
url = "${LMM_BASE}/README-Latin-Modern-Math.txt"
sha256 = "${LMM_README_SHA256}"
[[package.source]]
url = "${LMM_BASE}/MANIFEST-Latin-Modern-Math.txt"
sha256 = "${LMM_MANIFEST_SHA256}"

[[package]]
name = "mitex"
version = "${MITEX_VERSION}"
license = "Apache-2.0"
role = "typst-package"
date = "${PINNED_DATE}"
path = "typst-packages/preview/mitex/${MITEX_VERSION}"
[[package.source]]
url = "${MITEX_URL}"
sha256 = "${MITEX_SHA256}"

[[package]]
name = "typst-assets"
version = "${TYPST_ASSETS_VERSION}"
license = "Apache-2.0"
role = "license-only"
date = "${PINNED_DATE}"
path = "crates/typst-assets"
note = "linked through typst-kit embedded-fonts; only LICENSE and NOTICE are copied"
[[package.source]]
url = "${TYPST_ASSETS_URL}"
sha256 = "${TYPST_ASSETS_SHA256}"

[[package]]
name = "tylax"
version = "${TYLAX_VERSION}"
license = "Apache-2.0"
role = "license-only"
date = "${PINNED_DATE}"
path = "crates/tylax"
note = "assets/math/unicode-latex.toml is derived from it; only LICENSE is copied"
[[package.source]]
url = "${TYLAX_URL}"
sha256 = "${TYLAX_SHA256}"
EOF
    chmod 0644 "${OUT}/VERSIONS.toml"
}

main() {
    fetch katex "${KATEX_URL}" "${KATEX_SHA256}"
    fetch morphdom "${MORPHDOM_URL}" "${MORPHDOM_SHA256}"
    fetch_file latin-modern-math/latinmodern-math.woff2 \
        "${LMM_BASE}/latinmodern-math.woff2" "${LMM_FONT_SHA256}"
    fetch_file latin-modern-math/GUST-FONT-LICENSE.txt \
        "${LMM_BASE}/GUST-FONT-LICENSE.txt" "${LMM_LICENSE_SHA256}"
    fetch_file latin-modern-math/README-Latin-Modern-Math.txt \
        "${LMM_BASE}/README-Latin-Modern-Math.txt" "${LMM_README_SHA256}"
    fetch_file latin-modern-math/MANIFEST-Latin-Modern-Math.txt \
        "${LMM_BASE}/MANIFEST-Latin-Modern-Math.txt" "${LMM_MANIFEST_SHA256}"
    fetch mitex "${MITEX_URL}" "${MITEX_SHA256}"
    fetch typst-assets "${TYPST_ASSETS_URL}" "${TYPST_ASSETS_SHA256}"
    fetch tylax "${TYLAX_URL}" "${TYLAX_SHA256}"
    mkdir -p "${OUT}"
    vendor_katex
    vendor_morphdom
    copy "${WORK}/latin-modern-math" "${OUT}/latin-modern-math" \
        latinmodern-math.woff2 GUST-FONT-LICENSE.txt \
        README-Latin-Modern-Math.txt MANIFEST-Latin-Modern-Math.txt
    copy_found \
        "${WORK}/mitex" "${OUT}/typst-packages/preview/mitex/${MITEX_VERSION}" .
    copy "${WORK}/typst-assets/typst-assets-${TYPST_ASSETS_VERSION}" \
        "${OUT}/crates/typst-assets" LICENSE NOTICE
    copy "${WORK}/tylax/tylax-${TYLAX_VERSION}" "${OUT}/crates/tylax" LICENSE
    write_versions
    # Swap in the complete tree only now; any failure above left DEST unchanged.
    rm -rf "${DEST}"
    mkdir -p "$(dirname "${DEST}")"
    mv "${OUT}" "${DEST}"
    echo "vendored KaTeX ${KATEX_VERSION}, morphdom ${MORPHDOM_VERSION}," \
        "Latin Modern Math ${LMM_VERSION}, mitex ${MITEX_VERSION}," \
        "and the licenses of typst-assets ${TYPST_ASSETS_VERSION} and" \
        "tylax ${TYLAX_VERSION}, into ${DEST}" >&2
}

main "$@"
