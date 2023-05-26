AC_INIT([PIFS], [1.0], [hello@email.me], [pifs])
AC_CONFIG_AUX_DIR(build)
AC_CONFIG_SRCDIR(src/pifs.c)
AM_CONFIG_HEADER(config.h)
AM_INIT_AUTOMAKE

# Checks for programs
AC_PROG_CC_C99

# Set language to C
AC_LANG_C

# Checks for header files
AC_HEADER_DIRENT
AC_HEADER_STDC

PKG_CHECK_MODULES([FUSE], [fuse3 >= 3.1])

AC_SUBST([EXTRA_CFLAGS], ["-Wall -Wextra -Wno-unused-parameter"])

AC_CONFIG_FILES([Makefile
 src/Makefile
 ])
AC_OUTPUT
