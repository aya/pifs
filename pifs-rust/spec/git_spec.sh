#shellcheck shell=sh
#
# Tests for Git MDD versioning (--git flag).
#
# Uses its own pifs mount (separate from the shared one) because
# it needs the --git flag.
#

Describe 'Git MDD Versioning'
    # ─── Dedicated mount with --git ──────────────────────────────
    #
    # We cannot reuse the shared pifs_setup because we need --git.
    # Set up our own temp dirs, mount, and teardown.

    git_setup() {
        PROJECT_DIR="${SHELLSPEC_PROJECT_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
        PIFS_BIN="${PROJECT_DIR}/target/debug/pifs"

        _git_root=$(mktemp -d "${TMPDIR:-/tmp}/pifs-git-test.XXXXXX")
        _git_root=$(cd "$_git_root" && pwd -P)
        GIT_MDD="$_git_root/mdd"
        GIT_MNT="$_git_root/mnt"
        GIT_LOG="$_git_root/pifs.log"
        mkdir "$GIT_MDD" "$GIT_MNT"

        # Sentinel for mount detection
        touch "$GIT_MNT/.pifs_pre_mount_sentinel"

        _pifs_args="--mdd $GIT_MDD --git --log $GIT_LOG"
        if [ "${PIFS_STORAGE_MODE:-}" = "whole" ]; then
            _pifs_args="$_pifs_args --whole-file"
        fi

        "$PIFS_BIN" $_pifs_args "$GIT_MNT" \
            </dev/null >>"$GIT_LOG" 2>&1 3>&- 4>&- 5>&- 6>&- 7>&- 8>&- 9>&- &
        GIT_PIFS_PID=$!
        disown "$GIT_PIFS_PID" 2>/dev/null || true

        _retries=0
        while [ "$_retries" -lt 20 ]; do
            if [ ! -f "$GIT_MNT/.pifs_pre_mount_sentinel" ]; then
                break
            fi
            if ! kill -0 "$GIT_PIFS_PID" 2>/dev/null; then
                echo "ERROR: pifs (--git) process exited before mount was ready" >&2
                echo "  log: $(tail -5 "$GIT_LOG" 2>/dev/null)" >&2
                return 1
            fi
            sleep 0.5
            _retries=$((_retries + 1))
        done

        if [ "$_retries" -ge 20 ]; then
            echo "ERROR: pifs (--git) failed to mount after 10s" >&2
            echo "  log: $(tail -5 "$GIT_LOG" 2>/dev/null)" >&2
            kill "$GIT_PIFS_PID" 2>/dev/null || true
            return 1
        fi

        # Smoke test
        if ! echo "mount-check" > "$GIT_MNT/.pifs_mount_test" 2>/dev/null; then
            echo "ERROR: pifs (--git) mounted but write test failed" >&2
            return 1
        fi
        rm -f "$GIT_MNT/.pifs_mount_test" 2>/dev/null
    }

    git_teardown() {
        if [ -n "${GIT_PIFS_PID:-}" ]; then
            kill "$GIT_PIFS_PID" 2>/dev/null || true
            _w=0
            while kill -0 "$GIT_PIFS_PID" 2>/dev/null && [ "$_w" -lt 10 ]; do
                sleep 0.5
                _w=$((_w + 1))
            done
        fi
        if [ -n "${GIT_MNT:-}" ]; then
            diskutil unmount "$GIT_MNT" >/dev/null 2>&1 \
                || umount "$GIT_MNT" 2>/dev/null \
                || true
        fi
        if [ -n "${GIT_MDD:-}" ]; then
            rm -rf "$(dirname "$GIT_MDD")" 2>/dev/null || true
        fi
    }

    # Helper: count git commits in MDD
    git_commit_count() {
        git -C "$GIT_MDD" rev-list --count HEAD
    }

    # Helper: get last commit message
    git_last_message() {
        git -C "$GIT_MDD" log -1 --format=%B
    }

    # Helper: wait for git commit count to change (debounce up to 5s)
    wait_for_commit() {
        _target="$1"
        _waited=0
        while [ "$_waited" -lt 10 ]; do
            _current=$(git_commit_count)
            if [ "$_current" -ge "$_target" ]; then
                return 0
            fi
            sleep 0.5
            _waited=$((_waited + 1))
        done
        return 1
    }

    BeforeAll 'git_setup'
    AfterAll 'git_teardown'

    # ─── Git initialization ──────────────────────────────────────

    Describe 'git initialization'
        It ".git directory exists in MDD"
            When call test -d "$GIT_MDD/.git"
            The status should be success
        End

        It "initial commit exists"
            When call git_commit_count
            The output should equal "1"
        End

        It ".git is hidden from FUSE readdir"
            When call ls -a "$GIT_MNT"
            The output should not include ".git"
        End

        It ".git is hidden from FUSE lookup (stat fails)"
            When call stat "$GIT_MNT/.git"
            The status should be failure
            The stderr should be present
        End
    End

    # ─── File operations create git history ──────────────────────

    Describe 'file operations create git history'
        It "commit count increases after file write+close"
            _before=$(git_commit_count)
            echo "hello git" > "$GIT_MNT/git_test_file.txt"
            # Wait for debounce (2s) + margin
            When call wait_for_commit $((_before + 1))
            The status should be success
        End

        It "commit message lists changed file"
            When call git_last_message
            The output should include "modified:"
        End

        It "commit count increases after deletion"
            _before=$(git_commit_count)
            rm "$GIT_MNT/git_test_file.txt"
            When call wait_for_commit $((_before + 1))
            The status should be success
        End

        It "commit message shows deletion"
            When call git_last_message
            The output should include "deleted:"
        End
    End

    # ─── Directory operations ────────────────────────────────────

    Describe 'directory operations'
        It "commit after mkdir with file inside"
            _before=$(git_commit_count)
            mkdir "$GIT_MNT/git_test_dir"
            echo "content" > "$GIT_MNT/git_test_dir/file.txt"
            When call wait_for_commit $((_before + 1))
            The status should be success
        End

        It "commit after removing dir contents and rmdir"
            _before=$(git_commit_count)
            rm "$GIT_MNT/git_test_dir/file.txt"
            rmdir "$GIT_MNT/git_test_dir"
            When call wait_for_commit $((_before + 1))
            The status should be success
        End
    End

    # ─── Batching ────────────────────────────────────────────────

    Describe 'batching'
        It "3 rapid file creates produce at most 2 commits"
            _before=$(git_commit_count)
            echo "a" > "$GIT_MNT/batch_a.txt"
            echo "b" > "$GIT_MNT/batch_b.txt"
            echo "c" > "$GIT_MNT/batch_c.txt"
            sleep 4  # Wait for debounce to flush
            _after=$(git_commit_count)
            _diff=$((_after - _before))
            # Should be 1 or 2 commits (batched), not 3+
            When call test "$_diff" -le 2
            The status should be success
        End

        AfterAll 'rm -f "$GIT_MNT/batch_a.txt" "$GIT_MNT/batch_b.txt" "$GIT_MNT/batch_c.txt" 2>/dev/null || true'
    End

    # ─── Rename ──────────────────────────────────────────────────

    Describe 'rename'
        It "commit after mv"
            echo "rename me" > "$GIT_MNT/git_rename_src.txt"
            sleep 3  # wait for create commit
            _before=$(git_commit_count)
            mv "$GIT_MNT/git_rename_src.txt" "$GIT_MNT/git_rename_dst.txt"
            When call wait_for_commit $((_before + 1))
            The status should be success
        End

        It "commit message shows renamed"
            When call git_last_message
            The output should include "renamed:"
        End

        AfterAll 'rm -f "$GIT_MNT/git_rename_dst.txt" 2>/dev/null || true'
    End
End
