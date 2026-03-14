#shellcheck shell=sh
#
# ShellSpec spec_helper for pifs integration tests
#
# pifs_setup is called via BeforeAll in each spec file.
# The first caller mounts pifs and writes a state file; subsequent
# callers load state and verify the mount is still alive.
# Cleanup is best-effort: kill pifs process + remove temp dirs.
# pifs uses AutoUnmount so the FUSE mount disappears when pifs exits.
#

# ─── Configuration ────────────────────────────────────────────

set -u

spec_helper_configure() {
    after_all 'pifs_cleanup'
}

PROJECT_DIR="${SHELLSPEC_PROJECT_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
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

        if ! "$PIFS_BIN" --version >/dev/null 2>&1; then
            echo "ERROR: pifs binary not executable: $PIFS_BIN" >&2
            return 1
        fi

        # Sentinel file: FUSE mount hides underlying dir contents, so when
        # the sentinel disappears we know FUSE is mounted over the dir.
        touch "$MNT/.pifs_pre_mount_sentinel"

        # Start pifs in the background
        # Close fds 3-9 to prevent inheriting ShellSpec's internal pipes,
        # which would prevent ShellSpec from detecting EOF and exiting.
        "$PIFS_BIN" --mdd "$MDD" --log "$LOG" "$MNT" \
            </dev/null >>"$LOG" 2>&1 3>&- 4>&- 5>&- 6>&- 7>&- 8>&- 9>&- &
        PIFS_PID=$!
        disown "$PIFS_PID" 2>/dev/null || true

        # Poll until the FUSE mount is live (up to 10s)
        # Avoid `mount | grep` — it does statfs on ALL mounts and hangs
        # if any dead FUSE mount exists from a previous run.
        _retries=0
        while [ "$_retries" -lt 20 ]; do
            # Sentinel gone = FUSE mounted over the directory
            if [ ! -f "$MNT/.pifs_pre_mount_sentinel" ]; then
                break
            fi
            # Bail early if the process died
            if ! kill -0 "$PIFS_PID" 2>/dev/null; then
                echo "ERROR: pifs process exited before mount was ready" >&2
                echo "  log: $(tail -5 "$LOG" 2>/dev/null)" >&2
                return 1
            fi
            sleep 0.5
            _retries=$((_retries + 1))
        done

        if [ "$_retries" -ge 20 ]; then
            echo "ERROR: pifs failed to mount after 10s" >&2
            echo "  log: $(tail -5 "$LOG" 2>/dev/null)" >&2
            kill "$PIFS_PID" 2>/dev/null || true
            return 1
        fi

        # Smoke test: create and remove a file to confirm FUSE ops work
        if ! echo "mount-check" > "$MNT/.pifs_mount_test" 2>/dev/null; then
            echo "ERROR: pifs mounted but write test failed" >&2
            return 1
        fi
        rm -f "$MNT/.pifs_mount_test" 2>/dev/null
    fi
}

unmount_pifs() {
    if [ "$DO_MOUNT" = "true" ] && [ -n "$PIFS_PID" ]; then
        # Kill pifs process; AutoUnmount handles the FUSE unmount
        kill "$PIFS_PID" 2>/dev/null || true
        _w=0
        while kill -0 "$PIFS_PID" 2>/dev/null && [ "$_w" -lt 10 ]; do
            sleep 0.5
            _w=$((_w + 1))
        done
    fi
}

cleanup_temp_dirs() {
    if [ "$DO_MOUNT" = "true" ] && [ -n "$PIFS_PID" ]; then
        # Kill pifs; AutoUnmount makes macFUSE unmount automatically
        kill "$PIFS_PID" 2>/dev/null || true
        # Wait for process to exit (up to 5s)
        _w=0
        while kill -0 "$PIFS_PID" 2>/dev/null && [ "$_w" -lt 10 ]; do
            sleep 0.5
            _w=$((_w + 1))
        done
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

# ─── Setup / Teardown ────────────────────────────────────────
#
# pifs_setup is called via BeforeAll in each spec file.
# Uses an atomic lock (mkdir) to ensure only one process does
# the actual mount + file generation. Others wait and load state.
#

PIFS_LOCK_DIR="${TMPDIR:-/tmp}/pifs-shellspec-lock"

pifs_setup() {
    # Fast path: state file already exists — just load it
    if [ -f "$PIFS_STATE_FILE" ]; then
        # shellcheck disable=SC1090
        . "$PIFS_STATE_FILE"
        # Verify mount is still alive
        if [ "$DO_MOUNT" = "true" ]; then
            if ! kill -0 "$PIFS_PID" 2>/dev/null; then
                # Stale state from a previous run — clean up and re-setup
                rm -f "$PIFS_STATE_FILE"
                rmdir "$PIFS_LOCK_DIR" 2>/dev/null || true
                rm -rf "$SRC" "$MDD" 2>/dev/null || true
                rmdir "$MNT" 2>/dev/null || true
                # Fall through to fresh setup below
            else
                # Process alive — verify mount is responsive
                if ! stat "$MNT/." >/dev/null 2>&1; then
                    echo "ERROR: pifs mount not responding (MNT=$MNT)" >&2
                    return 1
                fi
                return 0
            fi
        else
            return 0
        fi
    fi

    # Atomic lock: only one process gets to do setup
    if mkdir "$PIFS_LOCK_DIR" 2>/dev/null; then
        # We hold the lock — do the actual setup
        # Use a shared suffix so mdd/mnt/src are easily associated
        _pifs_suffix=$(mktemp -d "${TMPDIR:-/tmp}/pifs-mdd.XXXXXX")
        _pifs_suffix="${_pifs_suffix##*.}"
        MDD="${TMPDIR:-/tmp}/pifs-mdd.$_pifs_suffix"
        MNT="${TMPDIR:-/tmp}/pifs-mnt.$_pifs_suffix"
        SRC="${TMPDIR:-/tmp}/pifs-src.$_pifs_suffix"
        mkdir -p "$MNT" "$SRC"
        LOG="${TMPDIR:-/tmp}/pifs-test.$_pifs_suffix.log"

        # Resolve real paths (macOS: /var -> /private/var)
        MNT=$(cd "$MNT" && pwd -P)
        MDD=$(cd "$MDD" && pwd -P)
        SRC=$(cd "$SRC" && pwd -P)

        # Build if needed
        if [ ! -x "$PIFS_BIN" ]; then
            (cd "$PROJECT_DIR" && cargo build 2>&1) || { rmdir "$PIFS_LOCK_DIR"; return 1; }
        fi

        generate_test_files
        mount_pifs || { rmdir "$PIFS_LOCK_DIR"; return 1; }

        # Save state so other spec files and subshells can find the dirs
        cat > "$PIFS_STATE_FILE" << EOF
export MDD="$MDD"
export MNT="$MNT"
export SRC="$SRC"
export LOG="$LOG"
export PIFS_PID="$PIFS_PID"
EOF
    else
        # Another process is doing setup — wait for state file
        _wait=0
        while [ ! -f "$PIFS_STATE_FILE" ] && [ "$_wait" -lt 60 ]; do
            sleep 1
            _wait=$((_wait + 1))
        done
        if [ ! -f "$PIFS_STATE_FILE" ]; then
            echo "ERROR: timed out waiting for pifs setup (60s)" >&2
            return 1
        fi
        # shellcheck disable=SC1090
        . "$PIFS_STATE_FILE"
    fi
}

# Cleanup: kill pifs, remove temp dirs and state files.
# Called manually or via pifs_force_cleanup.
pifs_cleanup() {
    if [ -f "$PIFS_STATE_FILE" ]; then
        # shellcheck disable=SC1090
        . "$PIFS_STATE_FILE"
        rm -f "$PIFS_STATE_FILE"
        cleanup_temp_dirs
    fi
    rmdir "$PIFS_LOCK_DIR" 2>/dev/null || true
}

# Force cleanup function (can be called manually, e.g. after interrupted test runs)
pifs_force_cleanup() {
    rm -f "$PIFS_STATE_FILE"
    rmdir "$PIFS_LOCK_DIR" 2>/dev/null || true
    cleanup_temp_dirs
}
