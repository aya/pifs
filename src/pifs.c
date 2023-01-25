/*  Copyright (C) 2012 Philip Langdale
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

#define FUSE_USE_VERSION 31

//#define FUSE_USE_VERSION 26
//#define HAVE_SETXATTR

#ifdef HAVE_CONFIG_H
//#include <config.h>
#endif
#ifdef linux
/* For pread()/pwrite()/utimensat() */
#define _XOPEN_SOURCE 700
#endif

#include <limits.h>
#include <fuse.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <assert.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/stat.h>
#include <dirent.h>
#include <time.h>
#ifdef HAVE_SETXATTR
 #include <sys/xattr.h>
#endif
#include <sys/wait.h>
/*
#include <sys/xattr.h>
#include <libgen.h>
#include <sys/statvfs.h>
* */
 
#define uid_t unsigned int 
#define gid_t unsigned int 
#define DEBUG_PIFS

/** variables globales */

static struct options {
  char *meta_data;
  char *montage;
  char *fichier_erreur_fuse;
  int show_help;
  char full_path[PATH_MAX];
  int retour_fonction;

} options;

/** macro to define options */

#define PIFS_OPTION(t, p) { t, offsetof(struct options, p),1 }

static const struct fuse_opt pifs_options[] =
{
  PIFS_OPTION("mdd=%s", meta_data),
  PIFS_OPTION("--erreur=%s", fichier_erreur_fuse),
  PIFS_OPTION("-h", show_help),
  PIFS_OPTION("--help", show_help),
  PIFS_OPTION("-help", show_help), 
  FUSE_OPT_END
};
#define PIFS_RETOUR() \
        return ( options.retour_fonction == -1 ? -errno : options.retour_fonction) ;

#define PIFS_READ()  \
		fp = popen("cat", "r");
//		fp = popen("ipfs cat", "r");

#define PIFS_WRITE()  \
		options.retour_fonction = execlp("cat", "cat",  NULL);
//		options.retour_fonction = execlp("ipfs", "ipfs", "add", "-q", "-s", "rabin-262144-524288-1048576", NULL);
//		options.retour_fonction = execlp("ipfs", "ipfs", "add", "-q",  NULL);
		
#define PIFS_TRACE(ret,...) { \
		fuse_log(FUSE_LOG_INFO,"%-7ld %-15.14s ligne=%-7d pid=%-10d ===> ret=%-10d  errno=%-30.29s ",clock(),__FUNCTION__,__LINE__,getpid(),ret,strerror(errno)); \
		fuse_log(FUSE_LOG_INFO,__VA_ARGS__); \
                }
#define FULL_PATH(path) \
		snprintf(options.full_path, PATH_MAX, "%s%s", options.meta_data, path);

/**************************** fonctions *****************************/
static int pifs_getattr(const char *path, struct stat *buf, struct fuse_file_info *info)
{ errno=0;
  FULL_PATH(path);
  options.retour_fonction = lstat(options.full_path, buf);
  if (options.retour_fonction == -1) sleep(0.8);
  //  buf->st_size /= 2;
//  PIFS_TRACE(options.retour_fonction,"POUR path=%s\n",options.full_path);
  PIFS_RETOUR()
}
static int pifs_lock(const char *path, struct fuse_file_info *info, int cmd,
                     struct flock *lock)
{ /* struct flock {
    ...
    short l_type;    * Type of lock: F_RDLCK,
                        F_WRLCK, F_UNLCK 
    short l_whence;  * How to interpret l_start:
                        SEEK_SET, SEEK_CUR, SEEK_END 
    off_t l_start;   * Starting offset for lock 
    off_t l_len;     * Number of bytes to lock 
    pid_t l_pid;     * PID of process blocking our lock
                        (F_GETLK only) 
    ...
};
	
F_SETLK (struct flock *)
    Acquire a lock (when l_type is F_RDLCK or F_WRLCK) or release a lock (when l_type is F_UNLCK) on the bytes specified by the l_whence, l_start, and l_len fields of lock. If a conflicting lock is held by another process, this call returns -1 and sets errno to EACCES or EAGAIN. 
F_SETLKW (struct flock *)
    As for F_SETLK, but if a conflicting lock is held on the file, then wait for that lock to be released. If a signal is caught while waiting, then the call is interrupted and (after the signal handler has returned) returns immediately (with return value -1 and errno set to EINTR; see signal(7)). 
F_GETLK (struct flock *)
    On input to this call, lock describes a lock we would like to place on the file. If the lock could be placed, fcntl() does not actually place it, but returns F_UNLCK in the l_type field of lock and leaves the other fields of the structure unchanged. If one or more incompatible locks would prevent this lock being placed, then fcntl() returns details about one of these locks in the l_type, l_whence, l_start, and l_len fields of lock and sets l_pid to be the PID of the process holding that lock. 
*/ 
  errno=0;
  options.retour_fonction = fcntl(info->fh, cmd, lock);
  PIFS_TRACE(options.retour_fonction,"pour path=%s info->fh=%d cmd=%s demande=%s\n",path,info->fh,
  (cmd==F_GETLK?"F_GETLK":(cmd==F_SETLK?"F_SETLK":"F_SETLKW")),
  (lock->l_type == F_RDLCK? "F_RDLCK":(lock->l_type == F_WRLCK?"F_WRLCK":"F_UNLCK")));
  PIFS_RETOUR()
}
static int pifs_open(const char *path, struct fuse_file_info *info)
{//ouvre un fichier
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = open(options.full_path, info->flags);
  PIFS_TRACE(options.retour_fonction,"pour path=%s avec flags=%d\n",options.full_path,info->flags);
  info->fh = options.retour_fonction;
  return options.retour_fonction == -1 ? -errno : 0;
}
static int pifs_release(const char *path, struct fuse_file_info *info)
{//ferme un fichier ouvert
  errno=0;
  options.retour_fonction = close(info->fh);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et info->fh=%d\n",path,info->fh);
  PIFS_RETOUR()
}

static int pifs_opendir(const char *path, struct fuse_file_info *info)
{//ouvre un repertoire
  DIR *dir;
  errno=0;
  FULL_PATH(path);
  dir = opendir(options.full_path);
  if (!dir){
     PIFS_TRACE(-1,"pas de dir pour path=%s\n",options.full_path);
	 return -errno;
  }
  info->fh = (uint64_t) dir;
  PIFS_TRACE(0,"pour path=%s info->fh=%d\n",options.full_path,info->fh);
  return 0;
}
static int pifs_releasedir(const char *path, struct fuse_file_info *info)
{//ferme un repertoire ouvert
  errno=0;
  options.retour_fonction = closedir((DIR *)info->fh);
  PIFS_TRACE(options.retour_fonction,"pour path=%s info->fh=%d\n",path,info->fh);
  PIFS_RETOUR()
}

static int pifs_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
                       off_t offset, struct fuse_file_info *info, enum fuse_readdir_flags flags)
{//lit le contenu d'un repertoire
  DIR *dir;
  struct dirent *de;
  errno=0;
  dir = (DIR *) info->fh;
  if (offset) {
    seekdir(dir, offset);
  }

  do {
    errno = 0;
    de = readdir(dir);
    if (!de) { 
      if (errno) {
		  PIFS_TRACE(-1,"pas de de pour path=%s info->fh=%d\n",path,info->fh);
         return -errno;
      } else {
         break;
      }
    }
    options.retour_fonction = filler(buf, de->d_name, NULL, de->d_off,0);
//    PIFS_TRACE(options.retour_fonction,"pour path=%s de->d_name=%s\n",path,de->d_name);
  } while (options.retour_fonction == 0);
  PIFS_TRACE(0,"pour path=%s info->fh=%d\n",path,info->fh);
  return 0;
}

static int pifs_unlink(const char *path)
{//supprime un fichier
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = unlink(options.full_path);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  PIFS_RETOUR()
}

static int pifs_symlink(const char *oldpath, const char *newpath)
{//cree un lien symbolique
  errno=0;
  FULL_PATH(newpath);
  options.retour_fonction = symlink(oldpath, options.full_path);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et oldpath=%s\n",options.full_path,oldpath);
  PIFS_RETOUR()
}

static int pifs_rename(const char *oldpath, const char *newpath, unsigned int flags)
{//renomme un fichier 
/*	 * *flags* may be `RENAME_EXCHANGE` or `RENAME_NOREPLACE`. If
	 * RENAME_NOREPLACE is specified, the filesystem must not
	 * overwrite *newname* if it exists and return an error
	 * instead. If `RENAME_EXCHANGE` is specified, the filesystem
	 * must atomically exchange the two files, i.e. both must
	 * exist and neither may be deleted.
*/
/* ************** ATTENTION FLAGS nEST PAS PRIS EN COMPTE */
  errno=0;
  FULL_PATH(newpath);
  options.retour_fonction = rename(oldpath, options.full_path);
  PIFS_TRACE(options.retour_fonction,"pour path=%s et oldpath=%s\n",options.full_path,oldpath);
  PIFS_RETOUR()
}

#if FUSE_USE_VERSION <30
static int pifs_utime(const char *path, struct utimbuf *times)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = utime(options.full_path, times);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",options.full_path);
  PIFS_RETOUR()
}
#else
//	 int (*utimens) (const char *, const struct timespec tv[2], struct fuse_file_info *fi);

static int pifs_utimens(const char *path, const struct timespec times[2], struct fuse_file_info *fi)
{
  DIR *dir;
  errno=0;
  dir = opendir(options.meta_data);
  if (!dir) {
  PIFS_TRACE(0," pas de dir pour path=%s\n",path);
    return -errno;
  }
  options.retour_fonction = utimensat(dirfd(dir), path, times, 0);
  closedir(dir);
  PIFS_TRACE(options.retour_fonction,"pour path=%s\n",path);
  PIFS_RETOUR()
}
#endif


static int pifs_create(const char *path, mode_t mode,
                       struct fuse_file_info *info)
{
  errno=0;
  FULL_PATH(path);
  options.retour_fonction = creat(options.full_path, mode);
  info->fh = options.retour_fonction;
  PIFS_TRACE(options.retour_fonction,"pour path=%s mode=%d\n",options.full_path,mode);
  return options.retour_fonction == -1 ? -errno : 0;
}

/********************************************************************************************/
#ifndef DEBUG_PIFS
static int pifs_read(const char *path, char *buf, size_t count, off_t offset,
                     struct fuse_file_info *info)
{
  char buffer[5];
  int size = 0;
  FILE *fp;
  errno=0;
  options.retour_fonction = lseek(info->fh, offset * 2, SEEK_SET);
  PIFS_TRACE(options.retour_fonction,"pour path=%s info->fh=%d\n",path,info->fh);
  if (options.retour_fonction == -1) {
    return -errno;
  }
  dup2(info->fh, STDIN_FILENO);
  PIFS_READ()
  if (fp == 0) {
    perror("popen(3) failed");
    return -1;
  }
  do {
    options.retour_fonction = fread(buffer, sizeof(char), sizeof(buffer)-1, fp);
    if (options.retour_fonction == -1 && errno != EAGAIN) {
      return -errno;
    }
    sprintf(&buf[offset+size], "%s", buffer);
    size += options.retour_fonction;
    count -= options.retour_fonction;
    PIFS_TRACE(options.retour_fonction,"pour path=%s contenu buffer=%s\n",path,buffer);

  } while ( options.retour_fonction == sizeof(buffer)-1 && count > 0 );
  buf[offset + size] = '\0';

  pclose(fp);
  return size;
}

static int pifs_write(const char *path, const char *buf, size_t count,
                      off_t offset, struct fuse_file_info *info)
{
  int fd[2];
  errno=0;
  options.retour_fonction = lseek(info->fh, offset * 2, SEEK_SET);
  PIFS_TRACE(options.retour_fonction,"pour path=%s info->fh=%d\n",path,info->fh);
  if (options.retour_fonction == -1) {
    return -errno;
  }
  if (pipe(fd)){
    perror("pipe(2) failed");
    return -1;
  }
  switch (fork()){
    case -1:
      perror("fork(2) failed");
      return -1;
    case 0:
      // child
      dup2(fd[0], STDIN_FILENO);
      dup2(info->fh, STDOUT_FILENO);
      close(fd[0]);
      close(fd[1]);
      PIFS_TRACE(options.retour_fonction,"pour path=%s avant execlp\n",path);
      PIFS_WRITE()
      if (options.retour_fonction == -1) {
        PIFS_TRACE(options.retour_fonction,"pour path=%s count=%d\n",path,count);
        return -errno;
      }
      break;
    default:
      // parent
      close(fd[0]);
      options.retour_fonction = write(fd[1], buf, count);
      if (options.retour_fonction == -1) {
        PIFS_TRACE(options.retour_fonction,"pour path=%s count=%d\n",path,count);
        return -errno;
      }
      close(fd[1]);
  }
  PIFS_TRACE(options.retour_fonction,"pour path=%s count=%d\n",path,count);

  return count;
}
#else //#ifdnef DEBUG_PIFS

int max_read(const char *path, char *buf, size_t count, off_t offset,
                     struct fuse_file_info *info){
  errno=0;
  options.retour_fonction=read(info->fh,&buf[offset],count);
  PIFS_TRACE(options.retour_fonction,"pour path=%s count=%d offset=%d info->fh=%d\n",path,count,offset,info->fh);
  PIFS_RETOUR()
}

static int pifs_read_max(const char *path, char *buf, size_t count, off_t offset,
                     struct fuse_file_info *info)
{
  int taille=4096;
  int size = 0;
  char buffer_max[taille];
  FILE *fp;
  errno=0;
  options.retour_fonction = lseek(info->fh, offset * 2, SEEK_SET);
  PIFS_TRACE(options.retour_fonction,"pour path=%s count=%d offset=%d info->fh=%d\n",path,count,offset,info->fh);
  if (options.retour_fonction == -1) {   
	return -errno;
  }

  dup2(info->fh, STDIN_FILENO);
  PIFS_READ()
  if (fp == 0) {
    perror("popen(3) failed");
    return -1;
  }
  do {
    options.retour_fonction = fread(buffer_max,1,taille-1,fp);
    buffer_max[taille] = '\0';
    PIFS_TRACE(options.retour_fonction,"pour path=%s buffer_max=%s\n",path);
//    options.retour_fonction = fread(&buf[offset+size],1,taille,fp);
    if (options.retour_fonction == -1 && errno != EAGAIN) {
      PIFS_TRACE(options.retour_fonction,"pour path=%s \n",path);
      return -errno;
    }
    sprintf(&buf[offset+size], "%s", buffer_max);
    size += options.retour_fonction;
    count -= options.retour_fonction;
    PIFS_TRACE(options.retour_fonction,"pour path=%s count=%d size=%d\n",path,count,size);
  } while ( options.retour_fonction == (taille-1) && count > 0 );
  buf[offset + size] = '\0';
  PIFS_TRACE(options.retour_fonction,"pour path=%s buffer=%s\n",path,&buf[offset]);
  pclose(fp);
  return size;
}

static int pifs_write_max(const char *path, const char *buf, size_t count,
                      off_t offset, struct fuse_file_info *info)
{
  pid_t  w, ch_pid;
  int status;
  int fd[2];
  errno=0;
  FULL_PATH(path)
  options.retour_fonction = lseek(info->fh, offset * 2, SEEK_SET);
  PIFS_TRACE(options.retour_fonction,"path=%s options.meta_data=%s info->fh=%d\n",options.full_path,options.meta_data,info->fh);
  if (options.retour_fonction == -1) {
   return -errno;
  }

//  pid_t ch_pid = lancer_le_fils(argument_program[0] , argument_program, buf,count,info) ;
  if (pipe(fd)){
    perror("pipe(2) failed");
    PIFS_TRACE(-1,"sortie erreur de pifs_write sur pipe(fd)"); 
    return -1;
  }

  ch_pid= fork();

  if (ch_pid == -1){
    perror("erreur sur fork");
    PIFS_TRACE(ch_pid,"sortie de pifs_write pour pid rendu par fwork=-1\n");
  } 
  else if (ch_pid == 0){ // child
    options.retour_fonction=dup2(fd[0], STDIN_FILENO);
    options.retour_fonction=dup2(info->fh, STDOUT_FILENO);
    close(fd[0]);
    close(fd[1]);
    PIFS_TRACE(ch_pid,"CHILD avant execlp()\n");
    PIFS_WRITE()
    PIFS_TRACE(options.retour_fonction,"CHILD apres execlp()\n");
    if (options.retour_fonction == -1) {
      return -errno;
    }
  } 
  else { // parent
//      close(fd[0]);
    PIFS_TRACE(0,"PARENT avant write ch_pid=%d\n",ch_pid);
    options.retour_fonction = write(fd[1], buf, count);// c'est lui qui ecrit en metadonnees
    if (options.retour_fonction == -1) {
      PIFS_TRACE(options.retour_fonction,"PARENT apres write ch_pid=%d\n",ch_pid);
      return -errno;
    }
    //attente de la fin du pid child
//      sleep(1);
    close(fd[1]);
    PIFS_TRACE(options.retour_fonction,"PARENT apres write ch_pid=%d\n",ch_pid);
  } 





  if(ch_pid <0) return -1;
  PIFS_TRACE(0,"path=%s apres lancer le fils renvoie pid=%d alors quon est pid=%d\n",options.full_path,ch_pid,getpid());

  do {
   PIFS_TRACE(0,"path=%s on lance waitpid pour pid=%d alors quon est pid=%d\n",options.full_path,ch_pid,getpid());
   w = waitpid(ch_pid, &status, WUNTRACED | WCONTINUED);
   if (w == -1) {
     PIFS_TRACE(w,"path=%s erreur fonction waitpid\n",options.full_path);
     break;
   }
   else if (w == ECHILD) {
     PIFS_TRACE(w,"path=%s erreur fonction waitpid le pid n'existe pas fonction waitpid\n",options.full_path);
     break;
   }
   else if (w == EINVAL) {
     PIFS_TRACE(w,"path=%s erreur fonction waitpid le parametre est mauvais fonction waitpid\n",options.full_path);
     break;
   }

   if (WIFEXITED(status)) {
     PIFS_TRACE(0,"path=%s exit avec status=%d\n",options.full_path, WEXITSTATUS(status));
   } else if (WIFSIGNALED(status)) {
     PIFS_TRACE(0,"path=%s tue par signal %d\n",options.full_path, WTERMSIG(status));
   } else if (WIFSTOPPED(status)) {
     PIFS_TRACE(0,"path=%s stop par signal %d\n",options.full_path,  WSTOPSIG(status));
   } else if (WIFCONTINUED(status)) {
     PIFS_TRACE(0,"path=%s continue\n",options.full_path,  WSTOPSIG(status));
   }
  } while (!WIFEXITED(status) && !WIFSIGNALED(status));

  PIFS_TRACE(0,"path=%s sortie de pifs_write",options.full_path);
  return count;
}
#endif //#ifdef DEBUG_PIFS
#include "pifs_inutile.c"
static struct fuse_operations pifs_ops = {
  .getattr = pifs_getattr,
//  .readlink = pifs_readlink,
//  .mknod = pifs_mknod,
//  .mkdir = pifs_mkdir,
//  .rmdir = pifs_rmdir,
  .unlink = pifs_unlink,
  .symlink = pifs_symlink,
  .rename = pifs_rename,
//  .link = pifs_link,
//  .chmod = pifs_chmod,
//  .chown = pifs_chown,
//  .truncate = pifs_truncate,
  .open = pifs_open,
#ifdef DEBUG_PIFS
//  .read = pifs_read_max,
  .read=max_read,
  .write = pifs_write_max,
#else
  .read = pifs_read,
  .write = pifs_write,
#endif
//  .statfs = pifs_statfs,
  .release = pifs_release,
//  .fsync = pifs_fsync,
#ifdef HAVE_SETXATTR
/* xattr operations are optional and can safely be left unimplemented */
  .setxattr = pifs_setxattr,
  .getxattr = pifs_getxattr,
  .listxattr = pifs_listxattr,
  .removexattr = pifs_removexattr,
#endif //#ifdef HAVE_SETXATTR
  .opendir = pifs_opendir,
  .readdir = pifs_readdir,
  .releasedir = pifs_releasedir,
//  .fsyncdir = pifs_fsyncdir,
//  .access = pifs_access,
  .create = pifs_create,
//  .ftruncate = pifs_ftruncate,
//  .fgetattr = pifs_fgetattr,
  .lock = pifs_lock,
#if FUSE_USE_VERSION <30
  .utime = pifs_utime,
#else
  .utimens = pifs_utimens,
#endif
//  .flag_nullpath_ok = 1,
};

static void pifs_suivi_erreur_fuse(enum fuse_log_level level, const char *fmt, va_list ap)
{
  FILE *f1;
  f1 = fopen(options.fichier_erreur_fuse, "a");
  vfprintf(f1, fmt, ap);
  fclose(f1);
  sleep(0.5);
}

static void show_help(const char *progname)
{
  printf("usage: %s [options] <repertoire_de_montage>\n "
  "specific [options]:\n"
         "   -h                     cet aide \n"
         "   --help                 aide supplementaire de fuse \n"
         "   --erreur=<fichier log> chemin absolu du fichier de log \n"
         "   -o mdd=<directory>     répertoire des meta-données\n"
         "\n"
"si on pratique un mount on obiendra \n%s on <repertoire_de_montage> type fuse.pifs (rw,nosuid,nodev,relatime,user_id=xxxx,group_id=xxxx)\n\
pour demonter le point de montage faire fusermount3 -u <repertoire_de_montage> ou sudo -fl umount <repertoire_de_montage>\n\n", progname,progname);

}

int main (int argc, char *argv[])
{
  struct fuse_args args = FUSE_ARGS_INIT(argc, argv);
 
  memset(&options, 0, sizeof(struct options));
  options.montage = strdup(argv[argc-1]);
  
  if ((options.retour_fonction=fuse_opt_parse(&args, &options, pifs_options, NULL)) == -1) {
   printf("sortie main erreur_fuse_opt_parse de pifs_mains avec ===> options.retour_fonction=%d  errno=%s\n",options.retour_fonction,strerror(errno));
   exit(1);
  }

  if (options.show_help) {
   show_help(argv[0]);
   assert(fuse_opt_add_arg(&args, "--help") == 0);
   args.argv[0][0] = '\0';
   }
   else if (!options.meta_data) {
    printf("ERROR %s: le repertoire de metadonnees doit etre donné par -o mdd=<directory>\n", argv[0]);
    exit(1);
   }
  else {
//  options.retour_fonction=pifs_access(options.meta_data, R_OK | W_OK | X_OK);
    options.retour_fonction=access(options.meta_data, R_OK | W_OK | X_OK);

    if (options.retour_fonction < 0) {
      printf("%s: impossible d'acceder au repertoire de metadonnees '%s': erreur=%s\n",argv[0], options.meta_data, strerror(errno));
      exit(1);
    }
  }


/**
 * Install a custom log handler function.
 *
 * Log messages are emitted by libfuse functions to report errors and debug
 * information.  Messages are printed to stderr by default but this can be
 * overridden by installing a custom log message handler function.
 *
 * The log message handler function is global and affects all FUSE filesystems
 * created within this process.
 *
 * @param func a custom log message handler function or NULL to revert to
 *             the default
 */
  if (options.fichier_erreur_fuse !=NULL){
    fuse_set_log_func((fuse_log_func_t)  pifs_suivi_erreur_fuse);
    fclose(fopen(options.fichier_erreur_fuse, "w+"));
    PIFS_TRACE(0,"option.show_help=%d\n",options.show_help);
//    int i=0; do {printf("args.argv[%d]=%s\n",i,args.argv[i]);i++;}while (args.argv[i]!=NULL);
    PIFS_TRACE(0,"montage de %s sur %s avec metadata=%s et fichier d'erreur=%s \n",args.argv[0], options.montage,options.meta_data ,options.fichier_erreur_fuse);
  }

  options.retour_fonction = fuse_main(args.argc, args.argv, &pifs_ops, NULL);
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
  if (options.retour_fonction ) {
	    PIFS_TRACE(options.retour_fonction,"exit sur erreur \n");
	    printf("erreur numero %d \n"
	     "*   1: Invalid option arguments \n"
         "*   2: No mount point specified \n"
         "*   3: FUSE setup failed \n"
         "*   4: Mounting failed \n"
         "*   5: Failed to daemonize (detach from session) \n"
         "*   6: Failed to set up signal handlers \n"
         "*   7: An error occurred during the life of the file system \n"
         ,options.retour_fonction);
	  exit(options.retour_fonction);
  }
  fuse_opt_free_args(&args);
  return options.retour_fonction;
}
