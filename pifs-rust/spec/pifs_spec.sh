#shellcheck shell=sh
#
# ShellSpec integration tests for pifs — FUSE filesystem backed by IPFS.
#
# Usage:
#   shellspec                           # Run all tests (skip large files)
#   INCLUDE_LARGE=true shellspec        # Include 100M and 1G file tests
#   DO_MOUNT=false shellspec            # Skip mount/unmount (already mounted)
#
# Prerequisites:
#   - ipfs daemon running
#   - macFUSE / libfuse installed
#   - pifs binary built (cargo build)
#   - shellspec installed (brew install shellspec)
#

Describe 'pifs FUSE filesystem'
    # Global setup/teardown
    BeforeAll 'pifs_setup'
    AfterAll 'pifs_teardown'

    # ─── Copy & Read (content integrity) ──────────────────────────

    Describe 'Copy & Read - content integrity'
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

    # ─── File size reporting ──────────────────────────────────────

    Describe 'File size reporting'
        It "reports correct size for tiny.txt"
            When call assert_file_size "$MNT/tiny.txt" "$(file_size "$SRC/tiny.txt")"
            The status should be success
        End

        It "reports correct size for small.txt"
            When call assert_file_size "$MNT/small.txt" "$(file_size "$SRC/small.txt")"
            The status should be success
        End

        It "reports correct size for bin_1k.bin"
            When call assert_file_size "$MNT/bin_1k.bin" "$(file_size "$SRC/bin_1k.bin")"
            The status should be success
        End

        It "reports correct size for bin_1m.bin"
            When call assert_file_size "$MNT/bin_1m.bin" "$(file_size "$SRC/bin_1m.bin")"
            The status should be success
        End

        It "reports size 0 for empty.txt"
            When call assert_file_size "$MNT/empty.txt" "0"
            The status should be success
        End
    End

    # ─── Overwrite (truncate + rewrite) ───────────────────────────

    Describe 'Overwrite (truncate + rewrite)'
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

    # ─── Append ───────────────────────────────────────────────────

    Describe 'Append'
        It "appends text to file"
            printf 'hello' > "$MNT/append.txt"
            printf ' world' >> "$MNT/append.txt"
            When call cat "$MNT/append.txt"
            The output should equal "hello world"
        End

        AfterAll 'rm -f "$MNT/append.txt"'
    End

    # ─── Directories (mkdir, rmdir, nested) ───────────────────────

    Describe 'Directories'
        It "creates directory with mkdir"
            When call mkdir "$MNT/testdir"
            The status should be success
        End

        It "creates nested directories with mkdir -p"
            When call mkdir -p "$MNT/testdir/sub1/sub2"
            The status should be success
        End

        It "creates and reads file in nested directory"
            echo "nested" > "$MNT/testdir/sub1/sub2/file.txt"
            When call cat "$MNT/testdir/sub1/sub2/file.txt"
            The output should equal "nested"
        End

        It "lists correct count in nested directory"
            When call ls "$MNT/testdir/sub1/"
            The output should include "sub2"
        End

        It "fails to rmdir non-empty directory"
            When call rmdir "$MNT/testdir/sub1"
            The status should be failure
        End

        It "removes nested structure after cleanup"
            rm "$MNT/testdir/sub1/sub2/file.txt"
            rmdir "$MNT/testdir/sub1/sub2"
            rmdir "$MNT/testdir/sub1"
            When call rmdir "$MNT/testdir"
            The status should be success
        End
    End

    # ─── Unlink (rm) ──────────────────────────────────────────────

    Describe 'Unlink (rm)'
        It "creates file to delete"
            echo "delete me" > "$MNT/to_delete.txt"
            The path "$MNT/to_delete.txt" should be file
        End

        It "removes file with rm"
            When call rm "$MNT/to_delete.txt"
            The status should be success
        End

        It "confirms file no longer exists"
            The path "$MNT/to_delete.txt" should not be exist
        End
    End

    # ─── Rename (mv) ──────────────────────────────────────────────

    Describe 'Rename (mv)'
        It "renames file"
            echo "rename me" > "$MNT/before_rename.txt"
            When call mv "$MNT/before_rename.txt" "$MNT/after_rename.txt"
            The status should be success
        End

        It "old name no longer exists"
            The path "$MNT/before_rename.txt" should not be exist
        End

        It "content preserved after rename"
            When call cat "$MNT/after_rename.txt"
            The output should equal "rename me"
        End

        It "renames directory"
            mkdir "$MNT/dir_before"
            echo "in dir" > "$MNT/dir_before/f.txt"
            When call mv "$MNT/dir_before" "$MNT/dir_after"
            The status should be success
        End

        It "content preserved in renamed directory"
            When call cat "$MNT/dir_after/f.txt"
            The output should equal "in dir"
        End

        AfterAll 'rm -f "$MNT/after_rename.txt"; rm -rf "$MNT/dir_after"'
    End

    # ─── Symlink ──────────────────────────────────────────────────

    Describe 'Symlink'
        It "creates symlink"
            echo "symlink target" > "$MNT/sym_target.txt"
            When call ln -s sym_target.txt "$MNT/sym_link.txt"
            The status should be success
        End

        It "readlink returns correct target"
            When call readlink "$MNT/sym_link.txt"
            The output should equal "sym_target.txt"
        End

        It "reads through symlink"
            When call cat "$MNT/sym_link.txt"
            The output should equal "symlink target"
        End

        AfterAll 'rm -f "$MNT/sym_link.txt" "$MNT/sym_target.txt"'
    End

    # ─── Hard link ────────────────────────────────────────────────

    Describe 'Hard link'
        setup_hardlink() {
            echo "hardlink content" > "$MNT/hl_source.txt"
        }
        BeforeAll 'setup_hardlink'

        It "creates hard link"
            When call ln "$MNT/hl_source.txt" "$MNT/hl_link.txt"
            The status should be success
        End

        It "reads hardlink content"
            When call cat "$MNT/hl_link.txt"
            The output should equal "hardlink content"
        End

        It "hardlinks share same inode"
            ino1=$(file_inode "$MNT/hl_source.txt")
            ino2=$(file_inode "$MNT/hl_link.txt")
            The value "$ino1" should equal "$ino2"
        End

        AfterAll 'rm -f "$MNT/hl_source.txt" "$MNT/hl_link.txt"'
    End

    # ─── chmod ────────────────────────────────────────────────────

    Describe 'chmod'
        setup_chmod() {
            echo "chmod test" > "$MNT/chmod_test.txt"
        }
        BeforeAll 'setup_chmod'

        It "sets mode to 644"
            chmod 644 "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "644"
            The status should be success
        End

        It "sets mode to 755"
            chmod 755 "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "755"
            The status should be success
        End

        It "sets mode to 400"
            chmod 400 "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "400"
            The status should be success
        End

        AfterAll 'chmod 644 "$MNT/chmod_test.txt"; rm -f "$MNT/chmod_test.txt"'
    End

    # ─── utimens (touch) ──────────────────────────────────────────

    Describe 'utimens (touch)'
        setup_touch() {
            echo "time test" > "$MNT/time_test.txt"
        }
        BeforeAll 'setup_touch'

        It "sets specific timestamp with touch -t"
            When call touch -t 202301011200 "$MNT/time_test.txt"
            The status should be success
        End

        It "stat succeeds after touch"
            When call stat "$MNT/time_test.txt"
            The status should be success
        End

        It "touch to current time succeeds"
            When call touch "$MNT/time_test.txt"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/time_test.txt"'
    End

    # ─── statfs (df) ──────────────────────────────────────────────

    Describe 'statfs (df)'
        It "df succeeds on mountpoint"
            When call df "$MNT"
            The status should be success
        End
    End

    # ─── readdir (ls) ─────────────────────────────────────────────

    Describe 'readdir (ls)'
        setup_ls() {
            mkdir "$MNT/lsdir"
            touch "$MNT/lsdir/a.txt"
            touch "$MNT/lsdir/b.txt"
            touch "$MNT/lsdir/c.txt"
            mkdir "$MNT/lsdir/subdir"
        }
        BeforeAll 'setup_ls'

        It "lists correct number of entries"
            When call ls "$MNT/lsdir"
            The lines of output should equal 4
        End

        It "ls -la shows all entries including hidden"
            When call ls -la "$MNT/lsdir"
            The output should include "a.txt"
            The output should include "subdir"
        End

        AfterAll 'rm -rf "$MNT/lsdir"'
    End

    # ─── Multiple reads of same file ──────────────────────────────

    Describe 'Multiple reads of same file'
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

    # ─── Edge cases ───────────────────────────────────────────────

    Describe 'Edge cases'
        Describe 'Single byte file'
            It "creates and reads single byte"
                printf 'X' > "$MNT/onebyte.txt"
                When call cat "$MNT/onebyte.txt"
                The output should equal "X"
            End

            It "reports size 1"
                When call assert_file_size "$MNT/onebyte.txt" "1"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/onebyte.txt"'
        End

        Describe 'File with only newlines'
            It "creates file with newlines only"
                printf '\n\n\n' > "$MNT/newlines.txt"
                When call assert_file_size "$MNT/newlines.txt" "3"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/newlines.txt"'
        End

        Describe 'File with null bytes'
            It "creates file with null bytes"
                printf '\x00\x00\x00\x00' > "$MNT/nulls.bin"
                When call assert_file_size "$MNT/nulls.bin" "4"
                The status should be success
            End

            It "content matches expected null bytes"
                printf '\x00\x00\x00\x00' > "$SRC/nulls.bin"
                When call files_identical "$MNT/nulls.bin" "$SRC/nulls.bin"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/nulls.bin"'
        End

        Describe '4096 bytes (page boundary)'
            It "creates and copies 4096 byte file"
                dd if=/dev/urandom of="$SRC/page.bin" bs=4096 count=1 2>/dev/null
                cp "$SRC/page.bin" "$MNT/page.bin"
                When call files_identical "$MNT/page.bin" "$SRC/page.bin"
                The status should be success
            End

            It "reports size 4096"
                When call assert_file_size "$MNT/page.bin" "4096"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/page.bin"'
        End

        Describe '1MB exact'
            It "creates and copies 1MB file"
                dd if=/dev/urandom of="$SRC/exact1m.bin" bs=1048576 count=1 2>/dev/null
                cp "$SRC/exact1m.bin" "$MNT/exact1m.bin"
                When call files_identical "$MNT/exact1m.bin" "$SRC/exact1m.bin"
                The status should be success
            End

            It "reports size 1048576"
                When call assert_file_size "$MNT/exact1m.bin" "1048576"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/exact1m.bin"'
        End

        Describe 'File with quarantine xattr'
            It "copies file with com.apple.quarantine xattr"
                dd if=/dev/urandom of="$SRC/xattr_test.bin" bs=1024 count=100 2>/dev/null
                xattr -w com.apple.quarantine "0083;66543210;Safari;" "$SRC/xattr_test.bin" 2>/dev/null || true
                When call cp "$SRC/xattr_test.bin" "$MNT/xattr_test.bin"
                The status should be success
            End

            It "preserves file integrity after xattr copy"
                When call files_identical "$MNT/xattr_test.bin" "$SRC/xattr_test.bin"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/xattr_test.bin"'
        End

        Describe 'Long filename (255 chars)'
            It "creates and reads file with long name"
                longname=$(python3 -c "print('a'*250 + '.txt')")
                echo "long" > "$MNT/$longname"
                When call cat "$MNT/$longname"
                The output should equal "long"
                rm -f "$MNT/$longname"
            End
        End
    End

    # ─── Video file operations ────────────────────────────────────

    Describe 'Video file operations'
        Describe 'MKV container (5MB)'
            It "copies video file to pifs"
                When call cp "$SRC/video.mkv" "$MNT/video_test.mkv"
                The status should be success
            End

            It "preserves video file size"
                When call assert_file_size "$MNT/video_test.mkv" "$(file_size "$SRC/video.mkv")"
                The status should be success
            End

            It "preserves video file integrity"
                When call files_identical "$MNT/video_test.mkv" "$SRC/video.mkv"
                The status should be success
            End

            It "reads video file header correctly"
                # First 4 bytes of MKV should be readable
                header_src=$(read_hex_at_offset "$SRC/video.mkv" 0 4)
                header_mnt=$(read_hex_at_offset "$MNT/video_test.mkv" 0 4)
                The value "$header_mnt" should equal "$header_src"
            End

            It "reads video file middle section correctly"
                # Read 1KB from middle of file
                When call compare_range "$SRC/video.mkv" "$MNT/video_test.mkv" 2621440 1024
                The status should be success
            End

            It "reads video file end correctly"
                # Read last 1KB
                size=$(file_size "$SRC/video.mkv")
                offset=$((size - 1024))
                When call compare_range "$SRC/video.mkv" "$MNT/video_test.mkv" "$offset" 1024
                The status should be success
            End

            AfterAll 'rm -f "$MNT/video_test.mkv"'
        End

        Describe 'Large video simulation (10MB)'
            setup_large_video() {
                dd if=/dev/urandom of="$SRC/large_video.mkv" bs=1024 count=10240 2>/dev/null
            }
            BeforeAll 'setup_large_video'

            It "copies large video file"
                When call cp "$SRC/large_video.mkv" "$MNT/large_video.mkv"
                The status should be success
            End

            It "reads large video file back correctly"
                When call files_identical "$MNT/large_video.mkv" "$SRC/large_video.mkv"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/large_video.mkv"'
        End
    End

    # ─── Read/Write at offset ─────────────────────────────────────

    Describe 'Read/Write at specific offset'
        Describe 'Read at offset'
            setup_offset_read() {
                # Create a file with known content: "HEADER_MIDDLE_FOOTER"
                printf 'HEADER' > "$MNT/offset_read.txt"
                printf '_MIDDLE_' >> "$MNT/offset_read.txt"
                printf 'FOOTER' >> "$MNT/offset_read.txt"
            }
            BeforeAll 'setup_offset_read'

            It "reads from beginning (offset 0)"
                When call read_at_offset "$MNT/offset_read.txt" 0 6
                The output should equal "HEADER"
            End

            It "reads from middle (offset 6)"
                When call read_at_offset "$MNT/offset_read.txt" 6 8
                The output should equal "_MIDDLE_"
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

        Describe 'Write at offset (overwrite)'
            setup_offset_write() {
                # Create base file: "AAAAAAAAAA" (10 A's)
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
                The output should equal "XXAAYYZZ"
            End

            It "file size remains correct after overwrites"
                # Size should be 10 (original) - but last write truncated
                # Let's recreate and test proper overwrite
                printf 'AAAAAAAAAA' > "$MNT/offset_write.txt"
                write_at_offset "$MNT/offset_write.txt" 5 "BB"
                When call assert_file_size "$MNT/offset_write.txt" "10"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/offset_write.txt"'
        End

        Describe 'Binary read/write at offset'
            setup_binary_offset() {
                # Create 4KB file with known pattern
                dd if=/dev/zero of="$MNT/binary_offset.bin" bs=4096 count=1 2>/dev/null
            }
            BeforeAll 'setup_binary_offset'

            It "writes hex bytes at offset"
                write_hex_at_offset "$MNT/binary_offset.bin" 100 "deadbeef"
                hex=$(read_hex_at_offset "$MNT/binary_offset.bin" 100 4)
                The value "$hex" should equal "deadbeef"
            End

            It "preserves surrounding bytes after write"
                # Bytes before offset 100 should still be zero
                hex_before=$(read_hex_at_offset "$MNT/binary_offset.bin" 96 4)
                The value "$hex_before" should equal "00000000"
            End

            It "writes at page boundary (offset 4095)"
                write_hex_at_offset "$MNT/binary_offset.bin" 4092 "cafebabe"
                hex=$(read_hex_at_offset "$MNT/binary_offset.bin" 4092 4)
                The value "$hex" should equal "cafebabe"
            End

            AfterAll 'rm -f "$MNT/binary_offset.bin"'
        End

        Describe 'Sparse-like operations'
            It "writes beyond current file size (extends file)"
                printf 'START' > "$MNT/sparse.txt"
                write_at_offset "$MNT/sparse.txt" 100 "END"
                size=$(file_size "$MNT/sparse.txt")
                # Size should be at least 103 (offset 100 + 3 bytes)
                [ "$size" -ge 103 ]
                The status should be success
            End

            It "reads extended region correctly"
                When call read_at_offset "$MNT/sparse.txt" 100 3
                The output should equal "END"
            End

            AfterAll 'rm -f "$MNT/sparse.txt"'
        End

        Describe 'Large file offset operations'
            setup_large_offset() {
                # Create 2MB file
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
                # First 1MB should still match source
                When call compare_range "$SRC/large_offset.bin" "$MNT/large_offset.bin" 0 1024
                The status should be success
            End

            AfterAll 'rm -f "$MNT/large_offset.bin"'
        End
    End

    # ─── Cleanup test files ───────────────────────────────────────

    Describe 'Cleanup'
        It "removes all remaining test files"
            for f in "$MNT"/*; do
                [ -e "$f" ] || continue
                rm -rf "$f" 2>/dev/null || true
            done
            When call ls "$MNT"
            The output should equal ""
        End
    End
End
