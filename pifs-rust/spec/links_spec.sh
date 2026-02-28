#shellcheck shell=sh
#
# Tests for link operations:
# - symlink, readlink
# - link (hard link)
# - unlink (rm)
# - rename (mv)
#
# FUSE operations covered: symlink, readlink, link, unlink, rename
#

Describe 'Link Operations'
    BeforeAll 'pifs_setup'

    # ─── Symlink ──────────────────────────────────────────────────

    Describe 'symlink'
        setup_symlink() {
            echo "symlink target content" > "$MNT/sym_target.txt"
        }
        BeforeAll 'setup_symlink'

        It "creates symbolic link"
            When call ln -s sym_target.txt "$MNT/sym_link.txt"
            The status should be success
        End

        It "symlink is a symbolic link"
            The path "$MNT/sym_link.txt" should be symlink
        End

        It "readlink returns correct target"
            When call readlink "$MNT/sym_link.txt"
            The output should equal "sym_target.txt"
        End

        It "reads content through symlink"
            When call cat "$MNT/sym_link.txt"
            The output should equal "symlink target content"
        End

        It "writes through symlink"
            echo "modified" > "$MNT/sym_link.txt"
            When call cat "$MNT/sym_target.txt"
            The output should equal "modified"
        End

        It "creates symlink to directory"
            mkdir -p "$MNT/sym_target_dir"
            When call ln -s sym_target_dir "$MNT/sym_link_dir"
            The status should be success
        End

        It "lists directory through symlink"
            touch "$MNT/sym_target_dir/file.txt"
            When call ls "$MNT/sym_link_dir"
            The output should include "file.txt"
        End

        It "creates symlink with absolute path"
            When call ln -s "$MNT/sym_target.txt" "$MNT/sym_abs_link.txt"
            The status should be success
        End

        It "creates dangling symlink"
            When call ln -s nonexistent_file "$MNT/dangling_link.txt"
            The status should be success
        End

        It "dangling symlink cannot be read"
            When call cat "$MNT/dangling_link.txt"
            The status should be failure
            The stderr should be present
        End

        AfterAll 'rm -rf "$MNT/sym_target.txt" "$MNT/sym_link.txt" "$MNT/sym_target_dir" "$MNT/sym_link_dir" "$MNT/sym_abs_link.txt" "$MNT/dangling_link.txt"'
    End

    # ─── Hard link ────────────────────────────────────────────────

    Describe 'hard link'
        setup_hardlink() {
            echo "hardlink content" > "$MNT/hl_source.txt"
        }
        BeforeAll 'setup_hardlink'

        It "creates hard link"
            When call ln "$MNT/hl_source.txt" "$MNT/hl_link.txt"
            The status should be success
        End

        It "hard link is a regular file"
            The path "$MNT/hl_link.txt" should be file
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

        It "modifying one affects the other"
            echo "modified via link" > "$MNT/hl_link.txt"
            When call cat "$MNT/hl_source.txt"
            The output should equal "modified via link"
        End

        It "removing original keeps link valid"
            rm "$MNT/hl_source.txt"
            When call cat "$MNT/hl_link.txt"
            The output should equal "modified via link"
        End

        It "cannot create hard link to directory"
            mkdir -p "$MNT/hl_dir"
            When call ln "$MNT/hl_dir" "$MNT/hl_dir_link"
            The status should be failure
            The stderr should be present
            rmdir "$MNT/hl_dir"
        End

        AfterAll 'rm -f "$MNT/hl_source.txt" "$MNT/hl_link.txt"'
    End

    # ─── Unlink (rm) ──────────────────────────────────────────────

    Describe 'unlink'
        It "removes file with rm"
            echo "delete me" > "$MNT/to_delete.txt"
            When call rm "$MNT/to_delete.txt"
            The status should be success
        End

        It "file no longer exists after rm"
            echo "delete me" > "$MNT/to_delete2.txt"
            rm "$MNT/to_delete2.txt"
            The path "$MNT/to_delete2.txt" should not be exist
        End

        It "rm fails on non-existent file"
            When call rm "$MNT/nonexistent_file"
            The status should be failure
            The stderr should be present
        End

        It "rm fails on directory without -r"
            mkdir -p "$MNT/unlink_dir"
            When call rm "$MNT/unlink_dir"
            The status should be failure
            The stderr should be present
            rmdir "$MNT/unlink_dir"
        End

        It "rm -f does not fail on non-existent file"
            When call rm -f "$MNT/nonexistent_file"
            The status should be success
        End

        It "unlink command removes file"
            echo "unlink me" > "$MNT/unlink_test.txt"
            When call unlink "$MNT/unlink_test.txt"
            The status should be success
            The path "$MNT/unlink_test.txt" should not be exist
        End
    End

    # ─── Rename (mv) ──────────────────────────────────────────────

    Describe 'rename'
        Describe 'rename file'
            It "renames file"
                echo "rename me" > "$MNT/before_rename.txt"
                When call mv "$MNT/before_rename.txt" "$MNT/after_rename.txt"
                The status should be success
            End

            It "old name no longer exists"
                The path "$MNT/before_rename.txt" should not be exist
            End

            It "new name exists"
                The path "$MNT/after_rename.txt" should be file
            End

            It "content preserved after rename"
                When call cat "$MNT/after_rename.txt"
                The output should equal "rename me"
            End

            AfterAll 'rm -f "$MNT/after_rename.txt"'
        End

        Describe 'rename directory'
            setup_rename_dir() {
                mkdir -p "$MNT/dir_before"
                echo "in dir" > "$MNT/dir_before/file.txt"
            }
            BeforeAll 'setup_rename_dir'

            It "renames directory"
                When call mv "$MNT/dir_before" "$MNT/dir_after"
                The status should be success
            End

            It "old directory name no longer exists"
                The path "$MNT/dir_before" should not be exist
            End

            It "content preserved in renamed directory"
                When call cat "$MNT/dir_after/file.txt"
                The output should equal "in dir"
            End

            AfterAll 'rm -rf "$MNT/dir_after"'
        End

        Describe 'rename to overwrite'
            It "rename overwrites existing file"
                echo "original" > "$MNT/mv_target.txt"
                echo "replacement" > "$MNT/mv_source.txt"
                mv "$MNT/mv_source.txt" "$MNT/mv_target.txt"
                When call cat "$MNT/mv_target.txt"
                The output should equal "replacement"
            End

            AfterAll 'rm -f "$MNT/mv_target.txt"'
        End

        Describe 'rename across directories'
            setup_rename_across() {
                mkdir -p "$MNT/rename_src"
                mkdir -p "$MNT/rename_dst"
                echo "moving" > "$MNT/rename_src/file.txt"
            }
            BeforeAll 'setup_rename_across'

            It "moves file to another directory"
                When call mv "$MNT/rename_src/file.txt" "$MNT/rename_dst/file.txt"
                The status should be success
            End

            It "file exists in destination"
                When call cat "$MNT/rename_dst/file.txt"
                The output should equal "moving"
            End

            It "file removed from source"
                The path "$MNT/rename_src/file.txt" should not be exist
            End

            AfterAll 'rm -rf "$MNT/rename_src" "$MNT/rename_dst"'
        End

        Describe 'rename with special characters'
            It "renames file with spaces"
                echo "spaces" > "$MNT/file with spaces.txt"
                mv "$MNT/file with spaces.txt" "$MNT/renamed with spaces.txt"
                When call cat "$MNT/renamed with spaces.txt"
                The output should equal "spaces"
                rm -f "$MNT/renamed with spaces.txt"
            End

            It "renames file with unicode"
                echo "unicode" > "$MNT/fichier.txt"
                mv "$MNT/fichier.txt" "$MNT/fichier-renommé.txt"
                When call cat "$MNT/fichier-renommé.txt"
                The output should equal "unicode"
                rm -f "$MNT/fichier-renommé.txt"
            End
        End
    End

    # ─── Link edge cases ──────────────────────────────────────────

    Describe 'link edge cases'
        It "symlink to file with spaces"
            echo "content" > "$MNT/target with spaces.txt"
            ln -s "target with spaces.txt" "$MNT/link with spaces.txt"
            When call cat "$MNT/link with spaces.txt"
            The output should equal "content"
            rm -f "$MNT/target with spaces.txt" "$MNT/link with spaces.txt"
        End

        It "nested symlinks"
            echo "deep" > "$MNT/nested_target.txt"
            ln -s nested_target.txt "$MNT/level1.txt"
            ln -s level1.txt "$MNT/level2.txt"
            When call cat "$MNT/level2.txt"
            The output should equal "deep"
            rm -f "$MNT/nested_target.txt" "$MNT/level1.txt" "$MNT/level2.txt"
        End

        It "symlink with very long target name"
            longname=$(python3 -c "print('a'*200)")
            echo "long" > "$MNT/$longname"
            ln -s "$longname" "$MNT/link_to_long"
            When call cat "$MNT/link_to_long"
            The output should equal "long"
            rm -f "$MNT/$longname" "$MNT/link_to_long"
        End
    End
End
