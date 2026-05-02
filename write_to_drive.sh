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

REPO="RustOS-Dev/RustOS"
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

# Ensure the target is a block device
if [[ ! -b "$DRIVE" ]]; then
    echo "Error: '$DRIVE' is not a block device." >&2
    exit 1
fi

# Refuse to write to a device that has a mounted partition used as / or /boot
if grep -qs "^${DRIVE}" /proc/mounts; then
    MOUNTS=$(grep "^${DRIVE}" /proc/mounts | awk '{print $2}' | tr '\n' ' ')
    echo "Error: '$DRIVE' (or one of its partitions) is currently mounted at: $MOUNTS" >&2
    echo "Unmount it first before flashing." >&2
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

for tool in lsblk sfdisk mkfs.fat; do
    if ! command -v "$tool" &>/dev/null; then
        echo "Error: required tool '$tool' is not installed." >&2
        exit 1
    fi
done

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
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/' \
    | tr -cd 'a-zA-Z0-9._-')

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
echo
echo "Creating storage partition from remaining free space..."

if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
    sudo blockdev --rereadpt "$DRIVE" || true
    sudo partprobe "$DRIVE" || true
else
    blockdev --rereadpt "$DRIVE" || true
    partprobe "$DRIVE" || true
fi

sleep 1

PTTYPE=$(lsblk -dn -o PTTYPE "$DRIVE" | tr -d '[:space:]')
if [[ -z "$PTTYPE" ]]; then
    echo "Error: could not detect partition table type on '$DRIVE' after flashing." >&2
    exit 1
fi

if [[ "$PTTYPE" == "gpt" ]]; then
    PART_SPEC='type=0700,name="rustos-storage"'
else
    PART_SPEC='type=c'
fi

if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
    printf '%s\n' "$PART_SPEC" | sudo sfdisk --append "$DRIVE"
else
    printf '%s\n' "$PART_SPEC" | sfdisk --append "$DRIVE"
fi

if [[ "$DRIVE" =~ [0-9]$ ]]; then
    STORAGE_PART="${DRIVE}p2"
else
    STORAGE_PART="${DRIVE}2"
fi

for _ in $(seq 1 20); do
    if [[ -b "$STORAGE_PART" ]]; then
        break
    fi
    sleep 0.2
done

if [[ ! -b "$STORAGE_PART" ]]; then
    echo "Error: storage partition device '$STORAGE_PART' was not created." >&2
    exit 1
fi

echo "Formatting ${STORAGE_PART} as FAT32 (label: RUSTOS_ROOT)..."
if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
    sudo mkfs.fat -F 32 -n RUSTOS_ROOT "$STORAGE_PART"
else
    mkfs.fat -F 32 -n RUSTOS_ROOT "$STORAGE_PART"
fi

echo
echo "Done! '$DRIVE' is ready:"
echo "  - Partition 1: RustOS boot partition"
echo "  - Partition 2: FAT32 storage/root filesystem"
echo "Remove the drive safely, then boot your system in UEFI mode."

# Clean up the downloaded image
rm -f "$IMG_FILE"
