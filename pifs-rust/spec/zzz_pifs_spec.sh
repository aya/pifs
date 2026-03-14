#shellcheck shell=sh
#
# pifs integration tests — final cleanup (runs last alphabetically)
#
# This file runs after all other spec files. It cleans up test files
# on the mount, then unmounts pifs and removes temp dirs.
#

Describe 'pifs cleanup'
    BeforeAll 'pifs_setup'
    AfterAll 'pifs_cleanup'

    Describe 'Final cleanup'
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
