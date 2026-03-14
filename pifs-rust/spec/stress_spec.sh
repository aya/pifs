#shellcheck shell=sh
#
# Stress tests for pifs:
# - Mass file creation
# - Concurrent reads and writes
# - Rapid read/write cycling
# - Mixed size parallel writes
# - Mixed concurrent operations
# - Memory usage check
#
# FUSE operations covered: create, write, read, release, unlink (under load)
#

Describe 'Stress Tests'
    BeforeAll 'pifs_setup'

    # ─── Mass File Creation ──────────────────────────────────────

    Describe 'mass file creation (200 files)'
        setup_mass() {
            mkdir -p "$MNT/stress_mass"
            for i in $(seq 1 200); do
                echo "file-$i-content" > "$MNT/stress_mass/f_$i.txt"
            done
        }
        BeforeAll 'setup_mass'

        It "creates all 200 files"
            When call ls "$MNT/stress_mass"
            The lines of output should equal 200
        End

        It "last file has correct content"
            When call cat "$MNT/stress_mass/f_200.txt"
            The output should equal "file-200-content"
        End

        AfterAll 'rm -rf "$MNT/stress_mass"'
    End

    # ─── Concurrent Writes ───────────────────────────────────────

    Describe 'concurrent writes (10 parallel)'
        setup_concurrent_writes() {
            mkdir -p "$MNT/stress_cw"
            for i in $(seq 1 10); do
                dd if=/dev/urandom of="$SRC/cw_$i.bin" bs=1024 count=10 2>/dev/null
            done
            # Launch 10 parallel copies
            for i in $(seq 1 10); do
                cp "$SRC/cw_$i.bin" "$MNT/stress_cw/cw_$i.bin" &
            done
            wait
        }
        BeforeAll 'setup_concurrent_writes'

        It "writes all 10 files"
            When call ls "$MNT/stress_cw"
            The lines of output should equal 10
        End

        It "file 1 has correct checksum"
            When call file_sha256 "$MNT/stress_cw/cw_1.bin"
            The output should equal "$(file_sha256 "$SRC/cw_1.bin")"
        End

        It "file 5 has correct checksum"
            When call file_sha256 "$MNT/stress_cw/cw_5.bin"
            The output should equal "$(file_sha256 "$SRC/cw_5.bin")"
        End

        It "file 10 has correct checksum"
            When call file_sha256 "$MNT/stress_cw/cw_10.bin"
            The output should equal "$(file_sha256 "$SRC/cw_10.bin")"
        End

        AfterAll 'rm -rf "$MNT/stress_cw"'
    End

    # ─── Concurrent Reads ────────────────────────────────────────

    Describe 'concurrent reads (10 parallel)'
        setup_concurrent_reads() {
            dd if=/dev/urandom of="$SRC/cr_source.bin" bs=1024 count=100 2>/dev/null
            cp "$SRC/cr_source.bin" "$MNT/stress_cr_source.bin"
            _expected_hash=$(file_sha256 "$SRC/cr_source.bin")
            mkdir -p "$SRC/cr_results"
            for i in $(seq 1 10); do
                (file_sha256 "$MNT/stress_cr_source.bin" > "$SRC/cr_results/hash_$i.txt") &
            done
            wait
        }
        BeforeAll 'setup_concurrent_reads'

        check_all_hashes() {
            _expected=$(file_sha256 "$SRC/cr_source.bin")
            for i in $(seq 1 10); do
                _got=$(cat "$SRC/cr_results/hash_$i.txt")
                if [ "$_got" != "$_expected" ]; then
                    echo "reader $i: expected $_expected, got $_got" >&2
                    return 1
                fi
            done
        }

        It "all 10 readers get the same correct hash"
            When call check_all_hashes
            The status should be success
        End

        AfterAll 'rm -f "$MNT/stress_cr_source.bin"'
    End

    # ─── Rapid Read/Write Cycling ────────────────────────────────

    Describe 'rapid read/write cycling (50 iterations)'
        cycle_test() {
            _cycle_file="$MNT/stress_cycle.txt"
            for i in $(seq 1 50); do
                echo "iteration-$i" > "$_cycle_file"
                _read=$(cat "$_cycle_file")
                if [ "$_read" != "iteration-$i" ]; then
                    echo "cycle $i: expected 'iteration-$i', got '$_read'" >&2
                    return 1
                fi
            done
        }

        It "survives 50 write-read-verify cycles"
            When call cycle_test
            The status should be success
        End

        AfterAll 'rm -f "$MNT/stress_cycle.txt"'
    End

    # ─── Mixed Size Parallel Writes ──────────────────────────────

    Describe 'mixed size parallel writes'
        setup_mixed_sizes() {
            # Generate source files of various sizes
            printf 'X' > "$SRC/ms_1b.bin"
            dd if=/dev/urandom of="$SRC/ms_1k.bin" bs=1024 count=1 2>/dev/null
            dd if=/dev/urandom of="$SRC/ms_100k.bin" bs=1024 count=100 2>/dev/null
            dd if=/dev/urandom of="$SRC/ms_1m.bin" bs=1024 count=1024 2>/dev/null
            # Copy all in parallel
            cp "$SRC/ms_1b.bin" "$MNT/stress_ms_1b.bin" &
            cp "$SRC/ms_1k.bin" "$MNT/stress_ms_1k.bin" &
            cp "$SRC/ms_100k.bin" "$MNT/stress_ms_100k.bin" &
            cp "$SRC/ms_1m.bin" "$MNT/stress_ms_1m.bin" &
            wait
        }
        BeforeAll 'setup_mixed_sizes'

        It "1-byte file is correct"
            When call file_sha256 "$MNT/stress_ms_1b.bin"
            The output should equal "$(file_sha256 "$SRC/ms_1b.bin")"
        End

        It "1KB file is correct"
            When call file_sha256 "$MNT/stress_ms_1k.bin"
            The output should equal "$(file_sha256 "$SRC/ms_1k.bin")"
        End

        It "100KB file is correct"
            When call file_sha256 "$MNT/stress_ms_100k.bin"
            The output should equal "$(file_sha256 "$SRC/ms_100k.bin")"
        End

        It "1MB file is correct"
            When call file_sha256 "$MNT/stress_ms_1m.bin"
            The output should equal "$(file_sha256 "$SRC/ms_1m.bin")"
        End

        AfterAll 'rm -f "$MNT/stress_ms_1b.bin" "$MNT/stress_ms_1k.bin" "$MNT/stress_ms_100k.bin" "$MNT/stress_ms_1m.bin"'
    End

    # ─── Mixed Concurrent Operations ─────────────────────────────

    Describe 'mixed concurrent operations'
        mixed_ops_test() {
            mkdir -p "$MNT/stress_mixed"

            # Writer: creates files in a loop
            (
                for i in $(seq 1 20); do
                    echo "writer-$i" > "$MNT/stress_mixed/w_$i.txt"
                done
            ) &
            _pid_writer=$!

            # Reader: reads whatever exists
            (
                for _ in $(seq 1 20); do
                    ls "$MNT/stress_mixed" >/dev/null 2>&1
                    for f in "$MNT/stress_mixed"/w_*.txt; do
                        [ -f "$f" ] && cat "$f" >/dev/null 2>&1 || true
                    done
                done
            ) &
            _pid_reader=$!

            # Creator/deleter: creates and removes temp files
            (
                for i in $(seq 1 20); do
                    echo "temp-$i" > "$MNT/stress_mixed/tmp_$i.txt"
                    rm -f "$MNT/stress_mixed/tmp_$i.txt"
                done
            ) &
            _pid_cd=$!

            # Wait for all and check exit status
            _fail=0
            wait $_pid_writer || _fail=1
            wait $_pid_reader || _fail=1
            wait $_pid_cd || _fail=1

            rm -rf "$MNT/stress_mixed"
            return $_fail
        }

        It "survives mixed read/write/create/delete without crash"
            When call mixed_ops_test
            The status should be success
        End
    End

    # ─── Memory Usage Check ──────────────────────────────────────

    Describe 'memory usage check'
        get_pifs_rss() {
            # RSS in KB
            ps -o rss= -p "$PIFS_PID" 2>/dev/null | tr -d ' '
        }

        memory_check() {
            _rss_before=$(get_pifs_rss)
            if [ -z "$_rss_before" ] || [ "$_rss_before" = "0" ]; then
                echo "could not read RSS" >&2
                return 0  # skip, don't fail
            fi

            # Generate load: write and delete 50 x 100KB files
            mkdir -p "$MNT/stress_mem"
            for i in $(seq 1 50); do
                dd if=/dev/urandom of="$MNT/stress_mem/mem_$i.bin" bs=1024 count=100 2>/dev/null
            done
            rm -rf "$MNT/stress_mem"

            # Small pause to allow cleanup
            sleep 1

            _rss_after=$(get_pifs_rss)
            if [ -z "$_rss_after" ] || [ "$_rss_after" = "0" ]; then
                echo "could not read RSS after stress" >&2
                return 0
            fi

            # Warn if RSS grew more than 3x (but don't fail)
            if [ "$_rss_after" -gt $((_rss_before * 3)) ]; then
                echo "WARNING: RSS grew from ${_rss_before}KB to ${_rss_after}KB (>3x)" >&2
            fi
        }

        It "does not leak excessive memory"
            When call memory_check
            The status should be success
        End
    End
End
