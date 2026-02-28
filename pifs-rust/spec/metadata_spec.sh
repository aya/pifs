#shellcheck shell=sh
#
# Tests for file metadata operations:
# - getattr (stat)
# - setattr (chmod, chown, truncate, utimens)
# - access
#
# FUSE operations covered: getattr, setattr, access
#

Describe 'File Metadata Operations'
    BeforeAll 'pifs_setup'

    # ─── getattr (stat) ───────────────────────────────────────────

    Describe 'getattr (stat)'
        setup_stat() {
            echo "stat test content" > "$MNT/stat_test.txt"
            mkdir -p "$MNT/stat_dir"
        }
        BeforeAll 'setup_stat'

        It "stat succeeds on file"
            When call stat "$MNT/stat_test.txt"
            The status should be success
        End

        It "stat shows correct file size"
            When call assert_file_size "$MNT/stat_test.txt" "18"
            The status should be success
        End

        It "stat succeeds on directory"
            When call stat "$MNT/stat_dir"
            The status should be success
        End

        It "stat fails on non-existent file"
            When call stat "$MNT/nonexistent_file"
            The status should be failure
        End

        It "stat shows file type"
            result=$(stat -f%HT "$MNT/stat_test.txt" 2>/dev/null || stat -c%F "$MNT/stat_test.txt" 2>/dev/null)
            # Should be "Regular File" or "regular file"
            The value "$result" should include "egular"
        End

        It "stat shows directory type"
            result=$(stat -f%HT "$MNT/stat_dir" 2>/dev/null || stat -c%F "$MNT/stat_dir" 2>/dev/null)
            # Should be "Directory" or "directory"
            The value "$result" should include "irectory"
        End

        AfterAll 'rm -rf "$MNT/stat_test.txt" "$MNT/stat_dir"'
    End

    # ─── File size reporting ──────────────────────────────────────

    Describe 'file size reporting'
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

        It "sets mode to 400 (read-only)"
            chmod 400 "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "400"
            The status should be success
        End

        It "sets mode to 000"
            chmod 000 "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "000"
            The status should be success
        End

        It "sets mode to 777"
            chmod 777 "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "777"
            The status should be success
        End

        It "uses symbolic mode +x"
            chmod 644 "$MNT/chmod_test.txt"
            chmod +x "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "755"
            The status should be success
        End

        It "uses symbolic mode -w"
            chmod 644 "$MNT/chmod_test.txt"
            chmod -w "$MNT/chmod_test.txt"
            When call assert_file_mode "$MNT/chmod_test.txt" "444"
            The status should be success
        End

        AfterAll 'chmod 644 "$MNT/chmod_test.txt"; rm -f "$MNT/chmod_test.txt"'
    End

    # ─── chown ────────────────────────────────────────────────────

    Describe 'chown'
        setup_chown() {
            echo "chown test" > "$MNT/chown_test.txt"
        }
        BeforeAll 'setup_chown'

        It "chown to current user succeeds"
            current_user=$(id -un)
            When call chown "$current_user" "$MNT/chown_test.txt"
            The status should be success
        End

        It "chown to current group succeeds"
            current_group=$(id -gn)
            When call chown ":$current_group" "$MNT/chown_test.txt"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/chown_test.txt"'
    End

    # ─── utimens (touch) ──────────────────────────────────────────

    Describe 'utimens (touch)'
        setup_utimens() {
            echo "time test" > "$MNT/time_test.txt"
        }
        BeforeAll 'setup_utimens'

        It "touch updates modification time"
            old_mtime=$(stat -f%m "$MNT/time_test.txt" 2>/dev/null || stat -c%Y "$MNT/time_test.txt")
            sleep 1
            touch "$MNT/time_test.txt"
            new_mtime=$(stat -f%m "$MNT/time_test.txt" 2>/dev/null || stat -c%Y "$MNT/time_test.txt")
            [ "$new_mtime" -gt "$old_mtime" ]
            The status should be success
        End

        It "touch -t sets specific timestamp"
            When call touch -t 202301011200 "$MNT/time_test.txt"
            The status should be success
        End

        It "touch -a updates access time only"
            When call touch -a "$MNT/time_test.txt"
            The status should be success
        End

        It "touch -m updates modification time only"
            When call touch -m "$MNT/time_test.txt"
            The status should be success
        End

        It "touch creates new file if not exists"
            rm -f "$MNT/touch_new.txt"
            When call touch "$MNT/touch_new.txt"
            The status should be success
            The path "$MNT/touch_new.txt" should be file
            rm -f "$MNT/touch_new.txt"
        End

        AfterAll 'rm -f "$MNT/time_test.txt"'
    End

    # ─── access ───────────────────────────────────────────────────

    Describe 'access'
        setup_access() {
            echo "access test" > "$MNT/access_test.txt"
            chmod 644 "$MNT/access_test.txt"
        }
        BeforeAll 'setup_access'

        It "test -r checks read permission"
            When call test -r "$MNT/access_test.txt"
            The status should be success
        End

        It "test -w checks write permission"
            When call test -w "$MNT/access_test.txt"
            The status should be success
        End

        It "test -x checks execute permission (should fail for 644)"
            When call test -x "$MNT/access_test.txt"
            The status should be failure
        End

        It "test -x succeeds after chmod +x"
            chmod +x "$MNT/access_test.txt"
            When call test -x "$MNT/access_test.txt"
            The status should be success
            chmod -x "$MNT/access_test.txt"
        End

        It "test -e checks existence"
            When call test -e "$MNT/access_test.txt"
            The status should be success
        End

        It "test -e fails for non-existent file"
            When call test -e "$MNT/nonexistent"
            The status should be failure
        End

        It "test -f checks regular file"
            When call test -f "$MNT/access_test.txt"
            The status should be success
        End

        It "test -d checks directory"
            mkdir -p "$MNT/access_dir"
            When call test -d "$MNT/access_dir"
            The status should be success
            rmdir "$MNT/access_dir"
        End

        AfterAll 'rm -f "$MNT/access_test.txt"'
    End

    # ─── Edge cases ───────────────────────────────────────────────

    Describe 'metadata edge cases'
        Describe 'single byte file'
            It "creates and stats single byte file"
                printf 'X' > "$MNT/onebyte.txt"
                When call assert_file_size "$MNT/onebyte.txt" "1"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/onebyte.txt"'
        End

        Describe 'file with newlines only'
            It "reports correct size for newlines-only file"
                printf '\n\n\n' > "$MNT/newlines.txt"
                When call assert_file_size "$MNT/newlines.txt" "3"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/newlines.txt"'
        End

        Describe 'file with null bytes'
            It "reports correct size for null bytes file"
                printf '\x00\x00\x00\x00' > "$MNT/nulls.bin"
                When call assert_file_size "$MNT/nulls.bin" "4"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/nulls.bin"'
        End

        Describe 'page boundary files'
            It "reports correct size for 4096 byte file"
                dd if=/dev/urandom of="$MNT/page.bin" bs=4096 count=1 2>/dev/null
                When call assert_file_size "$MNT/page.bin" "4096"
                The status should be success
            End

            It "reports correct size for 1MB file"
                dd if=/dev/urandom of="$MNT/1mb.bin" bs=1048576 count=1 2>/dev/null
                When call assert_file_size "$MNT/1mb.bin" "1048576"
                The status should be success
            End

            AfterAll 'rm -f "$MNT/page.bin" "$MNT/1mb.bin"'
        End

        Describe 'long filename'
            It "stats file with 255-char name"
                longname=$(python3 -c "print('a'*250 + '.txt')")
                echo "long" > "$MNT/$longname"
                When call stat "$MNT/$longname"
                The status should be success
                rm -f "$MNT/$longname"
            End
        End
    End
End
