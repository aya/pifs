
static int pifs_ftruncate(const char *path, off_t length,
                          struct fuse_file_info *info)
{
  errno=0;
  options.retour_fonction = ftruncate(info->fh, length * 2);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",path);
  PIFS_RETOUR()
}

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
