#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# download_geo_db.sh — Download MaxMind GeoLite2 databases for bundling
#
# Usage:
#   MAXMIND_LICENSE_KEY=<key> ./scripts/download_geo_db.sh
#
# The script downloads GeoLite2-Country.mmdb and GeoLite2-ASN.mmdb into
# src-tauri/resources/geo/ for inclusion in the signed app bundle.
#
# MaxMind GeoLite2 is free but requires a MaxMind account:
#   https://www.maxmind.com/en/geolite2/signup
# After signup, generate a license key at:
#   https://www.maxmind.com/en/accounts/current/license-key
#
# The .mmdb files MUST NOT be committed to the public repository.
# Add them to .gitignore after downloading.
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GEO_DIR="${REPO_ROOT}/src-tauri/resources/geo"

if [[ -z "${MAXMIND_LICENSE_KEY:-}" ]]; then
    echo "ERROR: MAXMIND_LICENSE_KEY environment variable is not set." >&2
    echo "       Register at https://www.maxmind.com/en/geolite2/signup" >&2
    echo "       then set MAXMIND_LICENSE_KEY=<your_key>" >&2
    exit 1
fi

mkdir -p "${GEO_DIR}"

download_db() {
    local edition="$1"
    local dest="${GEO_DIR}/${edition}.mmdb"
    local tmp_tar="/tmp/${edition}.tar.gz"

    echo "Downloading ${edition}..."
    rm -f "${tmp_tar}"

    local status_code
    if [[ -n "${MAXMIND_ACCOUNT_ID:-}" ]]; then
        status_code=$(curl -s -L -w "%{http_code}" -u "${MAXMIND_ACCOUNT_ID}:${MAXMIND_LICENSE_KEY}" \
            "https://download.maxmind.com/geoip/databases/${edition}/download?suffix=tar.gz" \
            -o "${tmp_tar}")
    else
        status_code=$(curl -s -L -w "%{http_code}" \
            "https://download.maxmind.com/app/geoip_download?edition_id=${edition}&license_key=${MAXMIND_LICENSE_KEY}&suffix=tar.gz" \
            -o "${tmp_tar}")
    fi

    if [[ "${status_code}" != "200" ]]; then
        local err_msg
        err_msg=$(cat "${tmp_tar}" 2>/dev/null || echo "HTTP ${status_code}")
        echo "ERROR: Download failed for ${edition} (HTTP ${status_code}): ${err_msg}" >&2
        rm -f "${tmp_tar}"
        exit 1
    fi

    local tmp_extract
    tmp_extract=$(mktemp -d)
    tar -xzf "${tmp_tar}" -C "${tmp_extract}"
    local mmdb_path
    mmdb_path=$(find "${tmp_extract}" -name "${edition}.mmdb" | head -1)

    if [[ -z "${mmdb_path}" ]]; then
        echo "ERROR: ${edition}.mmdb not found after extraction." >&2
        rm -rf "${tmp_extract}" "${tmp_tar}"
        exit 1
    fi

    mv "${mmdb_path}" "${dest}"
    rm -rf "${tmp_extract}" "${tmp_tar}"
    echo "  → ${dest}"
}

download_db "GeoLite2-Country"
download_db "GeoLite2-ASN"

echo ""
echo "Done. Files written to:"
ls -lh "${GEO_DIR}"/*.mmdb
echo ""
echo "IMPORTANT: These files are in .gitignore and must NOT be committed to the"
echo "           public repository (MaxMind license requirement)."
