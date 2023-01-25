/*
static int pifs_readlink(const char *path, char *buf, size_t bufsiz)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = readlink(options.full_path, buf, bufsiz - 1);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  if (options.retour_fonction == -1) {
    return -errno;
  }

  buf[options.retour_fonction] = '\0';
  return 0;
}
*/
/*
static int pifs_mknod(const char *path, mode_t mode, dev_t dev)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = mknod(options.full_path, mode, dev);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  PIFS_RETOUR()
}
*/
/*
static int pifs_mkdir(const char *path, mode_t mode)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = mkdir(options.full_path, mode | S_IFDIR);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  PIFS_RETOUR()
}
*/
/*
static int pifs_rmdir(const char *path)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = rmdir(options.full_path);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  PIFS_RETOUR()
}
*/
/*
static int pifs_link(const char *oldpath, const char *newpath)
{
  errno=0;
  FULL_PATH(newpath);
  options.retour_fonction = link(oldpath, options.full_path);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et oldpath=%s\n",options.full_path,oldpath);
  PIFS_RETOUR()
}
*/
/*
static int pifs_chmod(const char *path, unsigned int mode, struct fuse_file_info *info)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = chmod(options.full_path, mode);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et mode=%d\n",options.full_path,mode);
  PIFS_RETOUR()
}
*/
/*
static int pifs_chown(const char *path, uid_t owner, gid_t group, struct fuse_file_info *info)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = chown(options.full_path, owner, group);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et owner=%d groupe=%d\n",options.full_path,owner,group);
  PIFS_RETOUR()
}
*/
/*
static int pifs_truncate(const char *path, off_t length,  struct fuse_file_info *info)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = truncate(options.full_path, length * 2);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et length=%d\n",options.full_path,length * 2);
  PIFS_RETOUR()
}
*/
/*
static int pifs_statfs(const char *path, struct statvfs *buf)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = statvfs(options.full_path, buf);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  PIFS_RETOUR()
}
*/

/*
static int pifs_fsync(const char *path, int datasync,
                      struct fuse_file_info *info)
{
  errno=0;
  options.retour_fonction = datasync ? fdatasync(info->fh) : fsync(info->fh);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et info->fh=%d\n",path,info->fh);
//  qui_est_ce(info->fh);
  PIFS_RETOUR()
}
*/
#ifdef HAVE_SETXATTR
/* xattr operations are optional and can safely be left unimplemented */
static int pifs_setxattr(const char *path, const char *name, const char *value,
                         size_t size, int flags)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = setxattr(options.full_path, name, value, size, flags);
  PIFS_TRACE(options.retour_fonction,"pour path=%s name=%s\n",options.full_path,name);
  PIFS_RETOUR()
}

static int pifs_getxattr(const char *path, const char *name, char *value,
                         size_t size)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = getxattr(options.full_path, name, value, size);
  PIFS_TRACE(options.retour_fonction,"pour path=%s name=%s\n",options.full_path,name);
  PIFS_RETOUR()
}

static int pifs_listxattr(const char *path, char *list, size_t size)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = listxattr(options.full_path, list, size);
  PIFS_TRACE(options.retour_fonction,"pour path=%s list=%s\n",options.full_path,list);
  PIFS_RETOUR()
}

static int pifs_removexattr(const char *path, const char *name)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = removexattr(options.full_path, name);
  PIFS_TRACE(options.retour_fonction,"pour path=%s name=%s\n",options.full_path,name);
  PIFS_RETOUR()
}
#endif // HAVE_SETXATTR

/*
static int pifs_fsyncdir(const char *path, int datasync,
                         struct fuse_file_info *info)
{
  errno=0;
  int fd = dirfd((DIR *)info->fh);
  if (fd == -1) {
    PIFS_TRACE(fd,"pas de dirfd pour path=%s info->fh=%d\n",path,info->fh);
    return -errno;
  }

  options.retour_fonction = datasync ? fdatasync(fd) : fsync(fd);
  PIFS_TRACE(options.retour_fonction,"pour path=%s info->fh=%d\n",path,info->fh);
 PIFS_RETOUR()
}
*/
/*
static int pifs_access(const char *path, int mode)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = access(options.full_path, mode);
  PIFS_TRACE(options.retour_fonction,"pour path=%s mode=%d\n",options.full_path,mode);
  #ifdef DEBUG_PIFS
   if (options.retour_fonction == -1) { 
       PIFS_TRACE(options.retour_fonction,"EXIT du programme sous pifs_access"); 
	   exit(-9);
   }
  #endif
  PIFS_RETOUR()
}
*/
/*
static int pifs_ftruncate(const char *path, off_t length,
                          struct fuse_file_info *info)
{
  errno=0;
  options.retour_fonction = ftruncate(info->fh, length * 2);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",path);
  PIFS_RETOUR()
}
*/
/*
static int pifs_fgetattr(const char *path, struct stat *buf,
                        struct fuse_file_info *info)
{
  errno=0;
  options.retour_fonction = fstat(info->fh, buf);
  PIFS_TRACE(options.retour_fonction,"pour path=%s info->fh=%d\n",path,info-fh);
  PIFS_RETOUR()
}
*/
