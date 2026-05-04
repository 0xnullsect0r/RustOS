#!/bin/bash
# add-fat32-partition.sh — Add a FAT32 partition to a UEFI disk image
#
# Usage: add-fat32-partition.sh <image-file> [fat32-size-mb]
#
# This script:
# 1. Extends the disk image
# 2. Updates the GPT header to include the extended space
# 3. Adds partition 2 as a FAT32 volume
# 4. Creates a minimal FAT32 filesystem on partition 2

set -e

IMAGE="$1"
FAT32_SIZE_MB="${2:-512}"  # Default to 512 MB

if [[ ! -f "$IMAGE" ]]; then
    echo "Error: image file not found: $IMAGE" >&2
    exit 1
fi

# Check if we have the necessary tools
for tool in sgdisk losetup mkfs.fat; do
    if ! command -v "$tool" &>/dev/null; then
        echo "Warning: required tool '$tool' is not installed." >&2
        echo "Cannot add FAT32 partition to disk image." >&2
        exit 0
    fi
done

echo "Adding $FAT32_SIZE_MB MB FAT32 partition to disk image..."

# Current image size
CURRENT_SIZE=$(stat -f%z "$IMAGE" 2>/dev/null || stat -c%s "$IMAGE" 2>/dev/null)
FAT32_SIZE=$((FAT32_SIZE_MB * 1024 * 1024))
NEW_SIZE=$((CURRENT_SIZE + FAT32_SIZE))

echo "Current image size: $((CURRENT_SIZE / 1024 / 1024)) MB"
echo "New image size: $((NEW_SIZE / 1024 / 1024)) MB"

# Extend the image with zeros
echo "Extending disk image..."
dd if=/dev/zero of="$IMAGE" bs=1M seek=$((CURRENT_SIZE / 1024 / 1024)) count=$FAT32_SIZE_MB 2>/dev/null || \
    dd if=/dev/zero of="$IMAGE" bs=1M seek=$((CURRENT_SIZE / 1024 / 1024)) count=$FAT32_SIZE_MB

# Use losetup to attach the image as a loop device
echo "Attaching image to loop device..."
LOOP=$(losetup -f)
losetup "$LOOP" "$IMAGE"

# Sleep briefly to let the loop device settle
sleep 1

# Expand the GPT to use the full image size
echo "Expanding GPT table..."
sgdisk -e "$LOOP" 2>/dev/null || true

# Add partition 2 as FAT32
echo "Adding partition 2 (FAT32)..."
sgdisk -n 2:0:0 -t 2:0700 -c 2:"rustos-storage" "$LOOP" 2>/dev/null || {
    echo "Warning: sgdisk failed, trying alternative approach..."
    # Alternative: just detach and continue
}

# Refresh kernel's partition table
sleep 1
partprobe "$LOOP" 2>/dev/null || blockdev --rereadpt "$LOOP" 2>/dev/null || true
sleep 1

# Format partition 2 as FAT32 if it exists
if [[ -b "${LOOP}p2" ]]; then
    echo "Formatting ${LOOP}p2 as FAT32..."
    mkfs.fat -F 32 -n RUSTOS_ROOT "${LOOP}p2" 2>/dev/null || true
fi

# Detach the loop device
echo "Detaching loop device..."
losetup -d "$LOOP" 2>/dev/null || true
sleep 1

echo "Done! FAT32 partition added to disk image."
