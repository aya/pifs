#shellcheck shell=sh
#
# Tests for directory operations:
# - mkdir, rmdir
# - opendir, readdir, releasedir
# - fsyncdir
#
# FUSE operations covered: mkdir, rmdir, opendir, readdir, releasedir, fsyncdir
#

Describe 'Directory Operations'
    BeforeAll 'pifs_setup'

    # ─── mkdir ────────────────────────────────────────────────────

    Describe 'mkdir'
        It "creates directory"
            When call mkdir "$MNT/mkdir_test"
            The status should be success
            The path "$MNT/mkdir_test" should be directory
        End

        It "creates directory with specific mode"
            When call mkdir -m 755 "$MNT/mkdir_mode"
            The status should be success
        End

        It "verifies directory mode"
            When call assert_file_mode "$MNT/mkdir_mode" "755"
            The status should be success
        End

        It "creates nested directories with mkdir -p"
            When call mkdir -p "$MNT/nested/sub1/sub2/sub3"
            The status should be success
            The path "$MNT/nested/sub1/sub2/sub3" should be directory
        End

        It "fails to create directory without parent"
            When call mkdir "$MNT/nonexistent/subdir"
            The status should be failure
            The stderr should be present
        End

        It "fails to create directory that already exists"
            mkdir -p "$MNT/already_exists"
            When call mkdir "$MNT/already_exists"
            The status should be failure
            The stderr should be present
        End

        AfterAll 'rm -rf "$MNT/mkdir_test" "$MNT/mkdir_mode" "$MNT/nested" "$MNT/already_exists"'
    End

    # ─── rmdir ────────────────────────────────────────────────────

    Describe 'rmdir'
        setup_rmdir() {
            mkdir -p "$MNT/rmdir_empty"
            mkdir -p "$MNT/rmdir_nonempty"
            touch "$MNT/rmdir_nonempty/file.txt"
        }
        BeforeAll 'setup_rmdir'

        It "removes empty directory"
            When call rmdir "$MNT/rmdir_empty"
            The status should be success
            The path "$MNT/rmdir_empty" should not be exist
        End

        It "fails to remove non-empty directory"
            When call rmdir "$MNT/rmdir_nonempty"
            The status should be failure
            The stderr should be present
        End

        It "removes directory after emptying it"
            rm "$MNT/rmdir_nonempty/file.txt"
            When call rmdir "$MNT/rmdir_nonempty"
            The status should be success
        End

        It "fails to remove non-existent directory"
            When call rmdir "$MNT/nonexistent_dir"
            The status should be failure
            The stderr should be present
        End
    End

    # ─── readdir (ls) ─────────────────────────────────────────────

    Describe 'readdir'
        setup_readdir() {
            mkdir -p "$MNT/lsdir"
            touch "$MNT/lsdir/file_a.txt"
            touch "$MNT/lsdir/file_b.txt"
            touch "$MNT/lsdir/file_c.txt"
            mkdir "$MNT/lsdir/subdir"
        }
        BeforeAll 'setup_readdir'

        It "lists directory contents"
            When call ls "$MNT/lsdir"
            The output should include "file_a.txt"
            The output should include "file_b.txt"
            The output should include "file_c.txt"
            The output should include "subdir"
        End

        It "lists correct number of entries"
            When call ls "$MNT/lsdir"
            The lines of output should equal 4
        End

        It "lists with ls -la including . and .."
            When call ls -la "$MNT/lsdir"
            The output should include "."
            The output should include ".."
        End

        It "lists in long format with details"
            When call ls -l "$MNT/lsdir"
            The output should include "file_a.txt"
        End

        It "lists empty directory"
            mkdir -p "$MNT/lsdir/empty"
            When call ls "$MNT/lsdir/empty"
            The output should equal ""
        End

        It "lists hidden files with ls -a"
            touch "$MNT/lsdir/.hidden"
            When call ls -a "$MNT/lsdir"
            The output should include ".hidden"
            rm "$MNT/lsdir/.hidden"
        End

        AfterAll 'rm -rf "$MNT/lsdir"'
    End

    # ─── Nested directory operations ──────────────────────────────

    Describe 'nested directories'
        It "creates deeply nested structure"
            When call mkdir -p "$MNT/a/b/c/d/e/f/g"
            The status should be success
        End

        It "creates file in deep directory"
            echo "deep" > "$MNT/a/b/c/d/e/f/g/deep.txt"
            When call cat "$MNT/a/b/c/d/e/f/g/deep.txt"
            The output should equal "deep"
        End

        It "removes nested structure with rm -rf"
            When call rm -rf "$MNT/a"
            The status should be success
            The path "$MNT/a" should not be exist
        End
    End

    # ─── fsyncdir ─────────────────────────────────────────────────

    Describe 'fsyncdir'
        setup_fsyncdir() {
            mkdir -p "$MNT/syncdir"
            touch "$MNT/syncdir/file1.txt"
            touch "$MNT/syncdir/file2.txt"
        }
        BeforeAll 'setup_fsyncdir'

        It "sync succeeds on directory"
            When call sync
            The status should be success
        End

        It "directory contents persist after sync"
            sync
            When call ls "$MNT/syncdir"
            The output should include "file1.txt"
        End

        AfterAll 'rm -rf "$MNT/syncdir"'
    End

    # ─── Directory with special names ─────────────────────────────

    Describe 'directories with special names'
        It "creates directory with spaces"
            When call mkdir "$MNT/dir with spaces"
            The status should be success
        End

        It "creates file in directory with spaces"
            echo "content" > "$MNT/dir with spaces/file.txt"
            When call cat "$MNT/dir with spaces/file.txt"
            The output should equal "content"
        End

        It "creates directory with unicode characters"
            When call mkdir "$MNT/répertoire"
            The status should be success
        End

        It "creates directory with dots"
            When call mkdir "$MNT/dir.with.dots"
            The status should be success
        End

        It "creates directory starting with dash"
            When call mkdir -- "$MNT/-dashdir"
            The status should be success
        End

        AfterAll 'rm -rf "$MNT/dir with spaces" "$MNT/répertoire" "$MNT/dir.with.dots" "$MNT/-dashdir"'
    End

    # ─── Directory permissions ────────────────────────────────────

    Describe 'directory permissions'
        setup_dir_perms() {
            mkdir -p "$MNT/permdir"
        }
        BeforeAll 'setup_dir_perms'

        It "changes directory mode to 700"
            chmod 700 "$MNT/permdir"
            When call assert_file_mode "$MNT/permdir" "700"
            The status should be success
        End

        It "changes directory mode to 755"
            chmod 755 "$MNT/permdir"
            When call assert_file_mode "$MNT/permdir" "755"
            The status should be success
        End

        It "changes directory mode to 777"
            chmod 777 "$MNT/permdir"
            When call assert_file_mode "$MNT/permdir" "777"
            The status should be success
        End

        AfterAll 'rm -rf "$MNT/permdir"'
    End
End
