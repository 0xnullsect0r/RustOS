#!/usr/bin/env bash
# write_to_drive.sh — Download the latest RustOS release image and flash it to a drive.
#
# Usage:
#   ./write_to_drive.sh --drive /dev/sdX
#
# Requirements:
#   - curl or wget
#   - dd (coreutils)
#   - Root privileges (or write access to the target drive)

set -euo pipefail

REPO="0xnullsect0r/RustOS"
DRIVE=""

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --drive)
            if [[ -z "${2:-}" ]]; then
                echo "Error: --drive requires a path argument" >&2
                exit 1
            fi
            DRIVE="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 --drive /dev/sdX"
            echo
            echo "Downloads the latest RustOS UEFI disk image and writes it to the"
            echo "specified drive.  The drive will be COMPLETELY OVERWRITTEN."
            echo
            echo "Example:"
            echo "  $0 --drive /dev/sdb"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            echo "Usage: $0 --drive /dev/sdX" >&2
            exit 1
            ;;
    esac
done

if [[ -z "$DRIVE" ]]; then
    echo "Error: --drive is required." >&2
    echo "Usage: $0 --drive /dev/sdX" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Sanity checks
# ---------------------------------------------------------------------------
if [[ ! -e "$DRIVE" ]]; then
    echo "Error: device '$DRIVE' does not exist." >&2
    exit 1
fi

# Warn if a partition was given instead of the whole disk
if [[ "$DRIVE" =~ [0-9]$ ]]; then
    echo "Warning: '$DRIVE' looks like a partition. For a bootable image you" >&2
    echo "         usually want the whole disk (e.g. /dev/sdb, not /dev/sdb1)." >&2
fi

# ---------------------------------------------------------------------------
# Detect download tool
# ---------------------------------------------------------------------------
if command -v curl &>/dev/null; then
    DL_CMD="curl"
elif command -v wget &>/dev/null; then
    DL_CMD="wget"
else
    echo "Error: neither curl nor wget is available. Please install one." >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Fetch latest release metadata from the GitHub API
# ---------------------------------------------------------------------------
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
echo "Fetching latest release info from ${API_URL} ..."

if [[ "$DL_CMD" == "curl" ]]; then
    RELEASE_JSON=$(curl -fsSL "$API_URL")
else
    RELEASE_JSON=$(wget -qO- "$API_URL")
fi

TAG=$(printf '%s' "$RELEASE_JSON" \
    | grep -m1 '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

DOWNLOAD_URL=$(printf '%s' "$RELEASE_JSON" \
    | grep '"browser_download_url"' \
    | grep '\.img"' \
    | head -1 \
    | sed 's/.*"browser_download_url": *"\([^"]*\)".*/\1/')

if [[ -z "$TAG" ]]; then
    echo "Error: could not determine the latest release tag." >&2
    exit 1
fi

if [[ -z "$DOWNLOAD_URL" ]]; then
    echo "Error: no .img asset found in release '${TAG}'." >&2
    exit 1
fi

IMG_FILE="rustos-${TAG}.img"

# ---------------------------------------------------------------------------
# Download
# ---------------------------------------------------------------------------
echo "Downloading RustOS ${TAG} ..."
echo "  URL:  $DOWNLOAD_URL"
echo "  File: $IMG_FILE"
echo

if [[ "$DL_CMD" == "curl" ]]; then
    curl -L --progress-bar -o "$IMG_FILE" "$DOWNLOAD_URL"
else
    wget --progress=bar:force:noscroll -O "$IMG_FILE" "$DOWNLOAD_URL"
fi

# ---------------------------------------------------------------------------
# Flash
# ---------------------------------------------------------------------------
echo
echo "Target drive: $DRIVE"
echo
echo "WARNING: ALL DATA ON '$DRIVE' WILL BE PERMANENTLY DESTROYED."
echo "Press Ctrl+C within 5 seconds to abort ..."
sleep 5
echo

echo "Writing image to $DRIVE ..."
if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
    sudo dd if="$IMG_FILE" of="$DRIVE" bs=4M status=progress conv=fsync
    sudo sync
else
    dd if="$IMG_FILE" of="$DRIVE" bs=4M status=progress conv=fsync
    sync
fi

echo
echo "Done! '$DRIVE' is ready to boot RustOS ${TAG} in UEFI mode."
echo "Remove the drive safely, then boot your system from it."

# Clean up the downloaded image
rm -f "$IMG_FILE"
