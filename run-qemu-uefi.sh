#!/usr/bin/env bash
# run-qemu-uefi.sh — UEFI QEMU runner for RustOS test binaries and the main kernel.
#
# Usage (via .cargo/config.toml runner):
#   run-qemu-uefi.sh <kernel-elf> [extra-qemu-args...]
#
# The script:
#   1. Builds a UEFI disk image from the kernel ELF using crates/create-image.
#   2. Launches qemu-system-x86_64 with OVMF firmware.

set -e

BINARY="$1"
shift

REPO_ROOT="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")" && pwd)"
TMPDIR_WORK="$(mktemp -d)"
IMG="$TMPDIR_WORK/rustos.img"

cleanup() {
    rm -rf "$TMPDIR_WORK"
}
trap cleanup EXIT

# Build UEFI disk image from the kernel ELF
(
    cd "$REPO_ROOT/crates/create-image"
    cargo run \
        --quiet \
        -- "$BINARY" "$IMG"
)

# Locate OVMF firmware (path varies by distro)
OVMF=""
for candidate in \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/ovmf/OVMF.fd \
    /usr/share/edk2/ovmf/OVMF_CODE.fd \
    /usr/share/qemu/OVMF.fd; do
    if [ -f "$candidate" ]; then
        OVMF="$candidate"
        break
    fi
done

if [ -z "$OVMF" ]; then
    echo "Error: OVMF firmware not found. Install it with: sudo apt-get install ovmf" >&2
    exit 1
fi

exec qemu-system-x86_64 \
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
    -drive "format=raw,file=$IMG" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -serial stdio \
    -display none \
    "$@"
