#!/usr/bin/env bash
# write_to_drive.sh — Build RustOS locally and flash it to a drive.
#
# Usage:
#   ./write_to_drive.sh --drive /dev/sdX
#
# Requirements:
#   - cargo + Rust nightly toolchain
#   - dd (coreutils), lsblk, sfdisk, sgdisk, mkfs.fat
#   - Root privileges (or write access to the target drive)

set -euo pipefail

DRIVE=""
PARTITION_SYNC_DELAY_SECONDS=1

# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------
reload_partition_table() {
    local device="$1"
    if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
        sudo blockdev --rereadpt "$device" || true
        sudo partprobe "$device" || true
    else
        blockdev --rereadpt "$device" || true
        partprobe "$device" || true
    fi
}

run_as_root() {
    if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
        sudo "$@"
    else
        "$@"
    fi
}

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
            echo "Builds a local RustOS UEFI disk image and writes it to the"
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
# Tool checks
# ---------------------------------------------------------------------------
for tool in cargo lsblk sfdisk mkfs.fat find; do
    if ! command -v "$tool" &>/dev/null; then
        echo "Error: required tool '$tool' is not installed." >&2
        exit 1
    fi
done

# ---------------------------------------------------------------------------
# Build local image
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")" && pwd)"
cd "$SCRIPT_DIR"

echo "Updating submodules to pinned repository commits..."
git submodule update --init --recursive

echo "Building kernel (release)..."
cargo build --release

KERNEL_ELF=$(find "$SCRIPT_DIR/target" -path "*/release/rustos" -not -name "*.d" | head -1)
if [[ -z "$KERNEL_ELF" || ! -f "$KERNEL_ELF" ]]; then
    echo "Error: failed to locate release kernel ELF after build." >&2
    exit 1
fi

IMG_FILE="$SCRIPT_DIR/rustos-local.img"
echo "Creating local UEFI disk image: $IMG_FILE"
(cd "$SCRIPT_DIR/crates/create-image" && cargo run --release -- "$KERNEL_ELF" "$IMG_FILE")

# ---------------------------------------------------------------------------
# Flash
# ---------------------------------------------------------------------------
echo
echo "Target drive: $DRIVE"
echo
echo "WARNING: ALL DATA ON '$DRIVE' WILL BE PERMANENTLY DESTROYED."
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
echo "Done! '$DRIVE' is ready to boot local RustOS build in UEFI mode."
echo
echo "Creating storage partition from remaining free space..."

reload_partition_table "$DRIVE"

sleep 1

PTTYPE=$(lsblk -dn -o PTTYPE "$DRIVE" | tr -d '[:space:]')
if [[ -z "$PTTYPE" ]]; then
    echo "Error: could not detect partition table type on '$DRIVE' after flashing." >&2
    exit 1
fi

if [[ "$PTTYPE" == "gpt" ]]; then
    if ! command -v sgdisk &>/dev/null; then
        echo "Error: detected GPT disk image, but required tool 'sgdisk' is not installed." >&2
        echo "Install it (usually package: gdisk) and re-run." >&2
        exit 1
    fi

    echo "Repairing GPT metadata to use full target drive size..."
    run_as_root sgdisk -e "$DRIVE"
    reload_partition_table "$DRIVE"

    # Give the kernel a brief moment to expose the updated GPT layout.
    sleep "$PARTITION_SYNC_DELAY_SECONDS"

    if run_as_root sgdisk -i 2 "$DRIVE" >/dev/null 2>&1; then
        STORAGE_START_SECTOR=$(
            run_as_root sgdisk -i 2 "$DRIVE" |
                awk -F: '/First sector:/ {gsub(/^[[:space:]]+/, "", $2); split($2, a, " "); print a[1]}'
        )
        if [[ -z "$STORAGE_START_SECTOR" ]]; then
            echo "Error: failed to determine the start sector for existing GPT partition 2." >&2
            exit 1
        fi

        echo "Resizing existing storage partition 2 to fill remaining space..."
        run_as_root sgdisk -d 2 "$DRIVE"
        run_as_root sgdisk -n 2:${STORAGE_START_SECTOR}:0 -t 2:0700 -c 2:"rustos-storage" "$DRIVE"
    else
        echo "Adding storage partition using sgdisk..."
        run_as_root sgdisk -n 2:0:0 -t 2:0700 -c 2:"rustos-storage" "$DRIVE"
    fi
    reload_partition_table "$DRIVE"
else
    # For MBR/DOS partition tables, use sfdisk
    PART_SPEC='type=c'
    printf '%s\n' "$PART_SPEC" | run_as_root sfdisk --append "$DRIVE"
    reload_partition_table "$DRIVE"
fi

# Give the kernel time to create the partition device node
sleep "$PARTITION_SYNC_DELAY_SECONDS"

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
run_as_root mkfs.fat -F 32 -n RUSTOS_ROOT "$STORAGE_PART"

# Populate the FAT32 root filesystem with a standard directory skeleton so
# that the kernel has a proper persistent root from first boot.
echo "Populating FAT32 storage partition with initial directory skeleton..."
MOUNT_TMP=$(mktemp -d)
if command -v sudo &>/dev/null && [[ "$(id -u)" -ne 0 ]]; then
    sudo mount -t vfat "$STORAGE_PART" "$MOUNT_TMP"
    sudo mkdir -p \
        "$MOUNT_TMP/bin"  \
        "$MOUNT_TMP/etc"  \
        "$MOUNT_TMP/home" \
        "$MOUNT_TMP/mnt"  \
        "$MOUNT_TMP/mnt/c" \
        "$MOUNT_TMP/mnt/d" \
        "$MOUNT_TMP/proc" \
        "$MOUNT_TMP/sys"  \
        "$MOUNT_TMP/tmp"  \
        "$MOUNT_TMP/usr"  \
        "$MOUNT_TMP/usr/bin" \
        "$MOUNT_TMP/var"  \
        "$MOUNT_TMP/var/log"
    sudo umount "$MOUNT_TMP"
else
    mount -t vfat "$STORAGE_PART" "$MOUNT_TMP"
    mkdir -p \
        "$MOUNT_TMP/bin"  \
        "$MOUNT_TMP/etc"  \
        "$MOUNT_TMP/home" \
        "$MOUNT_TMP/mnt"  \
        "$MOUNT_TMP/mnt/c" \
        "$MOUNT_TMP/mnt/d" \
        "$MOUNT_TMP/proc" \
        "$MOUNT_TMP/sys"  \
        "$MOUNT_TMP/tmp"  \
        "$MOUNT_TMP/usr"  \
        "$MOUNT_TMP/usr/bin" \
        "$MOUNT_TMP/var"  \
        "$MOUNT_TMP/var/log"
    umount "$MOUNT_TMP"
fi
rmdir "$MOUNT_TMP"

echo
echo "Done! '$DRIVE' is ready:"
echo "  - Partition 1: RustOS boot partition"
echo "  - Partition 2: FAT32 storage/root filesystem"
echo "Remove the drive safely, then boot your system in UEFI mode."

# Clean up the local image
rm -f "$IMG_FILE"
