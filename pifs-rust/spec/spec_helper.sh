#shellcheck shell=sh
#
# ShellSpec spec_helper for pifs integration tests
#
# Setup/teardown is done ONCE here at load time, not per-spec-file.
# Setup is done ONCE at load time. Teardown via shellspec_after_all.
# Spec files call pifs_setup in BeforeAll to load state into subshells.
#

# ─── Configuration ────────────────────────────────────────────

set -u

SPEC_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SPEC_DIR/.." && pwd)"
PIFS_BIN="${PROJECT_DIR}/target/debug/pifs"

# Test options
export INCLUDE_LARGE="${INCLUDE_LARGE:-false}"
export DO_MOUNT="${DO_MOUNT:-true}"

# State file for communicating dirs between shellspec processes
PIFS_STATE_FILE="${TMPDIR:-/tmp}/pifs-shellspec-state"

# Temp directories
export MDD=""
export MNT=""
export SRC=""
export LOG=""
export PIFS_PID=""

# ─── Helper functions ─────────────────────────────────────────

# Get file size (cross-platform)
file_size() {
    stat -f%z "$1" 2>/dev/null || stat -c%s "$1" 2>/dev/null
}

# Get file mode (cross-platform)
file_mode() {
    stat -f%Lp "$1" 2>/dev/null || stat -c%a "$1" 2>/dev/null
}

# Get file inode (cross-platform)
file_inode() {
    stat -f%i "$1" 2>/dev/null || stat -c%i "$1" 2>/dev/null
}

# Calculate SHA256 hash
file_sha256() {
    shasum -a 256 "$1" | awk '{print $1}'
}

# Check if two files are identical by SHA256
files_identical() {
    local sum_a sum_b
    sum_a=$(file_sha256 "$1")
    sum_b=$(file_sha256 "$2")
    [ "$sum_a" = "$sum_b" ]
}

# ─── Test file generation ─────────────────────────────────────

generate_test_files() {
    # Text files
    printf 'Hi' > "$SRC/tiny.txt"                                       # 2 bytes
    echo "Hello, pifs!" > "$SRC/small.txt"                              # 13 bytes
    python3 -c "print('test line for pifs integration\n' * 30, end='')" > "$SRC/medium.txt"

    # Empty file
    touch "$SRC/empty.txt"

    # Binary files of specific sizes
    dd if=/dev/urandom of="$SRC/bin_1k.bin" bs=1024 count=1 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_10k.bin" bs=1024 count=10 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_100k.bin" bs=1024 count=100 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_1m.bin" bs=1024 count=1024 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_10m.bin" bs=1024 count=10240 2>/dev/null

    # tar.gz archive
    mkdir -p "$SRC/tardir"
    for i in $(seq 1 20); do
        dd if=/dev/urandom of="$SRC/tardir/file_$i.dat" bs=1024 count=50 2>/dev/null
    done
    tar czf "$SRC/archive.tar.gz" -C "$SRC" tardir
    rm -rf "$SRC/tardir"

    # Minimal valid PDF
    cat > "$SRC/document.pdf" << 'PDFEOF'
%PDF-1.0
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]
   /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length 44 >>
stream
BT /F1 24 Tf 100 700 Td (Hello pifs!) Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
0000000266 00000 n
0000000360 00000 n
trailer
<< /Size 6 /Root 1 0 R >>
startxref
441
%%EOF
PDFEOF

    # Simulated audio (WAV header + random data)
    python3 -c "
import struct, sys, os
sr=44100; ch=1; bps=16; dur=3
data_size = sr * ch * (bps//8) * dur
sys.stdout.buffer.write(b'RIFF')
sys.stdout.buffer.write(struct.pack('<I', 36 + data_size))
sys.stdout.buffer.write(b'WAVE')
sys.stdout.buffer.write(b'fmt ')
sys.stdout.buffer.write(struct.pack('<IHHIIHH', 16, 1, ch, sr, sr*ch*(bps//8), ch*(bps//8), bps))
sys.stdout.buffer.write(b'data')
sys.stdout.buffer.write(struct.pack('<I', data_size))
sys.stdout.buffer.write(os.urandom(data_size))
" > "$SRC/audio.wav"

    # Simulated video
    dd if=/dev/urandom of="$SRC/video.mkv" bs=1024 count=5120 2>/dev/null

    # Files with special characters in name
    echo "special" > "$SRC/file with spaces.txt"
    echo "diacritics" > "$SRC/fichier-accentué.txt"

    # Large files (optional)
    if [ "$INCLUDE_LARGE" = "true" ]; then
        dd if=/dev/urandom of="$SRC/bin_100m.bin" bs=1M count=100 2>/dev/null
        dd if=/dev/urandom of="$SRC/bin_1g.bin" bs=1M count=1024 2>/dev/null
    fi
}

# ─── Mount / Unmount ──────────────────────────────────────────

mount_pifs() {
    if [ "$DO_MOUNT" = "true" ]; then
        if ! command -v ipfs >/dev/null 2>&1; then
            echo "ERROR: ipfs command not found" >&2
            return 1
        fi

        RUST_LOG=info "$PIFS_BIN" --mdd "$MDD" --log "$LOG" "$MNT" >>"$LOG" 2>&1 &
        PIFS_PID=$!
        sleep 2

        if ! mount | grep -q "pifs"; then
            echo "ERROR: pifs failed to mount" >&2
            return 1
        fi
    fi
}

unmount_pifs() {
    if [ "$DO_MOUNT" = "true" ]; then
        umount "$MNT" 2>/dev/null || diskutil unmount "$MNT" 2>/dev/null || true
        sleep 1
    fi
}

cleanup_temp_dirs() {
    if [ "$DO_MOUNT" = "true" ]; then
        umount "$MNT" 2>/dev/null || diskutil unmount "$MNT" 2>/dev/null || true
        sleep 1
    fi
    rm -rf "$SRC" "$MDD" 2>/dev/null || true
    if [ "$DO_MOUNT" = "true" ]; then
        rmdir "$MNT" 2>/dev/null || true
    fi
}

# ─── Assertion helpers ────────────────────────────────────────

# Assert file has specific size
assert_file_size() {
    local file="$1" expected="$2"
    local actual
    actual=$(file_size "$file")
    if [ "$actual" = "$expected" ]; then
        return 0
    else
        echo "expected size: $expected, actual: $actual" >&2
        return 1
    fi
}

# Assert file has specific mode
assert_file_mode() {
    local file="$1" expected="$2"
    local actual
    actual=$(file_mode "$file")
    if [ "$actual" = "$expected" ]; then
        return 0
    else
        echo "expected mode: $expected, actual: $actual" >&2
        return 1
    fi
}

# ─── File operation helpers ───────────────────────────────────

# Copy file from SRC to MNT and verify integrity
copy_and_verify() {
    local f="$1"
    [ -f "$SRC/$f" ] || { echo "source not found: $SRC/$f" >&2; return 1; }
    cp "$SRC/$f" "$MNT/$f" || { echo "copy failed: $f" >&2; return 1; }
    files_identical "$SRC/$f" "$MNT/$f"
}

# Read bytes at offset from file (cross-platform)
# Usage: read_at_offset <file> <offset> <length>
read_at_offset() {
    local file="$1" offset="$2" length="$3"
    dd if="$file" bs=1 skip="$offset" count="$length" 2>/dev/null
}

# Write bytes at offset to file (cross-platform)
# Usage: write_at_offset <file> <offset> <data>
# Note: data is passed via stdin or as string
write_at_offset() {
    local file="$1" offset="$2"
    shift 2
    if [ $# -gt 0 ]; then
        printf '%s' "$1" | dd of="$file" bs=1 seek="$offset" conv=notrunc 2>/dev/null
    else
        dd of="$file" bs=1 seek="$offset" conv=notrunc 2>/dev/null
    fi
}

# Read bytes at offset and return hex representation
read_hex_at_offset() {
    local file="$1" offset="$2" length="$3"
    dd if="$file" bs=1 skip="$offset" count="$length" 2>/dev/null | xxd -p | tr -d '\n'
}

# Write hex bytes at offset
# Usage: write_hex_at_offset <file> <offset> <hex_string>
write_hex_at_offset() {
    local file="$1" offset="$2" hex="$3"
    printf '%s' "$hex" | xxd -r -p | dd of="$file" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

# Compare specific byte range between two files
# Usage: compare_range <file1> <file2> <offset> <length>
compare_range() {
    local file1="$1" file2="$2" offset="$3" length="$4"
    local data1 data2
    data1=$(read_hex_at_offset "$file1" "$offset" "$length")
    data2=$(read_hex_at_offset "$file2" "$offset" "$length")
    [ "$data1" = "$data2" ]
}

# ─── Global Setup (runs once when spec_helper is loaded) ─────

_pifs_global_setup() {
    if [ -f "$PIFS_STATE_FILE" ]; then
        # Already set up by a previous load — just restore state
        # shellcheck disable=SC1090
        . "$PIFS_STATE_FILE"
        return 0
    fi

    MDD=$(mktemp -d "${TMPDIR:-/tmp}/pifs-mdd.XXXXXX")
    MNT=$(mktemp -d "${TMPDIR:-/tmp}/pifs-mnt.XXXXXX")
    SRC=$(mktemp -d "${TMPDIR:-/tmp}/pifs-src.XXXXXX")
    LOG="${TMPDIR:-/tmp}/pifs-test.log"

    # Build if needed
    if [ ! -x "$PIFS_BIN" ]; then
        (cd "$PROJECT_DIR" && cargo build 2>&1) || return 1
    fi

    generate_test_files
    mount_pifs

    # Save state so child processes / reloads can find the dirs
    cat > "$PIFS_STATE_FILE" << EOF
export MDD="$MDD"
export MNT="$MNT"
export SRC="$SRC"
export LOG="$LOG"
export PIFS_PID="$PIFS_PID"
EOF
}

_pifs_global_setup

# ─── Global Teardown (ShellSpec calls this once after ALL specs) ──

shellspec_after_all() {
    if [ -f "$PIFS_STATE_FILE" ]; then
        # shellcheck disable=SC1090
        . "$PIFS_STATE_FILE"
        rm -f "$PIFS_STATE_FILE"
        cleanup_temp_dirs
    fi
}

# ─── No-op stubs (spec files still call these via BeforeAll/AfterAll) ──

pifs_setup() {
    # State already loaded at helper init; re-source in case of subshell
    if [ -f "$PIFS_STATE_FILE" ]; then
        # shellcheck disable=SC1090
        . "$PIFS_STATE_FILE"
    fi
}

# Force cleanup function (can be called manually)
pifs_force_cleanup() {
    rm -f "$PIFS_STATE_FILE"
    cleanup_temp_dirs
}
