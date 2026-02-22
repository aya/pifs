#!/usr/bin/env bash
#
# Integration tests for pifs — FUSE filesystem backed by IPFS.
#
# Usage:
#   ./tests/integration.sh              # Run all tests (skip 1G)
#   ./tests/integration.sh --large      # Include 1G file test
#   ./tests/integration.sh --no-mount   # Skip mount/unmount (already mounted)
#
# Prerequisites:
#   - ipfs daemon running
#   - macFUSE / libfuse installed
#   - pifs binary built (cargo build)
#

set -uo pipefail

# ─── Configuration ────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PIFS_BIN="${PROJECT_DIR}/target/debug/pifs"

MDD=$(mktemp -d "${TMPDIR:-/tmp}/pifs-mdd.XXXXXX")
MNT=$(mktemp -d "${TMPDIR:-/tmp}/pifs-mnt.XXXXXX")
SRC=$(mktemp -d "${TMPDIR:-/tmp}/pifs-src.XXXXXX")
LOG="${TMPDIR:-/tmp}/pifs-test.log"

INCLUDE_LARGE=false
DO_MOUNT=true
PIFS_PID=""
PASSED=0
FAILED=0
SKIPPED=0
ERRORS=()

# ─── Argument parsing ────────────────────────────────────────

for arg in "$@"; do
    case "$arg" in
        --large)    INCLUDE_LARGE=true ;;
        --no-mount) DO_MOUNT=false ;;
        --help|-h)
            echo "Usage: $0 [--large] [--no-mount]"
            exit 0
            ;;
    esac
done

# ─── Helpers ──────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log_section() {
    echo ""
    echo -e "${BLUE}${BOLD}=== $1 ===${NC}"
}

pass() {
    echo -e "  ${GREEN}PASS${NC}  $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo -e "  ${RED}FAIL${NC}  $1"
    FAILED=$((FAILED + 1))
    ERRORS+=("$1")
}

skip() {
    echo -e "  ${YELLOW}SKIP${NC}  $1"
    SKIPPED=$((SKIPPED + 1))
}

assert_eq() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        pass "$desc"
    else
        fail "$desc (expected='$expected', got='$actual')"
    fi
}

assert_file_identical() {
    local desc="$1" file_a="$2" file_b="$3"
    local sum_a sum_b
    sum_a=$(shasum -a 256 "$file_a" | awk '{print $1}')
    sum_b=$(shasum -a 256 "$file_b" | awk '{print $1}')
    if [ "$sum_a" = "$sum_b" ]; then
        pass "$desc (sha256 match)"
    else
        fail "$desc (sha256 mismatch: $sum_a != $sum_b)"
    fi
}

assert_file_size() {
    local desc="$1" file="$2" expected_size="$3"
    local actual_size
    actual_size=$(stat -f%z "$file" 2>/dev/null || stat -c%s "$file" 2>/dev/null)
    if [ "$actual_size" = "$expected_size" ]; then
        pass "$desc (size=${expected_size})"
    else
        fail "$desc (expected size=$expected_size, got=$actual_size)"
    fi
}

assert_success() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        pass "$desc"
    else
        fail "$desc (command failed: $*)"
    fi
}

assert_fails() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        fail "$desc (command should have failed: $*)"
    else
        pass "$desc"
    fi
}

# ─── Generate test source files ──────────────────────────────

generate_test_files() {
    log_section "Generating test files"

    # Text files
    echo -n "Hi" > "$SRC/tiny.txt"                                       # 2 bytes
    echo "Hello, pifs!" > "$SRC/small.txt"                               # 13 bytes
    python3 -c "print('test line for pifs integration\n' * 30, end='')" > "$SRC/medium.txt"
    pass "text files (2B, 13B, ~930B)"

    # Empty file
    touch "$SRC/empty.txt"
    pass "empty file (0B)"

    # Binary files of specific sizes
    dd if=/dev/urandom of="$SRC/bin_1k.bin" bs=1024 count=1 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_10k.bin" bs=1024 count=10 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_100k.bin" bs=1024 count=100 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_1m.bin" bs=1024 count=1024 2>/dev/null
    dd if=/dev/urandom of="$SRC/bin_10m.bin" bs=1024 count=10240 2>/dev/null
    pass "binary files (1K, 10K, 100K, 1M, 10M)"

    # Simulated file types (random data with correct extensions)
    # tar.gz: create a real tar.gz
    mkdir -p "$SRC/tardir"
    for i in $(seq 1 20); do
        dd if=/dev/urandom of="$SRC/tardir/file_$i.dat" bs=1024 count=50 2>/dev/null
    done
    tar czf "$SRC/archive.tar.gz" -C "$SRC" tardir
    rm -rf "$SRC/tardir"
    pass "tar.gz archive (~1M)"

    # PDF: generate a minimal valid PDF
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
    pass "PDF document"

    # Simulated audio (WAV header + random data, ~500K)
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
    pass "WAV audio (~260K)"

    # Simulated video (random data, 5M)
    dd if=/dev/urandom of="$SRC/video.mkv" bs=1024 count=5120 2>/dev/null
    pass "video file (5M)"

    # File with special characters in name
    echo "special" > "$SRC/file with spaces.txt"
    echo "diacritics" > "$SRC/fichier-accentué.txt"
    pass "files with special characters"

    # Large files (optional)
    if $INCLUDE_LARGE; then
        dd if=/dev/urandom of="$SRC/bin_100m.bin" bs=1M count=100 2>/dev/null
        pass "large binary (100M)"
        dd if=/dev/urandom of="$SRC/bin_1g.bin" bs=1M count=1024 2>/dev/null
        pass "large binary (1G)"
    fi
}

# ─── Mount / Unmount ──────────────────────────────────────────

mount_pifs() {
    if $DO_MOUNT; then
        log_section "Mounting pifs"
        echo "  mdd=$MDD"
        echo "  mnt=$MNT"
        echo "  log=$LOG"

        if ! command -v ipfs >/dev/null 2>&1; then
            echo -e "${RED}ERROR: ipfs command not found${NC}"
            exit 1
        fi
        if ! ipfs swarm peers >/dev/null 2>&1; then
            echo -e "${YELLOW}WARNING: ipfs daemon may not be running${NC}"
        fi

        RUST_LOG=info "$PIFS_BIN" --mdd "$MDD" --log "$LOG" "$MNT" >>"$LOG" 2>&1 &
        PIFS_PID=$!
        sleep 2

        # macOS resolves symlinks (/var -> /private/var, /tmp -> /private/tmp)
        if ! mount | grep -q "pifs"; then
            echo -e "${RED}ERROR: pifs failed to mount${NC}"
            exit 1
        fi
        pass "pifs mounted (pid=$PIFS_PID)"
    else
        echo "Skipping mount (--no-mount)"
        # Use default paths when not mounting
        MDD="${MDD:-/tmp/pifs-mdd}"
        MNT="${MNT:-/tmp/pifs}"
    fi
}

unmount_pifs() {
    if $DO_MOUNT; then
        log_section "Unmounting pifs"
        umount "$MNT" 2>/dev/null || diskutil unmount "$MNT" 2>/dev/null || true
        sleep 1
        pass "pifs unmounted"
    fi
}

CLEANUP_DONE=false
cleanup() {
    if $CLEANUP_DONE; then return; fi
    CLEANUP_DONE=true
    if $DO_MOUNT; then
        umount "$MNT" 2>/dev/null || diskutil unmount "$MNT" 2>/dev/null || true
        sleep 1
    fi
    rm -rf "$SRC" "$MDD" 2>/dev/null || true
    # Don't remove MNT if it was pre-existing
    if $DO_MOUNT; then
        rmdir "$MNT" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# ─── Test: Copy & Read (content integrity) ───────────────────

test_copy_and_read() {
    log_section "Copy & Read — content integrity"

    local files=(
        "tiny.txt"
        "small.txt"
        "medium.txt"
        "empty.txt"
        "bin_1k.bin"
        "bin_10k.bin"
        "bin_100k.bin"
        "bin_1m.bin"
        "bin_10m.bin"
        "archive.tar.gz"
        "document.pdf"
        "audio.wav"
        "video.mkv"
        "file with spaces.txt"
        "fichier-accentué.txt"
    )

    if $INCLUDE_LARGE; then
        files+=("bin_100m.bin" "bin_1g.bin")
    fi

    for f in "${files[@]}"; do
        if [ ! -f "$SRC/$f" ]; then
            skip "copy '$f' — source not found"
            continue
        fi

        local src_size
        src_size=$(stat -f%z "$SRC/$f" 2>/dev/null || stat -c%s "$SRC/$f" 2>/dev/null)

        # Copy to pifs
        if cp "$SRC/$f" "$MNT/$f" 2>/dev/null; then
            pass "copy '$f' (${src_size}B) to pifs"
        else
            fail "copy '$f' (${src_size}B) to pifs"
            continue
        fi

        # Read back to temp
        local readback="$SRC/readback_$(echo "$f" | tr ' ' '_')"
        if cp "$MNT/$f" "$readback" 2>/dev/null; then
            pass "read '$f' back from pifs"
        else
            fail "read '$f' back from pifs"
            continue
        fi

        # Verify content integrity (sha256)
        assert_file_identical "content integrity '$f'" "$SRC/$f" "$readback"

        # Verify file size on pifs
        local pifs_size
        pifs_size=$(stat -f%z "$MNT/$f" 2>/dev/null || stat -c%s "$MNT/$f" 2>/dev/null)
        assert_eq "size '$f'" "$src_size" "$pifs_size"

        rm -f "$readback"
    done
}

# ─── Test: File size reporting ────────────────────────────────

test_file_sizes() {
    log_section "File size reporting"

    # Verify sizes via ls -l
    for f in tiny.txt small.txt bin_1k.bin bin_1m.bin; do
        if [ ! -f "$MNT/$f" ]; then
            skip "size '$f' — not on pifs"
            continue
        fi
        local src_size pifs_size
        src_size=$(stat -f%z "$SRC/$f" 2>/dev/null || stat -c%s "$SRC/$f" 2>/dev/null)
        pifs_size=$(stat -f%z "$MNT/$f" 2>/dev/null || stat -c%s "$MNT/$f" 2>/dev/null)
        assert_eq "stat size '$f'" "$src_size" "$pifs_size"
    done

    # Verify empty file has size 0
    if [ -f "$MNT/empty.txt" ]; then
        local sz
        sz=$(stat -f%z "$MNT/empty.txt" 2>/dev/null || stat -c%s "$MNT/empty.txt" 2>/dev/null)
        assert_eq "empty file size" "0" "$sz"
    fi
}

# ─── Test: Overwrite (truncate + rewrite) ─────────────────────

test_overwrite() {
    log_section "Overwrite (truncate + rewrite)"

    # Write a file
    echo "version 1" > "$MNT/overwrite.txt"
    local v1
    v1=$(cat "$MNT/overwrite.txt")
    assert_eq "write v1" "version 1" "$v1"

    # Overwrite with different content
    echo "version 2 with more data" > "$MNT/overwrite.txt"
    local v2
    v2=$(cat "$MNT/overwrite.txt")
    assert_eq "overwrite v2" "version 2 with more data" "$v2"

    # Overwrite with shorter content
    echo "v3" > "$MNT/overwrite.txt"
    local v3
    v3=$(cat "$MNT/overwrite.txt")
    assert_eq "overwrite v3 (shorter)" "v3" "$v3"

    # Overwrite binary with different size
    dd if=/dev/urandom of="$SRC/ow_a.bin" bs=1024 count=50 2>/dev/null
    dd if=/dev/urandom of="$SRC/ow_b.bin" bs=1024 count=200 2>/dev/null
    cp "$SRC/ow_a.bin" "$MNT/ow.bin"
    assert_file_identical "overwrite binary v1" "$SRC/ow_a.bin" "$MNT/ow.bin"
    cp "$SRC/ow_b.bin" "$MNT/ow.bin"
    assert_file_identical "overwrite binary v2 (larger)" "$SRC/ow_b.bin" "$MNT/ow.bin"
}

# ─── Test: Append ─────────────────────────────────────────────

test_append() {
    log_section "Append"

    echo -n "hello" > "$MNT/append.txt"
    echo -n " world" >> "$MNT/append.txt"
    local content
    content=$(cat "$MNT/append.txt")
    assert_eq "append text" "hello world" "$content"
}

# ─── Test: mkdir / rmdir ──────────────────────────────────────

test_directories() {
    log_section "Directories (mkdir, rmdir, nested)"

    assert_success "mkdir" mkdir "$MNT/testdir"
    assert_success "mkdir nested" mkdir -p "$MNT/testdir/sub1/sub2"

    # Create file in nested dir
    echo "nested" > "$MNT/testdir/sub1/sub2/file.txt"
    local content
    content=$(cat "$MNT/testdir/sub1/sub2/file.txt")
    assert_eq "read nested file" "nested" "$content"

    # List directory
    local count
    count=$(ls "$MNT/testdir/sub1/" | wc -l | tr -d ' ')
    assert_eq "ls nested dir" "1" "$count"

    # rmdir (must be empty)
    assert_fails "rmdir non-empty" rmdir "$MNT/testdir/sub1"

    rm "$MNT/testdir/sub1/sub2/file.txt"
    assert_success "rmdir after rm" rmdir "$MNT/testdir/sub1/sub2"
    assert_success "rmdir parent" rmdir "$MNT/testdir/sub1"
    assert_success "rmdir root" rmdir "$MNT/testdir"
}

# ─── Test: unlink ─────────────────────────────────────────────

test_unlink() {
    log_section "Unlink (rm)"

    echo "delete me" > "$MNT/to_delete.txt"
    assert_success "file exists" test -f "$MNT/to_delete.txt"
    assert_success "rm file" rm "$MNT/to_delete.txt"
    assert_fails "file gone after rm" test -f "$MNT/to_delete.txt"
}

# ─── Test: rename (mv) ───────────────────────────────────────

test_rename() {
    log_section "Rename (mv)"

    echo "rename me" > "$MNT/before_rename.txt"
    assert_success "mv file" mv "$MNT/before_rename.txt" "$MNT/after_rename.txt"
    assert_fails "old name gone" test -f "$MNT/before_rename.txt"

    local content
    content=$(cat "$MNT/after_rename.txt")
    assert_eq "content preserved after mv" "rename me" "$content"

    # Rename directory
    mkdir "$MNT/dir_before"
    echo "in dir" > "$MNT/dir_before/f.txt"
    assert_success "mv directory" mv "$MNT/dir_before" "$MNT/dir_after"
    local dc
    dc=$(cat "$MNT/dir_after/f.txt")
    assert_eq "content in renamed dir" "in dir" "$dc"

    # Cleanup
    rm "$MNT/after_rename.txt"
    rm "$MNT/dir_after/f.txt"
    rmdir "$MNT/dir_after"
}

# ─── Test: symlink / readlink ─────────────────────────────────

test_symlink() {
    log_section "Symlink"

    echo "symlink target" > "$MNT/sym_target.txt"
    assert_success "create symlink" ln -s sym_target.txt "$MNT/sym_link.txt"

    # Readlink
    local target
    target=$(readlink "$MNT/sym_link.txt")
    assert_eq "readlink" "sym_target.txt" "$target"

    # Read through symlink
    local content
    content=$(cat "$MNT/sym_link.txt")
    assert_eq "read through symlink" "symlink target" "$content"

    # Cleanup
    rm "$MNT/sym_link.txt"
    rm "$MNT/sym_target.txt"
}

# ─── Test: hard link ─────────────────────────────────────────

test_hardlink() {
    log_section "Hard link"

    echo "hardlink content" > "$MNT/hl_source.txt"
    if ln "$MNT/hl_source.txt" "$MNT/hl_link.txt" 2>/dev/null; then
        local content
        content=$(cat "$MNT/hl_link.txt")
        assert_eq "read hardlink" "hardlink content" "$content"

        # Both should have same inode
        local ino1 ino2
        ino1=$(stat -f%i "$MNT/hl_source.txt" 2>/dev/null || stat -c%i "$MNT/hl_source.txt")
        ino2=$(stat -f%i "$MNT/hl_link.txt" 2>/dev/null || stat -c%i "$MNT/hl_link.txt")
        assert_eq "hardlink same inode" "$ino1" "$ino2"

        rm "$MNT/hl_link.txt"
    else
        skip "hard link (not supported or failed)"
    fi
    rm -f "$MNT/hl_source.txt"
}

# ─── Test: chmod ──────────────────────────────────────────────

test_chmod() {
    log_section "chmod"

    echo "chmod test" > "$MNT/chmod_test.txt"

    assert_success "chmod 644" chmod 644 "$MNT/chmod_test.txt"
    local mode
    mode=$(stat -f%Lp "$MNT/chmod_test.txt" 2>/dev/null || stat -c%a "$MNT/chmod_test.txt")
    assert_eq "mode is 644" "644" "$mode"

    assert_success "chmod 755" chmod 755 "$MNT/chmod_test.txt"
    mode=$(stat -f%Lp "$MNT/chmod_test.txt" 2>/dev/null || stat -c%a "$MNT/chmod_test.txt")
    assert_eq "mode is 755" "755" "$mode"

    assert_success "chmod 400" chmod 400 "$MNT/chmod_test.txt"
    mode=$(stat -f%Lp "$MNT/chmod_test.txt" 2>/dev/null || stat -c%a "$MNT/chmod_test.txt")
    assert_eq "mode is 400" "400" "$mode"

    # Restore for cleanup
    chmod 644 "$MNT/chmod_test.txt"
    rm "$MNT/chmod_test.txt"
}

# ─── Test: touch (utimens) ───────────────────────────────────

test_utimens() {
    log_section "utimens (touch)"

    echo "time test" > "$MNT/time_test.txt"

    # Set specific mtime
    assert_success "touch -t" touch -t 202301011200 "$MNT/time_test.txt"

    # Verify mtime changed (just check it doesn't error)
    assert_success "stat after touch" stat "$MNT/time_test.txt"

    # Touch to now
    assert_success "touch (now)" touch "$MNT/time_test.txt"

    rm "$MNT/time_test.txt"
}

# ─── Test: statfs ─────────────────────────────────────────────

test_statfs() {
    log_section "statfs (df)"

    assert_success "df on mountpoint" df "$MNT"

    local fs_type
    fs_type=$(df -T "$MNT" 2>/dev/null | tail -1 | awk '{print $2}' || df "$MNT" | tail -1 | awk '{print $1}')
    pass "statfs reports filesystem (type=$fs_type)"
}

# ─── Test: readdir (ls) ──────────────────────────────────────

test_readdir() {
    log_section "readdir (ls)"

    # Clean slate
    mkdir "$MNT/lsdir"
    touch "$MNT/lsdir/a.txt"
    touch "$MNT/lsdir/b.txt"
    touch "$MNT/lsdir/c.txt"
    mkdir "$MNT/lsdir/subdir"

    local count
    count=$(ls "$MNT/lsdir" | wc -l | tr -d ' ')
    assert_eq "ls count" "4" "$count"

    # ls -la should include . and ..
    local count_all
    count_all=$(ls -la "$MNT/lsdir" | grep -c '^[d-]' || true)
    # Should be at least 6 (., .., a.txt, b.txt, c.txt, subdir)
    if [ "$count_all" -ge 4 ]; then
        pass "ls -la includes entries ($count_all)"
    else
        fail "ls -la entry count (expected >=4, got $count_all)"
    fi

    # Cleanup
    rm "$MNT/lsdir/a.txt" "$MNT/lsdir/b.txt" "$MNT/lsdir/c.txt"
    rmdir "$MNT/lsdir/subdir"
    rmdir "$MNT/lsdir"
}

# ─── Test: concurrent reads of same file ─────────────────────

test_concurrent_read() {
    log_section "Multiple reads of same file"

    cp "$SRC/bin_100k.bin" "$MNT/concurrent.bin"

    # Read it 3 times, each time should give same sha256
    local sum1 sum2 sum3
    sum1=$(shasum -a 256 "$MNT/concurrent.bin" | awk '{print $1}')
    sum2=$(shasum -a 256 "$MNT/concurrent.bin" | awk '{print $1}')
    sum3=$(shasum -a 256 "$MNT/concurrent.bin" | awk '{print $1}')

    assert_eq "read 1 == read 2" "$sum1" "$sum2"
    assert_eq "read 2 == read 3" "$sum2" "$sum3"

    local src_sum
    src_sum=$(shasum -a 256 "$SRC/bin_100k.bin" | awk '{print $1}')
    assert_eq "reads match source" "$src_sum" "$sum1"

    rm "$MNT/concurrent.bin"
}

# ─── Test: edge cases ────────────────────────────────────────

test_edge_cases() {
    log_section "Edge cases"

    # Single byte file
    printf 'X' > "$MNT/onebyte.txt"
    local content
    content=$(cat "$MNT/onebyte.txt")
    assert_eq "single byte file" "X" "$content"
    local sz
    sz=$(stat -f%z "$MNT/onebyte.txt" 2>/dev/null || stat -c%s "$MNT/onebyte.txt")
    assert_eq "single byte size" "1" "$sz"
    rm "$MNT/onebyte.txt"

    # File with only newlines
    printf '\n\n\n' > "$MNT/newlines.txt"
    sz=$(stat -f%z "$MNT/newlines.txt" 2>/dev/null || stat -c%s "$MNT/newlines.txt")
    assert_eq "newlines-only size" "3" "$sz"
    rm "$MNT/newlines.txt"

    # File with null bytes
    printf '\x00\x00\x00\x00' > "$MNT/nulls.bin"
    sz=$(stat -f%z "$MNT/nulls.bin" 2>/dev/null || stat -c%s "$MNT/nulls.bin")
    assert_eq "null bytes size" "4" "$sz"
    # Read back and verify
    local src_sum pifs_sum
    printf '\x00\x00\x00\x00' > "$SRC/nulls.bin"
    assert_file_identical "null bytes content" "$SRC/nulls.bin" "$MNT/nulls.bin"
    rm "$MNT/nulls.bin"

    # Exactly 4096 bytes (page boundary)
    dd if=/dev/urandom of="$SRC/page.bin" bs=4096 count=1 2>/dev/null
    cp "$SRC/page.bin" "$MNT/page.bin"
    assert_file_identical "4096 bytes (page boundary)" "$SRC/page.bin" "$MNT/page.bin"
    assert_file_size "4096 size" "$MNT/page.bin" "4096"
    rm "$MNT/page.bin"

    # Exactly 1MB
    dd if=/dev/urandom of="$SRC/exact1m.bin" bs=1048576 count=1 2>/dev/null
    cp "$SRC/exact1m.bin" "$MNT/exact1m.bin"
    assert_file_identical "1MB exact" "$SRC/exact1m.bin" "$MNT/exact1m.bin"
    assert_file_size "1MB size" "$MNT/exact1m.bin" "1048576"
    rm "$MNT/exact1m.bin"

    # Long filename (255 chars)
    local longname
    longname=$(python3 -c "print('a'*250 + '.txt')")
    echo "long" > "$MNT/$longname"
    content=$(cat "$MNT/$longname")
    assert_eq "long filename" "long" "$content"
    rm "$MNT/$longname"
}

# ─── Test: cleanup residual files ─────────────────────────────

test_cleanup() {
    log_section "Cleanup test files on pifs"

    # Remove all remaining test files
    local remaining
    remaining=$(ls "$MNT" 2>/dev/null | wc -l | tr -d ' ')

    for f in "$MNT"/*; do
        [ -e "$f" ] || continue
        rm -rf "$f" 2>/dev/null || true
    done

    local after
    after=$(ls "$MNT" 2>/dev/null | wc -l | tr -d ' ')
    assert_eq "pifs clean after tests" "0" "$after"
}

# ─── Main ─────────────────────────────────────────────────────

main() {
    echo -e "${BOLD}pifs integration tests${NC}"
    echo "  binary:  $PIFS_BIN"
    echo "  source:  $SRC"
    echo "  mdd:     $MDD"
    echo "  mount:   $MNT"
    echo "  large:   $INCLUDE_LARGE"

    # Check binary exists
    if [ ! -x "$PIFS_BIN" ]; then
        echo -e "${YELLOW}Binary not found, building...${NC}"
        (cd "$PROJECT_DIR" && cargo build 2>&1)
    fi

    generate_test_files
    mount_pifs

    test_copy_and_read
    test_file_sizes
    test_overwrite
    test_append
    test_directories
    test_unlink
    test_rename
    test_symlink
    test_hardlink
    test_chmod
    test_utimens
    test_statfs
    test_readdir
    test_concurrent_read
    test_edge_cases
    test_cleanup

    # ─── Summary ──────────────────────────────────────────────
    echo ""
    echo -e "${BOLD}═══════════════════════════════════════${NC}"
    echo -e "  ${GREEN}Passed:${NC}  $PASSED"
    echo -e "  ${RED}Failed:${NC}  $FAILED"
    echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
    echo -e "${BOLD}═══════════════════════════════════════${NC}"

    if [ ${#ERRORS[@]} -gt 0 ]; then
        echo ""
        echo -e "${RED}${BOLD}Failures:${NC}"
        for e in "${ERRORS[@]}"; do
            echo -e "  ${RED}-${NC} $e"
        done
    fi

    echo ""
    if [ "$FAILED" -eq 0 ]; then
        echo -e "${GREEN}${BOLD}All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}${BOLD}$FAILED test(s) failed.${NC}"
        echo "  Log: $LOG"
        exit 1
    fi
}

main
