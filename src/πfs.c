/*  Copyright (C) 2012 Philip Langdale
 *  Copyright (C) 2022 Yann Autissier
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Library General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program; if not, write to the Free Software
 *  Foundation, Inc., 59 Temple Place - Suite 330, Boston, MA 02111-1307, USA.
 */

#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#define PIFS_VERSION "0.0.1"
#define FUSE_USE_VERSION 31

#include <assert.h>
#include <dirent.h>
#include <errno.h>
#include <fuse.h>
#include <libgen.h>
#include <limits.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/statvfs.h>
#ifdef HAVE_SETXATTR
#include <sys/xattr.h>
#endif // HAVE_SETXATTR
#include <unistd.h>

struct options
{
  char *dir;
  char *mdd;
  char *log;
  int help;
  int version;
} options;

/* macro to define options */
#define PIFS_OPTIONS(t, p, v) { t, offsetof(struct options, p), v }
static struct fuse_opt pifs_opts[] = {
  PIFS_OPTIONS("-h", help, 1),
  PIFS_OPTIONS("--help", help, 1),
  PIFS_OPTIONS("log=%s", log, 0),
  PIFS_OPTIONS("mdd=%s", mdd, 0),
  PIFS_OPTIONS("-V", version, 1),
  PIFS_OPTIONS("--version", version, 1),
  FUSE_OPT_END
};

/* define debug infos in log file */
#define FUSE_LOG(ret,...) \
  fuse_log(FUSE_LOG_INFO,"pid=%-6d clock=%-8ld %-16s %-6d %-32s",getpid(),clock(),__FUNCTION__,ret,ret < 0 ? strerror(errno) : ""); \
  fuse_log(FUSE_LOG_INFO,__VA_ARGS__); \

/* define metadata file path */
#define MDD_PATH(path) \
  char mdd_path[PATH_MAX]; \
  snprintf(mdd_path, PATH_MAX, "%s%s", options.mdd, path); \

/** Get file attributes
 *
 * Similar to stat().  The 'st_dev' and 'st_blksize' fields are
 * ignored. The 'st_ino' field is ignored except if the 'use_ino'
 * mount option is given. In that case it is passed to userspace,
 * but libfuse and the kernel will still assign a different
 * inode for internal use (called the "nodeid").
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
 * int (*getattr) (const char *, struct stat *, struct fuse_file_info *fi);
 *
 * struct stat {
 *    dev_t     st_dev;      ID of device containing file
 *    ino_t     st_ino;      inode number
 *    mode_t    st_mode;     protection
 *    nlink_t   st_nlink;    number of hard links
 *    uid_t     st_uid;      user ID of owner
 *    gid_t     st_gid;      group ID of owner
 *    dev_t     st_rdev;     device ID (if special file)
 *    off_t     st_size;     total size, in bytes
 *    blksize_t st_blksize;  blocksize for file system I/O
 *    blkcnt_t  st_blocks;   number of 512B blocks allocated
 *    time_t    st_atime;    time of last access
 *    time_t    st_mtime;    time of last modification
 *    time_t    st_ctime;    time of last status change
 *  };
**/
static int pifs_getattr(const char *path, struct stat *buf, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = lstat(mdd_path, buf);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

/** Read the target of a symbolic link
 *
 * The buffer should be filled with a null terminated string.  The
 * buffer size argument includes the space for the terminating
 * null character.	If the linkname is too long to fit in the
 * buffer, it should be truncated.	The return value should be 0
 * for success.
**/
static int pifs_readlink(const char *path, char *buf, size_t bufsiz)
{
  MDD_PATH(path);
  int ret = readlink(mdd_path, buf, bufsiz - 1);
  if (ret == -1) {
    FUSE_LOG(ret,"mdd=%s\n",mdd_path);
    return -errno;
  }

  buf[ret] = '\0';
  FUSE_LOG(ret,"mdd=%s, buf=%s, bufsiz=%zu\n",mdd_path,buf,bufsiz);
  return 0;
}

/** Create a file node
 *
 * This is called for creation of all non-directory, non-symlink
 * nodes.  If the filesystem defines a create() method, then for
 * regular files that will be called instead.
**/
static int pifs_mknod(const char *path, mode_t mode, dev_t dev)
{
  MDD_PATH(path);
  int ret = mknod(mdd_path, mode, dev);
  FUSE_LOG(ret,"mdd=%s, mode=%o\n",mdd_path,mode);
  return ret == -1 ? -errno : ret;
}

/** Create a directory
 *
 * Note that the mode argument may not have the type specification
 * bits set, i.e. S_ISDIR(mode) can be false.  To obtain the
 * correct directory type bits use  mode|S_IFDIR
**/
static int pifs_mkdir(const char *path, mode_t mode)
{
  MDD_PATH(path);
  int ret = mkdir(mdd_path, mode | S_IFDIR);
  FUSE_LOG(ret,"mdd=%s, mode=%o\n",mdd_path,mode);
  return ret == -1 ? -errno : ret;
}

/** Remove a file **/
static int pifs_unlink(const char *path)
{
  MDD_PATH(path);
  int ret = unlink(mdd_path);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

/** Remove a directory **/
static int pifs_rmdir(const char *path)
{
  MDD_PATH(path);
  int ret = rmdir(mdd_path);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

/** Create a symbolic link **/
static int pifs_symlink(const char *oldpath, const char *newpath)
{
  MDD_PATH(newpath);
  int ret = symlink(oldpath, mdd_path);
  FUSE_LOG(ret,"mdd=%s, old=%s\n",mdd_path,oldpath);
  return ret == -1 ? -errno : ret;
}

/** Rename a file
 *
 * *flags* may be `RENAME_EXCHANGE` or `RENAME_NOREPLACE`. If
 * RENAME_NOREPLACE is specified, the filesystem must not
 * overwrite *newname* if it exists and return an error
 * instead. If `RENAME_EXCHANGE` is specified, the filesystem
 * must atomically exchange the two files, i.e. both must
 * exist and neither may be deleted.
**/
static int pifs_rename(const char *oldpath, const char *newpath, unsigned int flags)
{
  MDD_PATH(newpath);
  char old_path[PATH_MAX];
  snprintf(old_path, PATH_MAX, "%s%s", options.mdd, oldpath);
  if ((flags == 0) && (access(mdd_path, R_OK) == 0)) {
    FUSE_LOG(-1,"mdd=%s, old=%s, flags=%o\n",mdd_path,old_path,flags);
    return -1;
  }
  int ret = rename(old_path, mdd_path);
  FUSE_LOG(ret,"mdd=%s, old=%s, flags=%o\n",mdd_path,old_path,flags);
  return ret == -1 ? -errno : ret;
}

/** Create a hard link to a file **/
static int pifs_link(const char *oldpath, const char *newpath)
{
  MDD_PATH(newpath);
  int ret = link(oldpath, mdd_path);
  FUSE_LOG(ret,"mdd=%s, old=%s\n",mdd_path,oldpath);
  return ret == -1 ? -errno : ret;
}

/** Change the permission bits of a file
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
 * int chmod(const char *, mode_t, struct fuse_file_info *fi);
**/
static int pifs_chmod(const char *path, mode_t mode, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = chmod(mdd_path, mode);
  FUSE_LOG(ret,"mdd=%s, mode=%o\n",mdd_path,mode);
  return ret == -1 ? -errno : ret;
}

/** Change the owner and group of a file
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
 * int chown(const char *, uid_t, gid_t, struct fuse_file_info *fi);
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.
**/
static int pifs_chown(const char *path, uid_t owner, gid_t group, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = chown(mdd_path, owner, group);
  FUSE_LOG(ret,"mdd=%s, owner=%d, group=%d\n",mdd_path,owner,group);
  return ret == -1 ? -errno : ret;
}

/** Change the size of a file
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
 int truncate(const char *, off_t, struct fuse_file_info *fi);
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.
**/
static int pifs_truncate(const char *path, off_t length, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = truncate(mdd_path, length);
  FUSE_LOG(ret,"mdd=%s, length=%jd\n",mdd_path,(intmax_t)length);
  return ret == -1 ? -errno : ret;
}

/** Open a file
 *
 * Open flags are available in fi->flags. The following rules
 * apply.
 *
 *  - Creation (O_CREAT, O_EXCL, O_NOCTTY) flags will be
 *    filtered out / handled by the kernel.
 *
 *  - Access modes (O_RDONLY, O_WRONLY, O_RDWR, O_EXEC, O_SEARCH)
 *    should be used by the filesystem to check if the operation is
 *    permitted.  If the ``-o default_permissions`` mount option is
 *    given, this check is already done by the kernel before calling
 *    open() and may thus be omitted by the filesystem.
 *
 *  - When writeback caching is enabled, the kernel may send
 *    read requests even for files opened with O_WRONLY. The
 *    filesystem should be prepared to handle this.
 *
 *  - When writeback caching is disabled, the filesystem is
 *    expected to properly handle the O_APPEND flag and ensure
 *    that each write is appending to the end of the file.
 *
 *  - When writeback caching is enabled, the kernel will
 *    handle O_APPEND. However, unless all changes to the file
 *    come through the kernel this will not work reliably. The
 *    filesystem should thus either ignore the O_APPEND flag
 *    (and let the kernel handle it), or return an error
 *    (indicating that reliably O_APPEND is not available).
 *
 * Filesystem may store an arbitrary file handle (pointer,
 * index, etc) in fi->fh, and use this in other all other file
 * operations (read, write, flush, release, fsync).
 *
 * Filesystem may also implement stateless file I/O and not store
 * anything in fi->fh.
 *
 * There are also some flags (direct_io, keep_cache) which the
 * filesystem may set in fi, to change the way the file is opened.
 * See fuse_file_info structure in <fuse_common.h> for more details.
 *
 * If this request is answered with an error code of ENOSYS
 * and FUSE_CAP_NO_OPEN_SUPPORT is set in
 * `fuse_conn_info.capable`, this is treated as success and
 * future calls to open will also succeed without being send
 * to the filesystem process.
**/
static int pifs_open(const char *path, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = open(mdd_path, info->flags);
  FUSE_LOG(ret,"mdd=%s, flags=%o\n",mdd_path,info->flags);
  if (ret != -1) info->fh = ret;
  return ret == -1 ? -errno : 0;
}

/** Read data from an open file
 *
 * Read should return exactly the number of bytes requested except
 * on EOF or error, otherwise the rest of the data will be
 * substituted with zeroes.	 An exception to this is when the
 * 'direct_io' mount option is specified, in which case the return
 * value of the read system call will reflect the return value of
 * this operation.
**/
static int pifs_read(const char *path, char *buf, size_t count, off_t offset,
                     struct fuse_file_info *info)
{
  char buffer[4096];
  FILE *fp;
  int size = 0;
  int ret = lseek(info->fh, offset, SEEK_SET);
  if (ret == -1) {
    return -errno;
  }
  dup2(info->fh, STDIN_FILENO);
  // fp = popen("ipfs cat", "r");
  fp = popen("cat", "r");
  if (fp == 0) {
    perror("popen(3) failed");
    return -1;
  }
  do {
    ret = fread(buffer, sizeof(char), sizeof(buffer)-1, fp);
    if (ret == -1 && errno != EAGAIN) {
      return -errno;
    }
    sprintf(&buf[offset+size], "%s", buffer);
    size += ret;
    count -= ret;
  } while( ret == sizeof(buffer)-1 && count > 0 );
  buf[offset + size] = '\0';

  pclose(fp);
  return size;
}

/** Write data to an open file
 *
 * Write should return exactly the number of bytes requested
 * except on error.	 An exception to this is when the 'direct_io'
 * mount option is specified (see read operation).
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.
**/
static int pifs_write(const char *path, const char *buf, size_t count,
                      off_t offset, struct fuse_file_info *info)
{
  int fd[2];
  int ret = lseek(info->fh, offset, SEEK_SET);
  if (ret == -1) {
    return -errno;
  }
  if(pipe(fd)){
    perror("pipe(2) failed");
    return -1;
  }
  switch(fork()){
    case -1:
      perror("fork(2) failed");
      return -1;
    case 0:
      // child
      dup2(fd[0], STDIN_FILENO);
      dup2(info->fh, STDOUT_FILENO);
      close(fd[0]);
      close(fd[1]);
      // ret = execlp("ipfs", "ipfs", "add", "-q",  NULL);
      ret = execlp("cat", "cat",  NULL);
      if (ret == -1) {
        return -errno;
      }
      break;
    default:
      // parent
      close(fd[0]);
      ret = write(fd[1], buf, count);
      if (ret == -1) {
        return -errno;
      }
      close(fd[1]);
  }

  return count;
}

/** Get file system statistics
 *
 * The 'f_favail', 'f_fsid' and 'f_flag' fields are ignored
**/
static int pifs_statfs(const char *path, struct statvfs *buf)
{
  MDD_PATH(path);
  int ret = statvfs(mdd_path, buf);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

/** Possibly flush cached data
 *
 * BIG NOTE: This is not equivalent to fsync().  It's not a
 * request to sync dirty data.
 *
 * Flush is called on each close() of a file descriptor, as opposed to
 * release which is called on the close of the last file descriptor for
 * a file.  Under Linux, errors returned by flush() will be passed to
 * userspace as errors from close(), so flush() is a good place to write
 * back any cached dirty data. However, many applications ignore errors
 * on close(), and on non-Linux systems, close() may succeed even if flush()
 * returns an error. For these reasons, filesystems should not assume
 * that errors returned by flush will ever be noticed or even
 * delivered.
 *
 * NOTE: The flush() method may be called more than once for each
 * open().  This happens if more than one file descriptor refers to an
 * open file handle, e.g. due to dup(), dup2() or fork() calls.  It is
 * not possible to determine if a flush is final, so each flush should
 * be treated equally.  Multiple write-flush sequences are relatively
 * rare, so this shouldn't be a problem.
 *
 * Filesystems shouldn't assume that flush will be called at any
 * particular point.  It may be called more times than expected, or not
 * at all.
 *
 * [close]: http://pubs.opengroup.org/onlinepubs/9699919799/functions/close.html
**/
int pifs_flush(const char *, struct fuse_file_info *info);

/** Release an open file
 *
 * Release is called when there are no more references to an open
 * file: all file descriptors are closed and all memory mappings
 * are unmapped.
 *
 * For every open() call there will be exactly one release() call
 * with the same flags and file handle.  It is possible to
 * have a file opened more than once, in which case only the last
 * release will mean, that no more reads/writes will happen on the
 * file.  The return value of release is ignored.
**/
static int pifs_release(const char *path, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = close(info->fh);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

/** Synchronize file contents
 *
 * If the datasync parameter is non-zero, then only the user data
 * should be flushed, not the meta data.
**/
static int pifs_fsync(const char *path, int datasync,
                      struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = datasync ? fdatasync(info->fh) : fsync(info->fh);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

#ifdef HAVE_SETXATTR
/** Set extended attributes **/
static int pifs_setxattr(const char *path, const char *name, const char *value,
                         size_t size, int flags)
{
  MDD_PATH(path);
  int ret = setxattr(mdd_path, name, value, size, flags);
  FUSE_LOG(ret,"mdd=%s, name=%s, value=%s, size=%zu, flags=%o\n",mdd_path,name,value,size,flags);
  return ret == -1 ? -errno : ret;
}

/** Get extended attributes **/
static int pifs_getxattr(const char *path, const char *name, char *value,
                         size_t size)
{
  MDD_PATH(path);
  int ret = getxattr(mdd_path, name, value, size);
  FUSE_LOG(ret,"mdd=%s, name=%s, value=%s, size=%zu\n",mdd_path,name,value,size);
  return ret == -1 ? -errno : ret;
}

/** List extended attributes **/
static int pifs_listxattr(const char *path, char *list, size_t size)
{
  MDD_PATH(path);
  int ret = listxattr(mdd_path, list, size);
  FUSE_LOG(ret,"mdd=%s, list=%s, size=%zu\n",mdd_path,list,size);
  return ret == -1 ? -errno : ret;
}

/** Remove extended attributes **/
static int pifs_removexattr(const char *path, const char *name)
{
  MDD_PATH(path);
  int ret = removexattr(mdd_path, name);
  FUSE_LOG(ret,"mdd=%s, name=%s\n",mdd_path,name);
  return ret == -1 ? -errno : ret;
}
#endif // HAVE_SETXATTR

/** Open directory
 *
 * Unless the 'default_permissions' mount option is given,
 * this method should check if opendir is permitted for this
 * directory. Optionally opendir may also return an arbitrary
 * filehandle in the fuse_file_info structure, which will be
 * passed to readdir, releasedir and fsyncdir.
**/
static int pifs_opendir(const char *path, struct fuse_file_info *info)
{
  MDD_PATH(path);
  DIR *dir = opendir(mdd_path);
  if (!dir){
    FUSE_LOG(-1,"mdd=%s\n",mdd_path);
    return -errno;
  }
  FUSE_LOG(0,"mdd=%s\n",mdd_path);
  info->fh = (uint64_t) dir;
  return !dir ? -errno : 0;
}

/** Read directory
 *
 * The filesystem may choose between two modes of operation:
 *
 * 1) The readdir implementation ignores the offset parameter, and
 * passes zero to the filler function's offset.  The filler
 * function will not return '1' (unless an error happens), so the
 * whole directory is read in a single readdir operation.
 *
 * 2) The readdir implementation keeps track of the offsets of the
 * directory entries.  It uses the offset parameter and always
 * passes non-zero offset to the filler function.  When the buffer
 * is full (or an error happens) the filler function will return
 * '1'.
 *
 * When FUSE_READDIR_PLUS is not set, only some parameters of the
 * fill function (the fuse_fill_dir_t parameter) are actually used:
 * The file type (which is part of stat::st_mode) is used. And if
 * fuse_config::use_ino is set, the inode (stat::st_ino) is also
 * used. The other fields are ignored when FUSE_READDIR_PLUS is not
 * set.
**/
static int pifs_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
                       off_t offset, struct fuse_file_info *info, enum fuse_readdir_flags flags)
{
  MDD_PATH(path);
  DIR *dir = (DIR *) info->fh;
  if (offset) {
    seekdir(dir, offset);
  }

  int ret;
  do {
    errno = 0;
    struct dirent *de = readdir(dir);
    if (!de) { 
      if (errno) {
        FUSE_LOG(-1,"mdd=%s\n",mdd_path);
        return -errno;
      } else {
        break;
      }
    }

    ret = filler(buf, de->d_name, NULL, de->d_off, 0);
    FUSE_LOG(ret,"mdd=%s, name=%s\n",mdd_path,de->d_name);
  } while (ret == 0);

  return 0;
}

/** Release directory
 *
 * If the directory has been removed after the call to opendir, the
 * path parameter will be NULL.
**/
static int pifs_releasedir(const char *path, struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = closedir((DIR *)info->fh);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  return ret == -1 ? -errno : ret;
}

/** Synchronize directory contents
 *
 * If the directory has been removed after the call to opendir, the
 * path parameter will be NULL.
 *
 * If the datasync parameter is non-zero, then only the user data
 * should be flushed, not the meta data
**/
static int pifs_fsyncdir(const char *path, int datasync,
                         struct fuse_file_info *info)
{
  MDD_PATH(path);
  int fd = dirfd((DIR *)info->fh);
  if (fd == -1) {
    FUSE_LOG(-1,"mdd=%s\n",mdd_path);
    return -errno;
  }

  int ret = datasync ? fdatasync(fd) : fsync(fd);
  FUSE_LOG(ret,"mdd=%s, datasync=%d\n",mdd_path,datasync);
  return ret == -1 ? -errno : ret;
}

/**
 * Initialize filesystem
 *
 * The return value will passed in the `private_data` field of
 * `struct fuse_context` to all file operations, and as a
 * parameter to the destroy() method. It overrides the initial
 * value provided to fuse_main() / fuse_new().
**/
void *pifs_init(struct fuse_conn_info *conn,
                struct fuse_config *cfg);

/**
 * Clean up filesystem
 *
 * Called on filesystem exit.
**/
void pifs_destroy(void *private_data);

/**
 * Check file access permissions
 *
 * This will be called for the access() system call.  If the
 * 'default_permissions' mount option is given, this method is not
 * called.
 *
 * This method is not called under Linux kernel versions 2.4.x
**/
static int pifs_access(const char *path, int mode)
{
  MDD_PATH(path);
  int ret = access(mdd_path, mode);
  FUSE_LOG(ret,"mdd=%s, mode=%d\n",mdd_path,mode);
  return ret == -1 ? -errno : ret;
}

/**
 * Create and open a file
 *
 * If the file does not exist, first create it with the specified
 * mode, and then open it.
 *
 * If this method is not implemented or under Linux kernel
 * versions earlier than 2.6.15, the mknod() and open() methods
 * will be called instead.
**/
static int pifs_create(const char *path, mode_t mode,
                       struct fuse_file_info *info)
{
  MDD_PATH(path);
  int ret = creat(mdd_path, mode);
  FUSE_LOG(ret,"mdd=%s, mode=%o\n",mdd_path,mode);
  info->fh = ret;
  return ret == -1 ? -errno : 0;
}

/**
 * Perform POSIX file locking operation
 *
 * The cmd argument will be either F_GETLK, F_SETLK or F_SETLKW.
 *
 * For the meaning of fields in 'struct flock' see the man page
 * for fcntl(2).  The l_whence field will always be set to
 * SEEK_SET.
 *
 * For checking lock ownership, the 'fuse_file_info->owner'
 * argument must be used.
 *
 * For F_GETLK operation, the library will first check currently
 * held locks, and if a conflicting lock is found it will return
 * information without calling this method.	 This ensures, that
 * for local locks the l_pid field is correctly filled in.	The
 * results may not be accurate in case of race conditions and in
 * the presence of hard links, but it's unlikely that an
 * application would rely on accurate GETLK results in these
 * cases.  If a conflicting lock is not found, this method will be
 * called, and the filesystem may fill out l_pid by a meaningful
 * value, or it may leave this field zero.
 *
 * For F_SETLK and F_SETLKW the l_pid field will be set to the pid
 * of the process performing the locking operation.
 *
 * Note: if this method is not implemented, the kernel will still
 * allow file locking to work locally.  Hence it is only
 * interesting for network filesystems and similar.
 *
 * struct flock {
 *   ...
 *   short l_type;    * Type of lock: F_RDLCK,
 *                       F_WRLCK, F_UNLCK
 *   short l_whence;  * How to interpret l_start:
 *                       SEEK_SET, SEEK_CUR, SEEK_END
 *   off_t l_start;   * Starting offset for lock
 *   off_t l_len;     * Number of bytes to lock
 *   pid_t l_pid;     * PID of process blocking our lock
 *                       (F_GETLK only)
 *   ...
 * };
 *
 * F_SETLK (struct flock *)
 *     Acquire a lock (when l_type is F_RDLCK or F_WRLCK) or release
 *     a lock (when l_type is F_UNLCK) on the bytes specified by the
 *     l_whence, l_start, and l_len fields of lock. If a conflicting
 *     lock is held by another process, this call returns -1 and sets
 *     errno to EACCES or EAGAIN.
 * F_SETLKW (struct flock *)
 *     As for F_SETLK, but if a conflicting lock is held on the file,
 *     then wait for that lock to be released. If a signal is caught
 *     while waiting, then the call is interrupted and (after the
 *     signal handler has returned) returns immediately (with return
 *     value -1 and errno set to EINTR; see signal(7)).
 * F_GETLK (struct flock *)
 *     On input to this call, lock describes a lock we would like to
 *     place on the file. If the lock could be placed, fcntl() does
 *     not actually place it, but returns F_UNLCK in the l_type field
 *     of lock and leaves the other fields of the structure unchanged.
 *     If one or more incompatible locks would prevent this lock being
 *     placed, then fcntl() returns details about one of these locks
 *     in the l_type, l_whence, l_start, and l_len fields of lock and
 *     sets l_pid to be the PID of the process holding that lock.
**/
static int pifs_lock(const char *path, struct fuse_file_info *info, int cmd,
                     struct flock *lock)
{
  MDD_PATH(path);
  int ret = fcntl(info->fh, cmd, lock);
  FUSE_LOG(ret,"mdd=%s, cmd=%d(%s), lock=%d(%s)\n",mdd_path,
           cmd,(cmd==F_GETLK?"F_GETLK":(cmd==F_SETLK?"F_SETLK":"F_SETLKW")),
           lock->l_type,(lock->l_type == F_RDLCK? "F_RDLCK":(lock->l_type == F_WRLCK?"F_WRLCK":"F_UNLCK")));
  return ret == -1 ? -errno : ret;
}

/**
 * Change the access and modification times of a file with
 * nanosecond resolution
 *
 * This supersedes the old utime() interface.  New applications
 * should use this.
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
 *
 * See the utimensat(2) man page for details.
**/
static int pifs_utimens(const char *path, const struct timespec times[2], struct fuse_file_info *info)
{
  MDD_PATH(path);
  DIR *dir = opendir(options.mdd);
  if (!dir) {
    FUSE_LOG(-1,"mdd=%s\n",options.mdd);
    return -errno;
  }
  int ret = utimensat(dirfd(dir), basename((char *) path), times, 0);
  FUSE_LOG(ret,"mdd=%s\n",mdd_path);
  closedir(dir);
  return ret == -1 ? -errno : ret;
}

static struct fuse_operations pifs_ops = {
  .getattr = pifs_getattr,
  .readlink = pifs_readlink,
  .mknod = pifs_mknod,
  .mkdir = pifs_mkdir,
  .rmdir = pifs_rmdir,
  .unlink = pifs_unlink,
  .symlink = pifs_symlink,
  .rename = pifs_rename,
  .link = pifs_link,
  .chmod = pifs_chmod,
  .chown = pifs_chown,
  .truncate = pifs_truncate,
  .utimens = pifs_utimens,
  .open = pifs_open,
  .read = pifs_read,
  .write = pifs_write,
  .statfs = pifs_statfs,
  .release = pifs_release,
  .fsync = pifs_fsync,
#ifdef HAVE_SETXATTR
  .setxattr = pifs_setxattr,
  .getxattr = pifs_getxattr,
  .listxattr = pifs_listxattr,
  .removexattr = pifs_removexattr,
#endif // HAVE_SETXATTR
  .opendir = pifs_opendir,
  .readdir = pifs_readdir,
  .releasedir = pifs_releasedir,
  .fsyncdir = pifs_fsyncdir,
  .access = pifs_access,
  .create = pifs_create,
  .lock = pifs_lock,
};

static void pifs_log(enum fuse_log_level level, const char *fmt, va_list ap)
{
  FILE *fp = fopen(options.log, "a");
  vfprintf(fp, fmt, ap);
  fclose(fp);
}

static void usage(const char *progname)
{
  printf("Usage: %s [options] <mountpoint>\n"
  "pifs options:\n"
         "    -o log=<file>          log file to trace fuse calls\n"
         "    -o mdd=<directory>     metadata directory to store ipfs hashes\n"
         "\n", progname);
}

int main (int argc, char *argv[])
{
  int ret;
  FILE *fp;
  struct fuse_args args = FUSE_ARGS_INIT(argc, argv);

  memset(&options, 0, sizeof(struct options));
  if (fuse_opt_parse(&args, &options, pifs_opts, NULL) == -1) {
    return -1;
  }

  if (options.help) {
    usage(argv[0]);
    assert(fuse_opt_add_arg(&args, "--help") == 0);
    args.argv[0][0] = '\0';
  } else if (options.version) {
      fprintf(stdout, "pifs version %s\n", PIFS_VERSION);
      return 0;
  } else {

    options.dir = args.argv[1];
    if (!options.mdd) {
      fprintf(stderr,
              "%s: Metadata directory must be specified with -o mdd=<directory>\n",
              argv[0]);
      return 1;
    }

    if (access(options.mdd, R_OK | W_OK | X_OK) == -1) {
      fprintf(stderr, "%s: Cannot access metadata directory '%s': %s\n",
              argv[0], options.mdd, strerror(errno));
      return 1;
    }

    if (options.log != NULL) {
      if ((fp = fopen(options.log, "w")) == NULL) {
        fprintf(stderr, "%s: Cannot write to log file '%s': %s\n",
                argv[0], options.log, strerror(errno));
        return 1;
      }
      fclose(fp);
      fuse_set_log_func((fuse_log_func_t) pifs_log);
      FUSE_LOG(0,"bin=%s, dir=%s, mdd=%s, log=%s\n", args.argv[0], options.dir, options.mdd ,options.log);
    }

    if (options.dir == NULL) {
      fprintf(stderr, "%s: No mountpoint specified\n", argv[0]);
      return 2;
    } else if (strcmp(options.dir,"-o") == 0) {
      fprintf(stderr, "%s: Invalid option argument '%s'\n",
              argv[0], args.argv[2]);
      return 3;
    } else if (access(options.dir, W_OK | X_OK) == -1) {
      fprintf(stderr, "%s: Cannot mount directory '%s': %s\n",
              argv[0], options.dir, strerror(errno));
      return 4;
    }

  }

  ret = fuse_main(args.argc, args.argv, &pifs_ops, NULL);
/* The following error codes may be returned from fuse_main():
 *   1: Invalid option arguments
 *   2: No mount point specified
 *   3: FUSE setup failed
 *   4: Mounting failed
 *   5: Failed to daemonize (detach from session)
 *   6: Failed to set up signal handlers
 *   7: An error occurred during the life of the file system
 *  @return 0 on success, nonzero on failure
 */
  fuse_opt_free_args(&args);
  return ret;
}
