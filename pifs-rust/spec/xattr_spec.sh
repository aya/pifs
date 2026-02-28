#shellcheck shell=sh
#
# Tests for extended attributes:
# - getxattr
# - setxattr
# - listxattr
# - removexattr
#
# FUSE operations covered: getxattr, setxattr, listxattr, removexattr
#
# Note: xattr command availability varies by platform
# macOS: xattr -w/-r/-l/-d
# Linux: setfattr/getfattr or attr
#

Describe 'Extended Attributes (xattr)'
    BeforeAll 'pifs_setup'

    # Skip all if xattr command not available
    Skip if "xattr command not available" [ ! -x "$(command -v xattr 2>/dev/null)" ] && [ ! -x "$(command -v setfattr 2>/dev/null)" ]

    # ─── Helper functions ─────────────────────────────────────────

    # Cross-platform xattr set
    xattr_set() {
        local file="$1" name="$2" value="$3"
        if command -v xattr >/dev/null 2>&1; then
            xattr -w "$name" "$value" "$file"
        elif command -v setfattr >/dev/null 2>&1; then
            setfattr -n "user.$name" -v "$value" "$file"
        else
            return 1
        fi
    }

    # Cross-platform xattr get
    xattr_get() {
        local file="$1" name="$2"
        if command -v xattr >/dev/null 2>&1; then
            xattr -p "$name" "$file" 2>/dev/null
        elif command -v getfattr >/dev/null 2>&1; then
            getfattr -n "user.$name" --only-values "$file" 2>/dev/null
        else
            return 1
        fi
    }

    # Cross-platform xattr list
    xattr_list() {
        local file="$1"
        if command -v xattr >/dev/null 2>&1; then
            xattr -l "$file" 2>/dev/null
        elif command -v getfattr >/dev/null 2>&1; then
            getfattr -d "$file" 2>/dev/null
        else
            return 1
        fi
    }

    # Cross-platform xattr remove
    xattr_remove() {
        local file="$1" name="$2"
        if command -v xattr >/dev/null 2>&1; then
            xattr -d "$name" "$file" 2>/dev/null
        elif command -v setfattr >/dev/null 2>&1; then
            setfattr -x "user.$name" "$file" 2>/dev/null
        else
            return 1
        fi
    }

    # ─── setxattr / getxattr ──────────────────────────────────────

    Describe 'setxattr and getxattr'
        setup_xattr() {
            echo "xattr test" > "$MNT/xattr_test.txt"
        }
        BeforeAll 'setup_xattr'

        It "sets extended attribute"
            When call xattr_set "$MNT/xattr_test.txt" "user.test" "test_value"
            The status should be success
        End

        It "gets extended attribute"
            xattr_set "$MNT/xattr_test.txt" "user.myattr" "myvalue"
            When call xattr_get "$MNT/xattr_test.txt" "user.myattr"
            The output should include "myvalue"
        End

        It "overwrites extended attribute"
            xattr_set "$MNT/xattr_test.txt" "user.overwrite" "original"
            xattr_set "$MNT/xattr_test.txt" "user.overwrite" "replaced"
            When call xattr_get "$MNT/xattr_test.txt" "user.overwrite"
            The output should include "replaced"
        End

        It "sets multiple attributes"
            xattr_set "$MNT/xattr_test.txt" "user.attr1" "value1"
            xattr_set "$MNT/xattr_test.txt" "user.attr2" "value2"
            xattr_set "$MNT/xattr_test.txt" "user.attr3" "value3"
            val1=$(xattr_get "$MNT/xattr_test.txt" "user.attr1")
            val2=$(xattr_get "$MNT/xattr_test.txt" "user.attr2")
            The value "$val1" should include "value1"
            The value "$val2" should include "value2"
        End

        AfterAll 'rm -f "$MNT/xattr_test.txt"'
    End

    # ─── listxattr ────────────────────────────────────────────────

    Describe 'listxattr'
        setup_listxattr() {
            echo "list test" > "$MNT/xattr_list.txt"
            xattr_set "$MNT/xattr_list.txt" "user.list1" "val1" 2>/dev/null || true
            xattr_set "$MNT/xattr_list.txt" "user.list2" "val2" 2>/dev/null || true
        }
        BeforeAll 'setup_listxattr'

        It "lists extended attributes"
            When call xattr_list "$MNT/xattr_list.txt"
            The status should be success
            The output should be present
        End

        It "list includes set attributes"
            result=$(xattr_list "$MNT/xattr_list.txt")
            The value "$result" should include "list1"
        End

        AfterAll 'rm -f "$MNT/xattr_list.txt"'
    End

    # ─── removexattr ──────────────────────────────────────────────

    Describe 'removexattr'
        setup_removexattr() {
            echo "remove test" > "$MNT/xattr_remove.txt"
            xattr_set "$MNT/xattr_remove.txt" "user.toremove" "value" 2>/dev/null || true
        }
        BeforeAll 'setup_removexattr'

        It "removes extended attribute"
            When call xattr_remove "$MNT/xattr_remove.txt" "user.toremove"
            The status should be success
        End

        It "attribute no longer exists after removal"
            When call xattr_get "$MNT/xattr_remove.txt" "user.toremove"
            The status should be failure
        End

        AfterAll 'rm -f "$MNT/xattr_remove.txt"'
    End

    # ─── Quarantine xattr (macOS specific) ────────────────────────

    Describe 'quarantine xattr (macOS)'
        Skip if "not macOS" [ "$(uname)" != "Darwin" ]

        It "copies file with com.apple.quarantine xattr"
            dd if=/dev/urandom of="$SRC/xattr_quarantine.bin" bs=1024 count=100 2>/dev/null
            xattr -w com.apple.quarantine "0083;66543210;Safari;" "$SRC/xattr_quarantine.bin" 2>/dev/null || true
            When call cp "$SRC/xattr_quarantine.bin" "$MNT/xattr_quarantine.bin"
            The status should be success
        End

        It "preserves file integrity after xattr copy"
            When call files_identical "$MNT/xattr_quarantine.bin" "$SRC/xattr_quarantine.bin"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/xattr_quarantine.bin"'
    End

    # ─── xattr on directories ─────────────────────────────────────

    Describe 'xattr on directories'
        setup_dir_xattr() {
            mkdir -p "$MNT/xattr_dir"
        }
        BeforeAll 'setup_dir_xattr'

        It "sets xattr on directory"
            When call xattr_set "$MNT/xattr_dir" "user.dirattr" "dirvalue"
            The status should be success
        End

        It "gets xattr from directory"
            xattr_set "$MNT/xattr_dir" "user.dirtest" "testval" 2>/dev/null || true
            When call xattr_get "$MNT/xattr_dir" "user.dirtest"
            The output should include "testval"
        End

        AfterAll 'rm -rf "$MNT/xattr_dir"'
    End

    # ─── xattr edge cases ─────────────────────────────────────────

    Describe 'xattr edge cases'
        setup_xattr_edge() {
            echo "edge" > "$MNT/xattr_edge.txt"
        }
        BeforeAll 'setup_xattr_edge'

        It "handles empty value"
            xattr_set "$MNT/xattr_edge.txt" "user.empty" "" 2>/dev/null || true
            When call xattr_get "$MNT/xattr_edge.txt" "user.empty"
            The status should be success
            The output should be defined
        End

        It "handles value with spaces"
            xattr_set "$MNT/xattr_edge.txt" "user.spaces" "value with spaces"
            When call xattr_get "$MNT/xattr_edge.txt" "user.spaces"
            The output should include "value with spaces"
        End

        It "handles long attribute name"
            longname="user.$(python3 -c "print('a'*100)")"
            When call xattr_set "$MNT/xattr_edge.txt" "$longname" "longname"
            The status should be success
        End

        AfterAll 'rm -f "$MNT/xattr_edge.txt"'
    End
End
