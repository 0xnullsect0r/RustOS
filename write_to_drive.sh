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
AX210_FIRMWARE_SOURCE="${RUSTOS_AX210_FIRMWARE:-}"
PARTITION_SYNC_DELAY_SECONDS=1
AX210_FIRMWARE_PRIMARY="iwlwifi-ty-a0-gf-a0-72.ucode"
AX210_FIRMWARE_FALLBACK="iwlwifi-ty-a0-gf-a0-71.ucode"

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

populate_rootfs_skeleton() {
    local mount_point="$1"
    run_as_root mkdir -p \
        "$mount_point/bin" \
        "$mount_point/etc" \
        "$mount_point/home" \
        "$mount_point/lib" \
        "$mount_point/lib/firmware" \
        "$mount_point/mnt" \
        "$mount_point/mnt/c" \
        "$mount_point/mnt/d" \
        "$mount_point/proc" \
        "$mount_point/sys" \
        "$mount_point/tmp" \
        "$mount_point/usr" \
        "$mount_point/usr/bin" \
        "$mount_point/var" \
        "$mount_point/var/log"
}

provision_ax210_firmware() {
    local mount_point="$1"
    local source="$2"
    local firmware_dir="$mount_point/lib/firmware"

    run_as_root mkdir -p "$firmware_dir"

    if [[ -z "$source" ]]; then
        echo "AX210 firmware not provided. Expected runtime path(s):"
        echo "  /lib/firmware/$AX210_FIRMWARE_PRIMARY"
        echo "  /lib/firmware/$AX210_FIRMWARE_FALLBACK"
        echo "Re-run with --ax210-firmware <file-or-dir> or set RUSTOS_AX210_FIRMWARE to copy a local blob."
        return 0
    fi

    if [[ -d "$source" ]]; then
        local copied=0
        for firmware_name in "$AX210_FIRMWARE_PRIMARY" "$AX210_FIRMWARE_FALLBACK"; do
            if [[ -f "$source/$firmware_name" ]]; then
                run_as_root cp "$source/$firmware_name" "$firmware_dir/$firmware_name"
                echo "Provisioned AX210 firmware: /lib/firmware/$firmware_name"
                copied=1
            fi
        done
        if [[ "$copied" -eq 1 ]]; then
            return 0
        fi
        echo "Error: no AX210 firmware blobs were found in '$source'." >&2
        echo "Expected $AX210_FIRMWARE_PRIMARY and/or $AX210_FIRMWARE_FALLBACK." >&2
        return 1
    fi

    if [[ -f "$source" ]]; then
        local firmware_name="${source##*/}"
        case "$firmware_name" in
            "$AX210_FIRMWARE_PRIMARY"|"$AX210_FIRMWARE_FALLBACK")
                run_as_root cp "$source" "$firmware_dir/$firmware_name"
                echo "Provisioned AX210 firmware: /lib/firmware/$firmware_name"
                return 0
                ;;
            *)
                echo "Error: AX210 firmware file must be named '$AX210_FIRMWARE_PRIMARY' or '$AX210_FIRMWARE_FALLBACK'." >&2
                return 1
                ;;
        esac
    fi

    echo "Error: AX210 firmware source '$source' does not exist." >&2
    return 1
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
        --ax210-firmware)
            if [[ -z "${2:-}" ]]; then
                echo "Error: --ax210-firmware requires a path argument" >&2
                exit 1
            fi
            AX210_FIRMWARE_SOURCE="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 --drive /dev/sdX [--ax210-firmware <file-or-dir>]"
            echo
            echo "Builds a local RustOS UEFI disk image and writes it to the"
            echo "specified drive.  The drive will be COMPLETELY OVERWRITTEN."
            echo "If AX210 firmware is provided, it is copied to /lib/firmware/"
            echo "on the RustOS FAT32 root filesystem."
            echo
            echo "Example:"
            echo "  $0 --drive /dev/sdb"
            echo "  $0 --drive /dev/sdb --ax210-firmware /path/to/linux-firmware"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            echo "Usage: $0 --drive /dev/sdX [--ax210-firmware <file-or-dir>]" >&2
            exit 1
            ;;
    esac
done

if [[ -z "$DRIVE" ]]; then
    echo "Error: --drive is required." >&2
    echo "Usage: $0 --drive /dev/sdX [--ax210-firmware <file-or-dir>]" >&2
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
run_as_root mount -t vfat "$STORAGE_PART" "$MOUNT_TMP"
populate_rootfs_skeleton "$MOUNT_TMP"
provision_ax210_firmware "$MOUNT_TMP" "$AX210_FIRMWARE_SOURCE"
run_as_root umount "$MOUNT_TMP"
rmdir "$MOUNT_TMP"

echo
echo "Done! '$DRIVE' is ready:"
echo "  - Partition 1: RustOS boot partition"
echo "  - Partition 2: FAT32 storage/root filesystem"
echo "Remove the drive safely, then boot your system in UEFI mode."

# Clean up the local image
rm -f "$IMG_FILE"
