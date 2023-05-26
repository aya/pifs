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
// #define HAVE_SETXATTR

#ifdef HAVE_CONFIG_H
// #include <config.h>
#endif

#ifndef linux
#define linux
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

#define uid_t unsigned int
#define gid_t unsigned int
#define DEBUG_PIFS 1

#ifndef PATH_MAX
#define PATH_MAX 1024
#endif

static struct options
{
	char *meta_data;
	char *montage;
	char *fichier_erreur_fuse;
	int show_help;
	char full_path[PATH_MAX];
	int retour_fonction;

} options;

/** macro to define options */

#define PIFS_OPTION(t, p)                 \
	{                                     \
		t, offsetof(struct options, p), 1 \
	}

static const struct fuse_opt pifs_options[] =
	{
		PIFS_OPTION("mdd=%s", meta_data),
		PIFS_OPTION("--erreur=%s", fichier_erreur_fuse),
		PIFS_OPTION("-h", show_help),
		PIFS_OPTION("--help", show_help),
		PIFS_OPTION("-help", show_help),
		FUSE_OPT_END};

#define PIFS_RETOUR() \
	return (options.retour_fonction == -1 ? -errno : options.retour_fonction);

#define PIFS_READ() \
	fp = popen("cat", "r");
//		fp = popen("ipfs cat", "r");

#define PIFS_WRITE() \
	options.retour_fonction = execlp("sha256sum", "sha256sum", options.full_path, NULL);
//	options.retour_fonction = execlp("cat", "cat", NULL);
//	options.retour_fonction = execlp("echo", "echo", "-n", "HASH OUTPUT EX4MPLE", NULL);
//		options.retour_fonction = execlp("ipfs", "ipfs", "add", "-q",  NULL);
//		options.retour_fonction = execlp("ipfs", "ipfs", "add", "-q", "-s", "rabin-262144-524288-1048576", NULL);

#define LOG(ret, ...)                                                                                                                                                    \
	{                                                                                                                                                                    \
		fuse_log(FUSE_LOG_INFO, "%-7ld %-15.14s ligne=%-7d pid=%-10d ===> ret=%-10d  errno=%-30.29s ", clock(), __FUNCTION__, __LINE__, getpid(), ret, strerror(errno)); \
		fuse_log(FUSE_LOG_INFO, __VA_ARGS__);                                                                                                                            \
	}

#define FULL_PATH(path) \
	snprintf(options.full_path, PATH_MAX, "%s%s", options.meta_data, path);

// struct fuse_operations {
/**
 * The file system operations:
 *
 * Most of these should work very similarly to the well known UNIX
 * file system operations.  A major exception is that instead of
 * returning an error in 'errno', the operation should return the
 * negated error value (-errno) directly.
 *
 * All methods are optional, but some are essential for a useful
 * filesystem (e.g. getattr).  Open, flush, release, fsync, opendir,
 * releasedir, fsyncdir, access, create, truncate, lock, init and
 * destroy are special purpose methods, without which a full featured
 * filesystem can still be implemented.
 *
 * In general, all methods are expected to perform any necessary
 * permission checking. However, a filesystem may delegate this task
 * to the kernel by passing the `default_permissions` mount option to
 * `fuse_new()`. In this case, methods will only be called if
 * the kernel's permission check has succeeded.
 *
 * Almost all operations take a path which can be of any length.
 */

/**Les opérations du système de fichiers :

 La plupart d’entre elles devraient fonctionner de manière très similaire à l’UNIX bien connu

 Une exception importante est qu’au lieu de renvoyer une erreur dans 'errno', l’opération doit retourner la
 négation de la valeur (-errno) directement.

 Toutes les méthodes sont optionnelles, mais certaines sont essentielles pour un
 système de fichiers (p. ex., getattr). Open, flush, release, fsync, opendir,
 releasedir, fsyncdir, access, create, truncate, lock, init et
 destroy sont des méthodes à but spécial, sans lequel un
 système de fichiers peut encore être mis en œuvre.

 En général, toutes les méthodes sont censées exécuter les
 vérification d’autorisation. Cependant, un système de fichiers peut déléguer cette tâche
 au noyau en passant l’option 'default_permissions' à fuse_new().
 Dans ce cas, les méthodes ne seront appelées que si la vérification d’autorisation du noyau a réussi.

 Presque toutes les opérations prennent un chemin qui peut être de n’importe quelle longueur.
*/

/** Get file attributes.
 *
 * Similar to stat().  The 'st_dev' and 'st_blksize' fields are
 * ignored. The 'st_ino' field is ignored except if the 'use_ino'
 * mount option is given. In that case it is passed to userspace,
 * but libfuse and the kernel will still assign a different
 * inode for internal use (called the "nodeid").
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
  int (*getattr) (const char *, struct stat *, struct fuse_file_info *fi);

 Obtenir les attributs de fichier.
 Semblable à stat(). Les champs 'st_dev' et 'st_blksize' sont
 ignorés. Le champ 'st_ino' est ignoré sauf si le champ 'use_ino’
 mount est donné. Dans ce cas, il est passé à l’espace utilisateur,
 mais libfuse et le noyau assigneront toujours un autre
 inode à usage interne (appelé "nodeid").
 « info » sera toujours NULL si le fichier n’est pas actuellement ouvert, mais
 peut également être NULL si le fichier est ouvert.
 */
static int pifs_getattr(const char *path, struct stat *buf, struct fuse_file_info *info)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = lstat(options.full_path, buf);
	if (options.retour_fonction == -1)
	{
		LOG(options.retour_fonction, "path=%s\n", options.full_path);
	}
	PIFS_RETOUR()
}

/** Read the target of a symbolic link
 *
 * The buffer should be filled with a null terminated string.  The
 * buffer size argument includes the space for the terminating
 * null character.	If the linkname is too long to fit in the
 * buffer, it should be truncated.	The return value should be 0
 * for success.

Lire la cible d’un lien symbolique
Le tampon doit être rempli avec une chaîne terminée par NULL.
La taille du tampon inclut l’espace pour la terminaison \0
Si le nom de lien est trop long pour s’insérer dans le champ
tampon, il doit être tronqué. La valeur de retour doit être 0
pour le succès.
*/
static int pifs_readlink(const char *path, char *buf, size_t bufsiz)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = readlink(options.full_path, buf, bufsiz - 1);
	if (options.retour_fonction == -1)
	{
		LOG(options.retour_fonction, "path=%s\n", options.full_path);
		return -errno;
	}

	buf[options.retour_fonction] = '\0';
	return 0;
}

/** Create a file node
 *
 * This is called for creation of all non-directory, non-symlink
 * nodes.  If the filesystem defines a create() method, then for
 * regular files that will be called instead.

 Créer un nœud de fichier
 Ceci est appelé pour la création de tous les nœuds non directory et non symlink.
 Si le système de fichiers définit une méthode create(),
 alors pour les fichiers réguliers elle sera appelée à la place.
 */
static int pifs_mknod(const char *path, mode_t mode, dev_t dev)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = mknod(options.full_path, mode, dev);
	LOG(options.retour_fonction, "path=%s\n", options.full_path);
	PIFS_RETOUR()
}

/** Create a directory
 *
 * Note that the mode argument may not have the type specification
 * bits set, i.e. S_ISDIR(mode) can be false.  To obtain the
 * correct directory type bits use  mode|S_IFDIR
 * */
static int pifs_mkdir(const char *path, mode_t mode)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = mkdir(options.full_path, mode | S_IFDIR);
	LOG(options.retour_fonction, "path=%s\n", options.full_path);
	PIFS_RETOUR()
}

/** Remove a file */
static int pifs_unlink(const char *path)
{ // supprime un fichier
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = unlink(options.full_path);
	LOG(options.retour_fonction, "path=%s\n", options.full_path);
	PIFS_RETOUR()
}

/** Remove a directory */
static int pifs_rmdir(const char *path)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = rmdir(options.full_path);
	LOG(options.retour_fonction, "path=%s\n", options.full_path);
	PIFS_RETOUR()
}

/** Create a symbolic link */
static int pifs_symlink(const char *oldpath, const char *newpath)
{ // cree un lien symbolique
	errno = 0;
	FULL_PATH(newpath);
	options.retour_fonction = symlink(oldpath, options.full_path);
	LOG(options.retour_fonction, "path=%s et oldpath=%s\n", options.full_path, oldpath);
	PIFS_RETOUR()
}

/** Rename a file
 *
 * *flags* may be `RENAME_EXCHANGE` or `RENAME_NOREPLACE`. If
 * RENAME_NOREPLACE is specified, the filesystem must not
 * overwrite *newname* if it exists and return an error
 * instead. If `RENAME_EXCHANGE` is specified, the filesystem
 * must atomically exchange the two files, i.e. both must
 * exist and neither may be deleted.

 Renommer un fichier
flags peut être “RENAME_EXCHANGE” ou “RENAME_NOREPLACE”.
Si RENAME_NOREPLACE est spécifié, le système de fichiers ne doit pas
remplacer *newname* s’il existe et renvoyer une erreur à la place.
Si  'RENAME_EXCHANGE' est spécifié, le système de fichiers
doit automatiquement échanger les deux fichiers, i.e. les deux doivent
exister et ne peuvent être supprimés	 */
static int pifs_rename(const char *oldpath, const char *newpath, unsigned int flags)
{ // renomme un fichier
	/* ************** ATTENTION FLAGS nEST PAS PRIS EN COMPTE */
	errno = 0;
	FULL_PATH(newpath);
	options.retour_fonction = rename(oldpath, options.full_path);
	LOG(options.retour_fonction, "path=%s et oldpath=%s\n", options.full_path, oldpath);
	PIFS_RETOUR()
}

/** Create a hard link to a file */
static int pifs_link(const char *oldpath, const char *newpath)
{
	errno = 0;
	FULL_PATH(newpath);
	options.retour_fonction = link(oldpath, options.full_path);
	LOG(options.retour_fonction, "path=%s et oldpath=%s\n", options.full_path, oldpath);
	PIFS_RETOUR()
}

/** Change the permission bits of a file
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
  int chmod(const char *, mode_t, struct fuse_file_info *fi);

 Si le fichier n’est pas ouvert, la valeur « info » sera toujours NULL, mais
 peut également être NULL si le fichier est ouvert.
 */
static int pifs_chmod(const char *path, unsigned int mode, struct fuse_file_info *info)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = chmod(options.full_path, mode);
	LOG(options.retour_fonction, "path=%s et mode=%d\n", options.full_path, mode);
	PIFS_RETOUR()
}

/** Change the owner and group of a file
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
  int chown(const char *, uid_t, gid_t, struct fuse_file_info *fi);
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.
 *
 Si le fichier n’est pas ouvert, la valeur « info » sera toujours NULL, mais
 peut également être NULL si le fichier est ouvert.
 à moins que FUSE_CAP_HANDLE_KILLPRIV ne soit désactivé, cette méthode devrait réinitialiser les bits setuid et setgid.
 */
static int pifs_chown(const char *path, uid_t owner, gid_t group, struct fuse_file_info *info)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = chown(options.full_path, owner, group);
	LOG(options.retour_fonction, "path=%s et owner=%d groupe=%d\n", options.full_path, owner, group);
	PIFS_RETOUR()
}

/** Change the size of a file
 *
 * `fi` will always be NULL if the file is not currently open, but
 * may also be NULL if the file is open.
 int truncate(const char *, off_t, struct fuse_file_info *fi);
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.

 Si le fichier n’est pas ouvert, la valeur « info » sera toujours NULL, mais
 peut également être NULL si le fichier est ouvert.

 */
static int pifs_truncate(const char *path, off_t length, struct fuse_file_info *info)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = truncate(options.full_path, length);
	LOG(options.retour_fonction, "path=%s et length=%d\n", options.full_path, length);
	PIFS_RETOUR()
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
 *

Les drapeaux ouverts sont disponibles dans info->flags. Les règles suivantes s’appliquent.

- Les flags de création (O_CREAT, O_EXCL, O_NOCTTY) seront filtrés / gérés par le noyau.
- Les modes d’accès (O_RDONLY, O_WRONLY, O_RDWR, O_EXEC, O_SEARCH) doivent être utilisés par le système de fichiers pour vérifier si l’opération est autorisée.
Si l’option `-o default_permissions``’ mount est donnée, cette vérification est déjà effectuée par le noyau avant d’appeler open() et peut donc être omise par le système de fichiers.
- Lorsque la mise en cache writeback est activée, le noyau peut envoyer des demandes de lecture même pour les fichiers ouverts avec O_WRONLY.
Le système de fichiers devrait être prêt à gérer cela
- Lorsque le cache writeback est désactivé, le système de fichiers doit traiter correctement l’indicateur O_APPEND et s’assurer que chaque écriture est ajoutée à la fin du fichier.
- Lorsque le cache writeback est activé, le noyau gère O_APPEND. Cependant, à moins que toutes les modifications du fichier proviennent du noyau, cela ne fonctionnera pas de manière fiable.
Le système de fichiers doit donc soit ignorer l’indicateur O_APPEND   (et laisser le noyau s’en occuper), soit renvoyer une erreur   (indiquant que O_APPEND n’est pas disponible de manière fiable).

Le système de fichiers peut stocker une poignée de fichier arbitraire (pointeur, index, etc.) dans info>fh,
et l’utiliser dans d’autres opérations de fichiers (lecture, écriture, flush, release, fsync).

Le système de fichiers peut également implémenter des E/S sans état et ne rien stocker dans info->fh.
Il y a aussi quelques flags (direct_io, keep_cache) que le système de fichiers peut mettre en info, pour changer la façon dont le fichier est ouvert.
Voir la structure fuse_file_info dans <fuse_common. h> pour plus de détails.
Si cette demande reçoit une réponse avec un code d’erreur d’ENOSYS et que FUSE_CAP_NO_OPEN_SUPPORT est défini dans « fuse_conn_info.capable »,
cela est traité comme un succès et les futurs appels à ouvrir réussiront également sans être envoyés au processus du système de fichiers.
 */
static int pifs_open(const char *path, struct fuse_file_info *info)
{ // ouvre un fichier
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = open(options.full_path, info->flags);
	LOG(options.retour_fonction, "path=%s avec flags=%d\n", options.full_path, info->flags);
	if (options.retour_fonction != -1)
		info->fh = options.retour_fonction;
	return options.retour_fonction == -1 ? -errno : 0;
}

/** Read data from an open file
 *
 * Read should return exactly the number of bytes requested except
 * on EOF or error, otherwise the rest of the data will be
 * substituted with zeroes.	 An exception to this is when the
 * 'direct_io' mount option is specified, in which case the return
 * value of the read system call will reflect the return value of
 * this operation.

 Lire des données à partir d’un fichier ouvert

 Read devrait renvoyer exactement le nombre d’octets requis sauf
 -sur EOF ou erreur, sinon le reste des données sera remplacé par des zéros.
 Une exception est lorsque l’option de montage  'direct_io' est spécifiée, auquel cas la valeur de retour de l’appel système en lecture reflétera la valeur de retour de cette opération.
 */
static int pifs_read(const char *path, char *buf, size_t count, off_t offset,
					 struct fuse_file_info *info)
{
#if 1
	errno = 0;
	//  options.retour_fonction=read(info->fh,&buf[offset],count);
	options.retour_fonction = read(info->fh, buf, count);
	if (options.retour_fonction)
	{
		LOG(options.retour_fonction, "pour path=%s count=%d offset=%d info->fh=%d\n", path, count, offset, info->fh);
	}
	PIFS_RETOUR()
#else
	int taille = 4096;
	int size = 0;
	char buffer_max[taille];
	FILE *fp;
	errno = 0;
	options.retour_fonction = lseek(info->fh, offset, SEEK_SET);
	LOG(options.retour_fonction, "path=%s count=%d offset=%d info->fh=%d\n", path, count, offset, info->fh);
	if (options.retour_fonction == -1)
	{
		return -errno;
	}

	dup2(info->fh, STDIN_FILENO);
	PIFS_READ()
	if (fp == 0)
	{
		perror("popen(3) failed");
		return -1;
	}

	do
	{
		//    options.retour_fonction = fread(&buf[offset+size],1,taille,fp);
		options.retour_fonction = fread(buffer_max, 1, taille - 1, fp);
		buffer_max[taille] = '\0';
		LOG(options.retour_fonction, "path=%s taille=%d\n", path, taille);
		if (options.retour_fonction == -1 && errno != EAGAIN)
		{
			LOG(options.retour_fonction, "path=%s \n", path);
			return -errno;
		}
		sprintf(&buf[offset + size], "%s", buffer_max);
		size += options.retour_fonction;
		count -= options.retour_fonction;
		LOG(options.retour_fonction, "path=%s count=%d size=%d\n", path, count, size);
	} while (options.retour_fonction == (taille - 1) && count > 0);

	buf[offset + size] = '\0';
	LOG(options.retour_fonction, "path=%s buffer=%s\n", path, &buf[offset]);
	pclose(fp);

	return size;
#endif
}

/** Write data to an open file
 *
 * Write should return exactly the number of bytes requested
 * except on error.	 An exception to this is when the 'direct_io'
 * mount option is specified (see read operation).
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.

  Write devrait renvoyer exactement le nombre d’octets requis sauf sur erreur.
  Une exception est lorsque l’option 'direct_io'  mount est spécifiée (voir opération de lecture).
  À moins que FUSE_CAP_HANDLE_KILLPRIV ne soit désactivé, cette méthode devrait réinitialiser les bits setuid et setgid.
 */
static int pifs_write(const char *path, const char *buf, size_t count, off_t offset,
					  struct fuse_file_info *info)
{
	pid_t w, child_pid;
	int status;
	int meta_fd[2];
	int content_fd[2];
	int result;
	errno = 0;

	FULL_PATH(path)
	result = lseek(info->fh, offset, SEEK_SET);
	if (result == -1)
	{
		LOG(options.retour_fonction, "path=%s options.meta_data=%s info->fh=%d\n", options.full_path, options.meta_data, info->fh);
		return -errno;
	}

	if (pipe(meta_fd) || pipe(content_fd))
	{
		perror("pipe(2) failed");
		LOG(-1, "sortie erreur de pifs_write sur pipe()");
		return -1;
	}

	child_pid = fork();
	if (child_pid < 0)
	{
		LOG(child_pid, "fork() FAILED\n");
		perror("erreur sur fork");
	}
	else if (child_pid > 0) // PARENT process
	// c'est le PARENT process qui ecrit les données du buffer en STDIN du child,
	// puis il récupère les metadonnees sur un autre pipe et les écrit sur info->fh
	{
#define METADATA_SIZE (1024)
		char metadata[METADATA_SIZE];

		LOG(0, "PARENT avant write buffer\n");
		close(content_fd[0]); // not used by parent

		write(content_fd[1], buf, count);
		close(content_fd[1]); // not used anymore, so free up child reading from it !
		LOG(0, "PARENT après write buffer\n");

		LOG(0, "PARENT avant read metadata\n");
		close(meta_fd[1]); // not used by parent

		size_t r = read(meta_fd[0], metadata, METADATA_SIZE); // envoyé par le child
		close(meta_fd[0]);
		LOG(0, "PARENT après read metadata: %s\n", metadata);

		w = waitpid(child_pid, &status, WUNTRACED | WCONTINUED); // parent attend le child
		if (w == -1)
		{
			LOG(w, "path=%s erreur fonction waitpid\n", options.full_path);
		}
		else if (w == ECHILD)
		{
			LOG(w, "path=%s erreur fonction waitpid le pid n'existe pas fonction waitpid\n", options.full_path);
		}
		else if (w == EINVAL)
		{
			LOG(w, "path=%s erreur fonction waitpid le parametre est mauvais fonction waitpid\n", options.full_path);
		}

		if (WIFEXITED(status))
		{
			LOG(0, "path=%s exit avec status=%d\n", options.full_path, WEXITSTATUS(status));
		}
		else if (WIFSIGNALED(status))
		{
			LOG(0, "path=%s tue par signal %d\n", options.full_path, WTERMSIG(status));
		}
		else if (WIFSTOPPED(status))
		{
			LOG(0, "path=%s stop par signal %d\n", options.full_path, WSTOPSIG(status));
		}
		else if (WIFCONTINUED(status))
		{
			LOG(0, "path=%s continue\n", options.full_path, WSTOPSIG(status));
		}

		LOG(0, "PARENT avant write metadata\n");
		options.retour_fonction = write(info->fh, metadata, r); // info->fh contains the file descriptor of the metadata
		if (options.retour_fonction == -1)
		{
			LOG(options.retour_fonction, "PARENT ERREUR write metadata\n");
			return -errno;
		}

		LOG(options.retour_fonction, "PARENT apres write metadata\n");
	}
	else // CHILD process
	{
		options.retour_fonction = dup2(content_fd[0], STDIN_FILENO);
		close(content_fd[1]); // not used by child

		options.retour_fonction = dup2(meta_fd[1], STDOUT_FILENO);
		close(meta_fd[0]); // not used by child

		LOG(options.retour_fonction, "CHILD start\n");

		PIFS_WRITE()
		close(meta_fd[1]);	  // not used anymore, so free up parent reading from it !
		close(content_fd[0]); // not used anymore
		if (options.retour_fonction == -1)
		{
			LOG(options.retour_fonction, "CHILD ERREUR PIFS_WRITE()\n");
			_exit(EXIT_FAILURE);
		}

		LOG(options.retour_fonction, "CHILD apres PIFS_WRITE\n");
	}

	LOG(0, "path=%s sortie de pifs_write", options.full_path);
	return count;
}

/** Get file system statistics
 *
 * The 'f_favail', 'f_fsid' and 'f_flag' fields are ignored
 */
static int pifs_statfs(const char *path, struct statvfs *buf)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = statvfs(options.full_path, buf);
	LOG(options.retour_fonction, "path=%s\n", options.full_path);
	PIFS_RETOUR()
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

Peut-être des données en cache

GROSSE NOTE : Ce n’est pas l’équivalent de fsync(). Ce n’est pas un
demande de synchronisation de données sales.

Flush est appelé sur chaque close() d’un descripteur de fichier, par opposition à
release qui est appelée à la clôture du dernier descripteur de fichier pour
un fichier.  Sous Linux, les erreurs retournées par flush() seront transmises à
userspace comme erreurs de close(), donc flush() est un bon endroit pour écrire
sauvegarder les données sales mises en cache. Cependant, de nombreuses applications ignorent les erreurs
sur close(), et sur les systèmes non-Linux, close() peut réussir même si flush()
retourne une erreur. Pour ces raisons, les systèmes de fichiers ne doivent pas supposer
que les erreurs retournées par flush seront jamais remarquées ou même
livré.

NOTE : La méthode flush() peut être appelée plus d’une fois pour chaque
open().  Cela se produit si plus d’un descripteur de fichier fait référence à un
poignée de fichier ouverte, p. ex., en raison d’appels dup(), dup2() ou fork(). C’est
pas possible de déterminer si un flush est définitif, donc chaque flush devrait
être traités de la même façon. Les multiples séquences d’écriture sont relativement
Ça ne devrait pas être un problème.

Les systèmes de fichiers ne doivent pas supposer que flush sera appelé à tout
point particulier. Il peut être appelé plus de fois que prévu, ou pas
du tout.

[close] : http://pubs.opengroup.org/onlinepubs/9699919799/functions/close.html
 */
int pifs_flush(const char *, struct fuse_file_info *);

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

Libérer un fichier ouvert

release est appelée lorsqu’il n’y a plus de références à une ouverture
file : tous les descripteurs de fichiers sont fermés et tous les mappages de mémoire
ne sont pas appariés.

Pour chaque appel open() il y aura exactement un appel release()
avec les mêmes drapeaux et la même poignée de fichier. Il est possible de
avoir un dossier ouvert plus d’une fois, auquel cas seulement le dernier
la sortie signifiera qu’il n’y aura plus de lecture/écriture sur le
fichier.  La valeur de retour de release est ignorée.
 */
static int pifs_release(const char *path, struct fuse_file_info *info)
{ // ferme un fichier ouvert
	errno = 0;
	options.retour_fonction = close(info->fh);
	LOG(options.retour_fonction, "path=%s et info->fh=%d\n", path, info->fh);
	PIFS_RETOUR()
}

/** Synchronize file contents
 *
 * If the datasync parameter is non-zero, then only the user data
 * should be flushed, not the meta data.

 Synchronisation du contenu des fichiers
 Si le paramètre datasync est non nul, seules les données utilisateur doivent être purgées, et non les métadonnées.
 */
static int pifs_fsync(const char *path, int datasync,
					  struct fuse_file_info *info)
{
	errno = 0;
	options.retour_fonction = datasync ? fdatasync(info->fh) : fsync(info->fh);
	LOG(options.retour_fonction, "path=%s et info->fh=%d\n", path, info->fh);
	//  qui_est_ce(info->fh);
	PIFS_RETOUR()
}

/** Set extended attributes */
#ifdef HAVE_SETXATTR
/* xattr operations are optional and can safely be left unimplemented */
static int pifs_setxattr(const char *path, const char *name, const char *value,
						 size_t size, int flags)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = setxattr(options.full_path, name, value, size, flags);
	LOG(options.retour_fonction, "path=%s name=%s\n", options.full_path, name);
	PIFS_RETOUR()
}
#endif // HAVE_SETXATTR

/** Get extended attributes */
#ifdef HAVE_SETXATTR
static int pifs_getxattr(const char *path, const char *name, char *value,
						 size_t size)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = getxattr(options.full_path, name, value, size);
	LOG(options.retour_fonction, "path=%s name=%s\n", options.full_path, name);
	PIFS_RETOUR()
}
#endif // HAVE_SETXATTR

/** List extended attributes */
#ifdef HAVE_SETXATTR
static int pifs_listxattr(const char *path, char *list, size_t size)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = listxattr(options.full_path, list, size);
	LOG(options.retour_fonction, "path=%s list=%s\n", options.full_path, list);
	PIFS_RETOUR()
}
#endif // HAVE_SETXATTR

/** Remove extended attributes */
#ifdef HAVE_SETXATTR
static int pifs_removexattr(const char *path, const char *name)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = removexattr(options.full_path, name);
	LOG(options.retour_fonction, "path=%s name=%s\n", options.full_path, name);
	PIFS_RETOUR()
}
#endif // HAVE_SETXATTR

/** Open directory
 *
 * Unless the 'default_permissions' mount option is given,
 * this method should check if opendir is permitted for this
 * directory. Optionally opendir may also return an arbitrary
 * filehandle in the fuse_file_info structure, which will be
 * passed to readdir, releasedir and fsyncdir.

Sauf si l’option 'default_permissions' est donnée,
cette méthode devrait vérifier si opendir est autorisé pour ce répertoire.
Optionnellement opendir peut aussi renvoyer un filehandle dans la structure fuse_file_info, qui sera
transmis à readdir, releasedir et fsyncdir.
 */
static int pifs_opendir(const char *path, struct fuse_file_info *info)
{ // ouvre un repertoire
	DIR *dir;
	errno = 0;
	FULL_PATH(path);
	dir = opendir(options.full_path);
	if (!dir)
	{
		LOG(-1, "pas de dir path=%s\n", options.full_path);
		return -errno;
	}
	info->fh = (uint64_t)dir;
	LOG(0, "path=%s info->fh=%d\n", options.full_path, info->fh);
	return 0;
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
------------------------------------------------------------------------------------------
 * When FUSE_READDIR_PLUS is not set, only some parameters of the
 * fill function (the fuse_fill_dir_t parameter) are actually used:
 * The file type (which is part of stat::st_mode) is used. And if
 * fuse_config::use_ino is set, the inode (stat::st_ino) is also
 * used. The other fields are ignored when FUSE_READDIR_PLUS is not
 * set.
-------------------------------------------------------------------------------------------

Le système de fichiers peut choisir entre deux modes de fonctionnement :

1) L’implémentation readdir ignore le paramètre offset, et
passe zéro au décalage de la fonction de remplissage.  Le remplissage
ne retournera pas '1' (sauf si une erreur se produit), donc la
le répertoire entier est lu dans une seule opération readdir.

2) L’implémentation de readdir suit les décalages des
entrées du répertoire.  Il utilise le paramètre offset et
passe un décalage non nul à la fonction de remplissage.  Lorsque le tampon
est plein (ou une erreur se produit) la fonction de remplissage retournera
'1'.
------------------------------------------------------------------------------------------
Lorsque FUSE_READDIR_PLUS n’est pas défini, seuls certains paramètres du
la fonction de remplissage (le paramètre fuse_fill_dir_t) est actuellement utilisée :
Le type de fichier (qui fait partie du stat::st_mode) est utilisé. Et si
fuse_config::use_ino est défini, l’inode (stat::st_ino) est également
utilisé. Les autres champs sont ignorés lorsque FUSE_READDIR_PLUS n’est pas
set.
------------------------------------------------------------------------------------------
 */
static int pifs_readdir(const char *path, void *buf, fuse_fill_dir_t filler,
						off_t offset, struct fuse_file_info *info, enum fuse_readdir_flags flags)
{ // lit le contenu d'un repertoire
	DIR *dir;
	struct dirent *de;
	errno = 0;
	dir = (DIR *)info->fh;
	if (offset)
	{
		seekdir(dir, offset);
	}

	do
	{
		errno = 0;
		de = readdir(dir);
		if (!de)
		{
			if (errno)
			{
				LOG(-1, "pas de de path=%s info->fh=%d\n", path, info->fh);
				return -errno;
			}
			else
			{
				break;
			}
		}
		options.retour_fonction = filler(buf, de->d_name, NULL, de->d_off, 0);
		LOG(options.retour_fonction, "path=%s de->d_name=%s\n", path, de->d_name);
	} while (options.retour_fonction == 0);

	LOG(0, "path=%s info->fh=%d\n", path, info->fh);
	return 0;
}

/** Release directory
 *
 * If the directory has been removed after the call to opendir, the
 * path parameter will be NULL.

 Si le répertoire a été supprimé après l’appel à opendir, le
 le paramètre path sera NULL.

 */
static int pifs_releasedir(const char *path, struct fuse_file_info *info)
{ // ferme un repertoire ouvert
	errno = 0;
	options.retour_fonction = closedir((DIR *)info->fh);
	LOG(options.retour_fonction, "path=%s info->fh=%d\n", path, info->fh);
	PIFS_RETOUR()
}

/** Synchronize directory contents
 *
 * If the directory has been removed after the call to opendir, the
 * path parameter will be NULL.
 *
 * If the datasync parameter is non-zero, then only the user data
 * should be flushed, not the meta data

Si le répertoire a été supprimé après l’appel à opendir, le
path sera NULL.

Si le paramètre datasync est non nul, seules les données utilisateur
doit être effacé, pas les métadonnées

 */
static int pifs_fsyncdir(const char *path, int datasync,
						 struct fuse_file_info *info)
{
	errno = 0;
	int fd = dirfd((DIR *)info->fh);
	if (fd == -1)
	{
		LOG(fd, "pas de dirfd path=%s info->fh=%d\n", path, info->fh);
		return -errno;
	}

	options.retour_fonction = datasync ? fdatasync(fd) : fsync(fd);
	LOG(options.retour_fonction, "path=%s info->fh=%d\n", path, info->fh);
	PIFS_RETOUR()
}

/**
 * Initialize filesystem
 *
 * The return value will passed in the `private_data` field of
 * `struct fuse_context` to all file operations, and as a
 * parameter to the destroy() method. It overrides the initial
 * value provided to fuse_main() / fuse_new().

 Initialiser le système de fichiers
 La valeur de retour sera passée dans le champ « private_data » de « struct fuse_context » à toutes les opérations du fichier,
 et comme paramètre de la méthode destroy().
 *Elle remplace la valeur initiale fournie à fuse_main() / fuse_new().
 */
void *pifs_init(struct fuse_conn_info *conn,
				struct fuse_config *cfg);

/**
 * Clean up filesystem
 *
 * Called on filesystem exit.
 */
void pifs_destroy(void *private_data);

/**
 * Check file access permissions
 *
 * This will be called for the access() system call.  If the
 * 'default_permissions' mount option is given, this method is not
 * called.
 *
 * This method is not called under Linux kernel versions 2.4.x

Vérifier les autorisations d’accès aux fichiers

Ceci sera appelé pour l’appel système access().  Si l’option 'default_permissions' est donnée, cette méthode n’est pas appelée.

Cette méthode n’est pas appelée sous les versions du noyau Linux 2.4.x
 */
static int pifs_access(const char *path, int mode)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = access(options.full_path, mode);
	LOG(options.retour_fonction, "path=%s mode=%d\n", options.full_path, mode);
#ifdef DEBUG_PIFS
	if (options.retour_fonction == -1)
	{
		LOG(options.retour_fonction, "EXIT du programme sous pifs_access");
		exit(-9);
	}
#endif
	PIFS_RETOUR()
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

Si le fichier n’existe pas, créez-le d’abord avec le mode spécifié, puis ouvrez-le.

Si cette méthode n’est pas implémentée ou sous les versions du noyau Linux antérieures à 2.6.15, les méthodes mknod() et open() seront appelées à la place.

 */
static int pifs_create(const char *path, mode_t mode,
					   struct fuse_file_info *info)
{
	errno = 0;
	FULL_PATH(path);
	options.retour_fonction = creat(options.full_path, mode);
	info->fh = options.retour_fonction;
	LOG(options.retour_fonction, "path=%s mode=%d\n", options.full_path, mode);
	return options.retour_fonction == -1 ? -errno : 0;
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

Effectuer une opération de verrouillage de fichier POSIX

L’argument cmd sera soit F_GETLK, F_SETLK ou F_SETLKW.

Pour la signification des champs dans 'struct flock', voir la page de manuel pour fcntl(2).
Le champ l_whence sera toujours défini sur SEEK_SET.

Pour vérifier la propriété du verrou, l’argument 'fuse_file_info->owner' doit être utilisé.

Pour l’opération F_GETLK, la bibliothèque vérifiera d’abord les verrous actuellement détenus, et si un verrou en conflit est trouvé, elle renverra l’information sans appeler cette méthode.
Cela garantit que pour les verrous locaux, le champ l_pid est correctement rempli.
Les résultats peuvent ne pas être précis en cas de conditions de course et en présence de liens durs,
mais il est peu probable qu’une application s’appuie sur des résultats GETLK précis dans ces cas.
Si un verrou en conflit n’est pas trouvé, cette méthode sera appelée,
et le système de fichiers peut remplir l_pid par une valeur significative, ou il peut laisser ce champ zéro.

Pour F_SETLK et F_SETLKW, le champ l_pid sera défini sur le pid du processus effectuant l’opération de verrouillage.

Note : si cette méthode n’est pas implémentée, le noyau autorisera toujours le verrouillage des fichiers à fonctionner localement.
Par conséquent, il n’est intéressant que pour les systèmes de fichiers réseau et similaires.
 */
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
	errno = 0;
	options.retour_fonction = fcntl(info->fh, cmd, lock);
	LOG(options.retour_fonction, "path=%s info->fh=%d cmd=%s demande=%s\n", path, info->fh,
		(cmd == F_GETLK ? "F_GETLK" : (cmd == F_SETLK ? "F_SETLK" : "F_SETLKW")),
		(lock->l_type == F_RDLCK ? "F_RDLCK" : (lock->l_type == F_WRLCK ? "F_WRLCK" : "F_UNLCK")));
	PIFS_RETOUR()
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
 */
static int pifs_utimens(const char *path, const struct timespec times[2], struct fuse_file_info *fi)
{
	DIR *dir;
	errno = 0;
	dir = opendir(options.meta_data);
	if (!dir)
	{
		LOG(0, " pas de dir path=%s\n", path);
		return -errno;
	}
	options.retour_fonction = utimensat(dirfd(dir), path, times, 0);
	closedir(dir);
	LOG(options.retour_fonction, "path=%s\n", path);
	PIFS_RETOUR()
}

/**
 * Map block index within file to block index within device
 *
 * Note: This makes sense only for block device backed filesystems
 * mounted with the 'blkdev' option
 */
int pifs_bmap(const char *, size_t blocksize, uint64_t *idx);

#if FUSE_USE_VERSION < 35
int pifs_ioctl(const char *, int cmd, void *arg,
			   struct fuse_file_info *, unsigned int flags, void *data);
#else
/**
 * Ioctl
 *
 * flags will have FUSE_IOCTL_COMPAT set for 32bit ioctls in
 * 64bit environment.  The size and direction of data is
 * determined by _IOC_*() decoding of cmd.  For _IOC_NONE,
 * data will be NULL, for _IOC_WRITE data is out area, for
 * _IOC_READ in area and if both are set in/out area.  In all
 * non-NULL cases, the area is of _IOC_SIZE(cmd) bytes.
 *
 * If flags has FUSE_IOCTL_DIR then the fuse_file_info refers to a
 * directory file handle.
 *
 * Note : the unsigned long request submitted by the application
 * is truncated to 32 bits.
 */
int pifs_ioctl(const char *, unsigned int cmd, void *arg,
			   struct fuse_file_info *, unsigned int flags, void *data);
#endif

/**
 * Poll for IO readiness events
 *
 * Note: If ph is non-NULL, the client should notify
 * when IO readiness events occur by calling
 * fuse_notify_poll() with the specified ph.
 *
 * Regardless of the number of times poll with a non-NULL ph
 * is received, single notification is enough to clear all.
 * Notifying more times incurs overhead but doesn't harm
 * correctness.
 *
 * The callee is responsible for destroying ph with
 * fuse_pollhandle_destroy() when no longer in use.
 */
int pifs_poll(const char *, struct fuse_file_info *,
			  struct fuse_pollhandle *ph, unsigned *reventsp);

/** Write contents of buffer to an open file
 *
 * Similar to the write() method, but data is supplied in a
 * generic buffer.  Use fuse_buf_copy() to transfer data to
 * the destination.
 *
 * Unless FUSE_CAP_HANDLE_KILLPRIV is disabled, this method is
 * expected to reset the setuid and setgid bits.
 */
int pifs_write_buf(const char *, struct fuse_bufvec *buf, off_t off,
				   struct fuse_file_info *);

/** Store data from an open file in a buffer
 *
 * Similar to the read() method, but data is stored and
 * returned in a generic buffer.
 *
 * No actual copying of data has to take place, the source
 * file descriptor may simply be stored in the buffer for
 * later data transfer.
 *
 * The buffer must be allocated dynamically and stored at the
 * location pointed to by bufp.  If the buffer contains memory
 * regions, they too must be allocated using malloc().  The
 * allocated memory will be freed by the caller.
 */
int pifs_read_buf(const char *, struct fuse_bufvec **bufp,
				  size_t size, off_t off, struct fuse_file_info *);
/**
 * Perform BSD file locking operation
 *
 * The op argument will be either LOCK_SH, LOCK_EX or LOCK_UN
 *
 * Nonblocking requests will be indicated by ORing LOCK_NB to
 * the above operations
 *
 * For more information see the flock(2) manual page.
 *
 * Additionally fi->owner will be set to a value unique to
 * this open file.  This same value will be supplied to
 * ->release() when the file is released.
 *
 * Note: if this method is not implemented, the kernel will still
 * allow file locking to work locally.  Hence it is only
 * interesting for network filesystems and similar.
 */
int pifs_flock(const char *, struct fuse_file_info *, int op);

/**
 * Allocates space for an open file
 *
 * This function ensures that required space is allocated for specified
 * file.  If this function returns success then any subsequent write
 * request to specified range is guaranteed not to fail because of lack
 * of space on the file system media.
 */
int pifs_fallocate(const char *, int, off_t, off_t,
				   struct fuse_file_info *);

/**
 * Copy a range of data from one file to another
 *
 * Performs an optimized copy between two file descriptors without the
 * additional cost of transferring data through the FUSE kernel module
 * to user space (glibc) and then back into the FUSE filesystem again.
 *
 * In case this method is not implemented, applications are expected to
 * fall back to a regular file copy.   (Some glibc versions did this
 * emulation automatically, but the emulation has been removed from all
 * glibc release branches.)
 */
ssize_t pifs_copy_file_range(const char *path_in,
							 struct fuse_file_info *fi_in,
							 off_t offset_in, const char *path_out,
							 struct fuse_file_info *fi_out,
							 off_t offset_out, size_t size, int flags);

/**
 * Find next data or hole after the specified offset
 */
off_t pifs_lseek(const char *, off_t off, int whence, struct fuse_file_info *);
// } struct fuse_operations {

static int pifs_ftruncate(const char *path, off_t length,
						  struct fuse_file_info *info)
{
	errno = 0;
	options.retour_fonction = ftruncate(info->fh, length * 2);
	LOG(options.retour_fonction, "path=%s\n", path);
	PIFS_RETOUR()
}

/*
static int pifs_fgetattr(const char *path, struct stat *buf,
						struct fuse_file_info *info)
{
  errno=0;
  options.retour_fonction = fstat(info->fh, buf);
  LOG(options.retour_fonction,"path=%s info->fh=%d\n",path,info-fh);
  PIFS_RETOUR()
}
*/

const static struct fuse_operations pifs_ops = {
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
	.read = pifs_read,
	.write = pifs_write,
	//  .statfs = pifs_statfs,
	.release = pifs_release,
//  .fsync = pifs_fsync,
#ifdef HAVE_SETXATTR
	/* xattr operations are optional and can safely be left unimplemented */
	.setxattr = pifs_setxattr,
	.getxattr = pifs_getxattr,
	.listxattr = pifs_listxattr,
	.removexattr = pifs_removexattr,
#endif // #ifdef HAVE_SETXATTR
	.opendir = pifs_opendir,
	.readdir = pifs_readdir,
	.releasedir = pifs_releasedir,
	//  .fsyncdir = pifs_fsyncdir,
	//  .access = pifs_access,
	.create = pifs_create,
	//  .ftruncate = pifs_ftruncate,
	//  .fgetattr = pifs_fgetattr,
	.lock = pifs_lock,
#if FUSE_USE_VERSION < 30
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
pour demonter le point de montage faire fusermount3 -u <repertoire_de_montage> ou sudo -fl umount <repertoire_de_montage>\n\n",
		   progname, progname);
}

int main(int argc, char *argv[])
{
	struct fuse_args args = FUSE_ARGS_INIT(argc, argv);

	memset(&options, 0, sizeof(struct options));
	options.montage = strdup(argv[argc - 1]);

	if ((options.retour_fonction = fuse_opt_parse(&args, &options, pifs_options, NULL)) == -1)
	{
		printf("sortie main erreur_fuse_opt_parse de pifs_mains avec ===> options.retour_fonction=%d  errno=%s\n", options.retour_fonction, strerror(errno));
		exit(1);
	}

	if (options.show_help)
	{
		show_help(argv[0]);
		assert(fuse_opt_add_arg(&args, "--help") == 0);
		args.argv[0][0] = '\0';
	}
	else if (!options.meta_data)
	{
		printf("ERROR %s: le repertoire de metadonnees doit etre donné par -o mdd=<directory>\n", argv[0]);
		exit(1);
	}
	else
	{
		//  options.retour_fonction=pifs_access(options.meta_data, R_OK | W_OK | X_OK);
		options.retour_fonction = access(options.meta_data, R_OK | W_OK | X_OK);

		if (options.retour_fonction < 0)
		{
			printf("%s: impossible d'acceder au repertoire de metadonnees '%s': erreur=%s\n", argv[0], options.meta_data, strerror(errno));
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
	if (options.fichier_erreur_fuse != NULL)
	{
		fclose(fopen(options.fichier_erreur_fuse, "w+"));
		LOG(0, "option.show_help=%d\n", options.show_help);
		//    int i=0; do {printf("args.argv[%d]=%s\n",i,args.argv[i]);i++;}while (args.argv[i]!=NULL);
		LOG(0, "montage de %s sur %s avec metadata=%s et fichier d'erreur=%s \n", args.argv[0], options.montage, options.meta_data, options.fichier_erreur_fuse);
		fuse_set_log_func((fuse_log_func_t)pifs_suivi_erreur_fuse);
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
	if (options.retour_fonction)
	{
		LOG(options.retour_fonction, "exit sur erreur \n");
		printf("erreur numero %d \n"
			   "*   1: Invalid option arguments \n"
			   "*   2: No mount point specified \n"
			   "*   3: FUSE setup failed \n"
			   "*   4: Mounting failed \n"
			   "*   5: Failed to daemonize (detach from session) \n"
			   "*   6: Failed to set up signal handlers \n"
			   "*   7: An error occurred during the life of the file system \n",
			   options.retour_fonction);
		exit(options.retour_fonction);
	}
	fuse_opt_free_args(&args);
	return options.retour_fonction;
}
