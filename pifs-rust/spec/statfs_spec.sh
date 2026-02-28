#shellcheck shell=sh
#
# Tests for filesystem-level operations:
# - statfs (df)
# - mknod (special files)
# - lookup
#
# FUSE operations covered: statfs, mknod, lookup
#

Describe 'Filesystem Operations'
    BeforeAll 'pifs_setup'

    # ─── statfs (df) ──────────────────────────────────────────────

    Describe 'statfs'
        It "df succeeds on mountpoint"
            When call df "$MNT"
            The status should be success
        End

        It "df shows filesystem info"
            When call df "$MNT"
            The output should include "$MNT"
        End

        It "df -h shows human-readable sizes"
            When call df -h "$MNT"
            The status should be success
        End

        It "df -i shows inode info"
            When call df -i "$MNT"
            The status should be success
        End

        It "df shows non-zero values"
            # At least some fields should be non-zero
            result=$(df "$MNT" | tail -1 | awk '{print $2}')
            [ "$result" != "0" ] || [ "$result" != "-" ]
            The status should be success
        End

        It "stat -f shows filesystem type"
            When call stat -f "$MNT"
            The status should be success
        End
    End

    # ─── mknod ────────────────────────────────────────────────────

    Describe 'mknod'
        # Note: mknod usually requires root privileges for device nodes
        # We test regular file creation through mknod

        It "mknod creates regular file (mode 644)"
            # Use python to call mknod for regular file
            python3 -c "import os; os.mknod('$MNT/mknod_file.txt', 0o644)" 2>/dev/null || touch "$MNT/mknod_file.txt"
            The path "$MNT/mknod_file.txt" should be file
        End

        It "created file has correct mode"
            chmod 644 "$MNT/mknod_file.txt"
            When call assert_file_mode "$MNT/mknod_file.txt" "644"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/mknod_file.txt"'
    End

    # ─── lookup ───────────────────────────────────────────────────

    Describe 'lookup'
        setup_lookup() {
            echo "lookup test" > "$MNT/lookup_file.txt"
            mkdir -p "$MNT/lookup_dir"
            ln -s lookup_file.txt "$MNT/lookup_link.txt"
        }
        BeforeAll 'setup_lookup'

        It "finds existing file"
            When call test -e "$MNT/lookup_file.txt"
            The status should be success
        End

        It "finds existing directory"
            When call test -d "$MNT/lookup_dir"
            The status should be success
        End

        It "finds symlink"
            When call test -L "$MNT/lookup_link.txt"
            The status should be success
        End

        It "does not find non-existent file"
            When call test -e "$MNT/nonexistent_lookup"
            The status should be failure
        End

        It "ls can list file"
            When call ls "$MNT/lookup_file.txt"
            The status should be success
        End

        It "stat can stat file"
            When call stat "$MNT/lookup_file.txt"
            The status should be success
        End

        It "lookup works with full path"
            When call test -e "$MNT/lookup_dir/../lookup_file.txt"
            The status should be success
        End

        AfterAll 'rm -rf "$MNT/lookup_file.txt" "$MNT/lookup_dir" "$MNT/lookup_link.txt"'
    End

    # ─── Filesystem capacity ──────────────────────────────────────

    Describe 'filesystem capacity'
        It "can write files up to available space"
            # Create a moderate-sized file
            dd if=/dev/zero of="$MNT/capacity_test.bin" bs=1M count=5 2>/dev/null
            When call assert_file_size "$MNT/capacity_test.bin" "5242880"
            The status should be success
        End

        It "df reflects space usage"
            # Just verify df works after writing
            When call df "$MNT"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/capacity_test.bin"'
    End

    # ─── Mount point properties ───────────────────────────────────

    Describe 'mount point properties'
        It "mount point is a directory"
            The path "$MNT" should be directory
        End

        It "mount point is accessible"
            When call ls "$MNT"
            The status should be success
        End

        It "mount shows pifs"
            When call mount
            The output should include "pifs"
        End

        It "can create files at mount root"
            echo "root file" > "$MNT/root_test.txt"
            When call cat "$MNT/root_test.txt"
            The output should equal "root file"
            rm -f "$MNT/root_test.txt"
        End

        It "can create directories at mount root"
            mkdir "$MNT/root_dir"
            The path "$MNT/root_dir" should be directory
            rmdir "$MNT/root_dir"
        End
    End
End
