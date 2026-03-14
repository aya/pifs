# pifs — FUSE filesystem backed by IPFS

## Project Overview

FUSE filesystem that stores file content on IPFS and metadata (IPFS hashes) in a local directory (MDD).
Original C implementation in `src/pifs.c`. Active Rust rewrite in `pifs-rust/`.

## Architecture

```
pifs-rust/
  src/
    main.rs          - CLI (clap), mount with fuser::mount2
    filesystem.rs    - fuser::Filesystem trait impl (all FUSE ops)
    file_data.rs     - FileData trait + WholeFileData + ChunkedFileData backends
    ipfs.rs          - IPFS CLI wrappers (cat, add, files stat)
    git_sync.rs      - Git versioning of the MDD (background thread, debounced commits)
    types.rs         - PifsFilesystem struct, StorageMode
  fuser-patched/     - local patched copy of fuser 0.15.1
  spec/              - ShellSpec integration tests (9 spec files)
  Cargo.toml
```

## Storage Modes

- **Chunked** (default): 256KB chunks, lazy loading from IPFS, partial flush of dirty chunks
- **WholeFile** (`--whole-file`): Legacy, entire file in RAM, single IPFS hash
- MDD format: one IPFS hash per line; chunked writes multiple hashes
- Both modes track `write_frontier` to handle macFUSE page cache replays

## CLI

```
pifs --mdd <dir> [--whole-file] [--git <git-dir>] [--log <file>] <mountpoint>
```

- `--mdd`: metadata directory (required) — stores one file per user file, containing IPFS hashes
- `--git <dir>`: enable git versioning of MDD, git repo stored separately at `<dir>`
- `--whole-file`: use legacy whole-file storage mode
- `--log <file>`: enable logging to file

## IPFS xattrs

Files expose virtual extended attributes on the FUSE mount:

- `ipfs.hash` — whole-file IPFS CID (CIDv0, 46 chars, starts with `Qm`)
  - Whole-file mode: set at flush (free, hash already computed)
  - Chunked mode: lazy — invalidated at flush, computed on first `getxattr`, cached as xattr on MDD
- `ipfs.size` — file size in bytes (string)
  - Set at flush in both modes
  - Used as fallback in `getattr`/`lookup` to avoid `ipfs files stat` calls

## Critical Bugs & Pitfalls

1. **macFUSE 5.x rename ENOSYS**: fuser 0.15 sets `macfuse-4-compat` adding extra fields to `fuse_rename_in`, but macFUSE 5.x doesn't send them. Fix: patched `fuser-patched/build.rs`.
2. **macFUSE page cache replays**: macFUSE replays stale kernel page cache writes even with FOPEN_DIRECT_IO. Fix: `write_frontier` in both FileData impls, skip writes below frontier.
3. **ipfs_add pipe deadlock**: stdin pipe deadlocks for data >2MB. Fix: write to temp file, pass to `ipfs add -Q`.
4. **FOPEN flags in create()**: Don't pass request flags as FOPEN response flags (causes `fcopyfile failed: Invalid argument`).
5. **xattr symlink deadlock**: xattr functions follow symlinks by default → re-enters FUSE mount → deadlock. Fix: XATTR_NOFOLLOW on macOS, `l*xattr` on Linux.

## macFUSE Specifics

- macFUSE 5.1.3 (`/Library/Filesystems/macfuse.fs`)
- No `fdatasync` on macOS — use `fsync` fallback
- `statvfs` fields are `u32` on macOS — cast to `u64`
- `/var` symlinks to `/private/var` — affects mount detection
- xattr functions have extra `position`/`options` params on macOS
- `cp` uses `fcopyfile` → triggers page cache replay writes
- `daemon_timeout=600` mount option prevents idle disconnects

## Testing

All tests run from `pifs-rust/` directory.

```bash
cargo test                                  # unit tests (35)
shellspec --format doc                      # integration tests, chunked mode (default)
PIFS_STORAGE_MODE=whole shellspec           # integration tests, whole-file mode
shellspec spec/git_spec.sh --format doc     # git versioning tests only
shellspec spec/xattr_spec.sh --format doc   # xattr tests only
INCLUDE_LARGE=true shellspec                # include 100M/1G file tests
```

Spec files: `read_write_spec.sh`, `directory_spec.sh`, `links_spec.sh`, `metadata_spec.sh`, `statfs_spec.sh`, `xattr_spec.sh`, `git_spec.sh`, `stress_spec.sh`, `zzz_pifs_spec.sh`.

`spec_helper.sh`: shared setup, atomic lock, sentinel-based mount detection, fd 3-9 cleanup.

## Development Workflow

- TDD: write failing tests first, then implement
- Always run `cargo test` + `shellspec` after changes
- Test both storage modes (`chunked` and `whole-file`)
- Kill stale pifs mounts before re-running shellspec if binary changed: `pkill -f "target/debug/pifs"`
- Commit with `Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>`

## Git Versioning (--git)

- Git repo stored separately from MDD (`--git-dir` / `--work-tree`)
- Background thread debounces events (2s), batches into commits
- Events: FileChanged, FileDeleted, DirCreated, DirDeleted, Renamed, MetadataChanged
- No `.git` in the MDD → no filtering needed in FUSE ops
