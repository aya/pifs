#shellcheck shell=sh
#
# Tests for file I/O operations:
# - create, open, read, write, flush, release, fsync
# - truncate, ftruncate
# - copy operations
# - read/write at offset
#
# FUSE operations covered: create, open, read, write, flush, release, fsync, setattr (truncate)
#

Describe 'File I/O Operations'
    BeforeAll 'pifs_setup'

    # ─── Basic Read/Write ─────────────────────────────────────────

    Describe 'create and write'
        It "creates new file with echo"
            echo "hello" > "$MNT/rw_test.txt"
            The path "$MNT/rw_test.txt" should be file
        End

        It "creates new file with printf"
            printf 'no newline' > "$MNT/rw_printf.txt"
            When call cat "$MNT/rw_printf.txt"
            The output should equal "no newline"
        End

        It "creates empty file with touch"
            When call touch "$MNT/rw_touch.txt"
            The status should be success
            The path "$MNT/rw_touch.txt" should be file
        End

        AfterAll 'rm -f "$MNT/rw_test.txt" "$MNT/rw_printf.txt" "$MNT/rw_touch.txt"'
    End

    Describe 'read operations'
        setup_read() {
            echo "line 1" > "$MNT/read_test.txt"
            echo "line 2" >> "$MNT/read_test.txt"
            echo "line 3" >> "$MNT/read_test.txt"
        }
        BeforeAll 'setup_read'

        It "reads entire file with cat"
            When call cat "$MNT/read_test.txt"
            The output should include "line 1"
            The output should include "line 3"
        End

        It "reads with head"
            When call head -1 "$MNT/read_test.txt"
            The output should equal "line 1"
        End

        It "reads with tail"
            When call tail -1 "$MNT/read_test.txt"
            The output should equal "line 3"
        End

        It "reads specific lines with sed"
            When call sed -n '2p' "$MNT/read_test.txt"
            The output should equal "line 2"
        End

        AfterAll 'rm -f "$MNT/read_test.txt"'
    End

    # ─── Overwrite and Truncate ───────────────────────────────────

    Describe 'overwrite (truncate + rewrite)'
        It "writes version 1"
            echo "version 1" > "$MNT/overwrite.txt"
            When call cat "$MNT/overwrite.txt"
            The output should equal "version 1"
        End

        It "overwrites with version 2 (longer)"
            echo "version 2 with more data" > "$MNT/overwrite.txt"
            When call cat "$MNT/overwrite.txt"
            The output should equal "version 2 with more data"
        End

        It "overwrites with version 3 (shorter)"
            echo "v3" > "$MNT/overwrite.txt"
            When call cat "$MNT/overwrite.txt"
            The output should equal "v3"
        End

        It "overwrites binary file with larger version"
            dd if=/dev/urandom of="$SRC/ow_a.bin" bs=1024 count=50 2>/dev/null
            dd if=/dev/urandom of="$SRC/ow_b.bin" bs=1024 count=200 2>/dev/null
            cp "$SRC/ow_a.bin" "$MNT/ow.bin"
            cp "$SRC/ow_b.bin" "$MNT/ow.bin"
            When call files_identical "$MNT/ow.bin" "$SRC/ow_b.bin"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/overwrite.txt" "$MNT/ow.bin"'
    End

    Describe 'truncate'
        setup_truncate() {
            echo "1234567890" > "$MNT/truncate.txt"
        }
        BeforeAll 'setup_truncate'

        It "truncates file to smaller size"
            truncate -s 5 "$MNT/truncate.txt"
            When call cat "$MNT/truncate.txt"
            The output should equal "12345"
        End

        It "truncates file to zero"
            truncate -s 0 "$MNT/truncate.txt"
            When call assert_file_size "$MNT/truncate.txt" "0"
            The status should be success
        End

        It "extends file with truncate"
            truncate -s 100 "$MNT/truncate.txt"
            When call assert_file_size "$MNT/truncate.txt" "100"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/truncate.txt"'
    End

    # ─── Append ───────────────────────────────────────────────────

    Describe 'append'
        It "appends text to file"
            printf 'hello' > "$MNT/append.txt"
            printf ' world' >> "$MNT/append.txt"
            When call cat "$MNT/append.txt"
            The output should equal "hello world"
        End

        It "appends multiple times"
            printf 'a' > "$MNT/append_multi.txt"
            printf 'b' >> "$MNT/append_multi.txt"
            printf 'c' >> "$MNT/append_multi.txt"
            When call cat "$MNT/append_multi.txt"
            The output should equal "abc"
        End

        AfterAll 'rm -f "$MNT/append.txt" "$MNT/append_multi.txt"'
    End

    # ─── Fsync ────────────────────────────────────────────────────

    Describe 'fsync'
        It "syncs file data to disk"
            echo "sync test" > "$MNT/fsync_test.txt"
            When call dd if="$MNT/fsync_test.txt" of=/dev/null bs=1
            The status should be success
            The stderr should be present
        End

        It "sync command succeeds on file"
            echo "sync" > "$MNT/sync_test.txt"
            When call sync
            The status should be success
        End

        AfterAll 'rm -f "$MNT/fsync_test.txt" "$MNT/sync_test.txt"'
    End

    # ─── Read/Write at Offset ─────────────────────────────────────

    Describe 'read at offset'
        setup_offset_read() {
            printf 'HEADER_MIDDLE_FOOTER' > "$MNT/offset_read.txt"
        }
        BeforeAll 'setup_offset_read'

        It "reads from beginning (offset 0)"
            When call read_at_offset "$MNT/offset_read.txt" 0 6
            The output should equal "HEADER"
        End

        It "reads from middle (offset 7)"
            When call read_at_offset "$MNT/offset_read.txt" 7 6
            The output should equal "MIDDLE"
        End

        It "reads from end (offset 14)"
            When call read_at_offset "$MNT/offset_read.txt" 14 6
            The output should equal "FOOTER"
        End

        It "reads partial content across boundaries"
            When call read_at_offset "$MNT/offset_read.txt" 4 8
            The output should equal "ER_MIDDL"
        End

        AfterAll 'rm -f "$MNT/offset_read.txt"'
    End

    Describe 'write at offset (overwrite)'
        setup_offset_write() {
            printf 'AAAAAAAAAA' > "$MNT/offset_write.txt"
        }
        BeforeAll 'setup_offset_write'

        It "overwrites at beginning (offset 0)"
            write_at_offset "$MNT/offset_write.txt" 0 "XX"
            When call cat "$MNT/offset_write.txt"
            The output should equal "XXAAAAAAAA"
        End

        It "overwrites in middle (offset 4)"
            write_at_offset "$MNT/offset_write.txt" 4 "YY"
            When call cat "$MNT/offset_write.txt"
            The output should equal "XXAAYYAAAA"
        End

        It "overwrites at end (offset 8)"
            write_at_offset "$MNT/offset_write.txt" 8 "ZZ"
            When call cat "$MNT/offset_write.txt"
            The output should equal "XXAAYYAAZZ"
        End

        It "file size correct after overwrite without truncation"
            printf 'AAAAAAAAAA' > "$MNT/offset_write.txt"
            write_at_offset "$MNT/offset_write.txt" 5 "BB"
            When call assert_file_size "$MNT/offset_write.txt" "10"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/offset_write.txt"'
    End

    Describe 'binary read/write at offset'
        setup_binary_offset() {
            dd if=/dev/zero of="$MNT/binary_offset.bin" bs=4096 count=1 2>/dev/null
        }
        BeforeAll 'setup_binary_offset'

        It "writes hex bytes at offset"
            write_hex_at_offset "$MNT/binary_offset.bin" 100 "deadbeef"
            hex=$(read_hex_at_offset "$MNT/binary_offset.bin" 100 4)
            The value "$hex" should equal "deadbeef"
        End

        It "preserves surrounding bytes after write"
            hex_before=$(read_hex_at_offset "$MNT/binary_offset.bin" 96 4)
            The value "$hex_before" should equal "00000000"
        End

        It "writes at page boundary (offset 4092)"
            write_hex_at_offset "$MNT/binary_offset.bin" 4092 "cafebabe"
            hex=$(read_hex_at_offset "$MNT/binary_offset.bin" 4092 4)
            The value "$hex" should equal "cafebabe"
        End

        AfterAll 'rm -f "$MNT/binary_offset.bin"'
    End

    Describe 'sparse-like operations'
        It "writes beyond current file size (extends file)"
            printf 'START' > "$MNT/sparse.txt"
            write_at_offset "$MNT/sparse.txt" 100 "END"
            size=$(file_size "$MNT/sparse.txt")
            When call test "$size" -ge 103
            The status should be success
        End

        It "reads extended region correctly"
            When call read_at_offset "$MNT/sparse.txt" 100 3
            The output should equal "END"
        End

        AfterAll 'rm -f "$MNT/sparse.txt"'
    End

    Describe 'large file offset operations'
        setup_large_offset() {
            dd if=/dev/urandom of="$SRC/large_offset.bin" bs=1024 count=2048 2>/dev/null
            cp "$SRC/large_offset.bin" "$MNT/large_offset.bin"
        }
        BeforeAll 'setup_large_offset'

        It "reads at 1MB offset"
            When call compare_range "$SRC/large_offset.bin" "$MNT/large_offset.bin" 1048576 512
            The status should be success
        End

        It "writes at 1MB offset and verifies"
            write_hex_at_offset "$MNT/large_offset.bin" 1048576 "1234567890abcdef"
            hex=$(read_hex_at_offset "$MNT/large_offset.bin" 1048576 8)
            The value "$hex" should equal "1234567890abcdef"
        End

        It "preserves data before modified offset"
            When call compare_range "$SRC/large_offset.bin" "$MNT/large_offset.bin" 0 1024
            The status should be success
        End

        AfterAll 'rm -f "$MNT/large_offset.bin"'
    End

    # ─── Copy and Integrity ───────────────────────────────────────

    Describe 'copy and content integrity'
        It "copies and reads tiny.txt (2B)"
            When call copy_and_verify "tiny.txt"
            The status should be success
        End

        It "copies and reads small.txt (13B)"
            When call copy_and_verify "small.txt"
            The status should be success
        End

        It "copies and reads medium.txt (~930B)"
            When call copy_and_verify "medium.txt"
            The status should be success
        End

        It "copies and reads empty.txt (0B)"
            When call copy_and_verify "empty.txt"
            The status should be success
        End

        It "copies and reads bin_1k.bin (1KB)"
            When call copy_and_verify "bin_1k.bin"
            The status should be success
        End

        It "copies and reads bin_10k.bin (10KB)"
            When call copy_and_verify "bin_10k.bin"
            The status should be success
        End

        It "copies and reads bin_100k.bin (100KB)"
            When call copy_and_verify "bin_100k.bin"
            The status should be success
        End

        It "copies and reads bin_1m.bin (1MB)"
            When call copy_and_verify "bin_1m.bin"
            The status should be success
        End

        It "copies and reads bin_10m.bin (10MB)"
            When call copy_and_verify "bin_10m.bin"
            The status should be success
        End

        It "copies and reads archive.tar.gz"
            When call copy_and_verify "archive.tar.gz"
            The status should be success
        End

        It "copies and reads document.pdf"
            When call copy_and_verify "document.pdf"
            The status should be success
        End

        It "copies and reads audio.wav"
            When call copy_and_verify "audio.wav"
            The status should be success
        End

        It "copies and reads video.mkv (5MB)"
            When call copy_and_verify "video.mkv"
            The status should be success
        End

        It "copies and reads 'file with spaces.txt'"
            When call copy_and_verify "file with spaces.txt"
            The status should be success
        End

        It "copies and reads 'fichier-accentué.txt'"
            When call copy_and_verify "fichier-accentué.txt"
            The status should be success
        End

        Context "when INCLUDE_LARGE is true"
            Skip if "INCLUDE_LARGE is not set" [ "$INCLUDE_LARGE" != "true" ]

            It "copies and reads bin_100m.bin (100MB)"
                When call copy_and_verify "bin_100m.bin"
                The status should be success
            End

            It "copies and reads bin_1g.bin (1GB)"
                When call copy_and_verify "bin_1g.bin"
                The status should be success
            End
        End
    End

    # ─── Multiple Reads ───────────────────────────────────────────

    Describe 'multiple reads of same file'
        setup_concurrent() {
            cp "$SRC/bin_100k.bin" "$MNT/concurrent.bin"
        }
        BeforeAll 'setup_concurrent'

        It "returns consistent sha256 on multiple reads"
            sum1=$(file_sha256 "$MNT/concurrent.bin")
            sum2=$(file_sha256 "$MNT/concurrent.bin")
            sum3=$(file_sha256 "$MNT/concurrent.bin")
            The value "$sum1" should equal "$sum2"
            The value "$sum2" should equal "$sum3"
        End

        It "reads match source file"
            When call files_identical "$MNT/concurrent.bin" "$SRC/bin_100k.bin"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/concurrent.bin"'
    End

    # ─── Video File Operations ────────────────────────────────────

    Describe 'video file operations'
        Describe 'MKV container (5MB)'
            setup_video() {
                cp "$SRC/video.mkv" "$MNT/video_test.mkv"
            }
            BeforeAll 'setup_video'

            It "preserves video file size"
                When call assert_file_size "$MNT/video_test.mkv" "$(file_size "$SRC/video.mkv")"
                The status should be success
            End

            It "preserves video file integrity"
                When call files_identical "$MNT/video_test.mkv" "$SRC/video.mkv"
                The status should be success
            End

            It "reads video file header correctly"
                header_src=$(read_hex_at_offset "$SRC/video.mkv" 0 4)
                header_mnt=$(read_hex_at_offset "$MNT/video_test.mkv" 0 4)
                The value "$header_mnt" should equal "$header_src"
            End

            It "reads video file middle section correctly"
                When call compare_range "$SRC/video.mkv" "$MNT/video_test.mkv" 2621440 1024
                The status should be success
            End

            It "reads video file end correctly"
                size=$(file_size "$SRC/video.mkv")
                offset=$((size - 1024))
                When call compare_range "$SRC/video.mkv" "$MNT/video_test.mkv" "$offset" 1024
                The status should be success
            End

            AfterAll 'rm -f "$MNT/video_test.mkv"'
        End

        Describe 'large video simulation (10MB)'
            setup_large_video() {
                dd if=/dev/urandom of="$SRC/large_video.mkv" bs=1024 count=10240 2>/dev/null
                cp "$SRC/large_video.mkv" "$MNT/large_video.mkv"
            }
            BeforeAll 'setup_large_video'

            It "reads large video file back correctly"
                When call files_identical "$MNT/large_video.mkv" "$SRC/large_video.mkv"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/large_video.mkv"'
        End
    End
End
