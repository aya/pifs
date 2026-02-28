#shellcheck shell=sh
#
# pifs integration tests — main entry point
#
# This file handles global setup/teardown for all spec files.
# Tests are organized in separate files by category:
#
#   read_write_spec.sh  - File I/O (create, open, read, write, truncate, fsync)
#   directory_spec.sh   - Directories (mkdir, rmdir, readdir, fsyncdir)
#   metadata_spec.sh    - Attributes (stat, chmod, chown, utimens, access)
#   links_spec.sh       - Links (symlink, hardlink, unlink, rename)
#   xattr_spec.sh       - Extended attributes (getxattr, setxattr, listxattr, removexattr)
#   statfs_spec.sh      - Filesystem (statfs, mknod, lookup)
#
# Usage:
#   shellspec                           # Run all tests
#   shellspec spec/read_write_spec.sh   # Run specific category
#   INCLUDE_LARGE=true shellspec        # Include 100M/1G file tests
#   DO_MOUNT=false shellspec            # Skip mount (pifs already mounted)
#
# FUSE operations covered (28 total):
#   - Lifecycle: init
#   - Metadata: getattr, lookup, readlink, access, statfs
#   - Creation: mknod, mkdir, unlink, rmdir, symlink, link, rename
#   - Attributes: setattr (chmod, chown, truncate, utimens)
#   - File I/O: create, open, read, write, flush, release, fsync
#   - Directory: opendir, readdir, releasedir, fsyncdir
#   - Xattr: getxattr, setxattr, listxattr, removexattr
#

Describe 'pifs FUSE filesystem'
    BeforeAll 'pifs_setup'

    # ─── Final Cleanup ────────────────────────────────────────────

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
