#!/bin/bash
# create-test-disk.sh — Create a proper test disk image with GPT partition 2 as FAT32
#
# Usage: create-test-disk.sh <kernel-elf> <output-img> [size-mb]

set -e

KERNEL_ELF="$1"
OUTPUT_IMG="$2"
SIZE_MB="${3:-1024}"

if [[ ! -f "$KERNEL_ELF" ]]; then
    echo "Error: kernel ELF not found: $KERNEL_ELF" >&2
    exit 1
fi

echo "Creating test disk image..."

# Step 1: Create basic UEFI image using the build tool
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

cd "$(dirname "$0")"
./crates/create-image/target/x86_64-unknown-linux-gnu/release/create-image \
    "$KERNEL_ELF" "$TMPDIR/base.img"

# Step 2: Create a larger disk image with both partitions
dd if=/dev/zero of="$OUTPUT_IMG" bs=1M count=$SIZE_MB 2>/dev/null

# Step 3: Copy UEFI partition from base image to output
UEFI_SIZE=$(stat -c%s "$TMPDIR/base.img" 2>/dev/null || stat -f%z "$TMPDIR/base.img" 2>/dev/null)
dd if="$TMPDIR/base.img" of="$OUTPUT_IMG" bs=1 count=$UEFI_SIZE conv=notrunc 2>/dev/null

# Step 4: Initialize as GPT disk (if possible, using sgdisk if available)
if command -v sgdisk &>/dev/null; then
    echo "Expanding GPT to full disk size..."
    sgdisk -e "$OUTPUT_IMG" 2>/dev/null || true
    
    # Calculate partition 2 location (skip first 10 MB for boot partition)
    BOOT_SECTORS=$((10 * 1024 * 1024 / 512))
    TOTAL_SECTORS=$((SIZE_MB * 1024 * 1024 / 512))
    PART2_START=$BOOT_SECTORS
    PART2_END=$((TOTAL_SECTORS - 34))  # Leave room for backup GPT
    
    echo "Adding FAT32 partition (sectors $PART2_START to $PART2_END)..."
    sgdisk -n 2:$PART2_START:$PART2_END -t 2:0700 -c 2:"rustos-storage" "$OUTPUT_IMG" 2>/dev/null || true
fi

echo "Created test disk image: $OUTPUT_IMG ($SIZE_MB MB)"
echo ""
echo "To use with QEMU:"
echo "  qemu-system-x86_64 ... -drive format=raw,file=$OUTPUT_IMG ..."
